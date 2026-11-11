use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;

fn yes() -> bool {
    true
}
fn low() -> String {
    "low".into()
}
fn timeout() -> u64 {
    120_000
}
fn retries() -> u32 {
    2
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelConfig {
    pub id: String,
    pub name: Option<String>,
    #[serde(default = "yes")]
    pub reasoning: bool,
    pub thinking_level: Option<String>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub extra_body: Map<String, Value>,
    #[serde(default)]
    pub remove_body_fields: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub enum Api {
    #[default]
    #[serde(rename = "openai-completions")]
    OpenaiCompletions,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderConfig {
    pub name: Option<String>,
    pub base_url: String,
    #[serde(default)]
    pub api: Api,
    pub api_key: Option<String>,
    pub auth: Option<ProviderAuth>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub query_params: HashMap<String, String>,
    #[serde(default = "timeout")]
    pub timeout_ms: u64,
    #[serde(default = "retries")]
    pub max_retries: u32,
    #[serde(default)]
    pub extra_body: Map<String, Value>,
    pub models: Vec<ModelConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum ProviderAuth {
    None,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct AuthConfig {
    pub key: Option<String>,
    pub value: Option<String>,
    pub encryption: Option<String>,
}

// Credentials must never appear in diagnostics, even while encrypted.
impl std::fmt::Debug for AuthConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthConfig")
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct PromptConfig {
    /// Legacy system prompt, retained for existing configurations.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Config {
    pub providers: HashMap<String, ProviderConfig>,
    pub prompts: HashMap<String, PromptConfig>,
    pub auth: HashMap<String, AuthConfig>,
    pub default_provider: Option<String>,
    pub default_model: Option<String>,
    pub default_prompt: Option<String>,
    pub default_thinking_level: String,
    pub disable_stream: bool,
    pub pure: bool,
    pub verbose: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            providers: HashMap::new(),
            prompts: HashMap::new(),
            auth: HashMap::new(),
            default_provider: None,
            default_model: None,
            default_prompt: None,
            default_thinking_level: low(),
            disable_stream: false,
            pure: false,
            verbose: false,
        }
    }
}

impl Config {
    pub fn default_config_with_comments() -> String {
        include_str!("default.jsonc").replace(
            "INTEGRATION_PROMPT",
            &serde_json::to_string(crate::integrations::SHELL_INTEGRATION_PROMPT).unwrap(),
        )
    }

    pub fn validate(&self) -> Result<(), String> {
        for (name, prompt) in &self.prompts {
            if prompt.system.is_some() && !prompt.content.is_empty() {
                return Err(format!("prompts.{name}: content and system cannot be combined"));
            }
        }
        for (id, auth) in &self.auth {
            if auth.key.as_deref().is_some_and(str::is_empty) || (auth.key.is_none() && auth.value.is_none()) {
                return Err(format!("auth.{id}: provide a nonempty key or value"));
            }
            if auth.encryption.as_deref().is_some_and(str::is_empty)
                || (auth.encryption.is_some() && auth.value.as_deref().is_none_or(str::is_empty))
            {
                return Err(format!("auth.{id}: encryption requires nonempty encryption and value"));
            }
        }
        for (key, provider) in &self.providers {
            if provider.timeout_ms == 0 {
                return Err(format!("providers.{key}.timeoutMs must be positive"));
            }
            let url =
                reqwest::Url::parse(&provider.base_url).map_err(|_| format!("providers.{key}.baseUrl is invalid"))?;
            if !matches!(url.scheme(), "http" | "https") {
                return Err(format!("providers.{key}.baseUrl must use HTTP(S)"));
            }
            if provider.auth.is_some() && provider.api_key.is_some() {
                return Err(format!("providers.{key}: auth none and apiKey cannot be combined"));
            }
            let mut ids = std::collections::HashSet::new();
            for model in &provider.models {
                if model.id.is_empty() || !ids.insert(&model.id) {
                    return Err(format!("providers.{key}.models: empty or duplicate id"));
                }
            }
        }
        Ok(())
    }
}

impl ProviderConfig {
    pub fn get_name(&self, key: &str) -> String {
        self.name.as_deref().unwrap_or(key).to_owned()
    }
}
impl ModelConfig {
    pub fn get_name(&self) -> String {
        self.name.as_deref().unwrap_or(&self.id).to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generated_config_and_minimal_model_defaults() {
        let config: Config = serde_json_lenient::from_str(&Config::default_config_with_comments()).unwrap();
        config.validate().unwrap();
        let enabled = Config::default_config_with_comments()
            .replace("/*", "")
            .replace("*/", "");
        let example: Config = serde_json_lenient::from_str(&enabled).unwrap();
        example.validate().unwrap();
        assert_eq!(example.providers["my-gateway"].models[0].id, "deepseek-flash");
        let model: ModelConfig = serde_json::from_str(r#"{"id":"test"}"#).unwrap();
        assert!(model.reasoning);
        assert_eq!(model.get_name(), "test");
        assert!(model.thinking_level.is_none());
        let provider: ProviderConfig =
            serde_json::from_str(r#"{"baseUrl":"http://localhost/v1","models":[{"id":"test"}]}"#).unwrap();
        assert_eq!(provider.timeout_ms, 120000);
        assert_eq!(provider.max_retries, 2);
    }
    #[test]
    fn duplicate_models_and_invalid_auth_are_rejected() {
        let config: Config = serde_json::from_str(
            r#"{"providers":{"local":{"baseUrl":"http://localhost/v1","models":[{"id":"x"},{"id":"x"}]}}}"#,
        )
        .unwrap();
        assert!(config.validate().unwrap_err().contains("duplicate"));
        let config: Config = serde_json::from_str(r#"{"auth":{"account":{"value":"x","encryption":""}}}"#).unwrap();
        assert!(config.validate().unwrap_err().contains("auth.account"));
        assert!(serde_json::from_str::<AuthConfig>(r#"{"value":"x","algorithm":"custom"}"#).is_err());
    }
    #[test]
    fn unknown_auth_fields_report_the_full_path() {
        let source = r#"{"auth":{"account":{"value":"key","unexpected":true}}}"#;
        let mut deserializer = serde_json_lenient::Deserializer::from_str(source);
        let error = serde_path_to_error::deserialize::<_, Config>(&mut deserializer).unwrap_err();
        assert!(error.to_string().contains("auth.account.unexpected"));
    }
}
