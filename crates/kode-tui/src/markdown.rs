/// Minimal markdown-to-ratatui renderer
/// Handles: code blocks, inline code, bold, italic, headers, horizontal rules
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use crate::theme::Theme;

#[derive(Debug, Clone, PartialEq)]
enum BlockState {
    Normal,
    Code(String),
}

/// Parse a markdown string into ratatui Lines, using the given theme
pub fn render(text: &str, max_width: usize) -> Vec<Line<'static>> {
    // markdown.rs is called from ui.rs which passes theme separately
    // We use a default theme here; ui.rs wraps lines with theme colors
    render_with_theme(text, max_width, &crate::theme::CATPPUCCIN_MOCHA)
}

pub fn render_with_theme(text: &str, max_width: usize, t: &Theme) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut state = BlockState::Normal;
    let mut code_buf: Vec<String> = Vec::new();

    for raw in text.lines() {
        match &state {
            BlockState::Code(lang) => {
                if raw.trim_start().starts_with("```") {
                    let lang = lang.clone();
                    flush_code_block(&mut lines, &code_buf, &lang, max_width, t);
                    code_buf.clear();
                    state = BlockState::Normal;
                } else {
                    code_buf.push(raw.to_string());
                }
            }
            BlockState::Normal => {
                if raw.trim_start().starts_with("```") {
                    let lang = raw.trim_start().trim_start_matches('`').trim().to_string();
                    state = BlockState::Code(lang);
                    code_buf.clear();
                } else if raw.starts_with("### ") {
                    lines.push(header_line(&raw[4..], 3, t));
                } else if raw.starts_with("## ") {
                    lines.push(header_line(&raw[3..], 2, t));
                } else if raw.starts_with("# ") {
                    lines.push(header_line(&raw[2..], 1, t));
                } else if raw.trim() == "---" || raw.trim() == "***" {
                    lines.push(hr_line(max_width, t));
                } else if raw.is_empty() {
                    lines.push(Line::from(""));
                } else {
                    lines.push(inline_line(raw, t));
                }
            }
        }
    }

    if !code_buf.is_empty() {
        flush_code_block(&mut lines, &code_buf, "", max_width, t);
    }

    lines
}

fn header_line(text: &str, level: u8, t: &Theme) -> Line<'static> {
    let (color, prefix) = match level {
        1 => (t.accent,  "█ "),
        2 => (t.accent2, "▌ "),
        _ => (t.blue,    "░ "),
    };
    Line::from(vec![
        Span::styled(prefix, Style::default().fg(color)),
        Span::styled(
            text.to_string(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
    ])
}

fn hr_line(width: usize, t: &Theme) -> Line<'static> {
    let bar = "─".repeat(width.saturating_sub(2));
    Line::from(Span::styled(bar, Style::default().fg(t.surface1)))
}

fn flush_code_block(
    lines: &mut Vec<Line<'static>>,
    code: &[String],
    lang: &str,
    max_width: usize,
    t: &Theme,
) {
    let lang_tag = if lang.is_empty() { " code ".to_string() } else { format!(" {} ", lang) };
    let border_len = max_width.saturating_sub(lang_tag.len() + 2).min(60);
    let top = format!("╭{}{}╮", lang_tag, "─".repeat(border_len));
    lines.push(Line::from(Span::styled(top, Style::default().fg(t.surface1))));

    for l in code {
        lines.push(Line::from(vec![
            Span::styled("│ ", Style::default().fg(t.surface1)),
            Span::styled(l.clone(), Style::default().fg(t.green)),
        ]));
    }

    let bottom = format!("╰{}╯", "─".repeat((max_width.saturating_sub(2)).min(62)));
    lines.push(Line::from(Span::styled(bottom, Style::default().fg(t.surface1))));
}

fn inline_line(text: &str, t: &Theme) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut chars = text.chars().peekable();
    let mut buf = String::new();

    while let Some(c) = chars.next() {
        match c {
            '`' => {
                if !buf.is_empty() {
                    spans.push(Span::styled(buf.clone(), Style::default().fg(t.text)));
                    buf.clear();
                }
                let mut code = String::new();
                for ic in chars.by_ref() {
                    if ic == '`' { break; }
                    code.push(ic);
                }
                spans.push(Span::styled(
                    code,
                    Style::default().fg(t.green).bg(t.surface0),
                ));
            }
            '*' if chars.peek() == Some(&'*') => {
                chars.next();
                if !buf.is_empty() {
                    spans.push(Span::styled(buf.clone(), Style::default().fg(t.text)));
                    buf.clear();
                }
                let mut bold = String::new();
                loop {
                    match chars.next() {
                        Some('*') if chars.peek() == Some(&'*') => { chars.next(); break; }
                        Some(bc) => bold.push(bc),
                        None => break,
                    }
                }
                spans.push(Span::styled(
                    bold,
                    Style::default().fg(t.accent2).add_modifier(Modifier::BOLD),
                ));
            }
            '*' => {
                if !buf.is_empty() {
                    spans.push(Span::styled(buf.clone(), Style::default().fg(t.text)));
                    buf.clear();
                }
                let mut italic = String::new();
                for ic in chars.by_ref() {
                    if ic == '*' { break; }
                    italic.push(ic);
                }
                spans.push(Span::styled(
                    italic,
                    Style::default().fg(t.subtext1).add_modifier(Modifier::ITALIC),
                ));
            }
            _ => buf.push(c),
        }
    }

    if !buf.is_empty() {
        spans.push(Span::styled(buf, Style::default().fg(t.text)));
    }

    Line::from(spans)
}
