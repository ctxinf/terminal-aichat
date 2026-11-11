use clap::{Parser, ValueEnum};

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum IntegrationShell {
    Bash,
    Zsh,
    Fish,
}

#[derive(Parser)]
#[command(
    name = "aichat",
    version,
    about = "A terminal AI/LLM chat tool. Use --skill for detailed agent documentation.
Environment variables TERMINAL_AICHT_* override command-line values.",
    after_help = "Configuration commands: aichat config --help",
    arg_required_else_help = false,
    disable_help_flag = false,
    disable_version_flag = false
)]
pub struct Cli {
    /// Specify model selector to use (model ID; optionally provider/model-id)
    #[arg(short, long)]
    pub model: Option<String>,

    /// Specify prompt configuration to use
    #[arg(short, long)]
    pub prompt: Option<String>,

    /// Show verbose information
    #[arg(long, num_args = 0..=1, default_missing_value = "true", require_equals = true)]
    pub verbose: Option<bool>,

    /// Use pure output mode (no extra text and color rendering)
    #[arg(long, num_args = 0..=1, default_missing_value = "true", require_equals = true)]
    pub pure: Option<bool>,

    /// Disable streaming output
    #[arg(long, num_args = 0..=1, default_missing_value = "true", require_equals = true)]
    pub disable_stream: Option<bool>,

    /// Specify config file path
    #[arg(long)]
    pub config: Option<String>,

    /// Print shell integration script for the given shell and exit.
    /// Combine with `--prompt <name>` to choose which prompt key the
    /// generated script will call (default: shell-exec-or-chat).
    #[arg(long, value_name = "SHELL", value_enum)]
    pub init_integration: Option<IntegrationShell>,

    /// Display usage,model,prompt,reasoning,duration (comma separated; empty clears)
    #[arg(long)]
    pub display: Option<String>,

    /// Print detailed usage documentation for agents
    #[arg(long)]
    pub skill: bool,

    /// Logging level: ERROR, WARN, INFO, DEBUG, TRACE
    #[arg(long)]
    pub log_level: Option<String>,

    /// Chat input content
    #[arg(trailing_var_arg = true)]
    pub input: Vec<String>,
}

impl Cli {
    pub fn apply_environment(&mut self) -> Result<(), String> {
        fn value(name: &str) -> Result<Option<String>, String> {
            match std::env::var(name) {
                Ok(value) => Ok(Some(value)),
                Err(std::env::VarError::NotPresent) => Ok(None),
                Err(_) => Err(format!("{name}: invalid Unicode")),
            }
        }
        for (name, target) in [
            ("MODEL", &mut self.model),
            ("PROMPT", &mut self.prompt),
            ("CONFIG", &mut self.config),
            ("DISPLAY", &mut self.display),
            ("LOG_LEVEL", &mut self.log_level),
        ] {
            let key = format!("TERMINAL_AICHT_{name}");
            if let Some(v) = value(&key)? {
                if v.is_empty() && name != "DISPLAY" {
                    return Err(format!("{key}: value must not be empty"));
                }
                *target = Some(v);
            }
        }
        for (name, target) in [
            ("PURE", &mut self.pure),
            ("VERBOSE", &mut self.verbose),
            ("DISABLE_STREAM", &mut self.disable_stream),
        ] {
            let key = format!("TERMINAL_AICHT_{name}");
            if let Some(v) = value(&key)? {
                *target = Some(match v.to_ascii_lowercase().as_str() {
                    "true" | "1" => true,
                    "false" | "0" => false,
                    _ => return Err(format!("{key}: expected true/false or 1/0")),
                });
            }
        }
        if let Some(v) = value("TERMINAL_AICHT_INIT_INTEGRATION")? {
            self.init_integration = Some(match v.to_ascii_lowercase().as_str() {
                "bash" => IntegrationShell::Bash,
                "zsh" => IntegrationShell::Zsh,
                "fish" => IntegrationShell::Fish,
                _ => return Err("TERMINAL_AICHT_INIT_INTEGRATION: expected bash, zsh or fish".into()),
            });
        }
        Ok(())
    }
}
