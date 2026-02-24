use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModelConfig {
    pub name: Option<String>,
    pub temperature: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProviderConfig {
    pub name: Option<String>,
    #[serde(rename = "baseURL")]
    pub base_url: String,
    #[serde(rename = "apiKey")]
    pub api_key: Option<String>,
    pub models: HashMap<String, ModelConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PromptConfig {
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub providers: HashMap<String, ProviderConfig>,
    pub prompts: HashMap<String, PromptConfig>,
    #[serde(rename = "default-provider")]
    pub default_provider: Option<String>,
    #[serde(rename = "default-model")]
    pub default_model: Option<String>,
    #[serde(rename = "default-prompt")]
    pub default_prompt: Option<String>,
    #[serde(rename = "disable-stream")]
    pub disable_stream: bool,
    pub pure: bool,
    pub verbose: bool,
}

impl Config {
    pub fn default() -> Self {
        let mut providers = HashMap::new();
        let mut models = HashMap::new();

        models.insert(
            "gpt-5-mini".to_string(),
            ModelConfig {
                name: Some("gpt-5-mini".to_string()),
                temperature: None,
            },
        );

        providers.insert(
            "openai".to_string(),
            ProviderConfig {
                name: Some("OpenAI".to_string()),
                base_url: "https://api.openai.com/v1".to_string(),
                api_key: None,
                models,
            },
        );

        let mut prompts = HashMap::new();
        prompts.insert(
            "sample_prompt".to_string(),
            PromptConfig {
                content: r#"You are a terminal assistant.
You are giving help to user in the terminal.
Give concise responses whenever possible.
Because of terminal cannot render markdown, DO NOT contain any markdown syntax(`,```, #, ...) in your response, use plain text only.
"#
                .to_string(),
            },
        );

        Config {
            providers,
            prompts,
            default_provider: Some("openai".to_string()),
            default_model: Some("gpt-5-mini".to_string()),
            default_prompt: Some("sample_prompt".to_string()),
            disable_stream: false,
            pure: false,
            verbose: false,
        }
    }
}

impl ProviderConfig {
    pub fn get_name(&self, key: &str) -> String {
        self.name.as_ref().unwrap_or(&key.to_string()).clone()
    }
}

impl ModelConfig {
    pub fn get_name(&self, key: &str) -> String {
        self.name.as_ref().unwrap_or(&key.to_string()).clone()
    }
}
