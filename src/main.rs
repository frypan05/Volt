mod app;
mod config;
mod http;
mod scanner;
mod ui;

use std::io;
use std::time::Duration;

use app::{App, AppMsg, BodyType, CustomRouteField, EditorTab, FocusPane, InputTarget};
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    MouseEventKind,
};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // -----------------------------------------------------------------------
    // Pre-warm syntect statics BEFORE the event loop starts.
    // We AWAIT the task so the first request never pays the cold-start cost.
    // spawn_blocking runs on a dedicated OS thread so it doesn't block the
    // async executor while it deserialises the ~8 MB syntax data.
    // -----------------------------------------------------------------------
    tokio::task::spawn_blocking(ui::highlight::prewarm)
        .await
        .expect("prewarm panicked");

    let config = config::AppConfig::load()?;
    let routes = scanner::scan_current_dir()?;
    let (tx, mut rx) = mpsc::unbounded_channel::<AppMsg>();
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
        // Drain every pending message before drawing so the response appears
        // on the very next frame.
        while let Ok(msg) = rx.try_recv() {
            match msg {
                AppMsg::RawResult(result) => app.apply_raw_result(result),
                AppMsg::HighlightResult(lines) => app.apply_highlight_result(lines),
            }
        }

        terminal.draw(|f| ui::draw(f, &mut app))?;

        if app.should_quit {
            break;
        }

        // Poll at 5 ms while a request is in-flight so we pick up the result
        // within milliseconds of it arriving.  Idle: 50 ms saves CPU.
        let poll_ms: u64 = if app.pending_request || app.response.highlight_pending {
            5
        } else {
            50
        };
        if event::poll(Duration::from_millis(poll_ms))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    handle_key(&mut app, key).await;
                }
                Event::Mouse(m)
                    if m.kind == MouseEventKind::Down(crossterm::event::MouseButton::Left) =>
                {
                    app.handle_mouse_click(m.column, m.row);
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

// ---------------------------------------------------------------------------
// Key handler
// ---------------------------------------------------------------------------

async fn handle_key(app: &mut App, key: KeyEvent) {
    // -----------------------------------------------------------------------
    // Global: response view cycling works from ANY pane / mode.
    // -----------------------------------------------------------------------
    match key.code {
        KeyCode::Char('<') | KeyCode::Char('[') => {
            app.cycle_response_view(false);
            return;
        }
        KeyCode::Char('>') | KeyCode::Char(']') => {
            app.cycle_response_view(true);
            return;
        }
        _ => {}
    }

    // Dialog takes full priority.
    if app.custom_route_dialog.is_some() {
        handle_dialog_key(app, key);
        return;
    }

    // -----------------------------------------------------------------------
    // Text / KV editing mode
    // -----------------------------------------------------------------------
    if app.input_mode {
        match key.code {
            KeyCode::Esc => app.stop_editing(),

            KeyCode::Tab => {
                if let InputTarget::Tab(t) = app.input_target {
                    if t != EditorTab::Body {
                        let d = app.current_draft_mut();
                        d.active_col = (d.active_col + 1) % 2;
                    }
                }
            }

            KeyCode::Backspace => {
                let is_kv = matches!(app.input_target, InputTarget::Tab(t) if t != EditorTab::Body);
                let col = app.current_draft().active_col;
                if is_kv && col == 1 && app.active_buffer_mut().text.is_empty() {
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
                let is_kv = matches!(app.input_target, InputTarget::Tab(t) if t != EditorTab::Body);
                let col = app.current_draft().active_col;
                if is_kv && col == 1 {
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
                let is_kv = matches!(app.input_target, InputTarget::Tab(t) if t != EditorTab::Body);
                let col = app.current_draft().active_col;
                if is_kv && col == 0 {
                    let at_end = {
                        let b = app.active_buffer_mut();
                        b.cursor == b.text.len()
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

            KeyCode::Up if app.input_target == InputTarget::BaseUrl => app.cycle_url_history(true),
            KeyCode::Down if app.input_target == InputTarget::BaseUrl => {
                app.cycle_url_history(false)
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

    // -----------------------------------------------------------------------
    // Body-type selector sub-mode
    // -----------------------------------------------------------------------
    if app.body_type_focused {
        match key.code {
            KeyCode::Left | KeyCode::Char('h') => {
                let d = app.current_draft_mut();
                d.body_type = d.body_type.prev();
            }
            KeyCode::Right | KeyCode::Char('l') => {
                let d = app.current_draft_mut();
                d.body_type = d.body_type.next();
            }
            KeyCode::Esc | KeyCode::Up | KeyCode::Char('k') => {
                app.body_type_focused = false;
            }
            KeyCode::Char('i') => {
                if app.current_draft().body_type != BodyType::None {
                    app.body_type_focused = false;
                    app.start_editing(InputTarget::Tab(EditorTab::Body));
                }
            }
            _ => {}
        }
        return;
    }

    // -----------------------------------------------------------------------
    // Normal navigation
    // -----------------------------------------------------------------------
    match key.code {
        KeyCode::Char('q') => app.should_quit = true,

        KeyCode::Tab => app.focus_next(),
        KeyCode::BackTab => app.focus_prev(),

        // Explorer
        KeyCode::Char('j') | KeyCode::Down if app.focus == FocusPane::Explorer => {
            app.selected_route = (app.selected_route + 1).min(app.filtered_routes.len());
        }
        KeyCode::Char('k') | KeyCode::Up if app.focus == FocusPane::Explorer => {
            app.selected_route = app.selected_route.saturating_sub(1);
        }

        // Viewer scroll
        KeyCode::Char('j') | KeyCode::Down if app.focus == FocusPane::Viewer => {
            app.scroll_viewer(false)
        }
        KeyCode::Char('k') | KeyCode::Up if app.focus == FocusPane::Viewer => {
            app.scroll_viewer(true)
        }
        KeyCode::PageDown if app.focus == FocusPane::Viewer => app.scroll_viewer_page(false),
        KeyCode::PageUp if app.focus == FocusPane::Viewer => app.scroll_viewer_page(true),
        KeyCode::Home if app.focus == FocusPane::Viewer => app.viewer_scroll = 0,
        KeyCode::End if app.focus == FocusPane::Viewer => app.viewer_scroll = u16::MAX,

        // Editor tab navigation
        KeyCode::Char('h') | KeyCode::Left if app.focus == FocusPane::Editor => {
            if app.editor_tab == EditorTab::Body {
                app.body_type_focused = true;
                let d = app.current_draft_mut();
                d.body_type = d.body_type.prev();
            } else {
                app.editor_tab = app.editor_tab.prev();
            }
        }
        KeyCode::Char('l') | KeyCode::Right if app.focus == FocusPane::Editor => {
            if app.editor_tab == EditorTab::Body {
                app.body_type_focused = true;
                let d = app.current_draft_mut();
                d.body_type = d.body_type.next();
            } else {
                app.editor_tab = app.editor_tab.next();
            }
        }

        KeyCode::Char('u') => app.start_editing(InputTarget::BaseUrl),

        KeyCode::Char('i') if app.focus == FocusPane::Editor => {
            if app.editor_tab == EditorTab::Body {
                if app.current_draft().body_type != BodyType::None {
                    app.start_editing(InputTarget::Tab(EditorTab::Body));
                } else {
                    app.body_type_focused = true;
                }
            } else {
                app.start_editing(InputTarget::Tab(app.editor_tab));
            }
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
