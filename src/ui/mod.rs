pub mod highlight;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Tabs, Wrap, BorderType};

use crate::app::{self, App, EditorTab, FocusPane, InputTarget};

pub fn draw(frame: &mut Frame, app: &App) {
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Header
            Constraint::Min(0),    // Main content
            Constraint::Length(1), // Footer/Help
        ])
        .split(frame.area());

    draw_header(frame, app, main_layout[0]);
    draw_footer(frame, app, main_layout[2]);

    let content_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25), // Explorer
            Constraint::Percentage(35), // Editor
            Constraint::Percentage(40), // Viewer
        ])
        .split(main_layout[1]);

    draw_explorer(frame, app, content_layout[0]);
    draw_editor(frame, app, content_layout[1]);
    draw_viewer(frame, app, content_layout[2]);
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let current_dir = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "Unknown".to_string());

    let text = Line::from(vec![
        Span::styled(" VOLT ", Style::default().bg(Color::Cyan).fg(Color::Black).add_modifier(Modifier::BOLD)),
        Span::raw(" "),
        Span::styled(current_dir, Style::default().fg(Color::DarkGray)),
        Span::raw(" | "),
        Span::styled(format!("{} env vars", app.env_vars.len()), Style::default().fg(Color::Yellow)),
    ]);
    frame.render_widget(Paragraph::new(text), area);
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let _help_style = Style::default().fg(Color::Black).bg(Color::DarkGray);
    let key_style = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);

    let text = Line::from(vec![
        Span::styled(" q ", key_style), Span::raw("quit "),
        Span::styled(" tab ", key_style), Span::raw("switch pane "),
        Span::styled(" / ", key_style), Span::raw("search "),
        Span::styled(" r ", key_style), Span::raw("run "),
        Span::styled(" x ", key_style), Span::raw("curl "),
        Span::styled(" c ", key_style), Span::raw("copy "),
        Span::styled(" ? ", key_style), Span::raw("help "),
        Span::raw(" | "),
        Span::styled(&app.status_message, Style::default().fg(Color::White)),
    ]);
    
    frame.render_widget(Paragraph::new(text).style(Style::default().bg(Color::Indexed(234))), area);
}

fn draw_explorer(frame: &mut Frame, app: &App, area: Rect) {
    let show_search = app.search_active || !app.search_query.is_empty();
    
    let block = pane_block(" 1. EXPLORER ", app.focus == FocusPane::Explorer && !app.search_active);
    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    let (search_area, list_area) = if show_search {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(inner_area);
        (Some(chunks[0]), chunks[1])
    } else {
        (None, inner_area)
    };

    if let Some(area) = search_area {
        let title = if app.search_active { " Search [Insert Mode] " } else { " Search " };
        let border_style = if app.search_active { Color::Yellow } else { Color::DarkGray };
        
        frame.render_widget(
            Paragraph::new(app.search_query.clone())
                .block(Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(Style::default().fg(border_style))
                    .border_type(BorderType::Rounded)),
            area,
        );
    }

    let items: Vec<ListItem> = if app.filtered_routes.is_empty() {
        if app.routes.is_empty() {
             vec![ListItem::new(" No routes discovered ").fg(Color::DarkGray)]
        } else {
             vec![ListItem::new(" No matches found ").fg(Color::DarkGray)]
        }
    } else {
        app.filtered_routes
            .iter()
            .enumerate()
            .map(|(_index, route)| {
                let color = method_color(route.method.as_str());
                let line = Line::from(vec![
                    Span::styled(format!(" {:<7}", route.method), Style::default().fg(color).add_modifier(Modifier::BOLD)),
                    Span::raw(format!(" {}", route.path)),
                ]);
                ListItem::new(line)
            })
            .collect()
    };

    let list = List::new(items)
        .highlight_style(Style::default().bg(Color::Indexed(237)).add_modifier(Modifier::BOLD));
    
    let mut state = ratatui::widgets::ListState::default();
    if !app.filtered_routes.is_empty() {
        state.select(Some(app.selected_route));
    }
    frame.render_stateful_widget(list, list_area, &mut state);
}

fn draw_editor(frame: &mut Frame, app: &App, area: Rect) {
    let block = pane_block(" 2. EDITOR ", app.focus == FocusPane::Editor);
    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Base URL
            Constraint::Length(3), // Tabs
            Constraint::Min(0),    // Content
        ])
        .split(inner_area);

    let draft = app.current_draft();
    
    // Base URL
    let is_editing_url = app.input_mode && app.input_target == InputTarget::BaseUrl;
    let url_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(if is_editing_url { " Base URL [Editing] " } else { " Base URL [u] " })
        .border_style(if is_editing_url { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::DarkGray) });
    
    frame.render_widget(
        Paragraph::new(format!(" {}", draft.base_url.text))
            .block(url_block)
            .wrap(Wrap { trim: false }),
        chunks[0],
    );

    // Tabs
    let titles = EditorTab::ALL.into_iter().map(|t| {
        let text = format!(" {} ", t.title());
        Line::from(text)
    }).collect::<Vec<_>>();
    
    let tabs = Tabs::new(titles)
        .select(app.editor_tab as usize)
        .highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .divider(Span::raw("|"))
        .block(Block::default().borders(Borders::BOTTOM).border_style(Style::default().fg(Color::DarkGray)));
    frame.render_widget(tabs, chunks[1]);

    // Content
    let target = InputTarget::Tab(app.editor_tab);
    let is_editing_tab = app.input_mode && app.input_target == target;
    let content_title = if is_editing_tab { format!(" {} [Editing] ", app.editor_tab.title()) } else { format!(" {} [i] ", app.editor_tab.title()) };
    
    let content_block = Block::default()
        .title(content_title)
        .title_alignment(ratatui::layout::Alignment::Right)
        .border_style(if is_editing_tab { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::DarkGray) });

    frame.render_widget(
        Paragraph::new(draft.buffer(target).text.clone())
            .block(content_block)
            .scroll((app.editor_scroll, 0))
            .wrap(Wrap { trim: false }),
        chunks[2],
    );
}

fn draw_viewer(frame: &mut Frame, app: &App, area: Rect) {
    let block = pane_block(" 3. RESPONSE ", app.focus == FocusPane::Viewer);
    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // Metrics
            Constraint::Min(0),    // Body
        ])
        .split(inner_area);

    // Metrics
    let status_color = match app.response.status_code {
        Some(code) if (200..300).contains(&code) => Color::Green,
        Some(code) if code >= 400 => Color::Red,
        Some(_) => Color::Yellow,
        None => Color::DarkGray,
    };

    let status_text = app.response.status_code.map(|c| c.to_string()).unwrap_or_else(|| if app.pending_request { "...".into() } else { "None".into() });
    
    let metrics = vec![
        Line::from(vec![Span::raw(" Status:  "), Span::styled(status_text, Style::default().fg(status_color).add_modifier(Modifier::BOLD))]),
        Line::from(vec![Span::raw(" Time:    "), Span::raw(app.response.latency_ms.map(|l| format!("{}ms", l)).unwrap_or_else(|| "-".into()))]),
        Line::from(vec![Span::raw(" Size:    "), Span::raw(app::human_bytes(app.response.size_bytes))]),
        Line::from(vec![Span::raw(" Type:    "), Span::raw(if app.response.content_type.is_empty() { "-".into() } else { app.response.content_type.clone() })]),
    ];

    frame.render_widget(Paragraph::new(metrics).block(Block::default().borders(Borders::BOTTOM).border_style(Style::default().fg(Color::DarkGray))), chunks[0]);

    // Body
    let content = if app.response.highlighted.is_empty() {
        vec![Line::from(" No response received yet").fg(Color::DarkGray)]
    } else {
        app.response.highlighted.clone()
    };

    frame.render_widget(
        Paragraph::new(content)
            .scroll((app.viewer_scroll, 0))
            .wrap(Wrap { trim: false }),
        chunks[1],
    );
}

fn pane_block<'a>(title: &'a str, focused: bool) -> Block<'a> {
    let color = if focused { Color::Cyan } else { Color::DarkGray };
    let border_type = if focused { BorderType::Thick } else { BorderType::Plain };

    Block::default()
        .borders(Borders::ALL)
        .border_type(border_type)
        .border_style(Style::default().fg(color))
        .title(Span::styled(title, Style::default().fg(color).add_modifier(Modifier::BOLD)))
}

fn method_color(method: &str) -> Color {
    match method {
        "GET" => Color::Green,
        "POST" => Color::Blue,
        "PUT" => Color::Yellow,
        "PATCH" => Color::Magenta,
        "DELETE" => Color::Red,
        _ => Color::Cyan,
    }
}
