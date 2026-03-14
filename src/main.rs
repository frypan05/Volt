mod app;
mod config;
mod http;
mod scanner;
mod ui;

use std::io;
use std::time::Duration;

use anyhow::Context;
use app::{App, FocusPane, InputTarget};
use config::AppConfig;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = AppConfig::load()?;
    let routes = scanner::scan_current_dir().context("failed to scan routes")?;
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut app = App::new(routes, config, tx);

    enable_raw_mode().context("failed to enable raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).context("failed to enter alternate screen")?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend).context("failed to create terminal")?;

    let result = run_app(&mut terminal, &mut app, &mut rx).await;

    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();
    app.persist_config().ok();

    result
}

async fn run_app(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<io::Stdout>>,
    app: &mut App,
    rx: &mut mpsc::UnboundedReceiver<http::HttpResult>,
) -> anyhow::Result<()> {
    loop {
        while let Ok(result) = rx.try_recv() {
            app.apply_http_result(result);
        }

        terminal.draw(|frame| ui::draw(frame, app))?;

        if app.should_quit {
            break;
        }

        if event::poll(Duration::from_millis(50)).context("failed to poll event")? {
            if let Event::Key(key) = event::read().context("failed to read event")? {
                if key.kind == KeyEventKind::Press {
                    handle_key(app, key).await;
                }
            }
        }
    }

    Ok(())
}

async fn handle_key(app: &mut App, key: KeyEvent) {
    if app.input_mode {
        match key.code {
            KeyCode::Esc => app.stop_editing(),
            KeyCode::Left => app.active_buffer_mut().move_left(),
            KeyCode::Right => app.active_buffer_mut().move_right(),
            KeyCode::Home => app.active_buffer_mut().move_home(),
            KeyCode::End => app.active_buffer_mut().move_end(),
            KeyCode::Backspace => app.active_buffer_mut().backspace(),
            KeyCode::Delete => app.active_buffer_mut().delete(),
            KeyCode::Enter => app.active_buffer_mut().insert_newline(),
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.active_buffer_mut().clear();
            }
            KeyCode::Char(ch) => app.active_buffer_mut().insert_char(ch),
            _ => {}
        }
        return;
    }

    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Tab => app.focus_next(),
        KeyCode::BackTab => app.focus_prev(),
        KeyCode::Char('j') | KeyCode::Down => app.move_down(),
        KeyCode::Char('k') | KeyCode::Up => app.move_up(),
        KeyCode::Char('h') | KeyCode::Left => app.prev_tab(),
        KeyCode::Char('l') | KeyCode::Right => app.next_tab(),
        KeyCode::Char('i') => app.start_editing(InputTarget::Tab(app.editor_tab)),
        KeyCode::Char('u') => app.start_editing(InputTarget::BaseUrl),
        KeyCode::Char('r') => app.execute_current_request().await,
        KeyCode::Char('c') => app.copy_response_to_clipboard(),
        _ => {}
    }

    if app.focus == FocusPane::Editor {
        match key.code {
            KeyCode::Char('1') => app.set_tab(app::EditorTab::Headers),
            KeyCode::Char('2') => app.set_tab(app::EditorTab::Body),
            KeyCode::Char('3') => app.set_tab(app::EditorTab::Params),
            KeyCode::Char('4') => app.set_tab(app::EditorTab::Auth),
            _ => {}
        }
    }
}
