use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, List, ListItem, Paragraph,
        Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
    },
    Frame,
};

use crate::app::{App, AppMode, ChatMessage, MsgRole, ToolStatus};
use crate::markdown;
use crate::theme::Theme;

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.size();
    let t = &app.theme;
    f.render_widget(Block::default().style(Style::default().bg(t.base)), area);

    match app.mode {
        AppMode::Chat => draw_chat(f, app),
        AppMode::SessionList => {
            draw_chat(f, app);
            draw_sessions_overlay(f, app);
        }
        AppMode::ModelPicker => {
            draw_chat(f, app);
            draw_model_overlay(f, app);
        }
        AppMode::ThemePicker => {
            draw_chat(f, app);
            draw_theme_overlay(f, app);
        }
        AppMode::CommandPalette => {
            draw_chat(f, app);
            draw_command_palette(f, app);
        }
        AppMode::TodoManager => {
            draw_chat(f, app);
            draw_todo_overlay(f, app);
        }
        AppMode::ChangedFilesManager => {
            draw_chat(f, app);
            draw_changed_files_overlay(f, app);
        }
    }
}

// ── Main chat layout ──────────────────────────────────────────────────────────

fn draw_chat(f: &mut Frame, app: &App) {
    let area = f.size();

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    draw_titlebar(f, app, outer[0]);
    draw_statusbar(f, app, outer[2]);

    let body = if app.sidebar_visible {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(28), Constraint::Min(0)])
            .split(outer[1])
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0)])
            .split(outer[1])
    };

    if app.sidebar_visible {
        draw_sidebar(f, app, body[0]);
        draw_chat_panel(f, app, body[1]);
    } else {
        draw_chat_panel(f, app, body[0]);
    }
}

fn draw_titlebar(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let mut spans = vec![
        Span::styled(
            " kode ",
            Style::default().fg(t.base).bg(t.accent).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" v{} ", env!("CARGO_PKG_VERSION")),
            Style::default().fg(t.overlay0).bg(t.mantle),
        ),
        Span::styled(
            format!(" {} ", t.name),
            Style::default().fg(t.overlay0).bg(t.mantle),
        ),
        Span::styled(
            if app.auto_scroll { " follow " } else { " paused " },
            Style::default()
                .fg(t.base)
                .bg(if app.auto_scroll { t.green } else { t.yellow })
                .add_modifier(Modifier::BOLD),
        ),
    ];

    if app.thinking {
        spans.push(Span::styled("  ", Style::default().bg(t.mantle)));
        spans.push(Span::styled(
            app.spinner(),
            Style::default().fg(t.yellow).bg(t.mantle).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            " waiting response…",
            Style::default().fg(t.yellow).bg(t.mantle),
        ));
    }

    if app.models_loading {
        spans.push(Span::styled(
            "  ⟳ loading models…",
            Style::default().fg(t.sapphire).bg(t.mantle),
        ));
    }

    let keybinds = " ^P palette  ^B sidebar  Tab sessions  ^K model  ^T theme  ^Y todo  ^F files  ↑↓ history  ^C quit ";
    let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
    let pad = (area.width as usize).saturating_sub(used + keybinds.chars().count());
    spans.push(Span::styled(
        " ".repeat(pad),
        Style::default().bg(t.mantle),
    ));
    spans.push(Span::styled(
        keybinds,
        Style::default().fg(t.surface2).bg(t.mantle),
    ));

    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(t.mantle)),
        area,
    );
}

fn draw_statusbar(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let model_part = format!(" {} ", truncate(&app.model, 42));
    let mode_part = if app.thinking { " waiting " } else { " idle " };
    let stats = format!(
        " ↑{} ↓{}  ${:.5}  {}ms  {} msgs ",
        app.total_prompt_tokens,
        app.total_completion_tokens,
        app.total_cost_usd,
        app.last_response_ms,
        app.chat_messages.len(),
    );
    let pad = (area.width as usize)
        .saturating_sub(model_part.chars().count() + mode_part.chars().count() + stats.chars().count());

    let line = Line::from(vec![
        Span::styled(
            &model_part,
            Style::default().fg(t.base).bg(t.blue).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            mode_part,
            Style::default()
                .fg(t.base)
                .bg(if app.thinking { t.yellow } else { t.green })
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ".repeat(pad), Style::default().bg(t.crust)),
        Span::styled(&stats, Style::default().fg(t.overlay1).bg(t.crust)),
    ]);
    f.render_widget(
        Paragraph::new(line).style(Style::default().bg(t.crust)),
        area,
    );
}

fn draw_sidebar(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let sidebar_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(8),
            Constraint::Length(10),
            Constraint::Length(10),
        ])
        .split(area);

    let sessions = &app.sessions;
    let items: Vec<ListItem> = if sessions.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "  no sessions yet",
            Style::default().fg(t.overlay0),
        )))]
    } else {
        sessions.iter().enumerate().map(|(i, s)| {
            let title = s.title.as_deref().unwrap_or("untitled");
            let date = s.updated_at.format("%m-%d %H:%M").to_string();
            let sel = i == app.session_cursor;
            let bg = if sel { t.surface0 } else { t.mantle };
            let fg = if sel { t.accent2 } else { t.subtext0 };
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(if sel { " ▶ " } else { "   " }, Style::default().fg(t.accent).bg(bg)),
                    Span::styled(
                        truncate(title, 20),
                        Style::default().fg(fg).bg(bg)
                            .add_modifier(if sel { Modifier::BOLD } else { Modifier::empty() }),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("   ", Style::default().bg(bg)),
                    Span::styled(date, Style::default().fg(t.overlay0).bg(bg)),
                ]),
            ])
        }).collect()
    };

    let block = Block::default()
        .borders(Borders::RIGHT | Borders::BOTTOM)
        .border_style(Style::default().fg(t.surface0))
        .title(Span::styled(" sessions ", Style::default().fg(t.overlay1)))
        .style(Style::default().bg(t.mantle));

    f.render_widget(List::new(items).block(block), sidebar_chunks[0]);
    draw_changed_files_panel(f, app, sidebar_chunks[1]);
    draw_todo_panel(f, app, sidebar_chunks[2]);
}

fn draw_changed_files_panel(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let mut lines: Vec<Line> = Vec::new();
    if app.session.changed_files.is_empty() {
        lines.push(Line::from(Span::styled("  none", Style::default().fg(t.overlay0))));
    } else {
        for p in app.session.changed_files.iter().rev().take(4) {
            lines.push(Line::from(vec![
                Span::styled("  • ", Style::default().fg(t.sapphire)),
                Span::styled(truncate(p, area.width.saturating_sub(6) as usize), Style::default().fg(t.subtext1)),
            ]));
        }
    }

    let block = Block::default()
        .borders(Borders::RIGHT | Borders::BOTTOM)
        .border_style(Style::default().fg(t.surface0))
        .title(Span::styled(
            format!(" changed files ({}) ", app.session.changed_files.len()),
            Style::default().fg(t.overlay1),
        ))
        .style(Style::default().bg(t.mantle));

    f.render_widget(
        Paragraph::new(lines)
            .block(block)
            .style(Style::default().bg(t.mantle))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_todo_panel(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let done = app.session.todo_items.iter().filter(|x| x.done).count();
    let mut lines: Vec<Line> = Vec::new();

    if app.session.todo_items.is_empty() {
        lines.push(Line::from(Span::styled("  none", Style::default().fg(t.overlay0))));
    } else {
        for item in app.session.todo_items.iter().take(4) {
            let marker = if item.done { "[x]" } else { "[ ]" };
            let color = if item.done { t.green } else { t.yellow };
            lines.push(Line::from(vec![
                Span::styled(format!("  {} ", marker), Style::default().fg(color)),
                Span::styled(
                    truncate(&item.text, area.width.saturating_sub(8) as usize),
                    Style::default().fg(t.subtext1),
                ),
            ]));
        }
    }

    let block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(Style::default().fg(t.surface0))
        .title(Span::styled(
            format!(" todo ({}/{}) ", done, app.session.todo_items.len()),
            Style::default().fg(t.overlay1),
        ))
        .style(Style::default().bg(t.mantle));

    f.render_widget(
        Paragraph::new(lines)
            .block(block)
            .style(Style::default().bg(t.mantle))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_chat_panel(f: &mut Frame, app: &App, area: Rect) {
    let inner_w = area.width.saturating_sub(4) as usize;
    let input_h = input_height(&app.input, inner_w).max(1) as u16 + 2;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(input_h)])
        .split(area);

    draw_messages(f, app, chunks[0]);
    draw_input(f, app, chunks[1]);
}

fn draw_messages(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let inner_w = area.width.saturating_sub(4) as usize;
    let inner_h = area.height.saturating_sub(2) as usize;

    let mut all_lines: Vec<Line<'static>> = Vec::new();
    for msg in &app.chat_messages {
        render_message(&mut all_lines, msg, inner_w, t);
    }

    let total = all_lines.len();
    let scroll_offset = if app.auto_scroll {
        total.saturating_sub(inner_h)
    } else {
        app.scroll.min(total.saturating_sub(inner_h))
    };

    let visible: Vec<Line<'static>> = all_lines
        .into_iter()
        .skip(scroll_offset)
        .take(inner_h)
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.surface0))
        .style(Style::default().bg(t.base));

    f.render_widget(
        Paragraph::new(visible)
            .block(block)
            .style(Style::default().bg(t.base))
            .wrap(Wrap { trim: false }),
        area,
    );

    if total > inner_h {
        let mut sb = ScrollbarState::new(total.saturating_sub(inner_h)).position(scroll_offset);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(t.surface1)),
            area,
            &mut sb,
        );
    }
}

fn draw_input(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let (border_color, title) = if app.thinking {
        (t.yellow, format!(" {} waiting response… ", app.spinner()))
    } else {
        (t.accent, " message  Enter send  Shift+Enter newline  ↑↓ history  PgUp/PgDn scroll  ^P palette ".to_string())
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(&title, Style::default().fg(border_color)))
        .style(Style::default().bg(t.mantle));

    let input_value = if app.input.is_empty() && !app.thinking {
        "Type a request…".to_string()
    } else {
        app.input.clone()
    };
    let input_style = if app.input.is_empty() && !app.thinking {
        Style::default().fg(t.overlay0).bg(t.mantle)
    } else {
        Style::default().fg(t.text).bg(t.mantle)
    };

    f.render_widget(
        Paragraph::new(input_value)
            .block(block)
            .style(input_style)
            .wrap(Wrap { trim: false }),
        area,
    );

    if !app.thinking {
        let inner_w = area.width.saturating_sub(2) as usize;
        let w = inner_w.max(1);
        let char_x = app.cursor % w;
        let char_y = app.cursor / w;
        f.set_cursor(area.x + 1 + char_x as u16, area.y + 1 + char_y as u16);
    }
}

// ── Message rendering ─────────────────────────────────────────────────────────

fn render_message(out: &mut Vec<Line<'static>>, msg: &ChatMessage, width: usize, t: &Theme) {
    match msg.role {
        MsgRole::System => {
            let lines = wrap_preserve_layout(&msg.content, width.saturating_sub(6).max(6));
            for (i, line) in lines.into_iter().enumerate() {
                let prefix = if i == 0 { "  ◆ " } else { "    " };
                out.push(Line::from(vec![
                    Span::styled(prefix, Style::default().fg(t.overlay0)),
                    Span::styled(line, Style::default().fg(t.overlay0)),
                ]));
            }
            out.push(Line::from(""));
        }
        MsgRole::User => {
            // Divider
            out.push(Line::from(Span::styled(
                "─".repeat(width.min(100)),
                Style::default().fg(t.surface0),
            )));
            // Header
            out.push(Line::from(vec![
                Span::styled(" you ", Style::default().fg(t.base).bg(t.blue).add_modifier(Modifier::BOLD)),
                Span::styled(format!("  {}", msg.timestamp), Style::default().fg(t.overlay0)),
            ]));
            // Content
            for line in wrap_text(&msg.content, width.saturating_sub(2)) {
                out.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(line, Style::default().fg(t.text)),
                ]));
            }
            out.push(Line::from(""));
        }
        MsgRole::Assistant => {
            // Header
            let stream_dot = if msg.is_streaming {
                Span::styled(" ●", Style::default().fg(t.green))
            } else {
                Span::raw("")
            };
            out.push(Line::from(vec![
                Span::styled(" kode ", Style::default().fg(t.base).bg(t.accent).add_modifier(Modifier::BOLD)),
                Span::styled(format!("  {}", msg.timestamp), Style::default().fg(t.overlay0)),
                stream_dot,
            ]));

            // Reasoning / thinking block
            if !msg.reasoning.is_empty() {
                render_thinking_block(out, msg, width, t);
            }

            // Main content (markdown)
            if !msg.content.is_empty() {
                let md = markdown::render_with_theme(&msg.content, width.saturating_sub(2), t);
                for line in md {
                    let mut spans = vec![Span::styled("  ", Style::default())];
                    spans.extend(line.spans);
                    out.push(Line::from(spans));
                }
            }

            // Tool calls
            for tc in &msg.tool_calls {
                render_tool_call(out, tc, width, t);
            }

            out.push(Line::from(""));
        }
    }
}

fn render_thinking_block(out: &mut Vec<Line<'static>>, msg: &ChatMessage, width: usize, t: &Theme) {
    let collapsed = msg.reasoning_collapsed;
    let line_count = msg.reasoning.lines().count();

    // Header bar
    let toggle = if collapsed { "▶" } else { "▼" };
    let summary: String = msg.reasoning
        .lines()
        .next()
        .unwrap_or("")
        .chars()
        .take(60)
        .collect();
    let header_text = if collapsed {
        format!(" {} thinking  ({} lines)  {} ", toggle, line_count, truncate(&summary, 50))
    } else {
        format!(" {} thinking  ({} lines) ", toggle, line_count)
    };

    out.push(Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled(
            header_text,
            Style::default()
                .fg(t.base)
                .bg(t.sapphire)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    if !collapsed {
        // Top border
        out.push(Line::from(vec![
            Span::styled("  ╭", Style::default().fg(t.sapphire)),
            Span::styled("─".repeat(width.saturating_sub(5).min(80)), Style::default().fg(t.sapphire)),
        ]));

        for line in msg.reasoning.lines().take(50) {
            for wrapped in wrap_text(line, width.saturating_sub(8).max(8)) {
                out.push(Line::from(vec![
                    Span::styled("  │ ", Style::default().fg(t.sapphire)),
                    Span::styled(wrapped, Style::default().fg(t.subtext0)),
                ]));
            }
        }
        if line_count > 50 {
            out.push(Line::from(vec![
                Span::styled("  │ ", Style::default().fg(t.sapphire)),
                Span::styled(
                    format!("… {} more lines", line_count - 50),
                    Style::default().fg(t.overlay0),
                ),
            ]));
        }

        // Bottom border
        out.push(Line::from(vec![
            Span::styled("  ╰", Style::default().fg(t.sapphire)),
            Span::styled("─".repeat(width.saturating_sub(5).min(80)), Style::default().fg(t.sapphire)),
        ]));
    }
}

fn render_tool_call(out: &mut Vec<Line<'static>>, tc: &crate::app::ToolCallEntry, width: usize, t: &Theme) {
    let (icon, color) = match tc.status {
        ToolStatus::Running => ("⠿", t.yellow),
        ToolStatus::Done    => ("✓", t.green),
        ToolStatus::Error   => ("✗", t.red),
    };
    let label = match tc.status {
        ToolStatus::Running => "running",
        ToolStatus::Done    => "done",
        ToolStatus::Error   => "error",
    };

    out.push(Line::from(vec![
        Span::styled("  ╭─ ", Style::default().fg(t.surface1)),
        Span::styled(icon, Style::default().fg(color).add_modifier(Modifier::BOLD)),
        Span::styled(format!(" {} ", tc.name), Style::default().fg(t.sapphire).add_modifier(Modifier::BOLD)),
        Span::styled(label, Style::default().fg(color)),
    ]));

    if !tc.output_preview.is_empty() {
        for line in wrap_text(&tc.output_preview, width.saturating_sub(8).max(8)) {
            out.push(Line::from(vec![
                Span::styled("  │  ", Style::default().fg(t.surface1)),
                Span::styled(line, Style::default().fg(t.overlay1)),
            ]));
        }
    }

    out.push(Line::from(Span::styled("  ╰─", Style::default().fg(t.surface1))));
}

// ── Overlays ──────────────────────────────────────────────────────────────────

fn draw_sessions_overlay(f: &mut Frame, app: &App) {
    let t = &app.theme;
    let area = centered_rect(62, 72, f.size());
    f.render_widget(Clear, area);

    let items: Vec<ListItem> = if app.sessions.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "  no saved sessions",
            Style::default().fg(t.overlay0),
        )))]
    } else {
        app.sessions.iter().enumerate().map(|(i, s)| {
            let title = s.title.as_deref().unwrap_or("untitled");
            let date = s.updated_at.format("%Y-%m-%d %H:%M").to_string();
            let msgs = s.messages.len();
            let sel = i == app.session_cursor;
            let bg = if sel { t.surface0 } else { t.mantle };
            let fg = if sel { t.accent2 } else { t.text };
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(if sel { " ▶ " } else { "   " }, Style::default().fg(t.accent).bg(bg)),
                    Span::styled(
                        truncate(title, 38),
                        Style::default().fg(fg).bg(bg)
                            .add_modifier(if sel { Modifier::BOLD } else { Modifier::empty() }),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("   ", Style::default().bg(bg)),
                    Span::styled(
                        format!("{} msgs · {}", msgs, date),
                        Style::default().fg(t.overlay0).bg(bg),
                    ),
                ]),
            ])
        }).collect()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.accent))
        .title(Span::styled(
            " sessions  ↑↓ navigate  Enter open  d delete  Esc back ",
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(t.mantle));

    f.render_widget(List::new(items).block(block), area);
}

fn draw_model_overlay(f: &mut Frame, app: &App) {
    let t = &app.theme;
    let area = centered_rect(54, 55, f.size());
    f.render_widget(Clear, area);

    let items: Vec<ListItem> = app.model_list.iter().enumerate().map(|(i, m)| {
        let sel = i == app.model_cursor;
        let active = m == &app.model;
        let bg = if sel { t.surface0 } else { t.mantle };
        let fg = if active { t.green } else if sel { t.accent2 } else { t.text };
        let prefix = if active { " ✓ " } else if sel { " ▶ " } else { "   " };
        ListItem::new(Line::from(vec![
            Span::styled(prefix, Style::default().fg(t.accent).bg(bg)),
            Span::styled(
                m.clone(),
                Style::default().fg(fg).bg(bg)
                    .add_modifier(if sel { Modifier::BOLD } else { Modifier::empty() }),
            ),
        ]))
    }).collect();

    let title = if app.models_loading {
        " models  ⟳ loading… ".to_string()
    } else {
        format!(" models ({})  ↑↓ navigate  Enter select  ^R refresh  Esc back ", app.model_list.len())
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.blue))
        .title(Span::styled(&title, Style::default().fg(t.blue).add_modifier(Modifier::BOLD)))
        .style(Style::default().bg(t.mantle));

    f.render_widget(List::new(items).block(block), area);
}

fn draw_theme_overlay(f: &mut Frame, app: &App) {
    let t = &app.theme;
    let area = centered_rect(44, 50, f.size());
    f.render_widget(Clear, area);

    let items: Vec<ListItem> = app.theme_list.iter().enumerate().map(|(i, name)| {
        let sel = i == app.theme_cursor;
        let active = *name == t.name;
        let bg = if sel { t.surface0 } else { t.mantle };
        let fg = if active { t.green } else if sel { t.accent2 } else { t.text };
        let prefix = if active { " ✓ " } else if sel { " ▶ " } else { "   " };
        ListItem::new(Line::from(vec![
            Span::styled(prefix, Style::default().fg(t.accent).bg(bg)),
            Span::styled(
                name.to_string(),
                Style::default().fg(fg).bg(bg)
                    .add_modifier(if sel { Modifier::BOLD } else { Modifier::empty() }),
            ),
        ]))
    }).collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.accent))
        .title(Span::styled(
            " themes  ↑↓ navigate  Enter select  Esc back ",
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(t.mantle));

    f.render_widget(List::new(items).block(block), area);
}

fn draw_command_palette(f: &mut Frame, app: &App) {
    let t = &app.theme;
    let area = centered_rect(60, 70, f.size());
    f.render_widget(Clear, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    // Search box
    let search_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.accent))
        .title(Span::styled(" command palette ", Style::default().fg(t.accent).add_modifier(Modifier::BOLD)))
        .style(Style::default().bg(t.mantle));
    f.render_widget(
        Paragraph::new(format!(" {}", app.command_filter))
            .block(search_block)
            .style(Style::default().fg(t.text).bg(t.mantle)),
        inner[0],
    );

    // Commands list
    let cmds = app.filtered_commands();
    let items: Vec<ListItem> = if cmds.is_empty() {
        vec![ListItem::new(Line::from(vec![
            Span::styled("  no commands found", Style::default().fg(t.overlay0)),
        ]))]
    } else {
        cmds.iter().enumerate().map(|(i, cmd)| {
            let sel = i == app.command_cursor;
            let bg = if sel { t.surface0 } else { t.mantle };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {:12} ", cmd.key),
                    Style::default().fg(t.sapphire).bg(bg).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:16} ", cmd.label),
                    Style::default().fg(if sel { t.accent2 } else { t.text }).bg(bg)
                        .add_modifier(if sel { Modifier::BOLD } else { Modifier::empty() }),
                ),
                Span::styled(
                    truncate(cmd.description, 64),
                    Style::default().fg(t.overlay1).bg(bg),
                ),
            ]))
        }).collect()
    };

    let list_block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
        .border_style(Style::default().fg(t.accent))
        .style(Style::default().bg(t.mantle));

    f.render_widget(List::new(items).block(list_block), inner[1]);
}

fn draw_todo_overlay(f: &mut Frame, app: &App) {
    let t = &app.theme;
    let area = centered_rect(60, 60, f.size());
    f.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(area);

    let items: Vec<ListItem> = if app.session.todo_items.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "  no todo items",
            Style::default().fg(t.overlay0),
        )))]
    } else {
        app.session
            .todo_items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let sel = i == app.todo_cursor;
                let bg = if sel { t.surface0 } else { t.mantle };
                let marker = if item.done { "[x]" } else { "[ ]" };
                let marker_color = if item.done { t.green } else { t.yellow };
                ListItem::new(Line::from(vec![
                    Span::styled(if sel { " ▶ " } else { "   " }, Style::default().fg(t.accent).bg(bg)),
                    Span::styled(marker, Style::default().fg(marker_color).bg(bg)),
                    Span::styled(" ", Style::default().bg(bg)),
                    Span::styled(
                        truncate(&item.text, area.width.saturating_sub(12) as usize),
                        Style::default()
                            .fg(if sel { t.accent2 } else { t.text })
                            .bg(bg)
                            .add_modifier(if sel { Modifier::BOLD } else { Modifier::empty() }),
                    ),
                ]))
            })
            .collect()
    };

    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.yellow))
        .title(Span::styled(
            " todo manager  ↑↓ select  Space/Enter toggle  d delete  type + Enter add  Esc back ",
            Style::default().fg(t.yellow).add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(t.mantle));

    f.render_widget(List::new(items).block(list_block), chunks[0]);

    let input = if app.todo_input.is_empty() {
        " add todo item...".to_string()
    } else {
        format!(" {}", app.todo_input)
    };
    let input_style = if app.todo_input.is_empty() {
        Style::default().fg(t.overlay0).bg(t.mantle)
    } else {
        Style::default().fg(t.text).bg(t.mantle)
    };
    let input_block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
        .border_style(Style::default().fg(t.yellow))
        .style(Style::default().bg(t.mantle));
    f.render_widget(
        Paragraph::new(input)
            .block(input_block)
            .style(input_style)
            .wrap(Wrap { trim: false }),
        chunks[1],
    );
}

fn draw_changed_files_overlay(f: &mut Frame, app: &App) {
    let t = &app.theme;
    let area = centered_rect(68, 62, f.size());
    f.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(area);

    let files = app.filtered_changed_files();
    let items: Vec<ListItem> = if files.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "  no changed files (or filter has no matches)",
            Style::default().fg(t.overlay0),
        )))]
    } else {
        files
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let sel = i == app.changed_files_cursor;
                let bg = if sel { t.surface0 } else { t.mantle };
                ListItem::new(Line::from(vec![
                    Span::styled(if sel { " ▶ " } else { "   " }, Style::default().fg(t.accent).bg(bg)),
                    Span::styled(
                        truncate(p, area.width.saturating_sub(10) as usize),
                        Style::default()
                            .fg(if sel { t.accent2 } else { t.text })
                            .bg(bg)
                            .add_modifier(if sel { Modifier::BOLD } else { Modifier::empty() }),
                    ),
                ]))
            })
            .collect()
    };

    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.sapphire))
        .title(Span::styled(
            " changed files  ↑↓ select  Enter insert path  d delete  type filter  Esc back ",
            Style::default().fg(t.sapphire).add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(t.mantle));
    f.render_widget(List::new(items).block(list_block), chunks[0]);

    let filter_text = if app.changed_files_filter.is_empty() {
        " filter...".to_string()
    } else {
        format!(" {}", app.changed_files_filter)
    };
    let filter_style = if app.changed_files_filter.is_empty() {
        Style::default().fg(t.overlay0).bg(t.mantle)
    } else {
        Style::default().fg(t.text).bg(t.mantle)
    };
    let filter_block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
        .border_style(Style::default().fg(t.sapphire))
        .style(Style::default().bg(t.mantle));
    f.render_widget(
        Paragraph::new(filter_text)
            .block(filter_block)
            .style(filter_style)
            .wrap(Wrap { trim: false }),
        chunks[1],
    );
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 { return vec![text.to_string()]; }
    let mut result = Vec::new();
    for raw_line in text.split('\n') {
        if raw_line.is_empty() { result.push(String::new()); continue; }
        let mut current = String::new();
        let mut current_width = 0usize;
        for word in raw_line.split_whitespace() {
            let mut word_rest = word.to_string();
            while word_rest.chars().count() > max_width {
                let chunk: String = word_rest.chars().take(max_width).collect();
                if current_width > 0 {
                    result.push(current.clone());
                    current.clear();
                    current_width = 0;
                }
                result.push(chunk.clone());
                word_rest = word_rest.chars().skip(max_width).collect();
            }
            let word_w = word_rest.chars().count();
            if current_width == 0 {
                current.push_str(&word_rest);
                current_width = word_w;
            } else if current_width + 1 + word_w <= max_width {
                current.push(' ');
                current.push_str(&word_rest);
                current_width += 1 + word_w;
            } else {
                result.push(current.clone());
                current = word_rest;
                current_width = word_w;
            }
        }
        if !current.is_empty() { result.push(current); }
    }
    result
}

fn input_height(input: &str, width: usize) -> usize {
    if width == 0 { return 1; }
    wrap_text(input, width).len().max(1).min(8)
}

fn truncate(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max { return s.to_string(); }
    let mut t: String = chars[..max.saturating_sub(1)].iter().collect();
    t.push('…');
    t
}

fn wrap_preserve_layout(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![text.to_string()];
    }

    let mut out = Vec::new();
    for raw_line in text.split('\n') {
        if raw_line.is_empty() {
            out.push(String::new());
            continue;
        }
        let mut start = 0usize;
        let chars: Vec<char> = raw_line.chars().collect();
        while start < chars.len() {
            let end = (start + max_width).min(chars.len());
            out.push(chars[start..end].iter().collect());
            start = end;
        }
    }
    out
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let w = area.width * percent_x / 100;
    let h = area.height * percent_y / 100;
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}
