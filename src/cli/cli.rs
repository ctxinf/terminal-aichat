use std::io::{self, IsTerminal, Read};
use std::process::exit;

use crate::cli::interactive::interactive_input;
use crate::cli::structs::Cli;

use crate::config::{Config, ConfigManager, print_providers, print_prompts, print_config_location, ProviderConfig, ModelConfig};
use crate::utils::StringUtilsTrait;
use crate::utils::logger::set_log_level;
use crate::{chat, log_debug, utils};
use clap::{Parser, CommandFactory};
use crossterm::style::Stylize;
use utils::logger::{self};

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
    let (found_provider_key, found_model_key) = find_model_by_name(runtime_config, target_model)
        .ok_or_else(|| {
            eprintln!("❌ Model '{}' not found in config", target_model);
            exit(1);
        })?;
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

/// 在所有 providers 中查找 model，返回找到的 provider_key 和 model_key
fn find_model_by_name(config: &Config, name: &str) -> Option<(String, String)> {
    // 先尝试精确匹配 model key
    for (provider_key, provider) in &config.providers {
        if provider.models.contains_key(name) {
            return Some((provider_key.clone(), name.to_string()));
        }
    }
    // 再尝试匹配 model name
    for (provider_key, provider) in &config.providers {
        for (model_key, model) in &provider.models {
            if let Some(model_name) = &model.name {
                if model_name == name {
                    return Some((provider_key.clone(), model_key.clone()));
                }
            }
        }
    }
    None
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
