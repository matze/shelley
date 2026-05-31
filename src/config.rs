use std::path::PathBuf;
use std::time::Duration;
use std::{env, fs, io};

use serde::Deserialize;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    #[default]
    #[value(name = "openai")]
    OpenAi,
    #[value(name = "deepseek")]
    DeepSeek,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::ValueEnum)]
pub enum Sandbox {
    #[value(name = "enabled")]
    Enabled,
    #[value(name = "disabled")]
    Disabled,
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
            max_rounds: 16,
            max_tokens: 64_000,
            tool_output_cap: 16 * 1024,
            timeout: Duration::from_secs(60),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub sandbox: Sandbox,
    pub budget: Budget,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FileConfig {
    pub provider: Option<Provider>,
    pub model: Option<String>,
    pub api_key: Option<String>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing API key: set {0} or add api_key to the config file")]
    MissingApiKey(&'static str),
    #[error("reading config {}", .path.display())]
    Read { path: PathBuf, source: io::Error },
    #[error("parsing config {}", .path.display())]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
}

fn config_path() -> Option<PathBuf> {
    let base = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("shelley").join("config.toml"))
}

fn load_file() -> Result<FileConfig, ConfigError> {
    let Some(path) = config_path() else {
        return Ok(FileConfig::default());
    };
    match fs::read_to_string(&path) {
        Ok(text) => toml::from_str(&text).map_err(|source| ConfigError::Parse { path, source }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(FileConfig::default()),
        Err(source) => Err(ConfigError::Read { path, source }),
    }
}

impl Config {
    pub fn new(
        provider: Provider,
        model: Option<String>,
        api_key: String,
        sandbox: Sandbox,
    ) -> Self {
        Self {
            base_url: provider.base_url().to_string(),
            model: model.unwrap_or_else(|| provider.default_model().to_string()),
            api_key,
            sandbox,
            budget: Budget::default(),
        }
    }

    pub fn resolve(
        provider: Option<Provider>,
        model: Option<String>,
        sandbox: Sandbox,
    ) -> Result<Self, ConfigError> {
        Self::merge(load_file()?, provider, model, sandbox)
    }

    fn merge(
        file: FileConfig,
        provider: Option<Provider>,
        model: Option<String>,
        sandbox: Sandbox,
    ) -> Result<Self, ConfigError> {
        let provider = provider.or(file.provider).unwrap_or_default();
        let api_key = env::var(provider.api_key_env())
            .ok()
            .or(file.api_key)
            .ok_or(ConfigError::MissingApiKey(provider.api_key_env()))?;
        Ok(Self::new(provider, model.or(file.model), api_key, sandbox))
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
        let config = Config::new(Provider::DeepSeek, None, "k".into(), Sandbox::Disabled);
        assert_eq!(config.model, "deepseek-v4-pro");
        assert_eq!(config.base_url, "https://api.deepseek.com/v1");
    }

    #[test]
    fn new_respects_model_override() {
        let config = Config::new(
            Provider::OpenAi,
            Some("gpt-x".into()),
            "k".into(),
            Sandbox::Disabled,
        );
        assert_eq!(config.model, "gpt-x");
    }

    #[test]
    fn file_config_parses_all_fields() {
        let file: FileConfig = toml::from_str(
            "provider = \"deepseek\"\nmodel = \"deepseek-v4-flash\"\napi_key = \"sk-file\"\n",
        )
        .unwrap();
        assert_eq!(
            file,
            FileConfig {
                provider: Some(Provider::DeepSeek),
                model: Some("deepseek-v4-flash".into()),
                api_key: Some("sk-file".into()),
            }
        );
    }

    #[test]
    fn file_config_is_empty_by_default() {
        assert_eq!(
            toml::from_str::<FileConfig>("").unwrap(),
            FileConfig::default()
        );
    }

    #[test]
    fn file_config_rejects_unknown_fields() {
        assert!(toml::from_str::<FileConfig>("temperature = 0.5\n").is_err());
    }

    #[test]
    fn merge_prefers_cli_over_file() {
        let file = FileConfig {
            provider: Some(Provider::OpenAi),
            model: Some("from-file".into()),
            api_key: Some("k".into()),
        };
        let config = Config::merge(
            file,
            Some(Provider::DeepSeek),
            Some("from-cli".into()),
            Sandbox::Disabled,
        )
        .unwrap();
        assert_eq!(config.base_url, "https://api.deepseek.com/v1");
        assert_eq!(config.model, "from-cli");
    }

    #[test]
    fn merge_falls_back_to_file_then_defaults() {
        let file = FileConfig {
            provider: Some(Provider::DeepSeek),
            model: None,
            api_key: Some("k".into()),
        };
        let config = Config::merge(file, None, None, Sandbox::Disabled).unwrap();
        assert_eq!(config.base_url, "https://api.deepseek.com/v1");
        assert_eq!(config.model, "deepseek-v4-pro");
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
