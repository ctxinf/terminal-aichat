use crate::cli::structs::IntegrationShell;

pub mod init;

pub const ZSH_SCRIPT_TEMPLATE: &str = include_str!("integrations/zsh.zsh");
pub const BASH_SCRIPT_TEMPLATE: &str = include_str!("integrations/bash.bash");
pub const FISH_SCRIPT_TEMPLATE: &str = include_str!("integrations/fish.fish");

/// Default prompt key written into the user's config by `--init-integration`.
pub const DEFAULT_INTEGRATION_PROMPT_NAME: &str = "shell-exec-or-chat";

/// Built-in shell integration system prompt content.
///
/// This is exposed (not hidden) — `--init-integration` writes it into the
/// user's config file under a named prompt so the user can inspect and edit
/// it. The shell integration script then invokes
/// `aichat --prompt <name> --pure --disable-stream "<msg>"`.
pub const SHELL_INTEGRATION_PROMPT: &str = "\
You are a terminal assistant embedded in the user's shell. Decide between two reply modes:\n\
\n\
MODE A — RUNNABLE SHELL COMMAND (preferred when the user's request maps to one):\n\
Reply with EXACTLY one line of compact JSON and NOTHING else:\n\
    {\"command\": \"<the command>\"}\n\
Rules for the command string:\n\
- No surrounding prose, no markdown, no backticks, no code fences outside the JSON.\n\
- The command must be runnable as-is in the user's current shell.\n\
- It MAY be multi-line (use \\\\n inside the JSON string) and MAY include `#` comments explaining steps.\n\
- It MAY chain multiple statements with `;`, `&&`, `|`, or newlines when one logical action requires it.\n\
- Inside the JSON string, escape \\\" as \\\\\\\" and real newlines as \\\\n.\n\
- Prefer the simplest form that still does the job.\n\
\n\
MODE B — PLAIN-TEXT CHAT (explanations, comparisons, conceptual questions, multi-step procedures the user must follow by hand):\n\
- Reply in plain text. Simple markdown is OK because the user can still read it (bold, inline `code`, short bullet lists).\n\
- AVOID heavy markdown: no deep heading hierarchies, no large tables, no nested lists more than one level, no long fenced code blocks unless essential.\n\
- Be concise. Short sentences.\n\
\n\
TIE-BREAKER:\n\
- If a single runnable command answers the intent, choose MODE A even if a textual explanation is possible.\n\
- For \"how / why / what is / explain / difference between / compare\", choose MODE B.\n\
\n\
Never mix the two modes in one reply.\n";

fn template_for(shell: IntegrationShell) -> &'static str {
    match shell {
        IntegrationShell::Zsh => ZSH_SCRIPT_TEMPLATE,
        IntegrationShell::Bash => BASH_SCRIPT_TEMPLATE,
        IntegrationShell::Fish => FISH_SCRIPT_TEMPLATE,
    }
}

/// Render a shell integration script with the user-chosen prompt name
/// substituted in. The templates contain the placeholder `{{PROMPT_NAME}}`.
pub fn render_script(shell: IntegrationShell, prompt_name: &str) -> String {
    template_for(shell).replace("{{PROMPT_NAME}}", prompt_name)
}
