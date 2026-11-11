//! Helpers for `aichat --init-integration <SHELL> [--prompt <NAME>]`:
//! Config-file patching.

use super::SHELL_INTEGRATION_PROMPT;
use crate::config::{ConfigManager, PromptConfig};

/// Make sure the named integration prompt exists in the user's config file.
///
/// - If the config file is missing, write the default config (which already
///   contains `prompts.shell-exec-or-chat`).
/// - If the config file exists but does not define the named prompt, append
///   a JSONC block that adds it. This preserves all existing comments and
///   formatting in the file.
/// - If the named prompt already exists, do nothing.
pub fn ensure_integration_prompt(
    config_manager: &ConfigManager,
    prompt_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if !config_manager.exists() {
        if config_manager.is_default_path() {
            config_manager.init_default()?;
            eprintln!(
                "📝 Created default config at {}",
                config_manager.get_config_path().display()
            );
        } else {
            eprintln!(
                "⚠️  Config file {} does not exist; the shell integration will fail until you create it and define a `{}` prompt.",
                config_manager.get_config_path().display(),
                prompt_name
            );
            return Ok(());
        }
    }

    let config = config_manager.load()?;
    if config.prompts.contains_key(prompt_name) {
        return Ok(());
    }

    let path = config_manager.get_config_path();
    let original = std::fs::read_to_string(path)?;
    let patched = insert_prompt_into_jsonc(&original, prompt_name)?;
    std::fs::write(path, patched)?;
    eprintln!(
        "📝 Added prompt `{}` to {} — edit it any time.",
        prompt_name,
        path.display()
    );
    Ok(())
}

fn insert_prompt_into_jsonc(source: &str, prompt_name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let output = crate::config::edit::insert_prompt(
        source,
        prompt_name,
        &PromptConfig {
            system: Some(SHELL_INTEGRATION_PROMPT.to_string()),
            ..Default::default()
        },
    )?;
    let config: crate::config::Config = serde_json_lenient::from_str(&output)?;
    config.validate()?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_comments_and_quotes_prompt_names() {
        for source in [
            r#"{ // \"prompts\" in comment
                "prompts": {"existing": {"system":"literal } and {"} // trailing comment
                }, "display": {} }"#,
            r#"{ "display": {} /* no prompts yet */ }"#,
            r#"{ "prompts": { /* empty */ } }"#,
        ] {
            let name = "custom'\" prompt";
            let result = insert_prompt_into_jsonc(source, name).unwrap();
            assert!(result.contains("comment") || result.contains("/*"));
            let config: crate::config::Config = serde_json_lenient::from_str(&result).unwrap();
            assert_eq!(config.prompts[name].system.as_deref(), Some(SHELL_INTEGRATION_PROMPT));
            assert_eq!(insert_prompt_into_jsonc(&result, name).unwrap(), result);
        }
    }
}
