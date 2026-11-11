use std::{
    fs,
    path::PathBuf,
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "aichat-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_aichat"));
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().starts_with("TERMINAL_AICHT_") {
                command.env_remove(name);
            }
        }
        command.env("XDG_CONFIG_HOME", &self.0).stdin(Stdio::null());
        command
    }
    fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().unwrap()
    }
    fn config(&self) -> PathBuf {
        self.0.join("test.jsonc")
    }
    fn init(&self) {
        let result = self.run(&["config", "init", "--config", self.config().to_str().unwrap()]);
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn config_init_is_explicit_and_never_overwrites() {
    let f = Fixture::new();
    f.init();
    let source = fs::read_to_string(f.config()).unwrap();
    let parsed: serde_json::Value = serde_json_lenient::from_str(&source).unwrap();
    assert_eq!(parsed["defaultProvider"], "openrouter");
    assert_eq!(parsed["defaultModel"], "deepseek-flash");
    assert_eq!(parsed["providers"]["openrouter"]["models"][0]["stream"], true);
    assert!(parsed.get("disableStream").is_none());
    assert!(source.find("\"prompts\"").unwrap() < source.find("\"display\"").unwrap());
    assert_eq!(
        parsed["defaultModel"],
        parsed["providers"]["openrouter"]["models"][0]["id"]
    );
    assert_eq!(parsed["display"], serde_json::json!({}));
    for field in ["usage", "model", "prompt", "reasoning", "duration"] {
        assert!(source.contains(&format!("// \"{field}\": false")));
    }
    fs::write(f.config(), "invalid config must be preserved").unwrap();
    let result = f.run(&["config", "init", "--config", f.config().to_str().unwrap()]);
    assert!(result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("WARN"));
    assert_eq!(
        fs::read_to_string(f.config()).unwrap(),
        "invalid config must be preserved"
    );
}
#[test]
fn config_modes_ignore_chat_environment_and_redact_credentials() {
    let f = Fixture::new();
    f.init();
    let mut value: serde_json::Value = serde_json_lenient::from_str(&fs::read_to_string(f.config()).unwrap()).unwrap();
    value["providers"]["openrouter"]["apiKey"] =
        serde_json::json!({"value":"secret-not-for-output","encryption":"passphrase-not-for-output"});
    fs::write(f.config(), serde_json::to_string(&value).unwrap()).unwrap();
    for args in [
        vec!["config", "show"],
        vec!["config", "provider", "list"],
        vec!["config", "model", "list"],
    ] {
        let result = f
            .command()
            .args(args)
            .env("TERMINAL_AICHT_CONFIG", f.config())
            .env("TERMINAL_AICHT_INIT_INTEGRATION", "invalid")
            .env("TERMINAL_AICHT_DISPLAY", "invalid")
            .output()
            .unwrap();
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(text.contains("openrouter"));
        assert!(!text.contains("secret-not-for-output"));
        assert!(!text.contains("passphrase-not-for-output"));
    }
}
#[test]
fn config_path_environment_wins_and_arguments_are_isolated() {
    let f = Fixture::new();
    let result = f
        .command()
        .args(["config", "init", "--config", "/nonexistent/ignored.jsonc"])
        .env("TERMINAL_AICHT_CONFIG", f.config())
        .output()
        .unwrap();
    assert!(result.status.success());
    assert!(f.config().exists());
    for args in [
        vec!["config", "show", "--model", "x"],
        vec!["--list-configs"],
        vec!["--init-config"],
        vec!["--provider", "openrouter", "hello"],
    ] {
        assert!(!f.run(&args).status.success());
    }
    let result = f
        .command()
        .args(["config", "login", "--provider", "openrouter"])
        .env("TERMINAL_AICHT_CONFIG", f.config())
        .env("TERMINAL_AICHT_PROVIDER", "unknown")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("Provider 'unknown' not found"));
}

#[test]
fn verbose_is_removed_from_cli_and_help() {
    let f = Fixture::new();
    assert!(!f.run(&["--verbose"]).status.success());
    let help = f.run(&["--help"]);
    let text = String::from_utf8_lossy(&help.stdout);
    assert!(!text.contains("--verbose"));
    assert!(text.contains("--log-level"));
}

#[cfg(unix)]
#[test]
fn debug_log_reports_prompt_command_failure() {
    let f = Fixture::new();
    let config = serde_json::json!({
        "defaultProvider": "test", "defaultModel": "test", "defaultPrompt": "broken",
        "providers": {"test": {
            "baseUrl": "http://127.0.0.1:1/v1",
            "apiKey": {"value": "test-key"},
            "models": [{"id": "test"}]
        }},
        "prompts": {"broken": {"user": "{{ shell(\"printf diagnostic-output; exit 7\") }}"}}
    });
    fs::write(f.config(), serde_json::to_string(&config).unwrap()).unwrap();
    for level in ["INFO", "DEBUG", "TRACE"] {
        let result = f.run(&["--config", f.config().to_str().unwrap(), "--log-level", level, "hello"]);
        assert!(!result.status.success());
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(text.contains("prompt command failed"), "{text}");
        assert!(result.stdout.is_empty(), "diagnostics must stay on stderr");
        if level == "INFO" {
            assert!(!text.contains("diagnostic-output"), "{text}");
        } else {
            assert!(text.contains("diagnostic-output"), "{text}");
            assert!(text.contains("Prompt template field user failed"), "{text}");
            assert!(text.contains("exit status: 7"), "{text}");
        }
    }
}
