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

    if let Some(provider) = &config.default_provider {
        println!("Default provider: {}", provider);
    }

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
        println!(
            "  API Key: {}",
            if provider.api_key.is_some() {
                "configured"
            } else {
                "none"
            }
        );

        // 打印 models
        println!("  Models:");
        for model in &provider.models {
            let selector = format!("{}/{}", provider_key, model.id);
            let is_default_model = config.default_model.as_deref() == Some(&selector)
                || (config.default_model.as_deref() == Some(&model.id)
                    && config.default_provider.as_deref().is_none_or(|p| p == provider_key));
            println!(
                "    - {} ({}){}",
                model.get_name(),
                selector,
                if is_default_model { " *" } else { "" }
            );
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
        println!(
            "System:\n```\n{}\n```",
            prompt.system.as_deref().unwrap_or(&prompt.content)
        );
        println!("User:\n```\n{}\n```", prompt.user.as_deref().unwrap_or("{{ input }}"));
    }
}
