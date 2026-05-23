use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use bytes::Bytes;
use futures::StreamExt;
use kode_core::types::{Message, Role, ToolCall};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::mpsc::Sender;
use tracing::{debug, warn};

use crate::client::{
    CompletionRequest, CompletionResponse, LlmClient, StreamChunk, ToolDefinition,
};

pub struct OpenAiClient {
    http: Client,
    base_url: String,
    api_key: String,
    provider: String,
}

impl OpenAiClient {
    pub fn new(base_url: String, api_key: String, provider: String) -> Self {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .expect("failed to build HTTP client");
        Self { http, base_url, api_key, provider }
    }

    fn build_body(&self, req: &CompletionRequest, stream: bool) -> Value {
        let messages: Vec<Value> = req.messages.iter().map(msg_to_json).collect();

        let mut body = json!({
            "model": req.model,
            "messages": messages,
            "temperature": req.temperature,
            "stream": stream,
        });

        if let Some(max) = req.max_tokens {
            body["max_tokens"] = json!(max);
        }

        if !req.tools.is_empty() {
            let tools: Vec<Value> = req.tools.iter().map(tool_to_json).collect();
            body["tools"] = json!(tools);
            body["tool_choice"] = json!("auto");
        }

        if stream {
            body["stream_options"] = json!({ "include_usage": true });
        }

        body
    }

    fn endpoint(&self) -> String {
        let base = self.base_url.trim_end_matches('/');
        format!("{}/chat/completions", base)
    }

    fn models_endpoint(&self) -> String {
        let base = self.base_url.trim_end_matches('/');
        format!("{}/models", base)
    }

    /// Fetch available models from the provider's /models endpoint
    pub async fn list_models(&self) -> Result<Vec<String>> {
        debug!("GET {}", self.models_endpoint());
        let resp = self
            .http
            .get(&self.models_endpoint())
            .bearer_auth(&self.api_key)
            .send()
            .await
            .context("GET /models failed")?;

        let status = resp.status();
        let text = resp.text().await.context("reading /models body")?;

        if !status.is_success() {
            bail!("GET /models error {}: {}", status, text);
        }

        let parsed: ModelsResponse =
            serde_json::from_str(&text).context("parsing /models response")?;

        let mut ids: Vec<String> = parsed.data.into_iter().map(|m| m.id).collect();
        ids.sort();
        Ok(ids)
    }
}

#[async_trait]
impl LlmClient for OpenAiClient {
    fn provider_name(&self) -> &str {
        &self.provider
    }

    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse> {
        let body = self.build_body(&req, false);
        debug!("POST {} model={}", self.endpoint(), req.model);

        let resp = self
            .http
            .post(&self.endpoint())
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .context("HTTP request failed")?;

        let status = resp.status();
        let text = resp.text().await.context("reading response body")?;

        if !status.is_success() {
            bail!("API error {}: {}", status, text);
        }

        let parsed: ChatCompletionResponse =
            serde_json::from_str(&text).context("parsing completion response")?;

        let choice = parsed.choices.into_iter().next().context("no choices in response")?;
        let content = choice.message.content.unwrap_or_default();
        let tool_calls = choice
            .message
            .tool_calls
            .unwrap_or_default()
            .into_iter()
            .map(|tc| ToolCall {
                id: tc.id,
                name: tc.function.name,
                arguments: serde_json::from_str(&tc.function.arguments)
                    .unwrap_or(Value::String(tc.function.arguments.clone())),
            })
            .collect();

        let (prompt_tokens, completion_tokens) = parsed
            .usage
            .map(|u| (u.prompt_tokens, u.completion_tokens))
            .unwrap_or((0, 0));

        Ok(CompletionResponse {
            content,
            tool_calls,
            prompt_tokens,
            completion_tokens,
            model: parsed.model,
            finish_reason: choice.finish_reason.unwrap_or_default(),
        })
    }

    async fn stream(&self, req: CompletionRequest, tx: Sender<StreamChunk>) -> Result<()> {
        let body = self.build_body(&req, true);
        debug!("POST {} (stream) model={}", self.endpoint(), req.model);

        let resp = self
            .http
            .post(&self.endpoint())
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .context("HTTP stream request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            bail!("API stream error {}: {}", status, text);
        }

        let mut byte_stream = resp.bytes_stream();
        let mut buf = String::new();
        let mut prompt_tokens = 0u64;
        let mut completion_tokens = 0u64;
        let mut finish_reason = String::new();

        while let Some(chunk) = byte_stream.next().await {
            let chunk: Bytes = chunk.context("stream read error")?;
            buf.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(newline_pos) = buf.find('\n') {
                let line = buf[..newline_pos].trim().to_string();
                buf = buf[newline_pos + 1..].to_string();

                if line.is_empty() || line == "data: [DONE]" {
                    continue;
                }

                let data = line.strip_prefix("data: ").unwrap_or(&line);
                if data.is_empty() { continue; }

                match serde_json::from_str::<StreamResponse>(data) {
                    Ok(sr) => {
                        if let Some(usage) = sr.usage {
                            prompt_tokens = usage.prompt_tokens;
                            completion_tokens = usage.completion_tokens;
                        }
                        for choice in sr.choices {
                            if let Some(fr) = choice.finish_reason {
                                finish_reason = fr;
                            }
                            let delta = choice.delta;
                            // reasoning_content (DeepSeek / QwQ style)
                            if let Some(rc) = delta.reasoning_content {
                                if !rc.is_empty() {
                                    let _ = tx.send(StreamChunk::ReasoningDelta(rc)).await;
                                }
                            }
                            if let Some(text) = delta.content {
                                if !text.is_empty() {
                                    let _ = tx.send(StreamChunk::Delta(text)).await;
                                }
                            }
                            for tc in delta.tool_calls.unwrap_or_default() {
                                let _ = tx
                                    .send(StreamChunk::ToolCallDelta {
                                        index: tc.index.unwrap_or(0),
                                        id: tc.id,
                                        name: tc.function.as_ref().and_then(|f| f.name.clone()),
                                        args: tc.function
                                            .and_then(|f| f.arguments)
                                            .unwrap_or_default(),
                                    })
                                    .await;
                            }
                        }
                    }
                    Err(e) => {
                        warn!("failed to parse SSE chunk: {} — {:?}", e, data);
                    }
                }
            }
        }

        let _ = tx
            .send(StreamChunk::Done { prompt_tokens, completion_tokens, finish_reason })
            .await;

        Ok(())
    }
}

// ── JSON helpers ──────────────────────────────────────────────────────────────

fn msg_to_json(m: &Message) -> Value {
    let role = match m.role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    };

    let mut obj = json!({ "role": role, "content": m.content });

    if !m.tool_calls.is_empty() {
        let tcs: Vec<Value> = m
            .tool_calls
            .iter()
            .map(|tc| {
                json!({
                    "id": tc.id,
                    "type": "function",
                    "function": {
                        "name": tc.name,
                        "arguments": tc.arguments.to_string()
                    }
                })
            })
            .collect();
        obj["tool_calls"] = json!(tcs);
    }

    if let Some(id) = &m.tool_call_id {
        obj["tool_call_id"] = json!(id);
    }

    obj
}

fn tool_to_json(t: &ToolDefinition) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": t.name,
            "description": t.description,
            "parameters": t.parameters
        }
    })
}

// ── Response types ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ChatCompletionResponse {
    model: String,
    choices: Vec<ChatChoice>,
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ChatMessage {
    content: Option<String>,
    tool_calls: Option<Vec<ApiToolCall>>,
}

#[derive(Deserialize)]
struct ApiToolCall {
    id: String,
    function: ApiFunction,
}

#[derive(Deserialize)]
struct ApiFunction {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
struct Usage {
    prompt_tokens: u64,
    completion_tokens: u64,
}

// ── Stream response types ─────────────────────────────────────────────────────

#[derive(Deserialize)]
struct StreamResponse {
    choices: Vec<StreamChoice>,
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct StreamDelta {
    content: Option<String>,
    /// DeepSeek / QwQ reasoning field
    reasoning_content: Option<String>,
    tool_calls: Option<Vec<StreamToolCall>>,
}

#[derive(Deserialize)]
struct StreamToolCall {
    index: Option<usize>,
    id: Option<String>,
    function: Option<StreamFunction>,
}

#[derive(Deserialize)]
struct StreamFunction {
    name: Option<String>,
    arguments: Option<String>,
}

// ── Models endpoint ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ModelsResponse {
    data: Vec<ModelEntry>,
}

#[derive(Deserialize)]
struct ModelEntry {
    id: String,
}
