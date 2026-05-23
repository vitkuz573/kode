use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use crate::types::Message;

/// A single conversation session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: Uuid,
    pub title: Option<String>,
    pub model: String,
    pub messages: Vec<Message>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Accumulated cost in USD
    pub total_cost_usd: f64,
    /// Total tokens used
    pub total_tokens: u64,
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

/// Persistent session store backed by ~/.local/share/kode/sessions/
pub struct SessionStore {
    dir: std::path::PathBuf,
}

impl SessionStore {
    pub fn new() -> anyhow::Result<Self> {
        let dir = dirs::data_local_dir()
            .ok_or_else(|| anyhow::anyhow!("cannot determine data dir"))?
            .join("kode")
            .join("sessions");
        std::fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    pub fn save(&self, session: &Session) -> anyhow::Result<()> {
        let path = self.dir.join(format!("{}.json", session.id));
        let raw = serde_json::to_string_pretty(session)?;
        std::fs::write(path, raw)?;
        Ok(())
    }

    pub fn load(&self, id: Uuid) -> anyhow::Result<Session> {
        let path = self.dir.join(format!("{}.json", id));
        let raw = std::fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&raw)?)
    }

    pub fn list(&self) -> anyhow::Result<Vec<Session>> {
        let mut sessions = Vec::new();
        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            if entry.path().extension().map_or(false, |e| e == "json") {
                let raw = std::fs::read_to_string(entry.path())?;
                if let Ok(s) = serde_json::from_str::<Session>(&raw) {
                    sessions.push(s);
                }
            }
        }
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(sessions)
    }

    pub fn delete(&self, id: Uuid) -> anyhow::Result<()> {
        let path = self.dir.join(format!("{}.json", id));
        std::fs::remove_file(path)?;
        Ok(())
    }
}
