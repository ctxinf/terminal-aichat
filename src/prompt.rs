//! Configuration templates are parsed once; inserted values are never templates.
use crate::config::PromptConfig;
use serde_json::{Value, json};
use std::{error::Error, process::Stdio, time::Duration};
use tokio::{io::AsyncReadExt, process::Command, time::timeout};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const OUTPUT_LIMIT: usize = 64 * 1024;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(3);

pub async fn render(prompt: &PromptConfig, input: &str) -> Result<Value> {
    if prompt.system.is_some() && !prompt.content.is_empty() {
        return Err("prompt content and system cannot be combined".into());
    }
    // Legacy content remains literal, preserving existing prompts containing braces.
    let system = match &prompt.system {
        Some(template) => render_field("system", template, input).await?,
        None => prompt.content.clone(),
    };
    let user = match &prompt.user {
        Some(template) => render_field("user", template, input).await?,
        None => input.to_owned(),
    };
    Ok(json!([{"role":"system", "content":system}, {"role":"user", "content":user}]))
}

async fn render_field(field: &str, template: &str, input: &str) -> Result<String> {
    crate::log_debug!("Rendering prompt template field: {field}");
    let result = render_template(template, input).await;
    if let Err(error) = &result {
        crate::log_debug!("Prompt template field {field} failed: {error}");
    }
    result
}

async fn render_template(template: &str, input: &str) -> Result<String> {
    let mut result = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        result.push_str(&rest[..start]);
        rest = &rest[start + 2..];
        // A quoted command may itself contain braces, quotes and parentheses.
        let mut quoted = false;
        let mut escaped = false;
        let mut end = None;
        for (offset, ch) in rest.char_indices() {
            if escaped {
                escaped = false;
            } else if quoted && ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = !quoted;
            } else if !quoted && rest[offset..].starts_with("}}") {
                end = Some(offset);
                break;
            }
        }
        let end = end.ok_or("unclosed prompt template expression")?;
        let expression = rest[..end].trim();
        let value = match expression {
            "input" => input.to_owned(),
            "cwd" => std::env::current_dir()?
                .to_str()
                .ok_or("working directory is not Unicode")?
                .to_owned(),
            _ => {
                let (name, arguments) = expression.split_once('(').ok_or("unknown prompt template expression")?;
                let argument = arguments
                    .trim()
                    .strip_suffix(')')
                    .ok_or("invalid prompt function call")?;
                let argument: String =
                    serde_json::from_str(argument).map_err(|_| "prompt functions require one JSON string argument")?;
                match name.trim() {
                    "env" => std::env::var(&argument).map_err(|_| "missing or invalid prompt environment variable")?,
                    "shell" => run_shell(&argument, COMMAND_TIMEOUT).await?,
                    _ => return Err("unknown prompt template function".into()),
                }
            }
        };
        result.push_str(&value);
        rest = &rest[end + 2..];
    }
    result.push_str(rest);
    Ok(result)
}

// Killing the process group also stops descendants holding stdout/stderr open.
#[cfg(unix)]
struct ProcessGroup(u32);
#[cfg(unix)]
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        // SAFETY: this is the dedicated process group created for our child.
        unsafe {
            libc::kill(-(self.0 as i32), libc::SIGKILL);
        }
    }
}

async fn read_limited(reader: impl tokio::io::AsyncRead + Unpin) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take((OUTPUT_LIMIT + 1) as u64).read_to_end(&mut bytes).await?;
    if bytes.len() > OUTPUT_LIMIT {
        return Err("prompt command output exceeds 64 KiB".into());
    }
    Ok(bytes)
}

// Deliberately conservative: reject these words even as arguments. This is a
// guardrail for trusted configuration, not a sandbox for arbitrary shell code.
const BLOCKED_COMMANDS: &[&str] = &[
    "rm",
    "rf",
    "rmdir",
    "unlink",
    "mv",
    "cp",
    "dd",
    "shred",
    "truncate",
    "touch",
    "mkdir",
    "install",
    "tee",
    "chmod",
    "chown",
    "chgrp",
    "chattr",
    "mkfs",
    "mke2fs",
    "mkswap",
    "fdisk",
    "sfdisk",
    "parted",
    "wipefs",
    "mount",
    "umount",
    "swapon",
    "swapoff",
    "reboot",
    "shutdown",
    "poweroff",
    "halt",
    "kill",
    "killall",
    "pkill",
    "sudo",
    "su",
    "doas",
    "eval",
    "exec",
    "sh",
    "bash",
    "zsh",
    "fish",
    "dash",
    "ksh",
    "cmd",
    "powershell",
    "pwsh",
    "python",
    "python2",
    "python3",
    "perl",
    "ruby",
    "node",
    "lua",
    "php",
    "awk",
    "gawk",
    "sed",
    "find",
    "xargs",
    "busybox",
    "source",
    ".",
    "curl",
    "wget",
    "ssh",
    "scp",
    "rsync",
    "tar",
    "unzip",
    "zip",
    "apt",
    "apt-get",
    "dnf",
    "yum",
    "pacman",
    "brew",
    "npm",
    "pip",
    "pip3",
    "del",
    "erase",
    "rd",
    "move",
    "copy",
    "ren",
    "rename",
    "format",
    "diskpart",
];

fn check_shell(command: &str) -> Result<()> {
    fn check_word(word: &str) -> Result<()> {
        let name = word.rsplit(['/', '\\']).next().unwrap_or(word).to_ascii_lowercase();
        let name = name.strip_suffix(".exe").unwrap_or(&name);
        if BLOCKED_COMMANDS.contains(&name) || name.starts_with("mkfs.") {
            return Err(format!("prompt command blocked by built-in blacklist: {name}").into());
        }
        Ok(())
    }
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut chars = command.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' && quote != Some('\'') {
            if let Some(next) = chars.next() {
                if next != '\n' {
                    word.push(next);
                }
            }
            continue;
        }
        if matches!(ch, '\'' | '"') {
            if quote == Some(ch) {
                quote = None;
            } else if quote.is_none() {
                quote = Some(ch);
            } else {
                word.push(ch);
            }
            continue;
        }
        // Scan substitutions even inside quotes, conservatively treating their
        // punctuation as separators. Never execute text while checking it.
        if ch == '>' || (ch == '<' && chars.peek() == Some(&'>')) {
            return Err("prompt command blocked: file-writing redirection".into());
        }
        if ch.is_whitespace() || matches!(ch, ';' | '|' | '&' | '(' | ')' | '`' | '$' | '=') {
            check_word(&word)?;
            if !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
        } else {
            word.push(ch);
        }
    }
    check_word(&word)?;
    if !word.is_empty() {
        words.push(word);
    }
    for (index, word) in words.iter().enumerate() {
        if word
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(word)
            .eq_ignore_ascii_case("git")
        {
            let lookup = index > 0 && words[index - 1] == "which"
                || index > 1 && words[index - 2] == "command" && words[index - 1] == "-v";
            let version = words.get(index + 1).is_some_and(|arg| arg == "--version");
            if !lookup && !version {
                return Err("prompt command blocked: only git lookup or --version is allowed".into());
            }
        }
    }
    Ok(())
}

async fn run_shell(command: &str, duration: Duration) -> Result<String> {
    crate::log_debug!("Running prompt template command: {command:?}");
    let result = run_shell_inner(command, duration).await;
    match &result {
        Ok(output) => crate::log_trace!("Prompt template command stdout: {output:?}"),
        Err(error) => crate::log_debug!("Prompt template command {command:?} failed: {error}"),
    }
    result
}

async fn run_shell_inner(command: &str, duration: Duration) -> Result<String> {
    check_shell(command)?;
    #[cfg(unix)]
    let mut process = {
        let mut process = Command::new("/bin/sh");
        process.arg("-c").arg(command).process_group(0);
        process
    };
    #[cfg(windows)]
    let mut process = {
        let mut process = Command::new("cmd");
        process.arg("/D").arg("/S").arg("/C").arg(command);
        process
    };
    let mut child = process
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("prompt command failed to start: {error}"))?;
    #[cfg(unix)]
    let _group = ProcessGroup(child.id().ok_or("prompt command has no process ID")?);
    let stdout = child.stdout.take().ok_or("missing command stdout")?;
    let stderr = child.stderr.take().ok_or("missing command stderr")?;
    let (stdout, stderr, status) = timeout(duration, async {
        tokio::try_join!(read_limited(stdout), read_limited(stderr), async {
            child.wait().await.map_err(Into::into)
        })
    })
    .await
    .map_err(|_| "prompt command timed out")??;
    if stdout.len() + stderr.len() > OUTPUT_LIMIT {
        return Err("prompt command output exceeds 64 KiB".into());
    }
    if !status.success() {
        crate::log_debug!(
            "Prompt template command stdout: {:?}; stderr: {:?}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        );
        return Err(format!(
            "prompt command failed ({status}): {}",
            String::from_utf8_lossy(&stderr).trim_end()
        )
        .into());
    }
    let stdout = String::from_utf8(stdout).map_err(|_| "prompt command returned invalid Unicode")?;
    Ok(stdout.trim_end_matches(['\r', '\n']).to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dangerous_commands_are_blocked() {
        for command in [
            "rm -rf /tmp/example",
            "rf -rf /tmp/example",
            "/bin/rm -rf /tmp/example",
            "r'm' -rf /tmp/example",
            r"r\m -rf /tmp/example",
            "cat $(rm -rf /tmp/example)",
            "printf hello > /tmp/example",
            "cat <> /tmp/example",
            "echo x | tee /tmp/example",
            "sudo cat /etc/os-release",
            "sh -c 'echo x'",
            "mkfs.ext4 /dev/example",
            "git reset --hard",
            "sed -i s/a/b/ file",
            "find . -delete",
        ] {
            assert!(check_shell(command).is_err(), "{command}");
        }
        for command in [
            "cat /etc/os-release",
            "cat $(which git)",
            "command -v git",
            "printf '%s' hello",
        ] {
            // git as an argument to which/command is explicitly allowed below.
            assert!(check_shell(command).is_ok(), "{command}");
        }
    }

    #[tokio::test]
    async fn templates_and_legacy_messages() {
        let prompt = PromptConfig {
            system: Some("Directory: {{ cwd }}".into()),
            user: Some("Question: {{ input }} / {{input}}".into()),
            ..Default::default()
        };
        let input = "{{ shell(\"exit 1\") }}";
        let messages = render(&prompt, input).await.unwrap();
        assert_eq!(messages[1]["content"], format!("Question: {input} / {input}"));
        assert_eq!(
            messages[0]["content"],
            format!("Directory: {}", std::env::current_dir().unwrap().display())
        );
        let legacy = PromptConfig {
            content: input.into(),
            ..Default::default()
        };
        assert_eq!(
            render(&legacy, "hello").await.unwrap(),
            json!([{"role":"system","content":input},{"role":"user","content":"hello"}])
        );
        for invalid in ["{{input", "{{ unknown }}", "{{ exec(\"true\") }}", "{{ shell(123) }}"] {
            assert!(render_template(invalid, "").await.is_err());
        }
        assert!(render_template("{{ env(\"\") }}", "").await.is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn shell_substitution_limits_and_single_pass() {
        assert_eq!(
            render_template(r#"{{ shell("printf '%s' \"$(printf nested)\"") }}"#, "")
                .await
                .unwrap(),
            "nested"
        );
        assert_eq!(
            render_template(r#"{{ shell("printf '{{ input }}\\n\\n'") }}"#, "ignored")
                .await
                .unwrap(),
            "{{ input }}"
        );
        assert_eq!(run_shell("printf 'a\\nb\\n'", COMMAND_TIMEOUT).await.unwrap(), "a\nb");
        assert_eq!(run_shell("true", COMMAND_TIMEOUT).await.unwrap(), "");
        assert!(
            run_shell("printf failed; exit 7", COMMAND_TIMEOUT)
                .await
                .unwrap_err()
                .to_string()
                .contains("failed")
        );
        assert!(
            run_shell("head -c 65537 /dev/zero", COMMAND_TIMEOUT)
                .await
                .unwrap_err()
                .to_string()
                .contains("64 KiB")
        );
        assert!(
            run_shell("sleep 10 & wait", Duration::from_millis(50))
                .await
                .unwrap_err()
                .to_string()
                .contains("timed out")
        );
        assert!(run_shell("printf '\\377'", COMMAND_TIMEOUT).await.is_err());
    }
}
