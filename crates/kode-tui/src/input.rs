use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use crate::app::{App, AppMode};
use crate::theme::Theme;

pub enum InputAction {
    Submit(String),
    Quit,
    RefreshModels,
    None,
}

pub fn handle_key(app: &mut App, key: KeyEvent) -> InputAction {
    match app.mode {
        AppMode::Chat           => handle_chat_key(app, key),
        AppMode::SessionList    => handle_session_key(app, key),
        AppMode::ModelPicker    => handle_model_key(app, key),
        AppMode::ThemePicker    => handle_theme_key(app, key),
        AppMode::CommandPalette => handle_palette_key(app, key),
        AppMode::TodoManager    => handle_todo_key(app, key),
        AppMode::ChangedFilesManager => handle_changed_files_key(app, key),
    }
}

pub fn handle_mouse(app: &mut App, ev: MouseEvent) -> InputAction {
    app.mouse_x = ev.column;
    app.mouse_y = ev.row;

    match ev.kind {
        MouseEventKind::ScrollUp => {
            app.scroll = app.scroll.saturating_sub(3);
            app.auto_scroll = false;
        }
        MouseEventKind::ScrollDown => {
            app.scroll += 3;
            // auto_scroll re-enabled when we reach bottom (handled in ui)
        }
        MouseEventKind::Down(MouseButton::Left) => {
            // Click on thinking block header → toggle collapse
            // (row detection is approximate — good enough for UX)
        }
        _ => {}
    }
    InputAction::None
}

// ── Chat ──────────────────────────────────────────────────────────────────────

fn char_to_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices().nth(char_idx).map(|(b, _)| b).unwrap_or(s.len())
}
fn char_len(s: &str) -> usize { s.chars().count() }

fn handle_chat_key(app: &mut App, key: KeyEvent) -> InputAction {
    match (key.modifiers, key.code) {
        // ── Quit ──
        (KeyModifiers::CONTROL, KeyCode::Char('c')) => return InputAction::Quit,

        // ── Overlays ──
        (KeyModifiers::CONTROL, KeyCode::Char('p')) => {
            app.command_filter.clear();
            app.command_cursor = 0;
            app.mode = AppMode::CommandPalette;
        }
        (KeyModifiers::CONTROL, KeyCode::Char('y')) => {
            app.todo_cursor = app.todo_cursor.min(app.session.todo_items.len().saturating_sub(1));
            app.todo_input.clear();
            app.mode = AppMode::TodoManager;
        }
        (KeyModifiers::CONTROL, KeyCode::Char('f')) => {
            app.changed_files_cursor = 0;
            app.changed_files_filter.clear();
            app.mode = AppMode::ChangedFilesManager;
        }
        (KeyModifiers::CONTROL, KeyCode::Char('m')) => {
            app.model_cursor = app.model_list.iter().position(|m| m == &app.model).unwrap_or(0);
            app.mode = AppMode::ModelPicker;
        }
        (KeyModifiers::CONTROL, KeyCode::Char('k')) => {
            app.model_cursor = app.model_list.iter().position(|m| m == &app.model).unwrap_or(0);
            app.mode = AppMode::ModelPicker;
        }
        (KeyModifiers::CONTROL, KeyCode::Enter) => {
            app.model_cursor = app.model_list.iter().position(|m| m == &app.model).unwrap_or(0);
            app.mode = AppMode::ModelPicker;
        }
        (KeyModifiers::CONTROL, KeyCode::Char('t')) => {
            app.theme_cursor = app.theme_list.iter().position(|n| *n == app.theme.name).unwrap_or(0);
            app.mode = AppMode::ThemePicker;
        }
        (_, KeyCode::Tab) => {
            if let Ok(sessions) = app.store.list() { app.sessions = sessions; }
            app.session_cursor = 0;
            app.mode = AppMode::SessionList;
        }

        // ── Layout ──
        (KeyModifiers::CONTROL, KeyCode::Char('b')) => {
            app.sidebar_visible = !app.sidebar_visible;
        }

        // ── Session management ──
        (KeyModifiers::CONTROL, KeyCode::Char('n')) => {
            app.new_session();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('l')) => {
            app.clear_chat();
        }

        // ── Model refresh ──
        (KeyModifiers::CONTROL, KeyCode::Char('r')) => {
            return InputAction::RefreshModels;
        }

        // ── Send ──
        (KeyModifiers::SHIFT, KeyCode::Enter) => {
            let b = char_to_byte(&app.input, app.cursor);
            app.input.insert(b, '\n');
            app.cursor += 1;
        }
        (_, KeyCode::Enter) => {
            if app.thinking || app.input.trim().is_empty() { return InputAction::None; }
            let text = app.input.trim().to_string();
            app.input.clear();
            app.cursor = 0;
            return InputAction::Submit(text);
        }

        // ── Editing ──
        (_, KeyCode::Backspace) => {
            if app.cursor > 0 {
                app.cursor -= 1;
                let b = char_to_byte(&app.input, app.cursor);
                app.input.remove(b);
            }
        }
        (_, KeyCode::Delete) => {
            if app.cursor < char_len(&app.input) {
                let b = char_to_byte(&app.input, app.cursor);
                app.input.remove(b);
            }
        }
        (_, KeyCode::Left)  => { if app.cursor > 0 { app.cursor -= 1; } }
        (_, KeyCode::Right) => { if app.cursor < char_len(&app.input) { app.cursor += 1; } }
        (_, KeyCode::Home)  => { app.cursor = 0; }
        (_, KeyCode::End)   => { app.cursor = char_len(&app.input); }

        // ── Scroll ──
        (_, KeyCode::Up)     => { app.scroll = app.scroll.saturating_sub(1); app.auto_scroll = false; }
        (_, KeyCode::Down)   => { app.scroll += 1; }
        (_, KeyCode::PageUp) => { app.scroll = app.scroll.saturating_sub(10); app.auto_scroll = false; }
        (_, KeyCode::PageDown) => { app.scroll += 10; }

        // ── Typing ──
        (_, KeyCode::Char(c)) => {
            let b = char_to_byte(&app.input, app.cursor);
            app.input.insert(b, c);
            app.cursor += 1;
        }
        _ => {}
    }
    InputAction::None
}

// ── Session list ──────────────────────────────────────────────────────────────

fn handle_session_key(app: &mut App, key: KeyEvent) -> InputAction {
    match key.code {
        KeyCode::Esc => { app.mode = AppMode::Chat; }
        KeyCode::Up => { app.session_cursor = app.session_cursor.saturating_sub(1); }
        KeyCode::Down => {
            if app.session_cursor + 1 < app.sessions.len() { app.session_cursor += 1; }
        }
        KeyCode::PageUp => {
            app.session_cursor = app.session_cursor.saturating_sub(10);
        }
        KeyCode::PageDown => {
            app.session_cursor = (app.session_cursor + 10).min(app.sessions.len().saturating_sub(1));
        }
        KeyCode::Enter => {
            if let Some(s) = app.sessions.get(app.session_cursor) {
                app.session = s.clone();
                app.messages = s.messages.clone();
                app.model = s.model.clone();
                // Rebuild display
                app.chat_messages.clear();
                for m in &s.messages.clone() {
                    match m.role {
                        kode_core::types::Role::User => app.push_user_message(&m.content),
                        kode_core::types::Role::Assistant => {
                            app.begin_assistant_message();
                            if let Some(last) = app.chat_messages.last_mut() {
                                last.content = m.content.clone();
                                last.is_streaming = false;
                            }
                        }
                        _ => {}
                    }
                }
            }
            app.mode = AppMode::Chat;
        }
        KeyCode::Char('d') => {
            if let Some(s) = app.sessions.get(app.session_cursor) {
                let id = s.id;
                let _ = app.store.delete(id);
                if let Ok(sessions) = app.store.list() { app.sessions = sessions; }
                app.session_cursor = app.session_cursor.saturating_sub(1);
            }
        }
        _ => {}
    }
    InputAction::None
}

// ── Model picker ──────────────────────────────────────────────────────────────

fn handle_model_key(app: &mut App, key: KeyEvent) -> InputAction {
    match (key.modifiers, key.code) {
        (_, KeyCode::Esc) => { app.mode = AppMode::Chat; }
        (_, KeyCode::Up) => { app.model_cursor = app.model_cursor.saturating_sub(1); }
        (_, KeyCode::Down) => {
            if app.model_cursor + 1 < app.model_list.len() { app.model_cursor += 1; }
        }
        (_, KeyCode::PageUp) => {
            app.model_cursor = app.model_cursor.saturating_sub(10);
        }
        (_, KeyCode::PageDown) => {
            app.model_cursor = (app.model_cursor + 10).min(app.model_list.len().saturating_sub(1));
        }
        (_, KeyCode::Enter) => {
            if let Some(m) = app.model_list.get(app.model_cursor) {
                app.model = m.clone();
            }
            app.mode = AppMode::Chat;
        }
        (KeyModifiers::CONTROL, KeyCode::Char('r')) => {
            app.mode = AppMode::Chat;
            return InputAction::RefreshModels;
        }
        _ => {}
    }
    InputAction::None
}

// ── Theme picker ──────────────────────────────────────────────────────────────

fn handle_theme_key(app: &mut App, key: KeyEvent) -> InputAction {
    match key.code {
        KeyCode::Esc => {
            app.persist_theme_preference();
            app.mode = AppMode::Chat;
        }
        KeyCode::Up => {
            app.theme_cursor = app.theme_cursor.saturating_sub(1);
            if let Some(name) = app.theme_list.get(app.theme_cursor) {
                app.theme = Theme::by_name(name);
            }
        }
        KeyCode::Down => {
            if app.theme_cursor + 1 < app.theme_list.len() { app.theme_cursor += 1; }
            if let Some(name) = app.theme_list.get(app.theme_cursor) {
                app.theme = Theme::by_name(name);
            }
        }
        KeyCode::PageUp => {
            app.theme_cursor = app.theme_cursor.saturating_sub(10);
            if let Some(name) = app.theme_list.get(app.theme_cursor) {
                app.theme = Theme::by_name(name);
            }
        }
        KeyCode::PageDown => {
            app.theme_cursor = (app.theme_cursor + 10).min(app.theme_list.len().saturating_sub(1));
            if let Some(name) = app.theme_list.get(app.theme_cursor) {
                app.theme = Theme::by_name(name);
            }
        }
        KeyCode::Enter => {
            if let Some(name) = app.theme_list.get(app.theme_cursor) {
                app.theme = Theme::by_name(name);
            }
            app.persist_theme_preference();
            app.mode = AppMode::Chat;
        }
        // Live preview on arrow keys
        KeyCode::Left | KeyCode::Right => {}
        _ => {}
    }
    InputAction::None
}

// ── Command palette ───────────────────────────────────────────────────────────

fn handle_palette_key(app: &mut App, key: KeyEvent) -> InputAction {
    match (key.modifiers, key.code) {
        (_, KeyCode::Esc) => {
            app.command_filter.clear();
            app.mode = AppMode::Chat;
        }
        (_, KeyCode::Up) => {
            app.command_cursor = app.command_cursor.saturating_sub(1);
        }
        (_, KeyCode::Down) => {
            let max = app.filtered_commands().len().saturating_sub(1);
            if app.command_cursor < max { app.command_cursor += 1; }
        }
        (_, KeyCode::Enter) => {
            let cmds = app.filtered_commands();
            if let Some(cmd) = cmds.get(app.command_cursor) {
                let key_str = cmd.key;
                app.command_filter.clear();
                app.mode = AppMode::Chat;
                // Execute the selected command by its key binding
                return execute_command(app, key_str);
            }
        }
        (_, KeyCode::Backspace) => {
            app.command_filter.pop();
            app.command_cursor = 0;
        }
        (_, KeyCode::Char(c)) => {
            app.command_filter.push(c);
            app.command_cursor = 0;
        }
        _ => {}
    }
    InputAction::None
}

fn execute_command(app: &mut App, key: &str) -> InputAction {
    match key {
        "Ctrl+C"     => return InputAction::Quit,
        "Ctrl+B"     => { app.sidebar_visible = !app.sidebar_visible; }
        "Tab"        => {
            if let Ok(s) = app.store.list() { app.sessions = s; }
            app.mode = AppMode::SessionList;
        }
        "Ctrl+M"     => {
            app.model_cursor = app.model_list.iter().position(|m| m == &app.model).unwrap_or(0);
            app.mode = AppMode::ModelPicker;
        }
        "Ctrl+K"     => {
            app.model_cursor = app.model_list.iter().position(|m| m == &app.model).unwrap_or(0);
            app.mode = AppMode::ModelPicker;
        }
        "Ctrl+T"     => {
            app.theme_cursor = app.theme_list.iter().position(|n| *n == app.theme.name).unwrap_or(0);
            app.mode = AppMode::ThemePicker;
        }
        "Ctrl+N"     => { app.new_session(); }
        "Ctrl+L"     => { app.clear_chat(); }
        "Ctrl+R"     => { return InputAction::RefreshModels; }
        "Ctrl+Y"     => {
            app.todo_cursor = app.todo_cursor.min(app.session.todo_items.len().saturating_sub(1));
            app.todo_input.clear();
            app.mode = AppMode::TodoManager;
        }
        "Ctrl+F"     => {
            app.changed_files_cursor = 0;
            app.changed_files_filter.clear();
            app.mode = AppMode::ChangedFilesManager;
        }
        _ => {}
    }
    InputAction::None
}

fn handle_todo_key(app: &mut App, key: KeyEvent) -> InputAction {
    match key.code {
        KeyCode::Esc => {
            app.todo_input.clear();
            app.mode = AppMode::Chat;
        }
        KeyCode::Up => {
            app.todo_cursor = app.todo_cursor.saturating_sub(1);
        }
        KeyCode::Down => {
            if app.todo_cursor + 1 < app.session.todo_items.len() {
                app.todo_cursor += 1;
            }
        }
        KeyCode::Char(' ') => {
            app.toggle_todo_selected();
        }
        KeyCode::Char('d') => {
            app.delete_todo_selected();
        }
        KeyCode::Enter => {
            if app.todo_input.trim().is_empty() {
                app.toggle_todo_selected();
            } else {
                let text = std::mem::take(&mut app.todo_input);
                app.add_todo(text);
            }
        }
        KeyCode::Backspace => {
            app.todo_input.pop();
        }
        KeyCode::Char(c) => {
            app.todo_input.push(c);
        }
        _ => {}
    }
    InputAction::None
}

fn handle_changed_files_key(app: &mut App, key: KeyEvent) -> InputAction {
    match key.code {
        KeyCode::Esc => {
            app.changed_files_filter.clear();
            app.mode = AppMode::Chat;
        }
        KeyCode::Up => {
            app.changed_files_cursor = app.changed_files_cursor.saturating_sub(1);
        }
        KeyCode::Down => {
            let max = app.filtered_changed_files().len().saturating_sub(1);
            if app.changed_files_cursor < max {
                app.changed_files_cursor += 1;
            }
        }
        KeyCode::Backspace => {
            app.changed_files_filter.pop();
            app.changed_files_cursor = 0;
        }
        KeyCode::Char('d') => {
            app.remove_changed_file_at_cursor();
        }
        KeyCode::Enter => {
            let files = app.filtered_changed_files();
            if let Some(path) = files.get(app.changed_files_cursor) {
                let insertion = format!("{} ", path);
                let b = char_to_byte(&app.input, app.cursor);
                app.input.insert_str(b, &insertion);
                app.cursor += insertion.chars().count();
                app.mode = AppMode::Chat;
            }
        }
        KeyCode::Char(c) => {
            app.changed_files_filter.push(c);
            app.changed_files_cursor = 0;
        }
        _ => {}
    }
    InputAction::None
}
