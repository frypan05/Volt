mod app;
mod config;
mod http;
mod scanner;
mod ui;

use std::io;
use std::time::Duration;

use crate::ui::highlight::ResponseView;
use app::{App, AppMsg, BodyType, CustomRouteField, EditorTab, FocusPane, InputTarget};
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    MouseEventKind,
};
use tokio::sync::mpsc;
use clap::Parser;

#[derive(Parser)]
#[command(name = "volt")]
#[command(about = "A terminal HTTP client for developers.", long_about = None)]
#[command(version)] // Automatically uses version from Cargo.toml
struct Cli {
    /// List all available themes
    #[arg(long)]
    themes: bool,

    /// Set the theme
    #[arg(long)]
    theme: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Update volt to the latest version from GitHub
    Update,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if let Some(Commands::Update) = cli.command {
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

    let routes = scanner::scan_current_dir().unwrap_or_else(|_| scanner::ScannerReport {
        routes: Vec::new(),
        persisted_base_urls: std::collections::HashMap::new(),
        is_too_broad: false,
    });

    let (tx, mut rx) = mpsc::unbounded_channel::<AppMsg>();
    let mut app = App::new(routes, config, global_config, tx);

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
                    handle_key(&mut app, key).await;
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

// ---------------------------------------------------------------------------
// Key handler
// ---------------------------------------------------------------------------

async fn handle_update() -> anyhow::Result<()> {
    println!("Checking for updates...");
    let client = reqwest::Client::builder()
        .user_agent("volt-updater")
        .build()?;

    let release: serde_json::Value = client
        .get("https://api.github.com/repos/frypan05/volt/releases/latest")
        .send()
        .await?
        .json()
        .await?;

    let latest_tag = release["tag_name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Could not find latest version tag"))?
        .trim_start_matches('v');

    let current_version = env!("CARGO_PKG_VERSION");

    if latest_tag == current_version {
        println!("You are already on the latest version (v{}).", current_version);
        return Ok(());
    }

    println!("New version available: v{} (current: v{})", latest_tag, current_version);

    let assets = release["assets"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("No assets found in latest release"))?;

    // Determine target asset name based on OS
    let target_os = std::env::consts::OS;
    let asset_name = match target_os {
        "windows" => "volt-windows.exe",
        "linux" => "volt-linux",
        "macos" => "volt-macos",
        _ => return Err(anyhow::anyhow!("Unsupported OS for auto-update")),
    };

    let asset = assets
        .iter()
        .find(|a| a["name"].as_str() == Some(asset_name))
        .ok_or_else(|| anyhow::anyhow!("Could not find binary for {} in latest release", target_os))?;

    let download_url = asset["browser_download_url"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No download URL for asset"))?;

    println!("Downloading {}...", asset_name);
    let bytes = client.get(download_url).send().await?.bytes().await?;

    let current_exe = std::env::current_exe()?;
    let tmp_exe = current_exe.with_extension("tmp");

    std::fs::write(&tmp_exe, bytes)?;

    // On Windows, we can't replace a running binary directly.
    // We rename the current one and move the new one in.
    #[cfg(windows)]
    {
        let old_exe = current_exe.with_extension("old");
        if old_exe.exists() {
            std::fs::remove_file(&old_exe)?;
        }
        std::fs::rename(&current_exe, &old_exe)?;
        std::fs::rename(&tmp_exe, &current_exe)?;
        println!("Update successful! You can now delete {:?}.", old_exe);
    }

    #[cfg(not(windows))]
    {
        std::fs::rename(&tmp_exe, &current_exe)?;
        // Ensure it's executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&current_exe)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&current_exe, perms)?;
        }
    }

    println!("Successfully updated to v{}!", latest_tag);
    Ok(())
}

async fn handle_key(app: &mut App, key: KeyEvent) {
    // -----------------------------------------------------------------------
    // View picker popup — highest priority overlay, handles its own keys.
    // Opened with '/' from anywhere, closed with Esc or a selection.
    // -----------------------------------------------------------------------
    if app.view_picker_open {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                app.view_picker_open = false;
            }
            // Number shortcuts 1-5
            KeyCode::Char('1') => app.select_view(ResponseView::Auto),
            KeyCode::Char('2') => app.select_view(ResponseView::Json),
            KeyCode::Char('3') => app.select_view(ResponseView::Html),
            KeyCode::Char('4') => app.select_view(ResponseView::Text),
            KeyCode::Char('5') => app.select_view(ResponseView::Raw),
            // j/k or arrow keys move selection, Enter confirms
            KeyCode::Char('k') | KeyCode::Down => {
                app.response.view = app.response.view.next();
            }
            KeyCode::Char('j') | KeyCode::Up => {
                app.response.view = app.response.view.prev();
            }
            KeyCode::Enter => {
                // Confirm current selection and close
                let view = app.response.view;
                app.select_view(view);
            }
            _ => {}
        }
        return;
    }

    // Dialog takes full priority over editor modes.
    if app.custom_route_dialog.is_some() {
        handle_dialog_key(app, key);
        return;
    }

    // -----------------------------------------------------------------------
    // Text / KV editing mode — must be checked BEFORE any shortcut that uses
    // characters like '/' so that typing into URL/path fields is never stolen.
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
    // Body-type selector sub-mode (None / JSON / Text / Form row).
    //
    // FIX: Esc now navigates back to the Auth tab (the tab immediately
    // before Body), not just clears a flag and leaves the user stranded.
    // The user can also press ← when already at the leftmost type (None)
    // to exit back to Auth, mirroring how ← on Params wraps to Body.
    // -----------------------------------------------------------------------
    if app.body_type_focused {
        match key.code {
            KeyCode::Left | KeyCode::Char('h') => {
                // If we are already on None (leftmost), exit the sub-mode
                // back to the Auth tab — feels like continuing to move left.
                if app.current_draft().body_type == BodyType::None {
                    app.body_type_focused = false;
                    app.editor_tab = EditorTab::Auth;
                } else {
                    let d = app.current_draft_mut();
                    d.body_type = d.body_type.prev();
                }
            }
            KeyCode::Right | KeyCode::Char('l') => {
                // Wrap at the rightmost (Form) back to None — or just cycle.
                let d = app.current_draft_mut();
                d.body_type = d.body_type.next();
            }
            // Esc or ↑ or k: exit sub-mode and move focus to the Auth tab.
            KeyCode::Esc | KeyCode::Up | KeyCode::Char('k') => {
                app.body_type_focused = false;
                app.editor_tab = EditorTab::Auth;
            }
            // 'i' enters body text edit (only if a type other than None is chosen).
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
        KeyCode::Char('k') | KeyCode::Down if app.focus == FocusPane::Explorer => {
            app.move_explorer_selection(true);
        }
        KeyCode::Char('j') | KeyCode::Up if app.focus == FocusPane::Explorer => {
            app.move_explorer_selection(false);
        }

        // Delete selected custom route — only active in Explorer pane.
        KeyCode::Char('d') if app.focus == FocusPane::Explorer => {
            app.delete_selected_custom_route();
        }

        // Viewer scroll
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

        // View picker / cycling — ONLY when Viewer pane is focused and not in
        // any input mode, so '/' typed in URL fields or custom-route paths is
        // never swallowed by this shortcut.
        KeyCode::Char('/') if app.focus == FocusPane::Viewer => app.open_view_picker(),
        KeyCode::Char('[') if app.focus == FocusPane::Viewer => app.cycle_response_view(false),
        KeyCode::Char(']') if app.focus == FocusPane::Viewer => app.cycle_response_view(true),

        // Copy response body to clipboard — 'y' (yank) while Viewer is focused.
        KeyCode::Char('y') if app.focus == FocusPane::Viewer => {
            if app.copy_response_to_clipboard() {
                app.status_message = "Copied response to clipboard".to_string();
            } else {
                app.status_message = "Nothing to copy".to_string();
            }
        }

        // Editor tab navigation with h / l / arrows.
        // When already on the Body tab, ← / → enters the body-type sub-mode
        // WITHOUT immediately changing the type — the user first sees the
        // selector highlighted, then uses ← / → to change.
        KeyCode::Char('h') | KeyCode::Left if app.focus == FocusPane::Editor => {
            if app.editor_tab == EditorTab::Body {
                // Enter body-type selector without changing the value yet.
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
                    // No type chosen yet — drop into selector so user picks one first.
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
