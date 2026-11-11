#![cfg(unix)]
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "aichat-integration-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("config ' test.jsonc"), "{}").unwrap();
        let mock = path.join("aichat");
        fs::write(&mock, r#"#!/bin/sh
if [ "$MOCK_FAIL" = 1 ]; then echo request-failed >&2; exit 7; fi
if [ "$MOCK_CHECK" = 1 ]; then
  printf '%s\n' "$TERMINAL_AICHT_PROMPT" "$TERMINAL_AICHT_CONFIG" "$TERMINAL_AICHT_PURE" "$TERMINAL_AICHT_DISABLE_STREAM" "${TERMINAL_AICHT_INIT_INTEGRATION-unset}" "$@"
else
  printf '%s' "$MOCK_REPLY"
fi
"#).unwrap();
        fs::set_permissions(mock, fs::Permissions::from_mode(0o755)).unwrap();
        Self(path)
    }
    fn script(&self, shell: &str) -> String {
        let mut command = Command::new(env!("CARGO_BIN_EXE_aichat"));
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("TERMINAL_AICHT_") {
                command.env_remove(key);
            }
        }
        let output = command
            .args(["--init-integration", shell, "--prompt", "custom ' prompt", "--config"])
            .arg(self.0.join("config ' test.jsonc"))
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8(output.stdout).unwrap()
    }
    fn run(
        &self,
        shell: &str,
        script: &str,
        action: &str,
        reply: &str,
        fail: bool,
        check: bool,
    ) -> std::process::Output {
        let mut command = Command::new(shell);
        match shell {
            "bash" => {
                command.args(["--noprofile", "--norc"]);
            }
            "zsh" => {
                command.arg("-f");
            }
            _ => {
                command.arg("--no-config");
            }
        }
        command
            .args(["-c", &format!("{script}\n{action}\n")])
            .env(
                "PATH",
                format!("{}:{}", self.0.display(), std::env::var("PATH").unwrap_or_default()),
            )
            .env("MOCK_REPLY", reply)
            .env("MOCK_FAIL", if fail { "1" } else { "0" })
            .env("MOCK_CHECK", if check { "1" } else { "0" })
            .env("TERMINAL_AICHT_PROMPT", "wrong")
            .env("TERMINAL_AICHT_CONFIG", "wrong")
            .env("TERMINAL_AICHT_PURE", "false")
            .env("TERMINAL_AICHT_DISABLE_STREAM", "false")
            .env("TERMINAL_AICHT_INIT_INTEGRATION", "zsh")
            .output()
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn generated_scripts_preserve_protocol_and_handle_failures() {
    for shell in ["bash", "zsh", "fish"] {
        if Command::new(shell).arg("--version").output().is_err() {
            eprintln!("Skipping unavailable {shell}");
            continue;
        }
        let f = Fixture::new();
        let script = f.script(shell);
        let output = f.run(shell, &script, "_aichat_run '--option-as-text'", "", false, true);
        assert!(
            output.status.success(),
            "{shell}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains("custom ' prompt"), "{shell}: {text}");
        assert!(
            text.contains(f.0.join("config ' test.jsonc").to_str().unwrap()),
            "{shell}: {text}"
        );
        assert!(
            text.contains("true\ntrue\nunset\n--integration-shell\n"),
            "{shell}: {text}"
        );
        assert!(
            text.contains(&format!("{shell}\n--\n--option-as-text")),
            "{shell}: {text}"
        );
        for reply in [
            "plain chat\nsecond line",
            r#"{"command":123}"#,
            r#"{"command":""}"#,
            r#"{"command":"   "}"#,
            r#"{"command":"printf bad\u0000"}"#,
            r#"{"command":"printf bad"} trailing garbage"#,
            r#"{"command":"printf bad"} {"command":"printf bad"}"#,
        ] {
            let output = f.run(shell, &script, "_aichat_ask_exec hello", reply, false, false);
            assert!(
                output.status.success(),
                "{shell}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                String::from_utf8(output.stdout).unwrap(),
                format!("{reply}\n"),
                "{shell}"
            );
        }
        let output = f.run(
            shell,
            &script,
            "_aichat_ask_exec hello",
            r#"{"command":"printf '%s' 'command-ok'"}"#,
            false,
            false,
        );
        assert!(
            output.status.success(),
            "{shell}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).ends_with("command-ok"),
            "{shell}: {:?}",
            output
        );
        let output = f.run(
            shell,
            &script,
            "_aichat_ask hello",
            r#"{"command":"printf command-ok"}"#,
            false,
            false,
        );
        assert!(output.status.success());
        assert_eq!(String::from_utf8(output.stdout).unwrap(), "printf command-ok\n");
        for action in ["_aichat_ask_exec hello", "_aichat_ask hello"] {
            let output = f.run(shell, &script, action, "", true, false);
            assert_eq!(output.status.code(), Some(7), "{shell}: {:?}", output);
            assert!(output.stdout.is_empty());
        }
    }
}

#[test]
fn integration_uses_real_cli_without_mixing_debug_logs_into_reply() {
    use std::io::{Read, Write};
    let f = Fixture::new();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let config = serde_json::json!({
        "defaultProvider":"test", "defaultModel":"test", "logLevel":"DEBUG",
        "display":{"model":true,"prompt":true,"reasoning":true,"usage":true,"duration":true},
        "providers":{"test":{"baseUrl":format!("http://{address}/v1"),"maxRetries":0,"models":[{"id":"test","stream":true}]}},
        "prompts":{}
    });
    fs::write(f.0.join("config ' test.jsonc"), serde_json::to_string(&config).unwrap()).unwrap();
    let script = f.script("bash");
    fs::remove_file(f.0.join("aichat")).unwrap();
    std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_aichat"), f.0.join("aichat")).unwrap();
    let server = std::thread::spawn(move || {
        for success in [true, false] {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 4096];
            let (start, length) = loop {
                let n = socket.read(&mut buffer).unwrap();
                assert!(n > 0);
                request.extend_from_slice(&buffer[..n]);
                if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
                    let length: usize = headers
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length:").map(str::trim))
                        .unwrap()
                        .parse()
                        .unwrap();
                    if request.len() >= end + 4 + length {
                        break (end + 4, length);
                    }
                }
            };
            let body: serde_json::Value = serde_json::from_slice(&request[start..start + length]).unwrap();
            assert_eq!(body["stream"], false);
            assert!(
                body["messages"][0]["content"]
                    .as_str()
                    .unwrap()
                    .contains("Current shell: Bash.")
            );
            assert_eq!(body["messages"][1]["content"], "hello");
            let payload = if success {
                r#"{"choices":[{"message":{"content":"plain reply","reasoning_content":"hidden reasoning"}}]}"#
            } else {
                "{}"
            };
            let status = if success { "200 OK" } else { "500 Internal Server Error" };
            write!(socket, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}", payload.len()).unwrap();
        }
    });
    let output = f.run("bash", &script, "_aichat_ask hello", "", false, false);
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "plain reply\n");
    assert!(String::from_utf8_lossy(&output.stderr).contains("[DEBUG]"));
    let output = f.run("bash", &script, "_aichat_ask_exec hello", "", false, false);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("HTTP 500"));
    server.join().unwrap();
}

#[test]
fn python_fallback_decodes_json_without_sed() {
    if !std::path::Path::new("/usr/bin/python3").is_file() {
        return;
    }
    let f = Fixture::new();
    let path = f.0.join("python-only");
    fs::create_dir(&path).unwrap();
    std::os::unix::fs::symlink("/usr/bin/python3", path.join("python3")).unwrap();
    let script = f.script("bash");
    for (reply, expected) in [
        (
            r#"{"command":"printf \"hello\"\nprintf next"}"#,
            Some("printf \"hello\"\nprintf next"),
        ),
        (r#"{"command":false}"#, None),
        (r#"{"command":"  "}"#, None),
        (r#"{"command":"printf bad\u0000"}"#, None),
        (r#"{"command":"printf bad"} trailing"#, None),
    ] {
        let output = Command::new("/usr/bin/bash")
            .args([
                "--noprofile",
                "--norc",
                "-c",
                &format!("{script}\nprintf '%s' \"$MOCK_REPLY\" | _aichat_extract_cmd"),
            ])
            .env("PATH", &path)
            .env("MOCK_REPLY", reply)
            .output()
            .unwrap();
        assert_eq!(output.status.success(), expected.is_some());
        assert_eq!(String::from_utf8(output.stdout).unwrap(), expected.unwrap_or(""));
    }
    fs::remove_file(path.join("python3")).unwrap();
    let output = Command::new("/usr/bin/bash")
        .args([
            "--noprofile",
            "--norc",
            "-c",
            &format!("{script}\nprintf '%s' '{{\"command\":\"printf bad\"}}' | _aichat_extract_cmd"),
        ])
        .env("PATH", &path)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("install jq or python3"));
}
