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
    pub stream: bool,
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
    pub api_key: Option<ApiKeyConfig>,
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

#[derive(Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ApiKeyConfig {
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encryption: Option<String>,
}

impl std::fmt::Debug for ApiKeyConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApiKeyConfig { [redacted] }")
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

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct DisplayConfig {
    pub usage: bool,
    pub model: bool,
    pub prompt: bool,
    pub reasoning: bool,
    pub duration: bool,
}
impl DisplayConfig {
    pub fn parse(value: &str) -> Result<Self, String> {
        let mut result = Self::default();
        if value.is_empty() {
            return Ok(result);
        }
        for item in value.split(',') {
            match item.trim() {
                "usage" => result.usage = true,
                "model" => result.model = true,
                "prompt" => result.prompt = true,
                "reasoning" => result.reasoning = true,
                "duration" => result.duration = true,
                _ => return Err("display: expected usage,model,prompt,reasoning,duration".into()),
            }
        }
        Ok(result)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Config {
    pub providers: HashMap<String, ProviderConfig>,
    pub prompts: HashMap<String, PromptConfig>,
    pub default_provider: Option<String>,
    pub default_model: Option<String>,
    pub default_prompt: Option<String>,
    pub default_thinking_level: String,
    pub pure: bool,
    pub display: DisplayConfig,
    pub log_level: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            providers: HashMap::new(),
            prompts: HashMap::new(),
            default_provider: None,
            default_model: None,
            default_prompt: None,
            default_thinking_level: low(),
            pure: false,
            display: DisplayConfig::default(),
            log_level: "INFO".into(),
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
        for (key, provider) in &self.providers {
            if provider.timeout_ms == 0 {
                return Err(format!("providers.{key}.timeoutMs must be positive"));
            }
            let url =
                reqwest::Url::parse(&provider.base_url).map_err(|_| format!("providers.{key}.baseUrl is invalid"))?;
            if !matches!(url.scheme(), "http" | "https") {
                return Err(format!("providers.{key}.baseUrl must use HTTP(S)"));
            }
            if let Some(api_key) = &provider.api_key {
                if api_key.value.is_empty() {
                    return Err(format!("providers.{key}.apiKey: value must be nonempty"));
                }
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
        assert_eq!(example.providers["openrouter"].models[0].id, "deepseek-flash");
        let model: ModelConfig = serde_json::from_str(r#"{"id":"test"}"#).unwrap();
        assert!(model.reasoning);
        assert!(model.stream);
        assert!(
            !serde_json::from_str::<ModelConfig>(r#"{"id":"test","stream":false}"#)
                .unwrap()
                .stream
        );
        assert!(serde_json::from_str::<Config>(r#"{"disableStream":false}"#).is_err());
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
        assert!(serde_json::from_str::<Config>(r#"{"auth":{}}"#).is_err());
        assert!(serde_json::from_str::<ApiKeyConfig>(r#"{"key":"OLD_KEY","value":"x"}"#).is_err());
    }
    #[test]
    fn unknown_credential_fields_report_the_full_path() {
        let source = r#"{"providers":{"local":{"baseUrl":"http://localhost/v1","models":[],"apiKey":{"value":"key","unexpected":true}}}}"#;
        let mut deserializer = serde_json_lenient::Deserializer::from_str(source);
        let error = serde_path_to_error::deserialize::<_, Config>(&mut deserializer).unwrap_err();
        assert!(error.to_string().contains("providers.local.apiKey.unexpected"));
    }
}
