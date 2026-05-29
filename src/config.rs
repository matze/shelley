#![allow(dead_code)]

use std::time::Duration;

use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::ValueEnum)]
pub enum Provider {
    #[value(name = "openai")]
    OpenAi,
    #[value(name = "deepseek")]
    DeepSeek,
}

impl Provider {
    pub fn base_url(self) -> &'static str {
        match self {
            Provider::OpenAi => "https://api.openai.com/v1",
            Provider::DeepSeek => "https://api.deepseek.com/v1",
        }
    }

    pub fn default_model(self) -> &'static str {
        match self {
            Provider::OpenAi => "gpt-4o",
            Provider::DeepSeek => "deepseek-v4-pro",
        }
    }

    pub fn api_key_env(self) -> &'static str {
        match self {
            Provider::OpenAi => "OPENAI_API_KEY",
            Provider::DeepSeek => "DEEPSEEK_API_KEY",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Budget {
    pub max_rounds: u32,
    pub max_tokens: u32,
    pub tool_output_cap: usize,
    pub timeout: Duration,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            max_rounds: 6,
            max_tokens: 32_000,
            tool_output_cap: 16 * 1024,
            timeout: Duration::from_secs(60),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    pub provider: Provider,
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub budget: Budget,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing API key: set {0}")]
    MissingApiKey(&'static str),
}

impl Config {
    pub fn new(provider: Provider, model: Option<String>, api_key: String) -> Self {
        Self {
            base_url: provider.base_url().to_string(),
            model: model.unwrap_or_else(|| provider.default_model().to_string()),
            api_key,
            provider,
            budget: Budget::default(),
        }
    }

    pub fn from_env(provider: Provider, model: Option<String>) -> Result<Self, ConfigError> {
        std::env::var(provider.api_key_env())
            .map_err(|_| ConfigError::MissingApiKey(provider.api_key_env()))
            .map(|api_key| Self::new(provider, model, api_key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_endpoints_and_env() {
        assert_eq!(Provider::DeepSeek.base_url(), "https://api.deepseek.com/v1");
        assert_eq!(Provider::OpenAi.base_url(), "https://api.openai.com/v1");
        assert_eq!(Provider::OpenAi.api_key_env(), "OPENAI_API_KEY");
        assert_eq!(Provider::DeepSeek.api_key_env(), "DEEPSEEK_API_KEY");
    }

    #[test]
    fn new_uses_default_model_when_unset() {
        let config = Config::new(Provider::DeepSeek, None, "k".into());
        assert_eq!(config.model, "deepseek-v4-pro");
        assert_eq!(config.base_url, "https://api.deepseek.com/v1");
    }

    #[test]
    fn new_respects_model_override() {
        let config = Config::new(Provider::OpenAi, Some("gpt-x".into()), "k".into());
        assert_eq!(config.model, "gpt-x");
    }

    #[test]
    fn budget_has_expected_defaults() {
        let budget = Budget::default();
        assert_eq!(budget.max_rounds, 6);
        assert_eq!(budget.max_tokens, 32_000);
        assert_eq!(budget.tool_output_cap, 16 * 1024);
        assert_eq!(budget.timeout, Duration::from_secs(60));
    }
}
