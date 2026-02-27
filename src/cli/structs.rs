use clap::Parser;

#[derive(Parser)]
#[command(
    name = "aichat",
    version = "1.0.4",
    about = r#"
A terminal AI/LLM chat tool

aichat [MESSAGE]   # directly chat
aichat --list       # view configs

Edit the config file to setup providers and models."#,
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

    /// Chat input content
    #[arg(trailing_var_arg = true)]
    pub input: Vec<String>,
}
