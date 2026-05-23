use crate::types::Message;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A single TODO item attached to a session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TodoItem {
    pub text: String,
    #[serde(default)]
    pub done: bool,
}

/// A single conversation session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: Uuid,
    pub title: Option<String>,
    pub model: String,
    pub messages: Vec<Message>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Accumulated cost in USD.
    pub total_cost_usd: f64,
    /// Total tokens used.
    pub total_tokens: u64,
    /// Files changed during the session.
    pub changed_files: Vec<String>,
    /// Rolling TODO list for the session.
    pub todo_items: Vec<TodoItem>,
}

impl Session {
    pub fn new(model: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            title: None,
            model: model.into(),
            messages: Vec::new(),
            created_at: now,
            updated_at: now,
            total_cost_usd: 0.0,
            total_tokens: 0,
            changed_files: Vec::new(),
            todo_items: Vec::new(),
        }
    }

    pub fn push(&mut self, msg: Message) {
        self.updated_at = Utc::now();
        self.messages.push(msg);
    }

    pub fn token_count_estimate(&self) -> usize {
        // rough estimate: 4 chars per token
        self.messages.iter().map(|m| m.content.len() / 4).sum()
    }
}

/// Persistent session store backed by SQLite at ~/.local/share/kode/sessions.db
pub struct SessionStore {
    db_path: std::path::PathBuf,
}

impl SessionStore {
    pub fn new() -> Result<Self> {
        let base = dirs::data_local_dir()
            .ok_or_else(|| anyhow::anyhow!("cannot determine data dir"))?
            .join("kode");
        std::fs::create_dir_all(&base)?;
        let db_path = base.join("sessions.db");

        let store = Self { db_path };
        store.init_schema()?;
        Ok(store)
    }

    pub fn save(&self, session: &Session) -> Result<()> {
        let conn = self.open_conn()?;
        let messages_json = serde_json::to_string(&session.messages)?;
        let changed_files_json = serde_json::to_string(&session.changed_files)?;
        let todo_items_json = serde_json::to_string(&session.todo_items)?;
        conn.execute(
            r#"
            INSERT INTO sessions (
                id, title, model, messages_json, created_at, updated_at,
                total_cost_usd, total_tokens, changed_files_json, todo_items_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                model = excluded.model,
                messages_json = excluded.messages_json,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at,
                total_cost_usd = excluded.total_cost_usd,
                total_tokens = excluded.total_tokens,
                changed_files_json = excluded.changed_files_json,
                todo_items_json = excluded.todo_items_json
            "#,
            params![
                session.id.to_string(),
                session.title,
                session.model,
                messages_json,
                session.created_at.to_rfc3339(),
                session.updated_at.to_rfc3339(),
                session.total_cost_usd,
                session.total_tokens as i64,
                changed_files_json,
                todo_items_json,
            ],
        )?;
        Ok(())
    }

    pub fn load(&self, id: Uuid) -> Result<Session> {
        let conn = self.open_conn()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
                id, title, model, messages_json, created_at, updated_at,
                total_cost_usd, total_tokens, changed_files_json, todo_items_json
            FROM sessions
            WHERE id = ?1
            "#,
        )?;

        let row = stmt
            .query_row(params![id.to_string()], |r| row_to_session(r))
            .optional()?;
        row.ok_or_else(|| anyhow::anyhow!("session not found: {}", id))
    }

    pub fn list(&self) -> Result<Vec<Session>> {
        let conn = self.open_conn()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
                id, title, model, messages_json, created_at, updated_at,
                total_cost_usd, total_tokens, changed_files_json, todo_items_json
            FROM sessions
            ORDER BY updated_at DESC
            "#,
        )?;

        let rows = stmt.query_map([], row_to_session)?;
        let mut sessions = Vec::new();
        for row in rows {
            sessions.push(row?);
        }
        Ok(sessions)
    }

    pub fn delete(&self, id: Uuid) -> Result<()> {
        let conn = self.open_conn()?;
        conn.execute("DELETE FROM sessions WHERE id = ?1", params![id.to_string()])?;
        Ok(())
    }

    fn open_conn(&self) -> Result<Connection> {
        Connection::open(&self.db_path)
            .with_context(|| format!("opening sqlite database {}", self.db_path.display()))
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.open_conn()?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                title TEXT,
                model TEXT NOT NULL,
                messages_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                total_cost_usd REAL NOT NULL DEFAULT 0,
                total_tokens INTEGER NOT NULL DEFAULT 0,
                changed_files_json TEXT NOT NULL DEFAULT '[]',
                todo_items_json TEXT NOT NULL DEFAULT '[]'
            );
            "#,
        )?;
        Ok(())
    }
}

fn row_to_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<Session> {
    let id_str: String = row.get(0)?;
    let created_at_str: String = row.get(4)?;
    let updated_at_str: String = row.get(5)?;
    let messages_json: String = row.get(3)?;
    let changed_files_json: String = row.get(8)?;
    let todo_items_json: String = row.get(9)?;

    let id = Uuid::parse_str(&id_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e))
        })?
        .with_timezone(&Utc);
    let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e))
        })?
        .with_timezone(&Utc);

    let messages: Vec<Message> = serde_json::from_str(&messages_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let changed_files: Vec<String> = serde_json::from_str(&changed_files_json).unwrap_or_default();
    let todo_items: Vec<TodoItem> = serde_json::from_str(&todo_items_json).unwrap_or_default();

    Ok(Session {
        id,
        title: row.get(1)?,
        model: row.get(2)?,
        messages,
        created_at,
        updated_at,
        total_cost_usd: row.get(6)?,
        total_tokens: row.get::<_, i64>(7)? as u64,
        changed_files,
        todo_items,
    })
}
