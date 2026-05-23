use anyhow::Result;
use clap::{Parser, Subcommand};
use console::style;
use kode_agent::{tools::default_registry, Agent, AgentEvent};
use kode_core::{
    config::Config,
    session::SessionStore,
    types::Message,
};
use kode_llm::ModelRouter;
use std::{io::IsTerminal, sync::Arc};
use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;

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
async fn main() -> Result<()> {
    // Init tracing (RUST_LOG=debug for verbose)
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .without_time()
        .init();

    let cli = Cli::parse();
    let config = Config::load().unwrap_or_else(|_| Config::default_config());
    let model = cli
        .model
        .clone()
        .or_else(|| config.model.clone())
        .unwrap_or_else(|| "omniroute/kr/auto".into());

    match cli.command {
        Some(Commands::Models) => cmd_models(&config),
        Some(Commands::Sessions) => cmd_sessions()?,
        Some(Commands::DeleteSession { id }) => cmd_delete_session(&id)?,
        Some(Commands::Config) => cmd_config(),
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
                    std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf)?;
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

fn cmd_models(config: &Config) {
    println!("{}", style("Available models:").bold().cyan());
    let router = ModelRouter::new(config.clone());
    for m in router.list_models() {
        println!("  {}", style(m).green());
    }
}

fn cmd_sessions() -> Result<()> {
    let store = SessionStore::new()?;
    let sessions = store.list()?;
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

fn cmd_delete_session(id: &str) -> Result<()> {
    let store = SessionStore::new()?;
    let sessions = store.list()?;
    let target = sessions
        .iter()
        .find(|s| s.id.to_string().starts_with(id))
        .ok_or_else(|| anyhow::anyhow!("session not found: {}", id))?;
    store.delete(target.id)?;
    println!("{}", style(format!("Deleted session {}", target.id)).green());
    Ok(())
}

fn cmd_config() {
    match Config::config_path() {
        Ok(p) => println!("{}", p.display()),
        Err(e) => eprintln!("error: {}", e),
    }
}

// ── One-shot / pipe mode ──────────────────────────────────────────────────────

async fn run_oneshot(
    config: &Config,
    model: &str,
    prompt: &str,
    _agent_mode: bool,
    system: Option<&str>,
    raw: bool,
) -> Result<()> {
    let router = Arc::new(ModelRouter::new(config.clone()));
    let tools = Arc::new(default_registry());

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

    // Print events to stdout
    while let Some(event) = rx.recv().await {
        match event {
            AgentEvent::ReasoningDelta(text) => {
                if !raw {
                    eprint!("{}", style(text).dim());
                    use std::io::Write;
                    std::io::stderr().flush().ok();
                }
            }
            AgentEvent::TextDelta(text) => {
                if raw {
                    print!("{}", text);
                } else {
                    print!("{}", text);
                }
                use std::io::Write;
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
                eprintln!("{} {}", style("error:").red().bold(), e);
            }
        }
    }

    agent_handle.await??;
    Ok(())
}
