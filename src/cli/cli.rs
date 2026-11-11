use std::io::{self, IsTerminal, Read};
use std::process::exit;

use crate::cli::interactive::interactive_input;
use crate::cli::structs::Cli;

use crate::config::{
    Config, ConfigManager, ModelConfig, ProviderConfig, print_config_location, print_prompts, print_providers,
};
use crate::integrations;
use crate::utils::StringUtilsTrait;
use crate::utils::logger::set_log_level;
use crate::{chat, log_debug, utils};
use clap::{CommandFactory, Parser};
use crossterm::style::Stylize;
use utils::logger::{self};

#[derive(Debug, Clone, PartialEq, Eq)]
enum ModelLookupError {
    NotFound {
        selector: String,
    },
    ProviderScopedNotFound {
        selector: String,
        provider_key: String,
        model_id: String,
    },
    AmbiguousKey {
        model_id: String,
        providers: Vec<String>,
    },
    AmbiguousName {
        model_name: String,
        matches: Vec<String>,
    },
}

pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let orig_args: Vec<String> = std::env::args().collect();

    // First, get config path set up (we need it for help display)
    let config_dir = ConfigManager::get_config_dir()?;

    // Check for --config flag early to get the right config path
    let custom_config_path = orig_args.windows(2).find(|w| w[0] == "--config").map(|w| w[1].clone());

    let config_manager = ConfigManager::new(&config_dir, custom_config_path.as_deref())?;
    let config_path = config_manager.get_config_path().to_path_buf();

    // Fast path: `--init-integration <SHELL>` — write the prompt to the
    // user's config (transparent, editable) and emit the shell script.
    if let Some(shell) = integrations::init::parse_init_integration_arg(&orig_args) {
        let prompt_name = integrations::init::parse_prompt_arg(&orig_args)
            .unwrap_or_else(|| integrations::DEFAULT_INTEGRATION_PROMPT_NAME.to_string());
        integrations::init::ensure_integration_prompt(&config_manager, &prompt_name)?;
        print!("{}", integrations::render_script(shell, &prompt_name));
        return Ok(());
    }

    // Check for no-args case first (orig_args includes program name)
    if orig_args.len() == 1 {
        // Print help manually
        Cli::command().print_help()?;
        println!();
        print_config_location(&config_path);
        std::process::exit(0);
    }

    // Now handle pipe input
    let mut custom_args = orig_args.clone();
    let has_pipe_input = !io::stdin().is_terminal();
    if has_pipe_input {
        //if has pipe stdin
        let mut input = String::new();
        io::stdin().read_to_string(&mut input).unwrap_or_default();
        custom_args.push(input.trim().to_string());
    }

    // Use try_parse_from so we can handle help ourselves
    let cli = match Cli::try_parse_from(&custom_args) {
        Ok(cli) => cli,
        Err(e) => {
            // Print the clap error/help
            e.print()?;
            // Then print our config location
            println!();
            print_config_location(&config_path);
            std::process::exit(e.exit_code());
        }
    };

    let file_config = config_manager.load()?;

    // 如果配置文件不存在且使用默认路径，初始化默认配置（带注释）
    if !config_manager.exists() && config_manager.is_default_path() {
        config_manager.save_default_with_comments()?;
    }

    // 合并 CLI 和文件配置
    let runtime_config = merge_config(&file_config, &cli);

    if runtime_config.verbose {
        set_log_level(logger::LogLevel::Trace);
    }

    // Handle commands
    if cli.list {
        handle_list_command(&file_config, &config_path).await?;
    } else {
        log_debug!("Handling chat command");
        let input = get_chat_input(&cli).await?;
        handle_chat_command(&runtime_config, &cli, input).await?;
    }

    Ok(())
}

async fn get_chat_input(cli: &Cli) -> Result<String, Box<dyn std::error::Error>> {
    // If input is empty,(interactive mode) wait for input, then call single_message
    let input = if cli.input.is_empty() {
        interactive_input().await?
    } else {
        cli.input.join(" ")
    };

    if input.trim().is_empty() {
        println!("{}", "⚠ Input message is empty.".yellow());
        exit(1);
    }
    Ok(input)
}

async fn handle_list_command(
    file_config: &Config,
    config_path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    print_providers(file_config)?;
    print_prompts(file_config);
    println!();
    print_config_location(config_path);

    Ok(())
}

async fn handle_chat_command(
    runtime_config: &Config,
    cli: &Cli,
    input: String,
) -> Result<(), Box<dyn std::error::Error>> {
    // 找到要使用的 provider 和 model
    let (provider_key, provider, model): (&String, &ProviderConfig, &ModelConfig);

    let target_model = cli
        .model
        .as_ref()
        .or(runtime_config.default_model.as_ref())
        .ok_or_else(|| {
            let hint = format!(
                "Edit config file or set defaultModel. Use {} to see config location.",
                " -l".dark_green()
            );
            eprintln!("❌ No model specified, please:\n{}", hint);
            exit(78);
        })?;

    // 遍历所有 providers 查找匹配的 model
    let (found_provider_key, found_model_id) = match find_model_by_selector(runtime_config, target_model) {
        Ok(found) => found,
        Err(err) => {
            match err {
                ModelLookupError::NotFound { selector } => {
                    eprintln!(
                        "❌ Model '{}' not found in config. Use {} to list available models.",
                        selector,
                        "aichat -l".dark_green()
                    );
                }
                ModelLookupError::ProviderScopedNotFound {
                    selector,
                    provider_key,
                    model_id,
                } => {
                    eprintln!(
                        "❌ Model ID '{}' not found under provider '{}'. (selector: '{}')\nTip: If your model ID contains '/', use '<provider>/<model_id>' to disambiguate. Use {} to list models.",
                        model_id,
                        provider_key,
                        selector,
                        "aichat -l".dark_green()
                    );
                }
                ModelLookupError::AmbiguousKey { model_id, providers } => {
                    eprintln!(
                        "❌ Model ID '{}' exists in multiple providers: {}.\nTip: Use '<provider>/{}' to select one. Use {} to list models.",
                        model_id,
                        providers.join(", "),
                        model_id,
                        "aichat -l".dark_green()
                    );
                }
                ModelLookupError::AmbiguousName { model_name, matches } => {
                    eprintln!(
                        "❌ Model name '{}' matches multiple models: {}.\nTip: Use '<provider>/<model_id>' (from the list) to select one. Use {} to list models.",
                        model_name,
                        matches.join(", "),
                        "aichat -l".dark_green()
                    );
                }
            }
            exit(1);
        }
    };
    provider_key = runtime_config
        .providers
        .keys()
        .find(|k| *k == &found_provider_key)
        .unwrap();
    provider = runtime_config.providers.get(provider_key).unwrap();
    model = provider.models.iter().find(|m| m.id == found_model_id).unwrap();

    let prompt_name = runtime_config.default_prompt.as_ref().ok_or_else(|| {
        let hint = format!(
            "Edit config file or set defaultPrompt. Use {} to see config location.",
            " -l".dark_green()
        );
        eprintln!("❌ No prompt config specified, please:\n{}", hint);
        exit(78);
    })?;

    let prompt_config = runtime_config.prompts.get(prompt_name).ok_or_else(|| {
        eprintln!("❌ Prompt configuration '{}' not found", prompt_name);
        std::process::exit(78);
    })?;

    log_debug!(
        "Begin to chat. provider: {}, model: {}, prompt: {}, input: {}...",
        provider_key,
        model.id,
        prompt_name,
        &input.safe_substring(20)
    );

    chat::completion(&input, provider, model, prompt_name, prompt_config, runtime_config).await?;

    log_debug!("Chat Done.");
    Ok(())
}

/// Explicit provider/id wins; otherwise defaultProvider scopes bare IDs/names.
fn find_model_by_selector(config: &Config, selector: &str) -> Result<(String, String), ModelLookupError> {
    if let Some((provider_key, id)) = selector.split_once('/')
        && let Some(provider) = config.providers.get(provider_key)
    {
        return provider
            .models
            .iter()
            .find(|m| m.id == id)
            .map(|m| (provider_key.into(), m.id.clone()))
            .ok_or_else(|| ModelLookupError::ProviderScopedNotFound {
                selector: selector.into(),
                provider_key: provider_key.into(),
                model_id: id.into(),
            });
    }
    let providers: Vec<_> = config
        .providers
        .iter()
        .filter(|(key, _)| config.default_provider.as_ref().is_none_or(|p| p == *key))
        .collect();
    for by_name in [false, true] {
        let mut matches: Vec<_> = providers
            .iter()
            .flat_map(|(key, provider)| {
                provider
                    .models
                    .iter()
                    .filter(move |m| {
                        if by_name {
                            m.name.as_deref() == Some(selector)
                        } else {
                            m.id == selector
                        }
                    })
                    .map(move |m| ((*key).clone(), m.id.clone()))
            })
            .collect();
        matches.sort();
        match matches.len() {
            0 => {}
            1 => return Ok(matches.remove(0)),
            _ if by_name => {
                return Err(ModelLookupError::AmbiguousName {
                    model_name: selector.into(),
                    matches: matches.into_iter().map(|(p, m)| format!("{p}/{m}")).collect(),
                });
            }
            _ => {
                return Err(ModelLookupError::AmbiguousKey {
                    model_id: selector.into(),
                    providers: matches.into_iter().map(|(p, _)| p).collect(),
                });
            }
        }
    }
    Err(ModelLookupError::NotFound {
        selector: selector.into(),
    })
}

/// 合并 CLI 参数和文件配置
fn merge_config(file_config: &Config, cli: &Cli) -> Config {
    let mut config = file_config.clone();
    config.default_prompt = cli.prompt.clone().or(config.default_prompt);
    config.disable_stream |= cli.disable_stream;
    config.pure |= cli.pure;
    config.verbose |= cli.verbose;
    config
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> Config {
        serde_json::from_value(serde_json::json!({
            "providers": {
                "a": { "baseUrl": "http://localhost/v1", "models": [{"id":"shared"}, {"id":"vendor/model", "name":"friendly"}] },
                "b": { "baseUrl": "http://localhost/v1", "models": [{"id":"shared"}] }
            }
        })).unwrap()
    }
    #[test]
    fn selectors_and_default_provider() {
        let mut cfg = config();
        assert!(matches!(
            find_model_by_selector(&cfg, "shared"),
            Err(ModelLookupError::AmbiguousKey { .. })
        ));
        assert_eq!(
            find_model_by_selector(&cfg, "a/vendor/model").unwrap(),
            ("a".into(), "vendor/model".into())
        );
        assert_eq!(
            find_model_by_selector(&cfg, "friendly").unwrap(),
            ("a".into(), "vendor/model".into())
        );
        cfg.default_provider = Some("b".into());
        assert_eq!(
            find_model_by_selector(&cfg, "shared").unwrap(),
            ("b".into(), "shared".into())
        );
        assert_eq!(
            find_model_by_selector(&cfg, "a/shared").unwrap(),
            ("a".into(), "shared".into())
        );
        assert!(find_model_by_selector(&cfg, "friendly").is_err());
        assert!(find_model_by_selector(&cfg, "a/missing").is_err());
    }
}
