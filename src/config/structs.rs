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
        Config {
            providers: HashMap::new(),
            prompts: HashMap::new(),
            default_model: None,
            default_prompt: None,
            disable_stream: false,
            pure: false,
            verbose: false,
        }
    }

    /// Get default config content with comments (JSONC format)
    pub fn default_config_with_comments() -> String {
        r#"{
  "providers": {
  // Example provider configuration (uncomment and modify to use)
   /*
    "openai": {
      "name": "OpenAI",
      "baseURL": "https://api.openai.com/v1",
      "apiKey": "sk-...",  // Replace with your API key
      "models": {
        "gpt-5.4-codex": {  // model key used in request body: { "model": "<this_key>" }
          "name": "my-fav-codex-5.4", // display name for you (optional)
          "temperature": 0.7
        },
        "gpt-5-mini": {},
      }
    }*/
  },
  "default-model": null, // key of model, with provider prefix optionally. e.g. "gpt-5.4-codex", "openai/gpt-5-mini"
  "prompts": {
    "sample_prompt": {
      "content": "You are a terminal assistant. You are giving help to user in the terminal. Give concise responses whenever possible. Because of terminal cannot render markdown, DO NOT contain any markdown syntax(`,```, #, ...) in your response, use plain text only.\n"
    },
    "concise": {
      "content": "Use plain text, give extremely concise output"
    }
  },
  "default-prompt": "sample_prompt",
  "disable-stream": false,
  "pure": false,
  "verbose": false
}
"#.to_string()
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
