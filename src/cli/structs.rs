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
    version = "1.1.1",
    about = r#"
A terminal AI/LLM chat tool

aichat [MESSAGE]                       # directly chat
aichat --list                          # view configs
aichat --init-integration <SHELL>      # print shell integration script

Edit the config file to setup providers and models.

Shell integration adds `?` and `?!` commands. Install with:
  zsh:  echo 'eval "$(aichat --init-integration zsh)"'  >> ~/.zshrc
  bash: echo 'eval "$(aichat --init-integration bash)"' >> ~/.bashrc
  fish: aichat --init-integration fish | source

The integration uses a `shell-exec-or-chat` prompt written to your config
file; you can read and edit it freely. To pick a different prompt name:
  aichat --init-integration zsh --prompt my-prompt"#,
    arg_required_else_help = false,
    disable_help_flag = false,
    disable_version_flag = false
)]
pub struct Cli {
    /// List configurations
    #[arg(long, short)]
    pub list: bool,

    /// Specify model selector to use (model key; optionally provider/model_key)
    #[arg(short, long)]
    pub model: Option<String>,

    /// Specify prompt configuration to use
    #[arg(short, long)]
    pub prompt: Option<String>,

    /// Show verbose information
    #[arg(long)]
    pub verbose: bool,

    /// Use pure output mode (no extra text and color rendering)
    #[arg(long)]
    pub pure: bool,

    /// Disable streaming output
    #[arg(long)]
    pub disable_stream: bool,

    /// Specify config file path
    #[arg(long)]
    pub config: Option<String>,

    /// Print shell integration script for the given shell and exit.
    /// Combine with `--prompt <name>` to choose which prompt key the
    /// generated script will call (default: shell-exec-or-chat).
    #[arg(long, value_name = "SHELL", value_enum)]
    pub init_integration: Option<IntegrationShell>,

    /// Chat input content
    #[arg(trailing_var_arg = true)]
    pub input: Vec<String>,
}
