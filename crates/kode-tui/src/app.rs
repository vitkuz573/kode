use anyhow::Result;
use kode_agent::AgentEvent;
use kode_core::{
    config::Config,
    session::{Session, SessionStore, TodoItem},
    types::{Message, Role},
};
use kode_llm::ModelRouter;
use std::collections::HashSet;
use std::sync::Arc;

use crate::theme::{Theme, CATPPUCCIN_MOCHA};

/// Spinner frames (braille animation)
pub const SPINNER_FRAMES: &[&str] = &["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"];

#[derive(Debug, Clone, PartialEq)]
pub enum AppMode {
    Chat,
    SessionList,
    ModelPicker,
    ThemePicker,
    CommandPalette,
    TodoManager,
    ChangedFilesManager,
}

/// A rich rendered message in the chat view
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: MsgRole,
    pub content: String,
    pub reasoning: String,          // <think> content
    pub reasoning_collapsed: bool,  // collapsed by default after done
    pub timestamp: String,
    pub tool_calls: Vec<ToolCallEntry>,
    pub is_streaming: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MsgRole { User, Assistant, System }

#[derive(Debug, Clone)]
pub struct ToolCallEntry {
    pub name: String,
    pub status: ToolStatus,
    pub output_preview: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ToolStatus { Running, Done, Error }

/// A command palette entry
#[derive(Debug, Clone)]
pub struct Command {
    pub key: &'static str,
    pub label: &'static str,
    pub description: &'static str,
}

pub fn all_commands() -> Vec<Command> {
    vec![
        Command { key: "Ctrl+C",     label: "quit",           description: "Exit kode" },
        Command { key: "Ctrl+B",     label: "sidebar",        description: "Toggle session sidebar" },
        Command { key: "Tab",        label: "sessions",       description: "Open session list" },
        Command { key: "Ctrl+K",     label: "model",          description: "Switch model" },
        Command { key: "Ctrl+T",     label: "theme",          description: "Switch color theme" },
        Command { key: "Ctrl+P",     label: "palette",        description: "Command palette" },
        Command { key: "Ctrl+N",     label: "new session",    description: "Start a new session" },
        Command { key: "Ctrl+L",     label: "clear",          description: "Clear current chat" },
        Command { key: "Ctrl+R",     label: "refresh models", description: "Re-discover models from provider" },
        Command { key: "Ctrl+Y",     label: "todo",           description: "Open TODO manager" },
        Command { key: "Ctrl+F",     label: "files",          description: "Open changed files manager" },
        Command { key: "↑↓",         label: "history",        description: "Recall previous user messages into input" },
        Command { key: "Ctrl+↑↓",    label: "scroll",         description: "Scroll messages" },
        Command { key: "PgUp/PgDn",  label: "page scroll",    description: "Scroll by page" },
        Command { key: "Home/End",   label: "cursor",         description: "Move cursor to start/end" },
        Command { key: "Shift+Enter",label: "newline",        description: "Insert a new line in input" },
        Command { key: "Enter",      label: "send",           description: "Send message" },
        Command { key: "Esc",        label: "back",           description: "Close overlay / cancel" },
    ]
}

pub struct App {
    pub mode: AppMode,

    // Chat state
    pub messages: Vec<Message>,
    pub chat_messages: Vec<ChatMessage>,
    pub input: String,
    pub cursor: usize,
    pub input_history_cursor: Option<usize>,
    pub input_draft: String,
    pub scroll: usize,
    pub auto_scroll: bool,

    // Thinking / streaming
    pub thinking: bool,
    pub spinner_frame: usize,
    pub spinner_tick: u64,

    // Model / session
    pub model: String,
    pub session: Session,
    pub store: SessionStore,
    pub router: Arc<ModelRouter>,
    pub config: Config,

    // Session list
    pub sessions: Vec<Session>,
    pub session_cursor: usize,

    // Model picker
    pub model_list: Vec<String>,
    pub model_cursor: usize,
    pub models_loading: bool,

    // Theme picker
    pub theme: Theme,
    pub theme_list: Vec<&'static str>,
    pub theme_cursor: usize,

    // Command palette
    pub commands: Vec<Command>,
    pub command_cursor: usize,
    pub command_filter: String,
    pub todo_cursor: usize,
    pub todo_input: String,
    pub changed_files_cursor: usize,
    pub changed_files_filter: String,

    // Stats
    pub total_prompt_tokens: u64,
    pub total_completion_tokens: u64,
    pub total_cost_usd: f64,
    pub last_response_ms: u64,
    pub response_start: Option<std::time::Instant>,

    // Layout
    pub sidebar_visible: bool,

    // Mouse state
    pub mouse_x: u16,
    pub mouse_y: u16,
}

impl App {
    pub fn new(config: Config, model: String) -> Result<Self> {
        let router = Arc::new(ModelRouter::new(config.clone()));
        let store = SessionStore::new()?;
        let session = Session::new(&model);
        let model_list = router.list_models();
        let theme_list = Theme::all();
        let theme_name = config.theme.as_deref().unwrap_or(CATPPUCCIN_MOCHA.name);
        let theme = Theme::by_name(theme_name);
        let theme_cursor = theme_list.iter().position(|n| *n == theme.name).unwrap_or(0);
        let commands = all_commands();
        Ok(Self {
            mode: AppMode::Chat,
            messages: Vec::new(),
            chat_messages: Vec::new(),
            input: String::new(),
            cursor: 0,
            input_history_cursor: None,
            input_draft: String::new(),
            scroll: 0,
            auto_scroll: true,
            thinking: false,
            spinner_frame: 0,
            spinner_tick: 0,
            model: model.clone(),
            session,
            store,
            router,
            config,
            sessions: Vec::new(),
            session_cursor: 0,
            model_list,
            model_cursor: 0,
            models_loading: false,
            theme,
            theme_list,
            theme_cursor,
            commands,
            command_cursor: 0,
            command_filter: String::new(),
            todo_cursor: 0,
            todo_input: String::new(),
            changed_files_cursor: 0,
            changed_files_filter: String::new(),
            total_prompt_tokens: 0,
            total_completion_tokens: 0,
            total_cost_usd: 0.0,
            last_response_ms: 0,
            response_start: None,
            sidebar_visible: true,
            mouse_x: 0,
            mouse_y: 0,
        })
    }

    pub fn persist_theme_preference(&mut self) {
        self.config.theme = Some(self.theme.name.to_string());
        let _ = self.config.save();
    }

    pub fn refresh_sessions_cache(&mut self) {
        if let Ok(sessions) = self.store.list() {
            self.sessions = sessions;
        }
    }

    pub fn persist_current_session(&mut self) {
        let _ = self.store.save(&self.session);
        self.refresh_sessions_cache();
    }

    pub fn tick_spinner(&mut self) {
        if self.thinking {
            self.spinner_tick += 1;
            if self.spinner_tick % 2 == 0 {
                self.spinner_frame = (self.spinner_frame + 1) % SPINNER_FRAMES.len();
            }
        }
    }

    pub fn spinner(&self) -> &str { SPINNER_FRAMES[self.spinner_frame] }

    pub fn now_str() -> String {
        chrono::Local::now().format("%H:%M:%S").to_string()
    }

    pub fn new_session(&mut self) {
        self.messages.clear();
        self.chat_messages.clear();
        self.session = Session::new(&self.model);
        self.scroll = 0;
        self.auto_scroll = true;
        self.chat_messages.push(ChatMessage {
            role: MsgRole::System,
            content: format!("new session · model: {}", self.model),
            reasoning: String::new(),
            reasoning_collapsed: true,
            timestamp: Self::now_str(),
            tool_calls: Vec::new(),
            is_streaming: false,
        });
        self.persist_current_session();
    }

    pub fn clear_chat(&mut self) {
        self.chat_messages.clear();
        self.scroll = 0;
        self.auto_scroll = true;
    }

    pub fn reset_input_history_nav(&mut self) {
        self.input_history_cursor = None;
        self.input_draft.clear();
    }

    pub fn history_up(&mut self) {
        let user_msgs: Vec<&String> = self
            .messages
            .iter()
            .filter(|m| m.role == Role::User && !m.content.trim().is_empty())
            .map(|m| &m.content)
            .collect();
        if user_msgs.is_empty() {
            return;
        }

        if self.input_history_cursor.is_none() {
            self.input_draft = self.input.clone();
            self.input_history_cursor = Some(user_msgs.len().saturating_sub(1));
        } else if let Some(idx) = self.input_history_cursor {
            self.input_history_cursor = Some(idx.saturating_sub(1));
        }

        if let Some(idx) = self.input_history_cursor {
            if let Some(text) = user_msgs.get(idx) {
                self.input = (*text).clone();
                self.cursor = self.input.chars().count();
            }
        }
    }

    pub fn history_down(&mut self) {
        let user_msgs: Vec<&String> = self
            .messages
            .iter()
            .filter(|m| m.role == Role::User && !m.content.trim().is_empty())
            .map(|m| &m.content)
            .collect();
        let Some(idx) = self.input_history_cursor else { return; };
        if user_msgs.is_empty() {
            self.input_history_cursor = None;
            return;
        }

        if idx + 1 >= user_msgs.len() {
            self.input_history_cursor = None;
            self.input = self.input_draft.clone();
            self.cursor = self.input.chars().count();
            return;
        }

        let next = idx + 1;
        self.input_history_cursor = Some(next);
        if let Some(text) = user_msgs.get(next) {
            self.input = (*text).clone();
            self.cursor = self.input.chars().count();
        }
    }

    pub fn push_user_message(&mut self, text: &str) {
        self.chat_messages.push(ChatMessage {
            role: MsgRole::User,
            content: text.to_string(),
            reasoning: String::new(),
            reasoning_collapsed: true,
            timestamp: Self::now_str(),
            tool_calls: Vec::new(),
            is_streaming: false,
        });
        self.auto_scroll = true;
    }

    pub fn begin_assistant_message(&mut self) {
        self.chat_messages.push(ChatMessage {
            role: MsgRole::Assistant,
            content: String::new(),
            reasoning: String::new(),
            reasoning_collapsed: false,
            timestamp: Self::now_str(),
            tool_calls: Vec::new(),
            is_streaming: true,
        });
        self.response_start = Some(std::time::Instant::now());
        self.auto_scroll = true;
    }

    pub fn append_assistant_delta(&mut self, delta: &str) {
        if let Some(msg) = self.chat_messages.last_mut() {
            if msg.role == MsgRole::Assistant && msg.is_streaming {
                msg.content.push_str(delta);
                self.auto_scroll = true;
                return;
            }
        }
        self.begin_assistant_message();
        if let Some(msg) = self.chat_messages.last_mut() {
            msg.content.push_str(delta);
        }
    }

    pub fn append_reasoning_delta(&mut self, delta: &str) {
        if let Some(msg) = self.chat_messages.last_mut() {
            if msg.role == MsgRole::Assistant {
                msg.reasoning.push_str(delta);
                self.auto_scroll = true;
                return;
            }
        }
        self.begin_assistant_message();
        if let Some(msg) = self.chat_messages.last_mut() {
            msg.reasoning.push_str(delta);
        }
    }

    pub fn finish_assistant_message(&mut self) {
        if let Some(msg) = self.chat_messages.last_mut() {
            if msg.role == MsgRole::Assistant {
                msg.is_streaming = false;
                // Auto-collapse reasoning when done
                if !msg.reasoning.is_empty() {
                    msg.reasoning_collapsed = true;
                }
            }
        }
        if let Some(start) = self.response_start.take() {
            self.last_response_ms = start.elapsed().as_millis() as u64;
        }
    }

    pub fn toggle_reasoning(&mut self, msg_idx: usize) {
        if let Some(msg) = self.chat_messages.get_mut(msg_idx) {
            msg.reasoning_collapsed = !msg.reasoning_collapsed;
        }
    }

    pub fn add_tool_call(&mut self, name: &str) {
        if let Some(msg) = self.chat_messages.last_mut() {
            if msg.role == MsgRole::Assistant {
                msg.tool_calls.push(ToolCallEntry {
                    name: name.to_string(),
                    status: ToolStatus::Running,
                    output_preview: String::new(),
                });
                return;
            }
        }
        self.begin_assistant_message();
        if let Some(msg) = self.chat_messages.last_mut() {
            msg.tool_calls.push(ToolCallEntry {
                name: name.to_string(),
                status: ToolStatus::Running,
                output_preview: String::new(),
            });
        }
    }

    pub fn finish_tool_call(&mut self, name: &str, output: &str, is_error: bool) {
        for msg in self.chat_messages.iter_mut().rev() {
            for tc in msg.tool_calls.iter_mut().rev() {
                if tc.name == name && tc.status == ToolStatus::Running {
                    tc.status = if is_error { ToolStatus::Error } else { ToolStatus::Done };
                    tc.output_preview = output
                        .lines()
                        .take(2)
                        .collect::<Vec<_>>()
                        .join(" · ")
                        .chars()
                        .take(80)
                        .collect();
                    return;
                }
            }
        }
    }

    pub fn handle_agent_event(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::TextDelta(text) => {
                self.append_assistant_delta(&text);
            }
            AgentEvent::ReasoningDelta(text) => {
                self.append_reasoning_delta(&text);
            }
            AgentEvent::ToolCallStart { name, arguments, .. } => {
                self.add_tool_call(&name);
                self.track_changed_files_from_tool_start(&name, &arguments);
            }
            AgentEvent::ToolCallDone { id, name, output, is_error } => {
                self.finish_tool_call(&name, &output, is_error);
                self.messages.push(Message::tool_result(id.clone(), output.clone()));
                self.session.push(Message::tool_result(id, output));
            }
            AgentEvent::TurnDone { prompt_tokens, completion_tokens, .. } => {
                self.total_prompt_tokens += prompt_tokens;
                self.total_completion_tokens += completion_tokens;
                self.total_cost_usd += (prompt_tokens as f64 / 1_000_000.0) * 1.0
                    + (completion_tokens as f64 / 1_000_000.0) * 3.0;
                self.thinking = false;
                self.persist_assistant_artifacts();
                self.finish_assistant_message();
                self.persist_current_session();
            }
            AgentEvent::Done => {
                self.thinking = false;
                self.persist_assistant_artifacts();
                self.finish_assistant_message();
                self.persist_current_session();
            }
            AgentEvent::Error(e) => {
                self.thinking = false;
                self.finish_assistant_message();
                let formatted = format_error_payload(&e);
                self.chat_messages.push(ChatMessage {
                    role: MsgRole::System,
                    content: format!("⚠ error:\n{}", formatted),
                    reasoning: String::new(),
                    reasoning_collapsed: true,
                    timestamp: Self::now_str(),
                    tool_calls: Vec::new(),
                    is_streaming: false,
                });
            }
        }
    }

    pub fn filtered_commands(&self) -> Vec<&Command> {
        let f = self.command_filter.to_lowercase();
        self.commands.iter().filter(|c| {
            f.is_empty()
                || c.label.contains(&f)
                || c.description.to_lowercase().contains(&f)
                || c.key.to_lowercase().contains(&f)
        }).collect()
    }

    fn track_changed_files_from_tool_start(&mut self, tool_name: &str, args: &serde_json::Value) {
        if tool_name == "write_file" {
            if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                self.add_changed_file(path);
            }
        }
    }

    fn add_changed_file(&mut self, path: &str) {
        let normalized = path.trim();
        if normalized.is_empty() {
            return;
        }
        if !self.session.changed_files.iter().any(|p| p == normalized) {
            self.session.changed_files.push(normalized.to_string());
            self.session.updated_at = chrono::Utc::now();
        }
    }

    fn persist_assistant_artifacts(&mut self) {
        let Some(last) = self.chat_messages.last() else { return; };
        if last.role != MsgRole::Assistant {
            return;
        }
        let assistant_content = last.content.clone();

        let exists = self.messages.iter().rev().find(|m| matches!(m.role, kode_core::types::Role::Assistant));
        let should_push = exists.map(|m| m.content != assistant_content).unwrap_or(true);
        if should_push && !assistant_content.trim().is_empty() {
            let msg = Message::assistant(assistant_content.clone());
            self.messages.push(msg.clone());
            self.session.push(msg);
        }

        self.update_todos_from_assistant(&assistant_content);
    }

    fn update_todos_from_assistant(&mut self, text: &str) {
        let mut existing: HashSet<String> = self
            .session
            .todo_items
            .iter()
            .map(|t| t.text.to_lowercase())
            .collect();

        for raw in text.lines() {
            let line = raw.trim();
            let (done, payload) = if let Some(s) = line.strip_prefix("- [ ] ") {
                (false, s)
            } else if let Some(s) = line.strip_prefix("* [ ] ") {
                (false, s)
            } else if let Some(s) = line.strip_prefix("- [x] ") {
                (true, s)
            } else if let Some(s) = line.strip_prefix("- [X] ") {
                (true, s)
            } else if let Some(s) = line.strip_prefix("* [x] ") {
                (true, s)
            } else if let Some(s) = line.strip_prefix("* [X] ") {
                (true, s)
            } else {
                continue;
            };

            let todo_text = payload.trim();
            if todo_text.is_empty() {
                continue;
            }

            if let Some(existing_item) = self
                .session
                .todo_items
                .iter_mut()
                .find(|t| t.text.eq_ignore_ascii_case(todo_text))
            {
                existing_item.done = done;
                continue;
            }

            if existing.insert(todo_text.to_lowercase()) {
                self.session.todo_items.push(TodoItem {
                    text: todo_text.to_string(),
                    done,
                });
            }
        }
    }

    pub fn toggle_todo_selected(&mut self) {
        if let Some(item) = self.session.todo_items.get_mut(self.todo_cursor) {
            item.done = !item.done;
            self.session.updated_at = chrono::Utc::now();
            let _ = self.store.save(&self.session);
        }
    }

    pub fn delete_todo_selected(&mut self) {
        if self.todo_cursor < self.session.todo_items.len() {
            self.session.todo_items.remove(self.todo_cursor);
            self.todo_cursor = self.todo_cursor.saturating_sub(1);
            self.session.updated_at = chrono::Utc::now();
            let _ = self.store.save(&self.session);
        }
    }

    pub fn add_todo(&mut self, text: String) {
        let todo_text = text.trim();
        if todo_text.is_empty() {
            return;
        }
        if self
            .session
            .todo_items
            .iter()
            .any(|t| t.text.eq_ignore_ascii_case(todo_text))
        {
            return;
        }
        self.session.todo_items.push(TodoItem {
            text: todo_text.to_string(),
            done: false,
        });
        self.todo_cursor = self.session.todo_items.len().saturating_sub(1);
        self.session.updated_at = chrono::Utc::now();
        let _ = self.store.save(&self.session);
    }

    pub fn filtered_changed_files(&self) -> Vec<&str> {
        let needle = self.changed_files_filter.to_lowercase();
        self.session
            .changed_files
            .iter()
            .map(|s| s.as_str())
            .filter(|p| needle.is_empty() || p.to_lowercase().contains(&needle))
            .collect()
    }

    pub fn remove_changed_file_at_cursor(&mut self) {
        let filtered = self.filtered_changed_files();
        let Some(target) = filtered.get(self.changed_files_cursor).map(|s| s.to_string()) else { return; };
        self.session.changed_files.retain(|p| p != &target);
        self.changed_files_cursor = self
            .changed_files_cursor
            .min(self.filtered_changed_files().len().saturating_sub(1));
        self.session.updated_at = chrono::Utc::now();
        let _ = self.store.save(&self.session);
    }
}

fn format_error_payload(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(pretty) = try_pretty_json(trimmed) {
        return pretty;
    }
    if let Some(json_slice) = extract_first_json_value(trimmed) {
        if let Some(pretty) = try_pretty_json(json_slice) {
            return pretty;
        }
    }
    raw.to_string()
}

fn try_pretty_json(input: &str) -> Option<String> {
    let parsed = serde_json::from_str::<serde_json::Value>(input).ok()?;
    serde_json::to_string_pretty(&parsed).ok()
}

fn extract_first_json_value(input: &str) -> Option<&str> {
    let bytes = input.as_bytes();
    let mut start = None;
    let mut stack: Vec<u8> = Vec::new();
    let mut in_str = false;
    let mut escaped = false;

    for (i, &b) in bytes.iter().enumerate() {
        if in_str {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }

        if b == b'"' {
            in_str = true;
            continue;
        }

        if start.is_none() {
            if b == b'{' || b == b'[' {
                start = Some(i);
                stack.push(b);
            }
            continue;
        }

        match b {
            b'{' | b'[' => stack.push(b),
            b'}' => {
                if !matches!(stack.last(), Some(b'{')) {
                    return None;
                }
                stack.pop();
            }
            b']' => {
                if !matches!(stack.last(), Some(b'[')) {
                    return None;
                }
                stack.pop();
            }
            _ => {}
        }

        if stack.is_empty() {
            let s = start?;
            return input.get(s..=i);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::format_error_payload;

    #[test]
    fn formats_plain_json_error() {
        let raw = r#"{"error":{"message":"bad key","type":"invalid_request_error"}}"#;
        let out = format_error_payload(raw);
        assert!(out.contains("\n"));
        assert!(out.contains("\"message\": \"bad key\""));
    }

    #[test]
    fn formats_embedded_json_error() {
        let raw = r#"API stream error 401 Unauthorized: {"error":{"message":"no auth","code":"invalid_api_key"}}"#;
        let out = format_error_payload(raw);
        assert!(out.contains("\"code\": \"invalid_api_key\""));
        assert!(!out.contains("API stream error 401 Unauthorized"));
    }
}
