use anyhow::Result;
use kode_core::config::Config;
use std::sync::Arc;
use crate::client::LlmClient;
use crate::openai::OpenAiClient;

/// Routes requests to the correct provider based on "provider/model" ref
pub struct ModelRouter {
    config: Config,
}

impl ModelRouter {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// Resolve a model ref to (client, bare_model_id)
    pub fn resolve(&self, model_ref: &str) -> Result<(Arc<dyn LlmClient>, String)> {
        let (provider_id, model_id) = Config::parse_model_ref(model_ref);

        let provider = self.config.providers.get(provider_id).ok_or_else(|| {
            anyhow::anyhow!(
                "unknown provider '{}' in model ref '{}'. Available: {}",
                provider_id,
                model_ref,
                self.config.providers.keys().cloned().collect::<Vec<_>>().join(", ")
            )
        })?;

        let client: Arc<dyn LlmClient> = Arc::new(OpenAiClient::new(
            provider.base_url.clone(),
            provider.api_key.clone(),
            provider_id.to_string(),
        ));

        Ok((client, model_id.to_string()))
    }

    /// Active model ref from config
    pub fn default_model(&self) -> Result<String> {
        self.config
            .model
            .clone()
            .ok_or_else(|| anyhow::anyhow!("no default model configured"))
    }

    /// List all statically configured provider/model combinations
    pub fn list_models(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (provider_id, provider) in &self.config.providers {
            if provider.models.is_empty() {
                // No static list — return provider prefix so UI can show it
                out.push(format!("{}/", provider_id));
            } else {
                for m in &provider.models {
                    out.push(format!("{}/{}", provider_id, m));
                }
            }
        }
        out.sort();
        out
    }

    /// Discover models dynamically via GET /models for all providers.
    /// Falls back to static list if a provider doesn't support it.
    pub async fn discover_models(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (provider_id, provider) in &self.config.providers {
            let client = OpenAiClient::new(
                provider.base_url.clone(),
                provider.api_key.clone(),
                provider_id.clone(),
            );
            match client.list_models().await {
                Ok(models) => {
                    for m in models {
                        out.push(format!("{}/{}", provider_id, m));
                    }
                }
                Err(_) => {
                    // Fall back to static list
                    for m in &provider.models {
                        out.push(format!("{}/{}", provider_id, m));
                    }
                    if provider.models.is_empty() {
                        out.push(format!("{}/", provider_id));
                    }
                }
            }
        }
        out.sort();
        out
    }
}
