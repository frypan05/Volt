pub mod highlight;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Tabs, Wrap};

use crate::app::{self, App, EditorTab, FocusPane, InputTarget};

pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(28),
            Constraint::Percentage(34),
            Constraint::Percentage(38),
        ])
        .split(frame.area());

    draw_explorer(frame, app, chunks[0]);
    draw_editor(frame, app, chunks[1]);
    draw_viewer(frame, app, chunks[2]);

    let footer = Rect {
        x: 0,
        y: frame.area().height.saturating_sub(1),
        width: frame.area().width,
        height: 1,
    };
    let footer_text = Line::from(vec![
        Span::styled(
            "Aris ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(&app.status_message),
    ]);
    frame.render_widget(Paragraph::new(footer_text), footer);
}

fn draw_explorer(frame: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = if app.routes.is_empty() {
        vec![ListItem::new("No API routes discovered")]
    } else {
        app.routes
            .iter()
            .enumerate()
            .map(|(index, route)| {
                let style = method_style(route.method.as_str());
                let line = Line::from(vec![
                    Span::styled(format!("{:>6} ", route.method), style),
                    Span::raw(route.path.clone()),
                ]);
                let mut item = ListItem::new(line);
                if index == app.selected_route {
                    item = item.style(Style::default().add_modifier(Modifier::BOLD));
                }
                item
            })
            .collect()
    };

    let block = pane_block("Explorer", app.focus == FocusPane::Explorer);
    let list = List::new(items)
        .block(block)
        .highlight_symbol("-> ")
        .highlight_style(Style::default().bg(Color::DarkGray));
    let mut state = ratatui::widgets::ListState::default();
    if !app.routes.is_empty() {
        state.select(Some(app.selected_route));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_editor(frame: &mut Frame, app: &App, area: Rect) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(5),
        ])
        .split(area);

    let draft = app.current_draft();
    let base_url_title = if app.input_mode && app.input_target == InputTarget::BaseUrl {
        "Base URL [INSERT]"
    } else {
        "Base URL"
    };
    frame.render_widget(
        Paragraph::new(draft.base_url.text.clone())
            .block(pane_block(base_url_title, app.focus == FocusPane::Editor))
            .wrap(Wrap { trim: false }),
        sections[0],
    );

    let titles = EditorTab::ALL
        .into_iter()
        .map(|tab| Line::from(tab.title()))
        .collect::<Vec<_>>();
    let tabs = Tabs::new(titles)
        .select(app.editor_tab as usize)
        .block(Block::default().borders(Borders::ALL).title("Modes"))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(tabs, sections[1]);

    let target = InputTarget::Tab(app.editor_tab);
    let body_title = if app.input_mode && app.input_target == target {
        format!("{} [INSERT]", app.editor_tab.title())
    } else {
        app.editor_tab.title().to_string()
    };
    let body = draft.buffer(target).text.clone();
    frame.render_widget(
        Paragraph::new(body)
            .block(pane_block(&body_title, app.focus == FocusPane::Editor))
            .scroll((app.editor_scroll, 0))
            .wrap(Wrap { trim: false }),
        sections[2],
    );

    let help = Text::from(vec![
        Line::from("1-4 tabs  i edit tab  u edit base URL"),
        Line::from("Headers: Key: Value"),
        Line::from("Params: key=value  Auth: Authorization value"),
        Line::from("JSON body is validated before send"),
    ]);
    frame.render_widget(
        Paragraph::new(help).block(Block::default().borders(Borders::ALL).title("Hints")),
        sections[3],
    );
}

fn draw_viewer(frame: &mut Frame, app: &App, area: Rect) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(8),
            Constraint::Length(6),
        ])
        .split(area);

    let status_color = match app.response.status_code {
        Some(code) if (200..300).contains(&code) => Color::Green,
        Some(code) if code >= 400 => Color::Red,
        Some(_) => Color::Yellow,
        None => Color::Gray,
    };
    let status = app
        .response
        .status_code
        .map(|code| code.to_string())
        .unwrap_or_else(|| {
            if app.pending_request {
                "Running".to_string()
            } else {
                "Idle".to_string()
            }
        });
    let latency = app
        .response
        .latency_ms
        .map(|value| format!("{value} ms"))
        .unwrap_or_else(|| "-".to_string());
    let metrics = Text::from(vec![
        Line::from(vec![
            Span::raw("Status: "),
            Span::styled(
                status,
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(format!("Latency: {latency}")),
        Line::from(format!(
            "Size: {}",
            app::human_bytes(app.response.size_bytes)
        )),
        Line::from(format!(
            "Content-Type: {}",
            if app.response.content_type.is_empty() {
                "-"
            } else {
                &app.response.content_type
            }
        )),
    ]);
    frame.render_widget(
        Paragraph::new(metrics).block(pane_block("Response", app.focus == FocusPane::Viewer)),
        sections[0],
    );

    let content = if app.response.highlighted.is_empty() {
        vec![Line::from("No response yet")]
    } else {
        app.response.highlighted.clone()
    };
    frame.render_widget(
        Paragraph::new(Text::from(content))
            .block(Block::default().borders(Borders::ALL).title("Body"))
            .scroll((app.viewer_scroll, 0))
            .wrap(Wrap { trim: false }),
        sections[1],
    );

    frame.render_widget(
        Paragraph::new(Text::from(app.route_details()))
            .block(Block::default().borders(Borders::ALL).title("Route Meta")),
        sections[2],
    );
}

fn pane_block<'a>(title: &'a str, focused: bool) -> Block<'a> {
    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(border_style)
}

fn method_style(method: &str) -> Style {
    match method {
        "GET" => Style::default().fg(Color::Green),
        "POST" => Style::default().fg(Color::Blue),
        "PUT" => Style::default().fg(Color::Yellow),
        "PATCH" => Style::default().fg(Color::Magenta),
        "DELETE" => Style::default().fg(Color::Red),
        _ => Style::default().fg(Color::Cyan),
    }
}
