use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use kode_agent::{tools::default_registry, Agent, AgentEvent};
use kode_core::{config::Config, types::Message};
use kode_llm::ModelRouter;
use kode_tui::{
    app::{App, ChatMessage, MsgRole},
    input::{handle_key, handle_mouse, InputAction},
    ui::draw,
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{io, sync::Arc, time::Duration};
use tokio::sync::mpsc;

pub async fn run(config: Config, model: String) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_inner(&mut terminal, config, model).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

async fn run_inner(
    terminal: &mut ratatui::Terminal<CrosstermBackend<io::Stdout>>,
    config: Config,
    model: String,
) -> Result<()> {
    let mut app = App::new(config.clone(), model)?;

    // Welcome message
    app.chat_messages.push(ChatMessage {
        role: MsgRole::System,
        content: format!(
            "kode v{}  ·  {}  ·  ^P: palette  ^T: theme  ^M: model  ^B: sidebar  Tab: sessions",
            env!("CARGO_PKG_VERSION"),
            app.model
        ),
        reasoning: String::new(),
        reasoning_collapsed: true,
        timestamp: App::now_str(),
        tool_calls: Vec::new(),
        is_streaming: false,
    });

    // Load sessions into sidebar on start
    if let Ok(sessions) = app.store.list() {
        app.sessions = sessions;
    }

    let (agent_tx, mut agent_rx) = mpsc::channel::<AgentEvent>(256);
    // Channel for background model discovery
    let (models_tx, mut models_rx) = mpsc::channel::<Vec<String>>(4);

    loop {
        app.tick_spinner();
        terminal.draw(|f| draw(f, &app))?;

        // Drain agent events
        while let Ok(event) = agent_rx.try_recv() {
            app.handle_agent_event(event);
        }

        // Drain model discovery results
        while let Ok(models) = models_rx.try_recv() {
            app.model_list = models;
            app.models_loading = false;
        }

        // Poll events at 50ms (~20fps)
        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) => {
                    match handle_key(&mut app, key) {
                        InputAction::Quit => break,
                        InputAction::Submit(prompt) => {
                            submit_message(&mut app, &config, &agent_tx, prompt).await;
                        }
                        InputAction::RefreshModels => {
                            app.models_loading = true;
                            let cfg = config.clone();
                            let tx = models_tx.clone();
                            tokio::spawn(async move {
                                let router = ModelRouter::new(cfg);
                                let models = router.discover_models().await;
                                let _ = tx.send(models).await;
                            });
                        }
                        InputAction::None => {}
                    }
                }
                Event::Mouse(mouse) => {
                    handle_mouse(&mut app, mouse);
                }
                Event::Resize(_, _) => {
                    // ratatui handles resize automatically
                }
                _ => {}
            }
        }
    }

    Ok(())
}

async fn submit_message(
    app: &mut App,
    config: &Config,
    agent_tx: &mpsc::Sender<AgentEvent>,
    prompt: String,
) {
    app.push_user_message(&prompt);
    app.messages.push(Message::user(&prompt));
    app.session.push(Message::user(&prompt));
    app.thinking = true;
    app.begin_assistant_message();

    let messages = app.messages.clone();
    let router = Arc::new(ModelRouter::new(config.clone()));
    let tools = Arc::new(default_registry());
    let agent_config = config.agent.clone();
    let model_id = app.model.clone();
    let tx = agent_tx.clone();

    tokio::spawn(async move {
        let mut msgs = messages;
        let mut agent = Agent::new(router, tools, agent_config, model_id);
        if let Err(e) = agent.run(&mut msgs, tx.clone()).await {
            let _ = tx.send(AgentEvent::Error(e.to_string())).await;
        }
    });
}
