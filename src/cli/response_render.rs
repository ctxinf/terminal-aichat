use crate::config::DisplayConfig;
use crossterm::style::Stylize;
use std::io::{IsTerminal, Write};
use tokio::sync::mpsc::{self, Sender};

pub struct RenderConfig {
    pub pure: bool,
    pub model_name: String,
    pub provider_id: String,
    pub prompt_config_name: String,
    pub display: DisplayConfig,
}
impl RenderConfig {
    fn model_value(&self) -> String {
        if self.display.provider {
            format!("{}@{}", self.model_name, self.provider_id)
        } else {
            self.model_name.clone()
        }
    }
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
                        "model:".dark_green().bold().to_string()
                    } else {
                        "model:".into()
                    };
                    let value = config.model_value();
                    let name = if terminal { value.cyan().to_string() } else { value };
                    fields.push(format!("{label} {name}"));
                }
                if config.display.prompt {
                    let label = if terminal {
                        "prompt:".dark_green().bold().to_string()
                    } else {
                        "prompt:".into()
                    };
                    let name = if terminal {
                        config.prompt_config_name.blue().to_string()
                    } else {
                        config.prompt_config_name
                    };
                    fields.push(format!("{label} {name}"));
                }
                if !fields.is_empty() {
                    let marker = if terminal {
                        " > ".black().on_green().to_string()
                    } else {
                        ">".into()
                    };
                    println!("{marker}  {}\n", fields.join("    "));
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provider_suffix_is_opt_in() {
        let mut config = RenderConfig {
            pure: false,
            model_name: "DeepSeek(vendor/model)".into(),
            provider_id: "gateway".into(),
            prompt_config_name: "test".into(),
            display: DisplayConfig::default(),
        };
        assert_eq!(config.model_value(), "DeepSeek(vendor/model)");
        config.display = DisplayConfig::parse("model,provider").unwrap();
        assert!(config.display.model && config.display.provider);
        assert_eq!(config.model_value(), "DeepSeek(vendor/model)@gateway");
        let parsed: DisplayConfig = serde_json::from_str(r#"{"model":true,"provider":true}"#).unwrap();
        assert_eq!(parsed, config.display);
        assert!(!DisplayConfig::parse("").unwrap().provider);
    }
}
