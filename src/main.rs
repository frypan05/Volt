mod app;
mod config;
mod http;
mod scanner;
mod ui;

use std::io;
use std::time::Duration;

use app::{App, CustomRouteField, EditorTab, FocusPane, InputTarget};
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    MouseEventKind,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = config::AppConfig::load()?;
    let routes = scanner::scan_current_dir()?;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut app = App::new(routes, config, tx);

    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        EnableMouseCapture
    )?;
    let mut terminal = ratatui::Terminal::new(ratatui::backend::CrosstermBackend::new(stdout))?;

    loop {
        // Drain all pending HTTP results before drawing so the response appears
        // on the very next frame after the channel receives it.
        while let Ok(res) = rx.try_recv() {
            app.apply_http_result(res);
        }

        terminal.draw(|f| ui::draw(f, &mut app))?;

        if app.should_quit {
            break;
        }

        // Shorter timeout while a request is in-flight so the channel is
        // drained quickly (≤10 ms after arrival). Idle: 50 ms saves CPU.
        let poll_ms = if app.pending_request { 10 } else { 50 };
        if event::poll(Duration::from_millis(poll_ms))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    handle_key(&mut app, key).await;
                }
                Event::Mouse(mouse)
                    if mouse.kind == MouseEventKind::Down(crossterm::event::MouseButton::Left) =>
                {
                    app.handle_mouse_click(mouse.column, mouse.row);
                }
                _ => {}
            }
        }
    }

    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    Ok(())
}

async fn handle_key(app: &mut App, key: KeyEvent) {
    if app.custom_route_dialog.is_some() {
        handle_dialog_key(app, key);
        return;
    }

    if app.input_mode {
        match key.code {
            KeyCode::Esc => app.stop_editing(),

            KeyCode::Tab => {
                if let InputTarget::Tab(t) = app.input_target {
                    if t != EditorTab::Body {
                        let draft = app.current_draft_mut();
                        draft.active_col = (draft.active_col + 1) % 2;
                    }
                }
            }

            KeyCode::Backspace => {
                let is_kv = matches!(
                    app.input_target,
                    InputTarget::Tab(t) if t != EditorTab::Body
                );
                let current_col = app.current_draft().active_col;
                if is_kv && current_col == 1 && app.active_buffer_mut().text.is_empty() {
                    app.current_draft_mut().active_col = 0;
                } else {
                    app.active_buffer_mut().backspace();
                }
            }

            KeyCode::Delete => {
                app.active_buffer_mut().delete();
            }

            KeyCode::Left => {
                if app.input_target == InputTarget::BaseUrl {
                    app.active_buffer_mut().move_left();
                    return;
                }
                let is_kv = matches!(
                    app.input_target,
                    InputTarget::Tab(t) if t != EditorTab::Body
                );
                let current_col = app.current_draft().active_col;
                if is_kv && current_col == 1 {
                    let at_start = app.active_buffer_mut().cursor == 0;
                    if at_start {
                        app.current_draft_mut().active_col = 0;
                        app.active_buffer_mut().move_end();
                    } else {
                        app.active_buffer_mut().move_left();
                    }
                } else {
                    app.active_buffer_mut().move_left();
                }
            }

            KeyCode::Right => {
                let is_kv = matches!(
                    app.input_target,
                    InputTarget::Tab(t) if t != EditorTab::Body
                );
                let current_col = app.current_draft().active_col;
                if is_kv && current_col == 0 {
                    let at_end = {
                        let buf = app.active_buffer_mut();
                        buf.cursor == buf.text.len()
                    };
                    if at_end {
                        app.current_draft_mut().active_col = 1;
                        app.active_buffer_mut().move_home();
                    } else {
                        app.active_buffer_mut().move_right();
                    }
                } else {
                    app.active_buffer_mut().move_right();
                }
            }

            KeyCode::Home => app.active_buffer_mut().move_home(),
            KeyCode::End => app.active_buffer_mut().move_end(),

            KeyCode::Up if app.input_target == InputTarget::BaseUrl => {
                app.cycle_url_history(true);
            }
            KeyCode::Down if app.input_target == InputTarget::BaseUrl => {
                app.cycle_url_history(false);
            }

            KeyCode::Up => {
                if matches!(app.input_target, InputTarget::Tab(_)) {
                    app.move_row(true);
                }
            }
            KeyCode::Down => {
                if matches!(app.input_target, InputTarget::Tab(_)) {
                    app.move_row(false);
                }
            }

            KeyCode::Enter => {
                if app.input_target == InputTarget::Tab(EditorTab::Body) {
                    app.active_buffer_mut().insert_newline();
                } else if app.input_target == InputTarget::BaseUrl {
                    app.stop_editing();
                } else {
                    app.move_row(false);
                }
            }

            KeyCode::Char(c) => {
                app.active_buffer_mut().insert_char(c);
            }

            _ => {}
        }
        return;
    }

    match key.code {
        KeyCode::Char('q') => app.should_quit = true,

        KeyCode::Tab => app.focus_next(),
        KeyCode::BackTab => app.focus_prev(),

        KeyCode::Char('j') | KeyCode::Down if app.focus == FocusPane::Explorer => {
            app.selected_route = (app.selected_route + 1).min(app.filtered_routes.len());
        }
        KeyCode::Char('k') | KeyCode::Up if app.focus == FocusPane::Explorer => {
            app.selected_route = app.selected_route.saturating_sub(1);
        }

        KeyCode::Char('j') | KeyCode::Down if app.focus == FocusPane::Viewer => {
            app.scroll_viewer(false);
        }
        KeyCode::Char('k') | KeyCode::Up if app.focus == FocusPane::Viewer => {
            app.scroll_viewer(true);
        }
        KeyCode::PageDown if app.focus == FocusPane::Viewer => {
            app.scroll_viewer_page(false);
        }
        KeyCode::PageUp if app.focus == FocusPane::Viewer => {
            app.scroll_viewer_page(true);
        }
        KeyCode::Home if app.focus == FocusPane::Viewer => {
            app.viewer_scroll = 0;
        }
        KeyCode::End if app.focus == FocusPane::Viewer => {
            app.viewer_scroll = u16::MAX;
        }

        KeyCode::Char('h') | KeyCode::Left if app.focus == FocusPane::Editor => {
            if app.editor_tab == EditorTab::Body {
                let draft = app.current_draft_mut();
                draft.body_type = draft.body_type.prev();
            } else {
                app.editor_tab = app.editor_tab.prev();
            }
        }
        KeyCode::Char('l') | KeyCode::Right if app.focus == FocusPane::Editor => {
            if app.editor_tab == EditorTab::Body {
                let draft = app.current_draft_mut();
                draft.body_type = draft.body_type.next();
            } else {
                app.editor_tab = app.editor_tab.next();
            }
        }

        KeyCode::Char('u') => app.start_editing(InputTarget::BaseUrl),
        KeyCode::Char('i') if app.focus == FocusPane::Editor => {
            app.start_editing(InputTarget::Tab(app.editor_tab));
        }
        KeyCode::Char('n')
            if app.focus == FocusPane::Editor && app.editor_tab != EditorTab::Body =>
        {
            app.add_kv_row();
        }

        KeyCode::Char('r') => app.execute_current_request().await,

        KeyCode::Enter if app.focus == FocusPane::Explorer => {
            if app.selected_is_add_custom() {
                app.open_custom_route_dialog();
            } else {
                app.focus = FocusPane::Editor;
            }
        }

        _ => {}
    }
}

fn handle_dialog_key(app: &mut App, key: KeyEvent) {
    let dialog = app.custom_route_dialog.as_mut().unwrap();
    match dialog.active_field {
        CustomRouteField::Method => match key.code {
            KeyCode::Left | KeyCode::Char('h') => dialog.method = dialog.method.cycle_prev(),
            KeyCode::Right | KeyCode::Char('l') => dialog.method = dialog.method.cycle_next(),
            KeyCode::Tab | KeyCode::Enter => dialog.active_field = CustomRouteField::Path,
            KeyCode::Esc => app.custom_route_dialog = None,
            _ => {}
        },
        CustomRouteField::Path => match key.code {
            KeyCode::Char(c) => dialog.path.insert_char(c),
            KeyCode::Backspace => dialog.path.backspace(),
            KeyCode::Left => dialog.path.move_left(),
            KeyCode::Right => dialog.path.move_right(),
            KeyCode::Home => dialog.path.move_home(),
            KeyCode::End => dialog.path.move_end(),
            KeyCode::Enter => app.confirm_custom_route(),
            KeyCode::Esc => app.custom_route_dialog = None,
            _ => {}
        },
    }
}
