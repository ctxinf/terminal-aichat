//! Helpers for `aichat --init-integration <SHELL> [--prompt <NAME>]`:
//! Config-file patching.

use super::SHELL_INTEGRATION_PROMPT;
use crate::config::{Config, ConfigManager, PromptConfig};

/// Make sure the named integration prompt exists in the user's config file.
///
/// - If the config file is missing, write the default config (which already
///   contains `prompts.shell-exec-or-chat`).
/// - If the config file exists but does not define the named prompt, append
///   a JSONC block that adds it. This preserves all existing comments and
///   formatting in the file.
/// - If the named prompt already exists, do nothing.
pub fn ensure_integration_prompt(
    config_manager: &ConfigManager,
    prompt_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if !config_manager.exists() {
        if config_manager.is_default_path() {
            config_manager.save_default_with_comments()?;
            eprintln!(
                "📝 Created default config at {}",
                config_manager.get_config_path().display()
            );
        } else {
            eprintln!(
                "⚠️  Config file {} does not exist; the shell integration will fail until you create it and define a `{}` prompt.",
                config_manager.get_config_path().display(),
                prompt_name
            );
            return Ok(());
        }
    }

    let config = config_manager.load()?;
    if config.prompts.contains_key(prompt_name) {
        return Ok(());
    }

    let path = config_manager.get_config_path();
    let original = std::fs::read_to_string(path)?;
    let patched = insert_prompt_into_jsonc(&original, prompt_name)?;
    std::fs::write(path, patched)?;
    eprintln!(
        "📝 Added prompt `{}` to {} — edit it any time.",
        prompt_name,
        path.display()
    );
    Ok(())
}

/// Insert a `prompts.<name>` entry into a JSONC config string, preserving
/// the rest of the file. Falls back to a serde round-trip (which strips
/// comments) if textual insertion is not possible.
fn insert_prompt_into_jsonc(source: &str, prompt_name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let entry = format!(
        "    \"{name}\": {{\n      \"system\": {json}\n    }}",
        name = prompt_name,
        json = serde_json::to_string(SHELL_INTEGRATION_PROMPT)?
    );

    // Try to locate `"prompts"` and insert before its closing `}`.
    if let Some(start) = source.find("\"prompts\"") {
        if let Some(open_rel) = source[start..].find('{') {
            let open_idx = start + open_rel;
            let mut depth: i32 = 0;
            let bytes = source.as_bytes();
            let mut i = open_idx;
            while i < bytes.len() {
                let c = bytes[i] as char;
                match c {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            // Drop whitespace between the previous entry and `}`
                            // so we can rebuild cleanly: previous-entry(,) + entry + `  }`.
                            let head = &source[..i];
                            let tail = &source[i..];
                            let trimmed_head = head.trim_end();
                            let needs_comma = !trimmed_head.ends_with('{') && !trimmed_head.ends_with(',');
                            let mut out = String::with_capacity(source.len() + entry.len() + 8);
                            out.push_str(trimmed_head);
                            if needs_comma {
                                out.push(',');
                            }
                            out.push('\n');
                            out.push_str(&entry);
                            out.push('\n');
                            out.push_str("  ");
                            out.push_str(tail);
                            return Ok(out);
                        }
                    }
                    '"' => {
                        // skip string contents (handles escapes)
                        i += 1;
                        while i < bytes.len() {
                            let ch = bytes[i] as char;
                            if ch == '\\' {
                                i += 2;
                                continue;
                            }
                            if ch == '"' {
                                break;
                            }
                            i += 1;
                        }
                    }
                    '/' if i + 1 < bytes.len() => {
                        let next = bytes[i + 1] as char;
                        if next == '/' {
                            i += 2;
                            while i < bytes.len() && bytes[i] as char != '\n' {
                                i += 1;
                            }
                        } else if next == '*' {
                            i += 2;
                            while i + 1 < bytes.len() && !(bytes[i] as char == '*' && bytes[i + 1] as char == '/') {
                                i += 1;
                            }
                            i += 1; // step to '/'
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
        }
    }

    // Fallback: serde round-trip. Loses comments but always works.
    let mut config: Config = serde_json_lenient::from_str(source)?;
    config.prompts.insert(
        prompt_name.to_string(),
        PromptConfig {
            system: Some(SHELL_INTEGRATION_PROMPT.to_string()),
            ..Default::default()
        },
    );
    Ok(serde_json::to_string_pretty(&config)?)
}
