use crate::types::Message;
use crate::config::ContextConfig;

/// Manages the sliding context window
pub struct ContextManager {
    config: ContextConfig,
}

impl ContextManager {
    pub fn new(config: ContextConfig) -> Self {
        Self { config }
    }

    /// Trim messages to fit within the token budget.
    /// Always preserves the system message and the last N user/assistant turns.
    pub fn trim(&self, messages: &[Message]) -> Vec<Message> {
        let strategy = self.config.strategy.as_str();
        match strategy {
            "truncate" => self.truncate(messages),
            _ => self.sliding_window(messages),
        }
    }

    fn sliding_window(&self, messages: &[Message]) -> Vec<Message> {
        let max = self.config.max_tokens;
        let mut result: Vec<Message> = Vec::new();
        let mut tokens = 0usize;

        // Always keep system message
        let (system_msgs, rest): (Vec<_>, Vec<_>) = messages
            .iter()
            .partition(|m| m.role == crate::types::Role::System);

        for m in system_msgs {
            tokens += m.content.len() / 4 + 1;
            result.push(m.clone());
        }

        // Walk rest in reverse, keep until budget exhausted
        let mut tail: Vec<Message> = Vec::new();
        for m in rest.into_iter().rev() {
            let t = m.content.len() / 4 + 1;
            if tokens + t > max && !tail.is_empty() {
                break;
            }
            tokens += t;
            tail.push(m.clone());
        }
        tail.reverse();
        result.extend(tail);
        result
    }

    fn truncate(&self, messages: &[Message]) -> Vec<Message> {
        let max = self.config.max_tokens;
        let mut tokens = 0usize;
        let mut result = Vec::new();
        for m in messages {
            let t = m.content.len() / 4 + 1;
            if tokens + t > max { break; }
            tokens += t;
            result.push(m.clone());
        }
        result
    }
}
