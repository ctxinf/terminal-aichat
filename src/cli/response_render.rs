use crate::config::DisplayConfig;
use crossterm::style::Stylize;
use std::io::{IsTerminal, Write};
use tokio::sync::mpsc::{self, Sender};

pub struct RenderConfig {
    pub pure: bool,
    pub model_name: String,
    pub prompt_config_name: String,
    pub display: DisplayConfig,
}
pub enum RenderEvent {
    Content(String),
    Reasoning(String),
}
pub struct ResponseRenderer;
impl ResponseRenderer {
    pub fn new() -> Self {
        Self
    }
    pub fn start_render(&self, config: RenderConfig) -> (Sender<RenderEvent>, tokio::task::JoinHandle<()>) {
        let (tx, mut rx) = mpsc::channel(100);
        let task = tokio::spawn(async move {
            let terminal = std::io::stdout().is_terminal();
            if !config.pure {
                let mut fields = Vec::new();
                if config.display.model {
                    let label = if terminal {
                        "model:".dark_cyan().to_string()
                    } else {
                        "model:".into()
                    };
                    fields.push(format!("{label} {}", config.model_name));
                }
                if config.display.prompt {
                    let label = if terminal {
                        "prompt:".dark_cyan().to_string()
                    } else {
                        "prompt:".into()
                    };
                    fields.push(format!("{label} {}", config.prompt_config_name));
                }
                if !fields.is_empty() {
                    println!("{}", fields.join("   "));
                }
            }
            let mut reasoning = false;
            let mut needs_newline = false;
            while let Some(event) = rx.recv().await {
                match event {
                    RenderEvent::Content(text) => {
                        if text.is_empty() {
                            continue;
                        }
                        if reasoning {
                            if needs_newline {
                                println!();
                            }
                            reasoning = false;
                        }
                        needs_newline = !text.ends_with('\n');
                        print!("{text}");
                    }
                    RenderEvent::Reasoning(text) if config.display.reasoning && !config.pure => {
                        if text.is_empty() {
                            continue;
                        }
                        reasoning = true;
                        needs_newline = !text.ends_with('\n');
                        if terminal {
                            print!("{}", text.dark_grey().italic());
                        } else {
                            print!("{text}");
                        }
                    }
                    _ => {}
                }
                let _ = std::io::stdout().flush();
            }
            // Complete terminal lines to avoid the shell's partial-line marker.
            // Redirected and piped answer bytes remain unchanged.
            if terminal && needs_newline {
                println!();
            }
            let _ = std::io::stdout().flush();
        });
        (tx, task)
    }
}
