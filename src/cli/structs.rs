use clap::Parser;

#[derive(Parser)]
#[command(
    name = "aichat",
    version = "0.4.1",
    about = r#"
A terminal AI/LLM chat tool

aichat [MESSAGE]   # directly chat
aichat --list       # view configs"#,
    arg_required_else_help = true
)]
pub struct Cli {
    /// List configurations
    #[arg(long, short)]
    pub list: bool,

    /// Specify model configuration to use (searches all providers)
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
