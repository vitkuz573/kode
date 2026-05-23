use anyhow::Result;
use kode_core::{
    config::AgentConfig,
    cost::CostTracker,
    types::{Message, Role},
};
use kode_llm::{
    client::{CompletionRequest, StreamChunk},
    ModelRouter,
};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::debug;

use crate::tools::ToolRegistry;

/// Events emitted by the agent during a run
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// Streaming text delta from the model
    TextDelta(String),
    /// Reasoning/thinking delta (DeepSeek, QwQ, o1-style)
    ReasoningDelta(String),
    /// Model requested a tool call
    ToolCallStart { id: String, name: String },
    /// Tool call completed
    ToolCallDone { id: String, name: String, output: String, is_error: bool },
    /// Turn complete with usage stats
    TurnDone { prompt_tokens: u64, completion_tokens: u64, cost_summary: String },
    /// Agent loop finished (no more tool calls)
    Done,
    /// Error occurred
    Error(String),
}

pub struct Agent {
    router: Arc<ModelRouter>,
    tools: Arc<ToolRegistry>,
    config: AgentConfig,
    model: String,
    cost: CostTracker,
}

impl Agent {
    pub fn new(
        router: Arc<ModelRouter>,
        tools: Arc<ToolRegistry>,
        config: AgentConfig,
        model: String,
    ) -> Self {
        Self { router, tools, config, model, cost: CostTracker::default() }
    }

    /// Run the agent loop on a mutable message history.
    /// Emits events via the provided channel.
    pub async fn run(
        &mut self,
        messages: &mut Vec<Message>,
        event_tx: mpsc::Sender<AgentEvent>,
    ) -> Result<()> {
        let max_steps = self.config.max_steps;

        for step in 0..max_steps {
            debug!("agent step {}/{}", step + 1, max_steps);

            let (client, model_id) = self.router.resolve(&self.model)?;

            let req = CompletionRequest {
                model: model_id.clone(),
                messages: messages.clone(),
                temperature: self.config.temperature,
                max_tokens: None,
                tools: self.tools.definitions(),
                stream: true,
            };

            // Stream the response
            let (stream_tx, stream_rx) = mpsc::channel::<StreamChunk>(256);
            let client_clone = client.clone();
            let req_clone = req.clone();
            let stream_handle =
                tokio::spawn(async move { client_clone.stream(req_clone, stream_tx).await });

            // Collect stream, forwarding deltas to caller
            let mut full_text = String::new();
            // index -> (id, name, args)
            let mut tool_call_map: std::collections::BTreeMap<usize, (String, String, String)> =
                std::collections::BTreeMap::new();
            let mut prompt_tokens = 0u64;
            let mut completion_tokens = 0u64;
            let mut finish_reason = String::new();

            let mut rx = stream_rx;
            while let Some(chunk) = rx.recv().await {
                match chunk {
                    StreamChunk::Delta(text) => {
                        let _ = event_tx.send(AgentEvent::TextDelta(text.clone())).await;
                        full_text.push_str(&text);
                    }
                    StreamChunk::ReasoningDelta(text) => {
                        let _ = event_tx.send(AgentEvent::ReasoningDelta(text)).await;
                    }
                    StreamChunk::ToolCallDelta { index, id, name, args } => {
                        let entry = tool_call_map
                            .entry(index)
                            .or_insert_with(|| (String::new(), String::new(), String::new()));
                        if let Some(i) = id { if !i.is_empty() { entry.0 = i; } }
                        if let Some(n) = name { if !n.is_empty() { entry.1 = n; } }
                        entry.2.push_str(&args);
                    }
                    StreamChunk::Done { prompt_tokens: p, completion_tokens: c, finish_reason: fr } => {
                        prompt_tokens = p;
                        completion_tokens = c;
                        finish_reason = fr;
                    }
                }
            }

            stream_handle.await??;

            self.cost.add(prompt_tokens, completion_tokens, &model_id);

            let _ = event_tx
                .send(AgentEvent::TurnDone {
                    prompt_tokens,
                    completion_tokens,
                    cost_summary: self.cost.summary(),
                })
                .await;

            // Build assistant message
            let tool_calls: Vec<kode_core::types::ToolCall> = tool_call_map
                .into_values()
                .map(|(id, name, args)| kode_core::types::ToolCall {
                    id,
                    name,
                    arguments: serde_json::from_str(&args)
                        .unwrap_or(serde_json::Value::String(args)),
                })
                .collect();

            let mut assistant_msg = Message::assistant(full_text);
            assistant_msg.tool_calls = tool_calls.clone();
            messages.push(assistant_msg);

            // If no tool calls or finish_reason == "stop", we're done
            if tool_calls.is_empty() || finish_reason == "stop" {
                let _ = event_tx.send(AgentEvent::Done).await;
                return Ok(());
            }

            // Execute tool calls
            for tc in &tool_calls {
                let _ = event_tx
                    .send(AgentEvent::ToolCallStart { id: tc.id.clone(), name: tc.name.clone() })
                    .await;

                let result = self.tools.execute(tc).await;

                let _ = event_tx
                    .send(AgentEvent::ToolCallDone {
                        id: tc.id.clone(),
                        name: tc.name.clone(),
                        output: result.output.clone(),
                        is_error: result.is_error,
                    })
                    .await;

                messages.push(Message::tool_result(&result.call_id, &result.output));
            }
        }

        let _ = event_tx
            .send(AgentEvent::Error(format!("max steps ({}) reached", max_steps)))
            .await;
        Ok(())
    }
}
