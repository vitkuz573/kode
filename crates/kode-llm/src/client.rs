use anyhow::Result;
use async_trait::async_trait;
use kode_core::types::{Message, ToolCall};
use serde::{Deserialize, Serialize};

/// Request to the LLM
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub temperature: f32,
    pub max_tokens: Option<u32>,
    pub tools: Vec<ToolDefinition>,
    pub stream: bool,
}

/// Tool definition for function calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Non-streaming completion response
#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub model: String,
    pub finish_reason: String,
}

/// A single streaming chunk
#[derive(Debug, Clone)]
pub enum StreamChunk {
    /// Text delta
    Delta(String),
    /// Reasoning/thinking delta (DeepSeek, QwQ, o1-style)
    ReasoningDelta(String),
    /// Tool call delta — index identifies which tool call, id/name only in first chunk
    ToolCallDelta { index: usize, id: Option<String>, name: Option<String>, args: String },
    /// Stream finished with usage stats
    Done { prompt_tokens: u64, completion_tokens: u64, finish_reason: String },
}

/// Trait implemented by each provider backend
#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse>;
    async fn stream(
        &self,
        req: CompletionRequest,
        tx: tokio::sync::mpsc::Sender<StreamChunk>,
    ) -> Result<()>;
    fn provider_name(&self) -> &str;
}
