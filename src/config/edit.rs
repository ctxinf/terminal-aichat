//! Small JSONC token locator used to replace a credential without rewriting comments.
use std::ops::Range;

struct Token {
    range: Range<usize>,
    kind: u8,
}
fn tokens(source: &str) -> Result<Vec<Token>, String> {
    let bytes = source.as_bytes();
    let mut result = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if bytes[i..].starts_with(b"//") {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if bytes[i..].starts_with(b"/*") {
            i += 2;
            while i + 1 < bytes.len() && !bytes[i..].starts_with(b"*/") {
                i += 1;
            }
            if i + 1 >= bytes.len() {
                return Err("Unterminated config comment".into());
            }
            i += 2;
            continue;
        }
        let start = i;
        let kind = bytes[i];
        i += 1;
        if kind == b'"' {
            loop {
                if i >= bytes.len() {
                    return Err("Unterminated config string".into());
                }
                if bytes[i] == b'\\' {
                    i += 2;
                } else if bytes[i] == b'"' {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
        } else if !b"{}[]:,".contains(&kind) {
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() && !b"{}[]:,/".contains(&bytes[i]) {
                i += 1;
            }
        }
        result.push(Token { range: start..i, kind });
    }
    Ok(result)
}
fn value_end(tokens: &[Token], start: usize) -> Result<usize, String> {
    if !matches!(tokens[start].kind, b'{' | b'[') {
        return Ok(start);
    }
    let mut depth = 0;
    for (index, token) in tokens.iter().enumerate().skip(start) {
        match token.kind {
            b'{' | b'[' => depth += 1,
            b'}' | b']' => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            return Ok(index);
        }
    }
    Err("Unterminated config object".into())
}
fn member(source: &str, tokens: &[Token], object: usize, name: &str) -> Result<Option<usize>, String> {
    if tokens.get(object).is_none_or(|t| t.kind != b'{') {
        return Err("Expected config object".into());
    }
    let end = value_end(tokens, object)?;
    let mut index = object + 1;
    let mut found = None;
    while index < end {
        if tokens[index].kind == b',' {
            index += 1;
            continue;
        }
        let key: String =
            serde_json::from_str(&source[tokens[index].range.clone()]).map_err(|_| "Invalid config key")?;
        if tokens.get(index + 1).is_none_or(|t| t.kind != b':') {
            return Err("Invalid config property".into());
        }
        let value = index + 2;
        if value >= end {
            return Err("Missing config value".into());
        }
        if key == name {
            if found.is_some() {
                return Err("Duplicate config key; resolve duplicates before login".into());
            }
            found = Some(value);
        }
        index = value_end(tokens, value)? + 1;
    }
    Ok(found)
}

pub(super) fn replace_api_key(source: &str, provider: &str, value: &super::ApiKeyConfig) -> Result<String, String> {
    let tokens = tokens(source)?;
    let providers = member(source, &tokens, 0, "providers")?.ok_or("Missing providers")?;
    let target = member(source, &tokens, providers, provider)?.ok_or("Provider not found")?;
    let encoded = serde_json::to_string(value).map_err(|_| "Cannot serialize API key")?;
    let mut output = source.to_owned();
    if let Some(current) = member(source, &tokens, target, "apiKey")? {
        let end = value_end(&tokens, current)?;
        output.replace_range(tokens[current].range.start..tokens[end].range.end, &encoded);
    } else {
        let comma = if tokens.get(target + 1).is_some_and(|t| t.kind != b'}') {
            ","
        } else {
            ""
        };
        output.insert_str(
            tokens[target].range.end,
            &format!("\n      \"apiKey\": {encoded}{comma}"),
        );
    }
    Ok(output)
}

/// Add a prompt without confusing strings/comments with property names.
pub(crate) fn insert_prompt(source: &str, name: &str, prompt: &super::PromptConfig) -> Result<String, String> {
    let tokens = tokens(source)?;
    let key = serde_json::to_string(name).map_err(|e| e.to_string())?;
    let value = serde_json::to_string(prompt).map_err(|e| e.to_string())?;
    let (target, entry) = match member(source, &tokens, 0, "prompts")? {
        Some(target) => {
            if member(source, &tokens, target, name)?.is_some() {
                return Ok(source.to_owned());
            }
            (target, format!("{key}: {value}"))
        }
        None => (0, format!("\"prompts\": {{{key}: {value}}}")),
    };
    let comma = if tokens.get(target + 1).is_some_and(|t| t.kind != b'}') {
        ","
    } else {
        ""
    };
    let mut output = source.to_owned();
    output.insert_str(tokens[target].range.end, &format!("\n    {entry}{comma}\n"));
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_jsonc_comments_and_targets_only_requested_provider() {
        let source = r#"{ // keep display comment
          "display": { /* optional */ },
          "providers": {
            "escaped\"id": {"baseUrl":"https://example.com", "apiKey":{"value":"old"}, "models":[]},
            "other": {"baseUrl":"http://localhost", "models":[]}
          }
        }"#;
        let key = super::super::ApiKeyConfig {
            value: "new".into(),
            encryption: None,
        };
        let updated = replace_api_key(source, "escaped\"id", &key).unwrap();
        assert!(updated.contains("// keep display comment"));
        assert!(updated.contains("/* optional */"));
        let parsed: serde_json::Value = serde_json_lenient::from_str(&updated).unwrap();
        assert_eq!(parsed["providers"]["escaped\"id"]["apiKey"]["value"], "new");
        assert!(parsed["providers"]["other"]["apiKey"].is_null());
        let inserted = replace_api_key(source, "other", &key).unwrap();
        let parsed: serde_json::Value = serde_json_lenient::from_str(&inserted).unwrap();
        assert_eq!(parsed["providers"]["other"]["apiKey"]["value"], "new");
        assert_eq!(parsed["providers"]["escaped\"id"]["apiKey"]["value"], "old");
        assert!(replace_api_key(source, "missing", &key).is_err());
    }
}
