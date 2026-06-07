use std::io::{self, IsTerminal, Read};
use std::process::exit;

use crate::cli::interactive::interactive_input;
use crate::cli::structs::Cli;

use crate::config::{Config, ConfigManager, print_providers, print_prompts, print_config_location, ProviderConfig, ModelConfig};
use crate::integrations;
use crate::utils::StringUtilsTrait;
use crate::utils::logger::set_log_level;
use crate::{chat, log_debug, utils};
use clap::{Parser, CommandFactory};
use crossterm::style::Stylize;
use utils::logger::{self};

#[derive(Debug, Clone, PartialEq, Eq)]
enum ModelLookupError {
    NotFound { selector: String },
    ProviderScopedNotFound {
        selector: String,
        provider_key: String,
        model_key: String,
    },
    AmbiguousKey { model_key: String, providers: Vec<String> },
    AmbiguousName { model_name: String, matches: Vec<String> },
}

pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let orig_args: Vec<String> = std::env::args().collect();

    // First, get config path set up (we need it for help display)
    let config_dir = ConfigManager::get_config_dir()?;

    // Check for --config flag early to get the right config path
    let custom_config_path = orig_args.windows(2)
        .find(|w| w[0] == "--config")
        .map(|w| w[1].clone());

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
        let input = interactive_input().await?;
        input
    } else {
        cli.input.join(" ")
    };

    if input.trim().is_empty() {
        println!("{}", "⚠ Input message is empty.".yellow());
        exit(1);
    }
    Ok(input)
}

async fn handle_list_command(file_config: &Config, config_path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    print_providers(file_config)?;
    print_prompts(file_config);
    println!();
    print_config_location(config_path);

    Ok(())
}

async fn handle_chat_command(runtime_config: &Config, cli: &Cli, input: String) -> Result<(), Box<dyn std::error::Error>> {
    // 找到要使用的 provider 和 model
    let (provider_key, provider, model_key, model): (&String, &ProviderConfig, &String, &ModelConfig);

    let target_model = cli.model.as_ref().or_else(|| runtime_config.default_model.as_ref()).ok_or_else(|| {
        let hint = format!("Edit config file or set default-model. Use {} to see config location.", " -l".dark_green());
        eprintln!("❌ No model specified, please:\n{}", hint);
        exit(78);
    })?;

    // 遍历所有 providers 查找匹配的 model
    let (found_provider_key, found_model_key) = match find_model_by_selector(runtime_config, target_model) {
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
                    model_key,
                } => {
                    eprintln!(
                        "❌ Model key '{}' not found under provider '{}'. (selector: '{}')\nTip: If your model key contains '/', use '<provider>/<model_key>' to disambiguate. Use {} to list models.",
                        model_key,
                        provider_key,
                        selector,
                        "aichat -l".dark_green()
                    );
                }
                ModelLookupError::AmbiguousKey { model_key, providers } => {
                    eprintln!(
                        "❌ Model key '{}' exists in multiple providers: {}.\nTip: Use '<provider>/{}' to select one. Use {} to list models.",
                        model_key,
                        providers.join(", "),
                        model_key,
                        "aichat -l".dark_green()
                    );
                }
                ModelLookupError::AmbiguousName { model_name, matches } => {
                    eprintln!(
                        "❌ Model name '{}' matches multiple models: {}.\nTip: Use '<provider>/<model_key>' (from the list) to select one. Use {} to list models.",
                        model_name,
                        matches.join(", "),
                        "aichat -l".dark_green()
                    );
                }
            }
            exit(1);
        }
    };
    provider_key = runtime_config.providers.keys().find(|k| *k == &found_provider_key).unwrap();
    provider = runtime_config.providers.get(provider_key).unwrap();
    model_key = provider.models.keys().find(|k| *k == &found_model_key).unwrap();
    model = provider.models.get(model_key).unwrap();

    let prompt_name = runtime_config.default_prompt.as_ref().ok_or_else(|| {
        let hint = format!("Edit config file or set default-prompt. Use {} to see config location.", " -l".dark_green());
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
        model_key,
        prompt_name,
        &input.safe_substring(20)
    );

    chat::completion(
        &input,
        provider_key,
        provider,
        model_key,
        model,
        prompt_name,
        prompt_config,
        runtime_config.pure,
        runtime_config.disable_stream,
        runtime_config.verbose,
    )
    .await?;

    log_debug!("Chat Done.");
    Ok(())
}

/// Find model by selector:
/// - `model_key` (global search across providers)
/// - `provider_key/model_key` (provider-scoped)
/// - fallback to `model.name` if no key matched
fn find_model_by_selector(config: &Config, selector: &str) -> Result<(String, String), ModelLookupError> {
    if let Some((provider_key, rest)) = selector.split_once('/') {
        if let Some(provider) = config.providers.get(provider_key) {
            if provider.models.contains_key(rest) {
                return Ok((provider_key.to_string(), rest.to_string()));
            }

            // Try treating the whole selector as a model key/name, because model keys (or names)
            // may contain '/' and the prefix might coincidentally match a provider key.
            match find_model_global(config, selector) {
                Ok(found) => return Ok(found),
                Err(ModelLookupError::NotFound { .. }) => {
                    return Err(ModelLookupError::ProviderScopedNotFound {
                        selector: selector.to_string(),
                        provider_key: provider_key.to_string(),
                        model_key: rest.to_string(),
                    });
                }
                Err(e) => return Err(e),
            }
        }
    }

    find_model_global(config, selector)
}

fn find_model_global(config: &Config, selector: &str) -> Result<(String, String), ModelLookupError> {
    let mut key_matches: Vec<String> = Vec::new();
    for (provider_key, provider) in &config.providers {
        if provider.models.contains_key(selector) {
            key_matches.push(provider_key.clone());
        }
    }
    match key_matches.len() {
        1 => return Ok((key_matches[0].clone(), selector.to_string())),
        n if n > 1 => {
            key_matches.sort();
            return Err(ModelLookupError::AmbiguousKey {
                model_key: selector.to_string(),
                providers: key_matches,
            });
        }
        _ => {}
    }

    let mut name_matches: Vec<(String, String)> = Vec::new();
    for (provider_key, provider) in &config.providers {
        for (model_key, model) in &provider.models {
            if model.name.as_deref() == Some(selector) {
                name_matches.push((provider_key.clone(), model_key.clone()));
            }
        }
    }
    match name_matches.len() {
        1 => Ok((name_matches[0].0.clone(), name_matches[0].1.clone())),
        n if n > 1 => {
            let mut display: Vec<String> = name_matches
                .into_iter()
                .map(|(p, m)| format!("{}/{}", p, m))
                .collect();
            display.sort();
            Err(ModelLookupError::AmbiguousName {
                model_name: selector.to_string(),
                matches: display,
            })
        }
        _ => Err(ModelLookupError::NotFound {
            selector: selector.to_string(),
        }),
    }
}

/// 合并 CLI 参数和文件配置
fn merge_config(file_config: &Config, cli: &Cli) -> Config {
    Config {
        providers: file_config.providers.clone(),
        prompts: file_config.prompts.clone(),

        // 默认值保持文件配置
        default_model: file_config.default_model.clone(),
        default_prompt: cli.prompt.clone().or_else(|| file_config.default_prompt.clone()),

        // 全局标志: CLI 或文件任一为 true 则为 true
        disable_stream: cli.disable_stream || file_config.disable_stream,
        pure: cli.pure || file_config.pure,
        verbose: cli.verbose || file_config.verbose,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(base_url: &str, models: Vec<(&str, Option<&str>)>) -> ProviderConfig {
        ProviderConfig {
            name: None,
            base_url: base_url.to_string(),
            api_key: None,
            models: models
                .into_iter()
                .map(|(k, name)| {
                    (
                        k.to_string(),
                        ModelConfig {
                            name: name.map(|s| s.to_string()),
                            temperature: None,
                        },
                    )
                })
                .collect(),
        }
    }

    fn config_with_providers(providers: Vec<(&str, ProviderConfig)>) -> Config {
        Config {
            providers: providers
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
            prompts: Default::default(),
            default_model: None,
            default_prompt: None,
            disable_stream: false,
            pure: false,
            verbose: false,
        }
    }

    #[test]
    fn selector_provider_scoped_match() {
        let cfg = config_with_providers(vec![(
            "openai",
            provider("https://api.openai.com/v1", vec![("gpt-5-mini", None)]),
        )]);
        let found = find_model_by_selector(&cfg, "openai/gpt-5-mini").unwrap();
        assert_eq!(found, ("openai".to_string(), "gpt-5-mini".to_string()));
    }

    #[test]
    fn selector_provider_prefix_fallback_to_global_key() {
        let cfg = config_with_providers(vec![
            ("openai", provider("https://api.openai.com/v1", vec![("gpt-4o", None)])),
            (
                "openrouter",
                provider(
                    "https://openrouter.ai/api/v1",
                    vec![("openai/gpt-5-mini", Some("or-gpt-5-mini"))],
                ),
            ),
        ]);

        let found = find_model_by_selector(&cfg, "openai/gpt-5-mini").unwrap();
        assert_eq!(
            found,
            ("openrouter".to_string(), "openai/gpt-5-mini".to_string())
        );
    }

    #[test]
    fn selector_ambiguous_key_requires_provider_prefix() {
        let cfg = config_with_providers(vec![
            ("a", provider("http://a", vec![("gpt-5-mini", None)])),
            ("b", provider("http://b", vec![("gpt-5-mini", None)])),
        ]);

        let err = find_model_by_selector(&cfg, "gpt-5-mini").unwrap_err();
        assert!(matches!(err, ModelLookupError::AmbiguousKey { .. }));
    }

    #[test]
    fn selector_fallback_to_model_name() {
        let cfg = config_with_providers(vec![(
            "local",
            provider("http://localhost:11434/v1", vec![("llama3.1", Some("llama"))]),
        )]);

        let found = find_model_by_selector(&cfg, "llama").unwrap();
        assert_eq!(found, ("local".to_string(), "llama3.1".to_string()));
    }

    #[test]
    fn selector_provider_scoped_not_found_error() {
        let cfg = config_with_providers(vec![(
            "openai",
            provider("https://api.openai.com/v1", vec![("gpt-5-mini", None)]),
        )]);

        let err = find_model_by_selector(&cfg, "openai/does-not-exist").unwrap_err();
        assert!(matches!(err, ModelLookupError::ProviderScopedNotFound { .. }));
    }
}
