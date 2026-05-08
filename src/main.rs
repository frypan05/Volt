mod app;
mod config;
mod http;
mod scanner;
mod ui;

use std::io;
use std::time::Duration;

use crate::ui::highlight::ResponseView;
use app::{
    App, AppMsg, AuthDialogField, AuthType, BodyType, CustomRouteField, EditorTab, FocusPane,
    InputTarget,
};
use clap::Parser;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    MouseEventKind,
};
use tokio::sync::mpsc;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser)]
#[command(name = "volt")]
#[command(about = "A terminal HTTP client for developers.", long_about = None)]
#[command(version)]
struct Cli {
    #[arg(long)]
    themes: bool,
    #[arg(long)]
    theme: Option<String>,
    #[arg(long = "update", short = 'U')]
    update_flag: bool,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(clap::Subcommand)]
enum Commands {
    Update,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if matches!(cli.command, Some(Commands::Update)) || cli.update_flag {
        return handle_update().await;
    }

    let config = config::AppConfig::load()?;
    let mut global_config = config::GlobalConfig::load();

    if cli.themes {
        let options = vec!["vesper", "dracula", "gruvbox", "tokyo-night"];
        let ans = inquire::Select::new("Select a theme:", options).prompt();
        match ans {
            Ok(choice) => {
                global_config.theme = choice.to_string();
                global_config.save()?;
                println!("Theme set to {}", global_config.theme);
                return Ok(());
            }
            Err(_) => return Ok(()),
        }
    }

    if let Some(theme) = cli.theme {
        global_config.theme = theme;
        global_config.save()?;
        println!("Theme set to {}", global_config.theme);
        return Ok(());
    }

    tokio::task::spawn_blocking(ui::highlight::prewarm)
        .await
        .expect("prewarm panicked");

    let fallback_routes: scanner::ScannerReport = scanner::ScannerReport {
        routes: Vec::new(),
        persisted_base_urls: std::collections::HashMap::<String, String>::new(),
        is_too_broad: false,
    };
    let routes = scanner::scan_current_dir().unwrap_or(fallback_routes);

    let (tx, mut rx) = mpsc::unbounded_channel::<AppMsg>();
    let mut app: App = App::new(routes, config, global_config, tx);

    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        EnableMouseCapture
    )?;
    let mut terminal = ratatui::Terminal::new(ratatui::backend::CrosstermBackend::new(stdout))?;

    loop {
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

        let poll_ms: u64 = if app.pending_request || app.response.highlight_pending {
            5
        } else {
            50
        };
        if event::poll(Duration::from_millis(poll_ms))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    handle_key(&mut app, key).await
                }
                Event::Mouse(m)
                    if m.kind == MouseEventKind::Down(crossterm::event::MouseButton::Left) =>
                {
                    app.handle_mouse_click(m.column, m.row);
                }
                Event::Mouse(m)
                    if m.kind == MouseEventKind::Drag(crossterm::event::MouseButton::Left) =>
                {
                    app.handle_mouse_drag(m.column);
                }
                Event::Mouse(m)
                    if m.kind == MouseEventKind::Up(crossterm::event::MouseButton::Left) =>
                {
                    app.handle_mouse_release();
                }
                Event::Mouse(m) if m.kind == MouseEventKind::ScrollUp => {
                    app.handle_mouse_scroll(m.column, m.row, true);
                }
                Event::Mouse(m) if m.kind == MouseEventKind::ScrollDown => {
                    app.handle_mouse_scroll(m.column, m.row, false);
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

async fn handle_update() -> anyhow::Result<()> {
    println!("Checking for updates...");
    Ok(())
}

async fn handle_key(app: &mut App, key: KeyEvent) {
    if app.view_picker_open {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => app.view_picker_open = false,
            KeyCode::Char('1') => app.select_view(ResponseView::Auto),
            KeyCode::Char('2') => app.select_view(ResponseView::Json),
            KeyCode::Char('3') => app.select_view(ResponseView::Html),
            KeyCode::Char('4') => app.select_view(ResponseView::Text),
            KeyCode::Char('5') => app.select_view(ResponseView::Raw),
            KeyCode::Char('k') | KeyCode::Down => app.response.view = app.response.view.next(),
            KeyCode::Char('j') | KeyCode::Up => app.response.view = app.response.view.prev(),
            KeyCode::Enter => {
                let view = app.response.view;
                app.select_view(view);
            }
            _ => {}
        }
        return;
    }

    if app.custom_route_dialog.is_some() {
        handle_dialog_key(app, key);
        return;
    }

    if app.auth_dialog.is_some() {
        handle_auth_dialog_key(app, key);
        return;
    }

    if app.input_mode {
        match key.code {
            KeyCode::Esc => app.stop_editing(),
            KeyCode::Tab => {
                if let InputTarget::Tab(t) = app.input_target {
                    if t == EditorTab::Auth {
                        let d = app.current_draft_mut();
                        match d.auth_type {
                            AuthType::BasicAuth | AuthType::ApiKey => {
                                d.active_col = (d.active_col + 1) % 2;
                            }
                            _ => {}
                        }
                    } else if t != EditorTab::Body {
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
            KeyCode::Delete => app.active_buffer_mut().delete(),
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
            KeyCode::Char(c) => app.active_buffer_mut().insert_char(c),
            _ => {}
        }
        return;
    }

    if app.body_type_focused {
        match key.code {
            KeyCode::Left | KeyCode::Char('h') => {
                if app.current_draft().body_type == BodyType::None {
                    app.body_type_focused = false;
                    app.editor_tab = EditorTab::Auth;
                } else {
                    let d = app.current_draft_mut();
                    d.body_type = d.body_type.prev();
                }
            }
            KeyCode::Right | KeyCode::Char('l') => {
                let d = app.current_draft_mut();
                d.body_type = d.body_type.next();
            }
            KeyCode::Esc | KeyCode::Up | KeyCode::Char('k') => {
                app.body_type_focused = false;
                app.editor_tab = EditorTab::Auth;
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

    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Tab => app.focus_next(),
        KeyCode::BackTab => app.focus_prev(),
        KeyCode::Char('k') | KeyCode::Down if app.focus == FocusPane::Explorer => {
            app.move_explorer_selection(true);
        }
        KeyCode::Char('j') | KeyCode::Up if app.focus == FocusPane::Explorer => {
            app.move_explorer_selection(false);
        }
        KeyCode::Char('d') if app.focus == FocusPane::Explorer => {
            app.delete_selected_custom_route()
        }
        KeyCode::Char('k') | KeyCode::Down if app.focus == FocusPane::Viewer => {
            app.scroll_viewer(false)
        }
        KeyCode::Char('j') | KeyCode::Up if app.focus == FocusPane::Viewer => {
            app.scroll_viewer(true)
        }
        KeyCode::PageDown if app.focus == FocusPane::Viewer => app.scroll_viewer_page(false),
        KeyCode::PageUp if app.focus == FocusPane::Viewer => app.scroll_viewer_page(true),
        KeyCode::Home if app.focus == FocusPane::Viewer => app.viewer_scroll = 0,
        KeyCode::End if app.focus == FocusPane::Viewer => app.viewer_scroll = u16::MAX,
        KeyCode::Char('/') if app.focus == FocusPane::Viewer => app.open_view_picker(),
        KeyCode::Char('[') if app.focus == FocusPane::Viewer => app.cycle_response_view(false),
        KeyCode::Char(']') if app.focus == FocusPane::Viewer => app.cycle_response_view(true),
        KeyCode::Char('y') if app.focus == FocusPane::Viewer => {
            if app.copy_response_to_clipboard() {
                app.status_message = "Copied response to clipboard".to_string();
            } else {
                app.status_message = "Nothing to copy".to_string();
            }
        }
        KeyCode::Char('h') | KeyCode::Left if app.focus == FocusPane::Editor => {
            if app.editor_tab == EditorTab::Body {
                app.body_type_focused = true;
            } else {
                app.editor_tab = app.editor_tab.prev();
            }
        }
        KeyCode::Char('l') | KeyCode::Right if app.focus == FocusPane::Editor => {
            if app.editor_tab == EditorTab::Body {
                app.body_type_focused = true;
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
            } else if app.editor_tab == EditorTab::Auth {
                app.open_auth_dialog();
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

fn handle_auth_dialog_key(app: &mut App, key: KeyEvent) {
    let mode = app.auth_dialog.as_ref().map(|d| d.active_field);
    let Some(mode) = mode else {
        return;
    };
    match mode {
        AuthDialogField::TypeSelector => match key.code {
            KeyCode::Left | KeyCode::Char('h') => {
                let d = app.current_draft_mut();
                d.auth_type = d.auth_type.prev();
            }
            KeyCode::Right | KeyCode::Char('l') => {
                let d = app.current_draft_mut();
                d.auth_type = d.auth_type.next();
            }
            KeyCode::Tab | KeyCode::Enter => {
                if app.current_draft().auth_type != AuthType::None {
                    if let Some(dialog) = app.auth_dialog.as_mut() {
                        dialog.active_field = AuthDialogField::InputFields;
                    }
                    app.input_mode = true;
                    app.input_target = InputTarget::Tab(EditorTab::Auth);
                    app.current_draft_mut().active_col = 0;
                }
            }
            KeyCode::Esc => app.auth_dialog = None,
            _ => {}
        },
        AuthDialogField::InputFields => match key.code {
            KeyCode::Esc => {
                app.input_mode = false;
                app.auth_dialog = None;
            }
            KeyCode::Tab => {
                let auth_type = app.current_draft().auth_type;
                match auth_type {
                    AuthType::BasicAuth | AuthType::ApiKey => {
                        let d = app.current_draft_mut();
                        d.active_col = (d.active_col + 1) % 2;
                    }
                    _ => {}
                }
            }
            KeyCode::Enter => {
                app.auth_dialog = None;
                app.input_mode = false;
            }
            _ => {
                app.input_mode = true;
                app.input_target = InputTarget::Tab(EditorTab::Auth);
                if let KeyCode::Char(c) = key.code {
                    app.active_buffer_mut().insert_char(c);
                } else if key.code == KeyCode::Backspace {
                    app.active_buffer_mut().backspace();
                } else if key.code == KeyCode::Left {
                    app.active_buffer_mut().move_left();
                } else if key.code == KeyCode::Right {
                    app.active_buffer_mut().move_right();
                } else if key.code == KeyCode::Home {
                    app.active_buffer_mut().move_home();
                } else if key.code == KeyCode::End {
                    app.active_buffer_mut().move_end();
                }
            }
        },
    }
}
