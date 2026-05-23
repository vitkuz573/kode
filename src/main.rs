use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use console::style;
use kode_agent::{
    tools::{default_registry, ToolRegistry},
    Agent,
    AgentEvent,
};
use kode_core::{
    config::Config,
    session::SessionStore,
    types::Message,
};
use kode_llm::ModelRouter;
use std::{
    io::{IsTerminal, Read, Write},
    sync::Arc,
};
use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;
use serde_json::json;

mod tui_runner;

#[derive(Parser)]
#[command(
    name = "kode",
    version = env!("CARGO_PKG_VERSION"),
    about = "kode — universal AI coding CLI with multi-model routing, TUI, and agent mode",
    long_about = None,
)]
struct Cli {
    /// Model to use, e.g. "omniroute/kr/auto"
    #[arg(short, long, env = "KODE_MODEL")]
    model: Option<String>,

    /// Prompt (non-interactive / pipe mode)
    #[arg(short, long)]
    prompt: Option<String>,

    /// Run in agent mode (enables tools)
    #[arg(short, long)]
    agent: bool,

    /// System prompt override
    #[arg(short, long)]
    system: Option<String>,

    /// Output raw text only (no formatting)
    #[arg(long)]
    raw: bool,

    /// Output machine-readable JSON (subcommands only)
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// List configured providers and models
    Models,
    /// List saved sessions
    Sessions,
    /// Delete a session by ID
    DeleteSession { id: String },
    /// Show current config path
    Config,
    /// Run a one-shot prompt (alias for -p)
    Ask {
        /// The prompt text
        prompt: String,
        /// Run as agent with tools
        #[arg(short, long)]
        agent: bool,
    },
}

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!(
            "{} {}",
            style("error:").red().bold(),
            format_error_for_cli(&err.to_string())
        );
        std::process::exit(exit_code_for_error(&err));
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(&cli);
    let config = Config::load().unwrap_or_else(|_| Config::default_config());
    let model = cli
        .model
        .clone()
        .or_else(|| config.model.clone())
        .unwrap_or_else(|| "omniroute/kr/auto".into());

    match cli.command {
        Some(Commands::Models) => cmd_models(&config, cli.json),
        Some(Commands::Sessions) => cmd_sessions(cli.json)?,
        Some(Commands::DeleteSession { id }) => cmd_delete_session(&id, cli.json)?,
        Some(Commands::Config) => cmd_config(cli.json)?,
        Some(Commands::Ask { prompt, agent }) => {
            run_oneshot(&config, &model, &prompt, agent || cli.agent, cli.system.as_deref(), cli.raw).await?
        }
        None => {
            // Check for piped input
            let piped = !std::io::stdin().is_terminal();
            if piped || cli.prompt.is_some() {
                let prompt = if let Some(p) = cli.prompt {
                    p
                } else {
                    let mut buf = String::new();
                    std::io::stdin().read_to_string(&mut buf)?;
                    buf.trim().to_string()
                };
                run_oneshot(&config, &model, &prompt, cli.agent, cli.system.as_deref(), cli.raw).await?;
            } else {
                // Interactive TUI
                tui_runner::run(config, model).await?;
            }
        }
    }

    Ok(())
}

// ── Subcommands ───────────────────────────────────────────────────────────────

fn cmd_models(config: &Config, json_output: bool) {
    if config.providers.is_empty() {
        if json_output {
            println!("{}", json!({
                "providers": [],
                "discovered_total": 0usize,
            }));
            return;
        }
        println!("{}", style("No providers configured in config.toml").yellow());
        return;
    }

    if json_output {
        let mut provider_ids = config.providers.keys().cloned().collect::<Vec<_>>();
        provider_ids.sort();
        let providers = provider_ids
            .iter()
            .filter_map(|provider_id| config.providers.get(provider_id).map(|provider| (provider_id, provider)))
            .map(|(provider_id, provider)| {
                json!({
                    "id": provider_id,
                    "name": provider.name.as_deref().unwrap_or(provider_id),
                    "api_style": provider.api_style_label(),
                    "base_url": provider.base_url,
                    "models": provider.models,
                })
            })
            .collect::<Vec<_>>();
        let router = ModelRouter::new(config.clone());
        println!(
            "{}",
            json!({
                "providers": providers,
                "discovered_total": router.list_models().len(),
            })
        );
        return;
    }

    println!("{}", style("Configured providers and models").bold().cyan());
    let mut provider_ids = config.providers.keys().cloned().collect::<Vec<_>>();
    provider_ids.sort();

    for provider_id in provider_ids {
        if let Some(provider) = config.providers.get(&provider_id) {
            let provider_name = provider.name.as_deref().unwrap_or(&provider_id);
            let model_count = provider.models.len();
            println!(
                "\n{} {} {}",
                style("•").cyan(),
                style(provider_name).bold(),
                style(format!("({}/{}, {} models)", provider_id, provider.api_style_label(), model_count)).dim()
            );
            if provider.models.is_empty() {
                println!("  {}", style("no static models configured").dim());
            } else {
                for model in &provider.models {
                    println!("  {}", style(format!("{}/{}", provider_id, model)).green());
                }
            }
        }
    }

    let router = ModelRouter::new(config.clone());
    println!(
        "\n{} {}",
        style("Discovered total:").dim(),
        style(router.list_models().len()).bold()
    );
}

fn cmd_sessions(json_output: bool) -> Result<()> {
    let store = SessionStore::new()?;
    let sessions = store.list()?;
    if json_output {
        let payload = sessions
            .into_iter()
            .map(|s| {
                json!({
                    "id": s.id.to_string(),
                    "title": s.title,
                    "model": s.model,
                    "messages": s.messages.len(),
                    "created_at": s.created_at,
                    "updated_at": s.updated_at,
                    "total_cost_usd": s.total_cost_usd,
                    "total_tokens": s.total_tokens,
                })
            })
            .collect::<Vec<_>>();
        println!("{}", json!({ "sessions": payload }));
        return Ok(());
    }

    if sessions.is_empty() {
        println!("{}", style("No saved sessions.").dim());
        return Ok(());
    }
    println!("{}", style("Saved sessions:").bold().cyan());
    for s in sessions {
        let title = s.title.as_deref().unwrap_or("untitled");
        let date = s.updated_at.format("%Y-%m-%d %H:%M");
        println!(
            "  {} {} {} {}",
            style(&s.id.to_string()[..8]).dim(),
            style(title).white(),
            style(format!("[{}]", s.model)).dim(),
            style(date.to_string()).dim()
        );
    }
    Ok(())
}

fn cmd_delete_session(id: &str, json_output: bool) -> Result<()> {
    let store = SessionStore::new()?;
    let sessions = store.list()?;
    let target = sessions
        .iter()
        .find(|s| s.id.to_string().starts_with(id))
        .ok_or_else(|| anyhow::anyhow!("session not found: {}", id))?;
    store.delete(target.id)?;
    if json_output {
        println!(
            "{}",
            json!({
                "deleted": true,
                "id": target.id.to_string(),
            })
        );
        return Ok(());
    }
    println!("{}", style(format!("Deleted session {}", target.id)).green());
    Ok(())
}

fn cmd_config(json_output: bool) -> Result<()> {
    match Config::config_path() {
        Ok(p) => {
            if json_output {
                println!("{}", json!({ "config_path": p.display().to_string() }));
            } else {
                println!("{}", style(p.display()).cyan());
            }
            Ok(())
        }
        Err(e) => Err(e),
    }
}

// ── One-shot / pipe mode ──────────────────────────────────────────────────────

async fn run_oneshot(
    config: &Config,
    model: &str,
    prompt: &str,
    agent_mode: bool,
    system: Option<&str>,
    raw: bool,
) -> Result<()> {
    if prompt.trim().is_empty() {
        anyhow::bail!("prompt is empty");
    }

    let router = Arc::new(ModelRouter::new(config.clone()));
    let tools = if agent_mode {
        Arc::new(default_registry())
    } else {
        Arc::new(ToolRegistry::new())
    };

    let mut messages: Vec<Message> = Vec::new();

    // System prompt
    let sys = system
        .map(|s| s.to_string())
        .or_else(|| config.agent.system_prompt.clone())
        .unwrap_or_else(|| "You are kode, a helpful AI coding assistant.".into());
    messages.push(Message::system(sys));
    messages.push(Message::user(prompt));

    let (tx, mut rx) = mpsc::channel::<AgentEvent>(256);

    let mut agent = Agent::new(
        router,
        tools,
        config.agent.clone(),
        model.to_string(),
    );

    let agent_handle = tokio::spawn(async move {
        agent.run(&mut messages, tx).await
    });
    let mut runtime_error: Option<String> = None;

    // Print events to stdout
    while let Some(event) = rx.recv().await {
        match event {
            AgentEvent::ReasoningDelta(text) => {
                if !raw {
                    eprint!("{}", style(text).dim());
                    std::io::stderr().flush().ok();
                }
            }
            AgentEvent::TextDelta(text) => {
                print!("{}", text);
                std::io::stdout().flush().ok();
            }
            AgentEvent::ToolCallStart { name, .. } => {
                if !raw {
                    eprintln!("\n{} {}", style("⚙").yellow(), style(format!("calling: {}", name)).dim());
                }
            }
            AgentEvent::ToolCallDone { name, output, is_error, .. } => {
                if !raw {
                    let icon = if is_error { style("✗").red() } else { style("✓").green() };
                    let preview: String = output.lines().take(2).collect::<Vec<_>>().join(" | ");
                    eprintln!("{} {}: {}", icon, style(&name).dim(), style(preview).dim());
                }
            }
            AgentEvent::TurnDone { cost_summary, .. } => {
                if !raw && config.cost.show {
                    eprintln!("\n{}", style(cost_summary).dim());
                }
            }
            AgentEvent::Done => {
                println!(); // final newline
            }
            AgentEvent::Error(e) => {
                runtime_error = Some(e.clone());
            }
        }
    }

    agent_handle
        .await
        .context("agent task failed to join")??;
    if let Some(e) = runtime_error {
        anyhow::bail!(e);
    }
    Ok(())
}

trait ApiStyleLabel {
    fn api_style_label(&self) -> &'static str;
}

fn init_tracing(cli: &Cli) {
    let is_interactive_tui = cli.command.is_none() && cli.prompt.is_none() && std::io::stdin().is_terminal();
    let rust_log_set = std::env::var_os("RUST_LOG").is_some();
    if is_interactive_tui && !rust_log_set {
        return;
    }

    let default_filter = if is_interactive_tui { "warn" } else { "info" };
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_filter));

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .without_time()
        .with_writer(std::io::stderr)
        .init();
}

fn exit_code_for_error(err: &anyhow::Error) -> i32 {
    let msg = err.to_string().to_lowercase();
    if msg.contains("prompt is empty") || msg.contains("session not found") {
        return 2;
    }
    if msg.contains("config") || msg.contains("cannot determine config dir") {
        return 3;
    }
    if msg.contains("timed out")
        || msg.contains("dns")
        || msg.contains("connection")
        || msg.contains("http")
    {
        return 4;
    }
    1
}

fn format_error_for_cli(raw: &str) -> String {
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

impl ApiStyleLabel for kode_core::config::ProviderConfig {
    fn api_style_label(&self) -> &'static str {
        match &self.api_style {
            kode_core::config::ApiStyle::OpenAI => "openai",
            kode_core::config::ApiStyle::Anthropic => "anthropic",
        }
    }
}
