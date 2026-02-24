use super::Config;
use crossterm::style::Stylize;
use std::io;
use std::path::Path;

/// 打印配置文件位置
pub fn print_config_location(config_path: &Path) {
    println!("config file location: {}", config_path.display().to_string().cyan());
}

/// 打印配置列表
pub fn print_providers(config: &Config) -> io::Result<()> {
    println!("Providers & Models:");

    // 显示默认配置
    if let Some(default_model) = &config.default_model {
        println!("Default model: {}", default_model);
    }

    // 遍历所有 providers
    for (provider_key, provider) in &config.providers {
        let provider_name = provider.get_name(provider_key);

        // 打印 provider 名称
        println!("\n{} ({})", provider_name, provider_key);

        // 打印 base URL
        println!("  Base URL: {}", provider.base_url);

        // 打印 API key 状态
        let api_key_status = if provider.api_key.is_some() { "set" } else { "not set" };
        println!("  API Key: {}", api_key_status);

        // 打印 models
        println!("  Models:");
        for (model_key, model) in &provider.models {
            let model_name = model.get_name(model_key);
            let is_default_model = config.default_model.as_ref() == Some(model_key)
                || config.default_model.as_ref() == Some(&model_name);

            let default_mark = if is_default_model { " *" } else { "" };
            print!("    - {}", model_name);
            if let Some(temp) = model.temperature {
                print!(" [temperature: {}]", temp);
            }
            println!("{}", default_mark);
        }
    }

    Ok(())
}

/// 打印提示列表
pub fn print_prompts(config: &Config) {
    println!("\nPrompts:");
    // 显示默认 prompt
    if let Some(default) = &config.default_prompt {
        println!("Default prompt: {}", default);
    }
    for (name, prompt) in &config.prompts {
        let default_text = if config.default_prompt.as_deref() == Some(name) {
            " (default)"
        } else {
            ""
        };

        println!("\n{}{}:", name, default_text);
        println!("```\n{}\n```", prompt.content);
    }
}
