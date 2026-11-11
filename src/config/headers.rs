use std::{collections::HashMap, sync::OnceLock, time::Duration};
use tokio::{process::Command, sync::Mutex, time::timeout};

static COMMAND_CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

pub async fn resolve_header(value: &str) -> Result<String, String> {
    if let Some(command) = value.strip_prefix('!') {
        let mut cache = COMMAND_CACHE.get_or_init(Default::default).lock().await;
        if let Some(value) = cache.get(command) {
            return Ok(value.clone());
        }
        #[cfg(unix)]
        let mut child = {
            let mut c = Command::new("sh");
            c.arg("-c").arg(command);
            c
        };
        #[cfg(windows)]
        let mut child = {
            let mut c = Command::new("cmd");
            c.arg("/C").arg(command);
            c
        };
        child
            .kill_on_drop(true)
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let output = timeout(Duration::from_secs(10), child.output())
            .await
            .map_err(|_| "header command timed out")?
            .map_err(|_| "header command failed to start")?;
        if !output.status.success() {
            return Err("header command failed".into());
        }
        let result = String::from_utf8(output.stdout).map_err(|_| "header command returned invalid Unicode")?;
        let result = result.trim_end_matches(['\r', '\n']).to_owned();
        if result.is_empty() {
            return Err("header command returned empty output".into());
        }
        cache.insert(command.to_owned(), result.clone());
        return Ok(result);
    }
    interpolate(value)
}

fn interpolate(value: &str) -> Result<String, String> {
    let mut result = String::new();
    let mut rest = value;
    while let Some(pos) = rest.find('$') {
        result.push_str(&rest[..pos]);
        rest = &rest[pos + 1..];
        let (key, consumed) = if rest.starts_with('{') {
            let end = rest.find('}').ok_or("unclosed header environment reference")?;
            (&rest[1..end], end + 1)
        } else {
            let end = rest
                .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .unwrap_or(rest.len());
            (&rest[..end], end)
        };
        if key.is_empty() {
            return Err("empty header environment reference".into());
        }
        let value =
            std::env::var(key).map_err(|_| format!("missing or invalid header environment variable '{key}'"))?;
        if value.is_empty() {
            return Err(format!("empty header environment variable '{key}'"));
        }
        result.push_str(&value);
        rest = &rest[consumed..];
    }
    result.push_str(rest);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn literal_and_command_headers() {
        assert_eq!(resolve_header("plain-header").await.unwrap(), "plain-header");
        #[cfg(unix)]
        {
            assert_eq!(resolve_header("!printf test-header").await.unwrap(), "test-header");
            assert_eq!(resolve_header("!printf test-header").await.unwrap(), "test-header");
            assert!(resolve_header("!exit 1").await.is_err());
        }
        assert!(interpolate("${}").is_err());
        assert!(interpolate("${UNCLOSED").is_err());
    }
}
