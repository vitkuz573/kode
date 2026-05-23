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
    session::{Session, SessionStore, TodoItem},
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
mod notify;
use notify::NotifySettings;

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

    /// Enable desktop notifications on completion/error
    #[arg(long, global = true, env = "KODE_NOTIFY", default_value_t = true)]
    notify: bool,
    /// Notify only on errors
    #[arg(long, global = true, env = "KODE_NOTIFY_ERRORS_ONLY", default_value_t = false)]
    notify_errors_only: bool,
    /// Notify on success only if response took at least this many milliseconds
    #[arg(long, global = true, env = "KODE_NOTIFY_MIN_MS", default_value_t = 0)]
    notify_min_ms: u64,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// List configured providers and models
    Models,
    /// List saved sessions
    Sessions {
        /// Filter by model substring (case-insensitive)
        #[arg(long)]
        model: Option<String>,
        /// Include sessions updated within the last N days
        #[arg(long)]
        since_days: Option<i64>,
    },
    /// Aggregated sessions report for dashboards/BI
    SessionsReport {
        /// Filter by model substring (case-insensitive)
        #[arg(long)]
        model: Option<String>,
        /// Include sessions updated within the last N days
        #[arg(long)]
        since_days: Option<i64>,
    },
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
    let notify_settings = NotifySettings {
        enabled: cli.notify,
        errors_only: cli.notify_errors_only,
        min_ms: cli.notify_min_ms,
    };

    match cli.command {
        Some(Commands::Models) => cmd_models(&config, cli.json),
        Some(Commands::Sessions { model, since_days }) => cmd_sessions(cli.json, model.as_deref(), since_days)?,
        Some(Commands::SessionsReport { model, since_days }) => {
            cmd_sessions_report(cli.json, model.as_deref(), since_days)?
        }
        Some(Commands::DeleteSession { id }) => cmd_delete_session(&id, cli.json)?,
        Some(Commands::Config) => cmd_config(cli.json)?,
        Some(Commands::Ask { prompt, agent }) => {
            run_oneshot(
                &config,
                &model,
                &prompt,
                agent || cli.agent,
                cli.system.as_deref(),
                cli.raw,
                notify_settings,
            ).await?
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
                run_oneshot(
                    &config,
                    &model,
                    &prompt,
                    cli.agent,
                    cli.system.as_deref(),
                    cli.raw,
                    notify_settings,
                ).await?;
            } else {
                // Interactive TUI
                tui_runner::run(config, model, notify_settings).await?;
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

fn cmd_sessions(json_output: bool, model_filter: Option<&str>, since_days: Option<i64>) -> Result<()> {
    let store = SessionStore::new()?;
    let mut sessions = store.list()?;

    if let Some(mf) = model_filter {
        let needle = mf.to_lowercase();
        sessions.retain(|s| s.model.to_lowercase().contains(&needle));
    }
    if let Some(days) = since_days {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(days.max(0));
        sessions.retain(|s| s.updated_at >= cutoff);
    }
    if json_output {
        let mut total_messages: usize = 0;
        let mut total_files_refs: usize = 0;
        let mut total_todo_open: usize = 0;
        let mut total_todo_done: usize = 0;
        let mut file_freq: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

        let payload = sessions
            .iter()
            .map(|s| {
                let todo_done = s.todo_items.iter().filter(|t| t.done).count();
                let todo_open = s.todo_items.len().saturating_sub(todo_done);
                total_messages += s.messages.len();
                total_files_refs += s.changed_files.len();
                total_todo_open += todo_open;
                total_todo_done += todo_done;
                for p in &s.changed_files {
                    *file_freq.entry(p.clone()).or_insert(0) += 1;
                }

                json!({
                    "id": s.id.to_string(),
                    "title": s.title.clone(),
                    "model": s.model.clone(),
                    "messages": s.messages.len(),
                    "changed_files": s.changed_files.clone(),
                    "todo_items": s.todo_items.clone(),
                    "todo_open": todo_open,
                    "todo_done": todo_done,
                    "created_at": s.created_at,
                    "updated_at": s.updated_at,
                    "total_cost_usd": s.total_cost_usd,
                    "total_tokens": s.total_tokens,
                })
            })
            .collect::<Vec<_>>();

        let mut top_files = file_freq.into_iter().collect::<Vec<_>>();
        top_files.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let top_files = top_files
            .into_iter()
            .take(20)
            .map(|(path, count)| json!({ "path": path, "count": count }))
            .collect::<Vec<_>>();

        println!(
            "{}",
            json!({
                "sessions": payload,
                "summary": {
                    "count": sessions.len(),
                    "messages_total": total_messages,
                    "changed_files_total": total_files_refs,
                    "todo_open_total": total_todo_open,
                    "todo_done_total": total_todo_done,
                    "top_changed_files": top_files,
                }
            })
        );
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
        let todo_done = s.todo_items.iter().filter(|t| t.done).count();
        let todo_open = s.todo_items.len().saturating_sub(todo_done);
        println!(
            "  {} {} {} {} {} {} {}",
            style(&s.id.to_string()[..8]).dim(),
            style(title).white(),
            style(format!("[{}]", s.model)).dim(),
            style(date.to_string()).dim(),
            style(format!("files:{}", s.changed_files.len())).dim(),
            style(format!("todo:{}/{}", todo_done, s.todo_items.len())).dim(),
            style(format!("open:{}", todo_open)).dim(),
        );
    }
    Ok(())
}

fn cmd_sessions_report(json_output: bool, model_filter: Option<&str>, since_days: Option<i64>) -> Result<()> {
    let store = SessionStore::new()?;
    let mut sessions = store.list()?;

    if let Some(mf) = model_filter {
        let needle = mf.to_lowercase();
        sessions.retain(|s| s.model.to_lowercase().contains(&needle));
    }
    if let Some(days) = since_days {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(days.max(0));
        sessions.retain(|s| s.updated_at >= cutoff);
    }

    let mut by_model: std::collections::HashMap<String, serde_json::Value> = std::collections::HashMap::new();
    let mut by_day: std::collections::HashMap<String, serde_json::Value> = std::collections::HashMap::new();
    let mut top_file_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut total_messages = 0usize;
    let mut total_files = 0usize;
    let mut total_todo_open = 0usize;
    let mut total_todo_done = 0usize;
    let mut total_cost = 0.0f64;
    let mut total_tokens = 0u64;

    for s in &sessions {
        let todo_done = s.todo_items.iter().filter(|t| t.done).count();
        let todo_open = s.todo_items.len().saturating_sub(todo_done);
        total_messages += s.messages.len();
        total_files += s.changed_files.len();
        total_todo_open += todo_open;
        total_todo_done += todo_done;
        total_cost += s.total_cost_usd;
        total_tokens += s.total_tokens;

        for p in &s.changed_files {
            *top_file_counts.entry(p.clone()).or_insert(0) += 1;
        }

        let model_entry = by_model
            .entry(s.model.clone())
            .or_insert_with(|| json!({
                "sessions": 0usize,
                "messages": 0usize,
                "files": 0usize,
                "todo_open": 0usize,
                "todo_done": 0usize,
                "cost_usd": 0.0f64,
                "tokens": 0u64,
            }));
        model_entry["sessions"] = json!(model_entry["sessions"].as_u64().unwrap_or(0) + 1);
        model_entry["messages"] = json!(model_entry["messages"].as_u64().unwrap_or(0) + s.messages.len() as u64);
        model_entry["files"] = json!(model_entry["files"].as_u64().unwrap_or(0) + s.changed_files.len() as u64);
        model_entry["todo_open"] = json!(model_entry["todo_open"].as_u64().unwrap_or(0) + todo_open as u64);
        model_entry["todo_done"] = json!(model_entry["todo_done"].as_u64().unwrap_or(0) + todo_done as u64);
        model_entry["cost_usd"] = json!(model_entry["cost_usd"].as_f64().unwrap_or(0.0) + s.total_cost_usd);
        model_entry["tokens"] = json!(model_entry["tokens"].as_u64().unwrap_or(0) + s.total_tokens);

        let day = s.updated_at.format("%Y-%m-%d").to_string();
        let day_entry = by_day.entry(day).or_insert_with(|| json!({
            "sessions": 0usize,
            "messages": 0usize,
            "files": 0usize,
            "todo_open": 0usize,
            "todo_done": 0usize,
            "cost_usd": 0.0f64,
            "tokens": 0u64,
        }));
        day_entry["sessions"] = json!(day_entry["sessions"].as_u64().unwrap_or(0) + 1);
        day_entry["messages"] = json!(day_entry["messages"].as_u64().unwrap_or(0) + s.messages.len() as u64);
        day_entry["files"] = json!(day_entry["files"].as_u64().unwrap_or(0) + s.changed_files.len() as u64);
        day_entry["todo_open"] = json!(day_entry["todo_open"].as_u64().unwrap_or(0) + todo_open as u64);
        day_entry["todo_done"] = json!(day_entry["todo_done"].as_u64().unwrap_or(0) + todo_done as u64);
        day_entry["cost_usd"] = json!(day_entry["cost_usd"].as_f64().unwrap_or(0.0) + s.total_cost_usd);
        day_entry["tokens"] = json!(day_entry["tokens"].as_u64().unwrap_or(0) + s.total_tokens);
    }

    let mut top_files = top_file_counts.into_iter().collect::<Vec<_>>();
    top_files.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let top_files = top_files
        .into_iter()
        .take(50)
        .map(|(path, count)| json!({ "path": path, "count": count }))
        .collect::<Vec<_>>();

    let mut by_model_vec = by_model.into_iter().collect::<Vec<_>>();
    by_model_vec.sort_by(|a, b| a.0.cmp(&b.0));
    let by_model_json = by_model_vec
        .into_iter()
        .map(|(model, stats)| json!({ "model": model, "stats": stats }))
        .collect::<Vec<_>>();

    let mut by_day_vec = by_day.into_iter().collect::<Vec<_>>();
    by_day_vec.sort_by(|a, b| a.0.cmp(&b.0));
    let by_day_json = by_day_vec
        .into_iter()
        .map(|(day, stats)| json!({ "day": day, "stats": stats }))
        .collect::<Vec<_>>();

    let report = json!({
        "summary": {
            "sessions": sessions.len(),
            "messages_total": total_messages,
            "changed_files_total": total_files,
            "todo_open_total": total_todo_open,
            "todo_done_total": total_todo_done,
            "cost_usd_total": total_cost,
            "tokens_total": total_tokens,
        },
        "by_model": by_model_json,
        "by_day": by_day_json,
        "top_changed_files": top_files,
        "filters": {
            "model": model_filter,
            "since_days": since_days,
        }
    });

    if json_output {
        println!("{report}");
    } else {
        println!("{}", style("Sessions Report").bold().cyan());
        println!(
            "  sessions:{} messages:{} files:{} todo_open:{} todo_done:{} cost:${:.5} tokens:{}",
            sessions.len(),
            total_messages,
            total_files,
            total_todo_open,
            total_todo_done,
            total_cost,
            total_tokens
        );
        println!("\n{}", style("By model:").bold());
        for row in report["by_model"].as_array().unwrap_or(&Vec::new()) {
            let model = row["model"].as_str().unwrap_or("-");
            let stats = &row["stats"];
            println!(
                "  {} sessions:{} messages:{} files:{} todo_open:{} todo_done:{}",
                model,
                stats["sessions"].as_u64().unwrap_or(0),
                stats["messages"].as_u64().unwrap_or(0),
                stats["files"].as_u64().unwrap_or(0),
                stats["todo_open"].as_u64().unwrap_or(0),
                stats["todo_done"].as_u64().unwrap_or(0),
            );
        }
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
    notify_settings: NotifySettings,
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
    let mut session = Session::new(model.to_string());
    let store = SessionStore::new()?;
    let mut assistant_text = String::new();

    // System prompt
    let sys = system
        .map(|s| s.to_string())
        .or_else(|| config.agent.system_prompt.clone())
        .unwrap_or_else(|| "You are kode, a helpful AI coding assistant.".into());
    messages.push(Message::system(sys));
    messages.push(Message::user(prompt));
    session.push(Message::user(prompt));
    let _ = store.save(&session);

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
    let mut completion_notified = false;
    let mut last_response_ms: Option<u64> = None;

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
                assistant_text.push_str(&text);
            }
            AgentEvent::ToolCallStart { name, arguments, .. } => {
                if !raw {
                    eprintln!("\n{} {}", style("⚙").yellow(), style(format!("calling: {}", name)).dim());
                }
                if name == "write_file" {
                    if let Some(path) = arguments.get("path").and_then(|v| v.as_str()) {
                        let path = path.trim();
                        if !path.is_empty() && !session.changed_files.iter().any(|p| p == path) {
                            session.changed_files.push(path.to_string());
                        }
                    }
                }
            }
            AgentEvent::ToolCallDone { id, name, output, is_error } => {
                if !raw {
                    let icon = if is_error { style("✗").red() } else { style("✓").green() };
                    let preview: String = output.lines().take(2).collect::<Vec<_>>().join(" | ");
                    eprintln!("{} {}: {}", icon, style(&name).dim(), style(preview).dim());
                }
                session.push(Message::tool_result(id, output));
            }
            AgentEvent::TurnDone { cost_summary, .. } => {
                if !raw && config.cost.show {
                    eprintln!("\n{}", style(&cost_summary).dim());
                }
                // Keep latest available response timing for notification.
                if let Some(ms) = parse_response_ms_from_cost_summary(&cost_summary) {
                    last_response_ms = Some(ms);
                }
            }
            AgentEvent::Done => {
                println!(); // final newline
                if !assistant_text.trim().is_empty() {
                    session.push(Message::assistant(assistant_text.clone()));
                    merge_todos_from_markdown(&mut session.todo_items, &assistant_text);
                }
                let _ = store.save(&session);
                if notify::should_notify(notify_settings, true, last_response_ms) {
                    notify::notify_completion(true, model, last_response_ms);
                }
                completion_notified = true;
            }
            AgentEvent::Error(e) => {
                runtime_error = Some(e.clone());
                if !assistant_text.trim().is_empty() {
                    session.push(Message::assistant(assistant_text.clone()));
                    merge_todos_from_markdown(&mut session.todo_items, &assistant_text);
                }
                let _ = store.save(&session);
                if notify::should_notify(notify_settings, false, last_response_ms) {
                    notify::notify_completion(false, model, last_response_ms);
                }
                completion_notified = true;
            }
        }
    }

    agent_handle
        .await
        .context("agent task failed to join")??;
    if let Some(e) = runtime_error {
        anyhow::bail!(e);
    }
    if !completion_notified && notify::should_notify(notify_settings, true, last_response_ms) {
        notify::notify_completion(true, model, last_response_ms);
    }
    Ok(())
}

fn merge_todos_from_markdown(target: &mut Vec<TodoItem>, text: &str) {
    for (done, todo_text) in parse_markdown_todos(text) {
        if let Some(existing) = target
            .iter_mut()
            .find(|t| t.text.eq_ignore_ascii_case(&todo_text))
        {
            existing.done = done;
            continue;
        }
        target.push(TodoItem { text: todo_text, done });
    }
}

fn parse_markdown_todos(text: &str) -> Vec<(bool, String)> {
    let mut out = Vec::new();
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
        out.push((done, todo_text.to_string()));
    }
    out
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

fn parse_response_ms_from_cost_summary(summary: &str) -> Option<u64> {
    // Optional best-effort parse for patterns like "... 123ms ...".
    let pos = summary.find("ms")?;
    let prefix = &summary[..pos];
    let digits_rev: String = prefix
        .chars()
        .rev()
        .skip_while(|c| c.is_whitespace())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits_rev.is_empty() {
        return None;
    }
    digits_rev.chars().rev().collect::<String>().parse::<u64>().ok()
}

impl ApiStyleLabel for kode_core::config::ProviderConfig {
    fn api_style_label(&self) -> &'static str {
        match &self.api_style {
            kode_core::config::ApiStyle::OpenAI => "openai",
            kode_core::config::ApiStyle::Anthropic => "anthropic",
        }
    }
}
