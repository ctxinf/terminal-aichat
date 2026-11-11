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
            let selector = model.id.clone();
            let is_default_model = config
                .default_model
                .as_deref()
                .and_then(|id| crate::cli::cli::find_model_by_selector(config, id).ok())
                .is_some_and(|(key, id)| key == *provider_key && id == model.id);
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
