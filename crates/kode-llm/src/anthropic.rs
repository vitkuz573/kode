use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use bytes::Bytes;
use futures::StreamExt;
use kode_core::types::{Role, ToolCall};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::mpsc::Sender;
use tracing::{debug, warn};

use crate::client::{
    CompletionRequest, CompletionResponse, LlmClient, StreamChunk,
};

pub struct AnthropicClient {
    http: Client,
    base_url: String,
    api_key: String,
    provider: String,
    api_version: String,
}

impl AnthropicClient {
    pub fn new(base_url: String, api_key: String, provider: String, api_version: String) -> Self {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .expect("failed to build HTTP client");
        Self { http, base_url, api_key, provider, api_version }
    }

    fn endpoint(&self) -> String {
        let base = self.base_url.trim_end_matches('/');
        // Support both https://api.anthropic.com and custom base URLs
        if base.ends_with("/v1") {
            format!("{}/messages", base)
        } else {
            format!("{}/v1/messages", base)
        }
    }

    fn models_endpoint(&self) -> String {
        let base = self.base_url.trim_end_matches('/');
        if base.ends_with("/v1") {
            format!("{}/models", base)
        } else {
            format!("{}/v1/models", base)
        }
    }

    /// Convert kode messages to Anthropic format.
    /// Anthropic separates system prompt from messages array.
    fn build_request(&self, req: &CompletionRequest, stream: bool) -> (Option<String>, Value) {
        let mut system_parts: Vec<String> = Vec::new();
        let mut messages: Vec<Value> = Vec::new();

        for msg in &req.messages {
            match msg.role {
                Role::System => {
                    system_parts.push(msg.content.clone());
                }
                Role::User => {
                    messages.push(json!({
                        "role": "user",
                        "content": msg.content
                    }));
                }
                Role::Assistant => {
                    // If assistant message has tool_calls, encode them as content blocks
                    if msg.tool_calls.is_empty() {
                        messages.push(json!({
                            "role": "assistant",
                            "content": msg.content
                        }));
                    } else {
                        let mut content: Vec<Value> = Vec::new();
                        if !msg.content.is_empty() {
                            content.push(json!({ "type": "text", "text": msg.content }));
                        }
                        for tc in &msg.tool_calls {
                            content.push(json!({
                                "type": "tool_use",
                                "id": tc.id,
                                "name": tc.name,
                                "input": tc.arguments
                            }));
                        }
                        messages.push(json!({ "role": "assistant", "content": content }));
                    }
                }
                Role::Tool => {
                    // Tool results go as user messages with tool_result content blocks
                    let call_id = msg.tool_call_id.as_deref().unwrap_or("");
                    messages.push(json!({
                        "role": "user",
                        "content": [{
                            "type": "tool_result",
                            "tool_use_id": call_id,
                            "content": msg.content
                        }]
                    }));
                }
            }
        }

        // Merge consecutive same-role messages (Anthropic requires alternating)
        let messages = merge_consecutive_roles(messages);

        let system = if system_parts.is_empty() {
            None
        } else {
            Some(system_parts.join("\n\n"))
        };

        let mut body = json!({
            "model": req.model,
            "messages": messages,
            "max_tokens": req.max_tokens.unwrap_or(8192),
            "temperature": req.temperature,
            "stream": stream,
        });

        if let Some(sys) = &system {
            body["system"] = json!(sys);
        }

        // Tools
        if !req.tools.is_empty() {
            let tools: Vec<Value> = req.tools.iter().map(|t| json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.parameters
            })).collect();
            body["tools"] = json!(tools);
        }

        // Extended thinking (budget_tokens triggers it)
        // body["thinking"] = json!({ "type": "enabled", "budget_tokens": 10000 });

        (system, body)
    }

    pub async fn list_models(&self) -> Result<Vec<String>> {
        debug!("GET {}", self.models_endpoint());
        let resp = self.http
            .get(&self.models_endpoint())
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", &self.api_version)
            .send()
            .await
            .context("GET /models failed")?;

        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            bail!("GET /models error {}: {}", status, text);
        }

        #[derive(Deserialize)]
        struct ModelsResp { data: Vec<ModelEntry> }
        #[derive(Deserialize)]
        struct ModelEntry { id: String }

        let parsed: ModelsResp = serde_json::from_str(&text)?;
        let mut ids: Vec<String> = parsed.data.into_iter().map(|m| m.id).collect();
        ids.sort();
        Ok(ids)
    }
}

#[async_trait]
impl LlmClient for AnthropicClient {
    fn provider_name(&self) -> &str { &self.provider }

    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse> {
        let (_, body) = self.build_request(&req, false);
        debug!("POST {} model={}", self.endpoint(), req.model);

        let resp = self.http
            .post(&self.endpoint())
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", &self.api_version)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Anthropic HTTP request failed")?;

        let status = resp.status();
        let text = resp.text().await.context("reading response body")?;
        if !status.is_success() {
            bail!("Anthropic API error {}: {}", status, text);
        }

        let parsed: AnthropicResponse =
            serde_json::from_str(&text).context("parsing Anthropic response")?;

        let mut content_text = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();

        for block in parsed.content {
            match block.block_type.as_str() {
                "text" => {
                    if let Some(t) = block.text { content_text.push_str(&t); }
                }
                "tool_use" => {
                    tool_calls.push(ToolCall {
                        id: block.id.unwrap_or_default(),
                        name: block.name.unwrap_or_default(),
                        arguments: block.input.unwrap_or(Value::Null),
                    });
                }
                "thinking" => {
                    // thinking blocks are separate — we surface them via ReasoningDelta in stream
                }
                _ => {}
            }
        }

        let prompt_tokens = parsed.usage.as_ref().map(|u| u.input_tokens).unwrap_or(0);
        let completion_tokens = parsed.usage.as_ref().map(|u| u.output_tokens).unwrap_or(0);

        Ok(CompletionResponse {
            content: content_text,
            tool_calls,
            prompt_tokens,
            completion_tokens,
            model: parsed.model,
            finish_reason: parsed.stop_reason.unwrap_or_default(),
        })
    }

    async fn stream(&self, req: CompletionRequest, tx: Sender<StreamChunk>) -> Result<()> {
        let (_, body) = self.build_request(&req, true);
        debug!("POST {} (stream) model={}", self.endpoint(), req.model);

        let resp = self.http
            .post(&self.endpoint())
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", &self.api_version)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Anthropic stream request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            bail!("Anthropic stream error {}: {}", status, text);
        }

        let mut byte_stream = resp.bytes_stream();
        let mut buf = String::new();
        let mut prompt_tokens = 0u64;
        let mut completion_tokens = 0u64;
        let mut finish_reason = String::new();

        // Track current block type for delta routing
        let mut current_block_type = String::new();
        // index -> (id, name, args) for tool_use blocks
        let mut tool_blocks: std::collections::BTreeMap<usize, (String, String, String)> =
            std::collections::BTreeMap::new();

        while let Some(chunk) = byte_stream.next().await {
            let chunk: Bytes = chunk.context("stream read error")?;
            buf.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(newline_pos) = buf.find('\n') {
                let line = buf[..newline_pos].trim().to_string();
                buf = buf[newline_pos + 1..].to_string();

                if line.is_empty() { continue; }

                // SSE event type line: "event: ..."
                if line.starts_with("event: ") {
                    // We handle event types via the data payload type field
                    continue;
                }

                let data = match line.strip_prefix("data: ") {
                    Some(d) => d.to_string(),
                    None => continue,
                };

                if data == "[DONE]" { break; }

                match serde_json::from_str::<AnthropicStreamEvent>(&data) {
                    Ok(ev) => match ev.event_type.as_str() {
                        "content_block_start" => {
                            if let Some(block) = ev.content_block {
                                current_block_type = block.block_type.clone();
                                let idx = ev.index.unwrap_or(0);
                                if block.block_type == "tool_use" {
                                    tool_blocks.insert(
                                        idx,
                                        (
                                            block.id.unwrap_or_default(),
                                            block.name.unwrap_or_default(),
                                            String::new(),
                                        ),
                                    );
                                }
                            }
                        }
                        "content_block_delta" => {
                            if let Some(delta) = ev.delta {
                                let idx = ev.index.unwrap_or(0);
                                match delta.delta_type.as_deref().unwrap_or("") {
                                    "text_delta" => {
                                        if let Some(text) = delta.text {
                                            let _ = tx.send(StreamChunk::Delta(text)).await;
                                        }
                                    }
                                    "thinking_delta" => {
                                        if let Some(thinking) = delta.thinking {
                                            let _ = tx.send(StreamChunk::ReasoningDelta(thinking)).await;
                                        }
                                    }
                                    "input_json_delta" => {
                                        if let Some(partial) = delta.partial_json {
                                            if let Some(entry) = tool_blocks.get_mut(&idx) {
                                                entry.2.push_str(&partial);
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        "content_block_stop" => {
                            let idx = ev.index.unwrap_or(0);
                            // Emit completed tool call
                            if let Some((id, name, args)) = tool_blocks.get(&idx) {
                                if !name.is_empty() {
                                    let _ = tx.send(StreamChunk::ToolCallDelta {
                                        index: idx,
                                        id: Some(id.clone()),
                                        name: Some(name.clone()),
                                        args: args.clone(),
                                    }).await;
                                }
                            }
                        }
                        "message_delta" => {
                            if let Some(delta) = ev.delta {
                                if let Some(reason) = delta.stop_reason {
                                    finish_reason = reason;
                                }
                            }
                            if let Some(usage) = ev.usage {
                                completion_tokens = usage.output_tokens.unwrap_or(0);
                            }
                        }
                        "message_start" => {
                            if let Some(msg) = ev.message {
                                if let Some(usage) = msg.usage {
                                    prompt_tokens = usage.input_tokens.unwrap_or(0);
                                }
                            }
                        }
                        "error" => {
                            let err = ev.error.as_ref()
                                .and_then(|e| e.message.as_deref())
                                .unwrap_or("unknown error");
                            bail!("Anthropic stream error: {}", err);
                        }
                        _ => {}
                    },
                    Err(e) => {
                        warn!("failed to parse Anthropic SSE: {} — {:?}", e, data);
                    }
                }
            }
        }

        let _ = tx.send(StreamChunk::Done {
            prompt_tokens,
            completion_tokens,
            finish_reason,
        }).await;

        Ok(())
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Merge consecutive messages with the same role (Anthropic requires alternating user/assistant)
fn merge_consecutive_roles(messages: Vec<Value>) -> Vec<Value> {
    let mut result: Vec<Value> = Vec::new();
    for msg in messages {
        let role = msg["role"].as_str().unwrap_or("").to_string();
        if let Some(last) = result.last_mut() {
            let last_role = last["role"].as_str().unwrap_or("").to_string();
            if last_role == role {
                // Merge content
                let existing = last["content"].clone();
                let new_content = msg["content"].clone();
                last["content"] = match (existing, new_content) {
                    (Value::String(a), Value::String(b)) => json!(format!("{}\n{}", a, b)),
                    (Value::Array(mut a), Value::Array(b)) => { a.extend(b); json!(a) }
                    (Value::String(a), Value::Array(b)) => {
                        let mut arr = vec![json!({"type": "text", "text": a})];
                        arr.extend(b);
                        json!(arr)
                    }
                    (Value::Array(mut a), Value::String(b)) => {
                        a.push(json!({"type": "text", "text": b}));
                        json!(a)
                    }
                    (_, b) => b,
                };
                continue;
            }
        }
        result.push(msg);
    }
    result
}

// ── Response types ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct AnthropicResponse {
    model: String,
    content: Vec<ContentBlock>,
    stop_reason: Option<String>,
    usage: Option<AnthropicUsage>,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: Option<String>,
    id: Option<String>,
    name: Option<String>,
    input: Option<Value>,
}

#[derive(Deserialize)]
struct AnthropicUsage {
    input_tokens: u64,
    output_tokens: u64,
}

// ── Stream event types ────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct AnthropicStreamEvent {
    #[serde(rename = "type")]
    event_type: String,
    index: Option<usize>,
    content_block: Option<StreamContentBlock>,
    delta: Option<StreamDelta>,
    usage: Option<StreamUsage>,
    message: Option<StreamMessage>,
    error: Option<StreamError>,
}

#[derive(Deserialize)]
struct StreamContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    id: Option<String>,
    name: Option<String>,
}

#[derive(Deserialize)]
struct StreamDelta {
    #[serde(rename = "type")]
    delta_type: Option<String>,
    text: Option<String>,
    thinking: Option<String>,
    partial_json: Option<String>,
    stop_reason: Option<String>,
}

#[derive(Deserialize)]
struct StreamUsage {
    output_tokens: Option<u64>,
}

#[derive(Deserialize)]
struct StreamMessage {
    usage: Option<StreamMessageUsage>,
}

#[derive(Deserialize)]
struct StreamMessageUsage {
    input_tokens: Option<u64>,
}

#[derive(Deserialize)]
struct StreamError {
    message: Option<String>,
}
