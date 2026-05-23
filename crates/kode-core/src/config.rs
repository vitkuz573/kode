use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Top-level kode configuration, stored at ~/.config/kode/config.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Named provider definitions
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,

    /// Active model in "provider/model" format, e.g. "omniroute/kr/auto"
    pub model: Option<String>,

    /// Agent behaviour
    #[serde(default)]
    pub agent: AgentConfig,

    /// Context window management
    #[serde(default)]
    pub context: ContextConfig,

    /// Cost tracking
    #[serde(default)]
    pub cost: CostConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// OpenAI-compatible base URL
    pub base_url: String,
    /// API key (can reference env var with "$ENV_VAR" syntax)
    pub api_key: String,
    /// Human-readable name
    pub name: Option<String>,
    /// Per-provider model overrides
    #[serde(default)]
    pub models: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Max tool-call iterations per turn
    #[serde(default = "default_max_steps")]
    pub max_steps: u32,
    /// System prompt override
    pub system_prompt: Option<String>,
    /// Temperature
    #[serde(default = "default_temperature")]
    pub temperature: f32,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_steps: default_max_steps(),
            system_prompt: None,
            temperature: default_temperature(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContextConfig {
    /// Max tokens to keep in context window
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
    /// Strategy: "sliding" | "summarize" | "truncate"
    #[serde(default = "default_context_strategy")]
    pub strategy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CostConfig {
    /// Show cost after each response
    #[serde(default)]
    pub show: bool,
    /// Budget limit in USD (0 = unlimited)
    #[serde(default)]
    pub budget_usd: f64,
}

fn default_max_steps() -> u32 { 32 }
fn default_temperature() -> f32 { 0.1 }
fn default_max_tokens() -> usize { 128_000 }
fn default_context_strategy() -> String { "sliding".into() }

impl Config {
    /// Load config from ~/.config/kode/config.toml, creating defaults if absent
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if path.exists() {
            let raw = std::fs::read_to_string(&path)
                .with_context(|| format!("reading config {}", path.display()))?;
            let cfg: Config = toml::from_str(&raw)
                .with_context(|| format!("parsing config {}", path.display()))?;
            Ok(cfg.resolve_env())
        } else {
            Ok(Self::default_config())
        }
    }

    /// Save config to disk
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let raw = toml::to_string_pretty(self)?;
        std::fs::write(&path, raw)?;
        Ok(())
    }

    pub fn config_path() -> Result<PathBuf> {
        let base = dirs::config_dir().context("cannot determine config dir")?;
        Ok(base.join("kode").join("config.toml"))
    }

    /// Resolve "$ENV_VAR" references in api_key fields
    fn resolve_env(mut self) -> Self {
        for provider in self.providers.values_mut() {
            if provider.api_key.starts_with('$') {
                let var = &provider.api_key[1..];
                if let Ok(val) = std::env::var(var) {
                    provider.api_key = val;
                }
            }
        }
        self
    }

    /// Bootstrap config pre-populated from opencode settings
    pub fn default_config() -> Self {
        let mut providers = HashMap::new();
        providers.insert(
            "omniroute".into(),
            ProviderConfig {
                base_url: "http://127.0.0.1:20128/v1".into(),
                api_key: "sk-c34f1467d7d44f25-82f707-08b539d2".into(),
                name: Some("OmniRoute".into()),
                models: vec!["kr/auto".into()],
            },
        );
        Self {
            providers,
            model: Some("omniroute/kr/auto".into()),
            agent: AgentConfig::default(),
            context: ContextConfig::default(),
            cost: CostConfig { show: true, budget_usd: 0.0 },
        }
    }

    /// Parse "provider/model" string into (provider_id, model_id)
    pub fn parse_model_ref(model_ref: &str) -> (&str, &str) {
        if let Some(pos) = model_ref.find('/') {
            (&model_ref[..pos], &model_ref[pos + 1..])
        } else {
            ("", model_ref)
        }
    }

    /// Resolve provider config for a model ref
    pub fn provider_for<'a>(&self, model_ref: &'a str) -> Option<(&ProviderConfig, &'a str)> {
        let (provider_id, model_id) = Self::parse_model_ref(model_ref);
        self.providers.get(provider_id).map(|p| (p, model_id))
    }
}
