use anyhow::Result;
use async_trait::async_trait;
use kode_core::types::ToolCall;
use serde_json::Value;
use std::collections::HashMap;

/// A single tool the agent can call
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> Value;
    async fn call(&self, args: Value) -> Result<String>;
}

/// Registry of all available tools
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self { tools: HashMap::new() }
    }

    pub fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    pub fn definitions(&self) -> Vec<kode_llm::client::ToolDefinition> {
        self.tools
            .values()
            .map(|t| kode_llm::client::ToolDefinition {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters: t.parameters_schema(),
            })
            .collect()
    }

    pub async fn execute(&self, call: &ToolCall) -> kode_core::types::ToolResult {
        match self.tools.get(&call.name) {
            Some(tool) => match tool.call(call.arguments.clone()).await {
                Ok(output) => kode_core::types::ToolResult {
                    call_id: call.id.clone(),
                    output,
                    is_error: false,
                },
                Err(e) => kode_core::types::ToolResult {
                    call_id: call.id.clone(),
                    output: format!("Error: {}", e),
                    is_error: true,
                },
            },
            None => kode_core::types::ToolResult {
                call_id: call.id.clone(),
                output: format!("Unknown tool: {}", call.name),
                is_error: true,
            },
        }
    }
}

// ── Built-in tools ────────────────────────────────────────────────────────────

pub struct ReadFileTool;
pub struct WriteFileTool;
pub struct BashTool;
pub struct ListDirTool;
pub struct GlobTool;

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str { "read_file" }
    fn description(&self) -> &str { "Read the contents of a file at the given path" }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Absolute or relative file path" },
                "offset": { "type": "integer", "description": "Line number to start from (1-indexed)" },
                "limit": { "type": "integer", "description": "Max lines to read" }
            },
            "required": ["path"]
        })
    }
    async fn call(&self, args: Value) -> Result<String> {
        let path = args["path"].as_str().ok_or_else(|| anyhow::anyhow!("missing path"))?;
        let content = tokio::fs::read_to_string(path).await?;
        let offset = args["offset"].as_u64().unwrap_or(1).saturating_sub(1) as usize;
        let limit = args["limit"].as_u64().unwrap_or(2000) as usize;
        let lines: Vec<&str> = content.lines().collect();
        let slice = &lines[offset.min(lines.len())..];
        let slice = &slice[..limit.min(slice.len())];
        Ok(slice
            .iter()
            .enumerate()
            .map(|(i, l)| format!("{}: {}", offset + i + 1, l))
            .collect::<Vec<_>>()
            .join("\n"))
    }
}

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str { "write_file" }
    fn description(&self) -> &str { "Write content to a file, creating it if it doesn't exist" }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "content": { "type": "string" }
            },
            "required": ["path", "content"]
        })
    }
    async fn call(&self, args: Value) -> Result<String> {
        let path = args["path"].as_str().ok_or_else(|| anyhow::anyhow!("missing path"))?;
        let content = args["content"].as_str().ok_or_else(|| anyhow::anyhow!("missing content"))?;
        if let Some(parent) = std::path::Path::new(path).parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(path, content).await?;
        Ok(format!("Written {} bytes to {}", content.len(), path))
    }
}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str { "bash" }
    fn description(&self) -> &str { "Execute a bash command and return stdout+stderr" }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "Shell command to run" },
                "timeout_secs": { "type": "integer", "description": "Timeout in seconds (default 30)" }
            },
            "required": ["command"]
        })
    }
    async fn call(&self, args: Value) -> Result<String> {
        let cmd = args["command"].as_str().ok_or_else(|| anyhow::anyhow!("missing command"))?;
        let timeout = args["timeout_secs"].as_u64().unwrap_or(30);
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(timeout),
            tokio::process::Command::new("bash")
                .arg("-c")
                .arg(cmd)
                .output(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("command timed out after {}s", timeout))??;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit = output.status.code().unwrap_or(-1);

        let mut result = String::new();
        if !stdout.is_empty() { result.push_str(&stdout); }
        if !stderr.is_empty() {
            if !result.is_empty() { result.push('\n'); }
            result.push_str("[stderr]\n");
            result.push_str(&stderr);
        }
        if exit != 0 {
            result.push_str(&format!("\n[exit {}]", exit));
        }
        Ok(result)
    }
}

#[async_trait]
impl Tool for ListDirTool {
    fn name(&self) -> &str { "list_dir" }
    fn description(&self) -> &str { "List files and directories at a path" }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Directory path" }
            },
            "required": ["path"]
        })
    }
    async fn call(&self, args: Value) -> Result<String> {
        let path = args["path"].as_str().unwrap_or(".");
        let mut entries = tokio::fs::read_dir(path).await?;
        let mut lines = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let ft = entry.file_type().await?;
            let suffix = if ft.is_dir() { "/" } else { "" };
            lines.push(format!("{}{}", entry.file_name().to_string_lossy(), suffix));
        }
        lines.sort();
        Ok(lines.join("\n"))
    }
}

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str { "glob" }
    fn description(&self) -> &str { "Find files matching a glob pattern" }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "Glob pattern, e.g. src/**/*.rs" },
                "base": { "type": "string", "description": "Base directory (default: cwd)" }
            },
            "required": ["pattern"]
        })
    }
    async fn call(&self, args: Value) -> Result<String> {
        let pattern = args["pattern"].as_str().ok_or_else(|| anyhow::anyhow!("missing pattern"))?;
        let base = args["base"].as_str().unwrap_or(".");
        let full_pattern = format!("{}/{}", base.trim_end_matches('/'), pattern);
        let paths = glob::glob(&full_pattern)
            .map_err(|e| anyhow::anyhow!("invalid glob: {}", e))?
            .filter_map(|p| p.ok())
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>();
        if paths.is_empty() {
            Ok("No files found".into())
        } else {
            Ok(paths.join("\n"))
        }
    }
}

/// Build the default tool registry with all built-in tools
pub fn default_registry() -> ToolRegistry {
    let mut reg = ToolRegistry::new();
    reg.register(Box::new(ReadFileTool));
    reg.register(Box::new(WriteFileTool));
    reg.register(Box::new(BashTool));
    reg.register(Box::new(ListDirTool));
    reg.register(Box::new(GlobTool));
    reg
}
