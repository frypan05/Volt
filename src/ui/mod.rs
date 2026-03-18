pub mod highlight;

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation,
    ScrollbarState, Tabs, Wrap,
};

use crate::app::{App, BodyType, CustomRouteField, EditorTab, FocusPane, HttpMethod, InputTarget};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    app.last_area = area;
    // Advance the spinner every frame while a request is in-flight
    app.tick_loader();

    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    draw_header(frame, app, main_layout[0]);
    draw_footer(frame, app, main_layout[2]);

    let content_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(35),
            Constraint::Percentage(40),
        ])
        .split(main_layout[1]);

    draw_explorer(frame, app, content_layout[0]);
    draw_editor(frame, app, content_layout[1]);
    draw_viewer(frame, app, content_layout[2]);

    // Overlays — drawn last so they sit on top of everything
    if app.url_history_open && app.url_history.len() > 1 {
        draw_url_history_dropdown(frame, app, content_layout[1]);
    }
    if app.custom_route_dialog.is_some() {
        draw_custom_route_dialog(frame, app);
    }
    // Loader overlay inside the response pane when waiting for a response
    if app.pending_request {
        draw_loader(frame, app, content_layout[2]);
    }
}

// ---------------------------------------------------------------------------
// Header / Footer
// ---------------------------------------------------------------------------

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let text = Line::from(vec![
        Span::styled(
            " VOLT ",
            Style::default()
                .bg(Color::Cyan)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" | "),
        Span::styled(
            format!("{} env vars", app.env_vars.len()),
            Style::default().fg(Color::Yellow),
        ),
    ]);
    frame.render_widget(Paragraph::new(text), area);
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let key = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let text = Line::from(vec![
        Span::styled(" q ", key),
        Span::raw("quit "),
        Span::styled(" tab ", key),
        Span::raw("pane "),
        Span::styled(" u ", key),
        Span::raw("url "),
        Span::styled(" i ", key),
        Span::raw("edit "),
        Span::styled(" n ", key),
        Span::raw("add row "),
        Span::styled(" r ", key),
        Span::raw("run "),
        Span::raw(" | "),
        Span::raw(&app.status_message),
    ]);
    frame.render_widget(Paragraph::new(text).bg(Color::Indexed(234)), area);
}

// ---------------------------------------------------------------------------
// Explorer pane
// ---------------------------------------------------------------------------

fn draw_explorer(frame: &mut Frame, app: &App, area: Rect) {
    let block = pane_block(" 1. EXPLORER ", app.focus == FocusPane::Explorer);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut items: Vec<ListItem> = app
        .filtered_routes
        .iter()
        .map(|r| {
            let color = method_color(r.method.as_str());
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {:<7}", r.method),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!(" {}", r.path)),
            ]))
        })
        .collect();

    let sentinel_style = if app.selected_is_add_custom() {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    items.push(ListItem::new(Span::styled(
        " + Add custom route",
        sentinel_style,
    )));

    let list = List::new(items).highlight_style(Style::default().bg(Color::Indexed(237)));
    let mut state = ratatui::widgets::ListState::default();
    state.select(Some(app.selected_route));
    frame.render_stateful_widget(list, inner, &mut state);
}

// ---------------------------------------------------------------------------
// Editor pane
// ---------------------------------------------------------------------------

fn draw_editor(frame: &mut Frame, app: &App, area: Rect) {
    let block = pane_block(" 2. EDITOR ", app.focus == FocusPane::Editor);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Base URL box
            Constraint::Length(2), // Tab bar
            Constraint::Min(0),    // Tab content
        ])
        .split(inner);

    draw_base_url(frame, app, chunks[0]);
    draw_editor_tabs(frame, app, chunks[1]);

    if app.editor_tab == EditorTab::Body {
        draw_body_editor(frame, app, chunks[2]);
    } else {
        draw_kv_table(frame, app, chunks[2]);
    }
}

fn draw_base_url(frame: &mut Frame, app: &App, area: Rect) {
    let draft = app.current_draft();
    let is_editing = app.input_mode && app.input_target == InputTarget::BaseUrl;

    let url_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(if is_editing {
            " Base URL [Editing] (Up/Down: history, Esc: done) "
        } else {
            " Base URL [u] "
        })
        .border_style(if is_editing {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        });

    let url_content = if is_editing {
        cursor_line(&draft.base_url, " ")
    } else {
        Line::from(format!(" {}", draft.base_url.text))
    };
    frame.render_widget(Paragraph::new(url_content).block(url_block), area);
}

fn draw_editor_tabs(frame: &mut Frame, app: &App, area: Rect) {
    let tabs = Tabs::new(
        EditorTab::ALL
            .iter()
            .map(|t| Line::from(t.title().to_string()))
            .collect::<Vec<_>>(),
    )
    .select(
        EditorTab::ALL
            .iter()
            .position(|t| *t == app.editor_tab)
            .unwrap_or(0),
    )
    .highlight_style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )
    .divider("|");
    frame.render_widget(tabs, area);
}

// ---------------------------------------------------------------------------
// KV table (Params / Headers / Auth)
// ---------------------------------------------------------------------------

fn draw_kv_table(frame: &mut Frame, app: &App, area: Rect) {
    let draft = app.current_draft();
    let tab = app.editor_tab;
    let rows = match tab {
        EditorTab::Headers => &draft.headers,
        EditorTab::Params => &draft.params,
        EditorTab::Auth => &draft.auth,
        EditorTab::Body => return,
    };

    // Reserve the last row for the "+ Add row" button
    let table_height = area.height.saturating_sub(1);

    // Column layout: checkbox | key | value
    let constraints = [
        Constraint::Length(5),
        Constraint::Percentage(47),
        Constraint::Percentage(47),
    ];

    // ---- Column header row ----
    let header_area = Rect::new(area.x, area.y, area.width, 1);
    let header_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(header_area);

    frame.render_widget(Paragraph::new("").bg(Color::Indexed(236)), header_area);
    frame.render_widget(
        Paragraph::new(Span::styled(
            " name",
            Style::default()
                .fg(Color::Indexed(245))
                .add_modifier(Modifier::BOLD),
        )),
        header_layout[1],
    );
    frame.render_widget(
        Paragraph::new(Span::styled(
            " value",
            Style::default()
                .fg(Color::Indexed(245))
                .add_modifier(Modifier::BOLD),
        )),
        header_layout[2],
    );

    // ---- Separator ----
    if area.height > 1 {
        let sep_area = Rect::new(area.x, area.y + 1, area.width, 1);
        frame.render_widget(
            Paragraph::new(Span::styled(
                "─".repeat(area.width as usize),
                Style::default().fg(Color::Indexed(238)),
            )),
            sep_area,
        );
    }

    // ---- Data rows ----
    for (i, row) in rows.iter().enumerate() {
        let row_y = area.y + 2 + (i as u16);
        // Stop before the "+ Add row" button area
        if row_y >= area.y + table_height {
            break;
        }
        let row_area = Rect::new(area.x, row_y, area.width, 1);

        let bg = if i % 2 == 0 {
            Color::Reset
        } else {
            Color::Indexed(234)
        };
        frame.render_widget(Paragraph::new("").bg(bg), row_area);

        let cell_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(constraints)
            .split(row_area);

        let is_row_focused =
            app.input_mode && app.input_target == InputTarget::Tab(tab) && draft.active_row == i;

        // Checkbox
        let checkbox_style = if row.enabled {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        frame.render_widget(
            Paragraph::new(Span::styled(
                if row.enabled { " [x]" } else { " [ ]" },
                checkbox_style,
            )),
            cell_layout[0],
        );

        // Key cell
        let key_focused = is_row_focused && draft.active_col == 0;
        if key_focused {
            frame.render_widget(
                Paragraph::new(cursor_line_styled(&row.key)).bg(Color::Indexed(236)),
                cell_layout[1],
            );
        } else {
            let (text, style) = if row.key.text.is_empty() {
                (" key".to_string(), Style::default().fg(Color::Indexed(240)))
            } else {
                (
                    format!(" {}", row.key.text),
                    Style::default().fg(Color::White),
                )
            };
            frame.render_widget(Paragraph::new(Span::styled(text, style)), cell_layout[1]);
        }

        // Value cell
        let val_focused = is_row_focused && draft.active_col == 1;
        if val_focused {
            frame.render_widget(
                Paragraph::new(cursor_line_styled(&row.value)).bg(Color::Indexed(236)),
                cell_layout[2],
            );
        } else {
            let (text, style) = if row.value.text.is_empty() {
                (
                    " value".to_string(),
                    Style::default().fg(Color::Indexed(240)),
                )
            } else {
                (
                    format!(" {}", row.value.text),
                    Style::default().fg(Color::White),
                )
            };
            frame.render_widget(Paragraph::new(Span::styled(text, style)), cell_layout[2]);
        }
    }

    // ---- "+ Add row" button pinned to the bottom of the table area ----
    let btn_y = area.y + area.height - 1;
    let btn_area = Rect::new(area.x, btn_y, area.width, 1);
    let btn_label = match tab {
        EditorTab::Headers => " + Add header",
        EditorTab::Params => " + Add param",
        EditorTab::Auth => " + Add auth entry",
        EditorTab::Body => "",
    };
    frame.render_widget(
        Paragraph::new(Span::styled(
            btn_label,
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ))
        .bg(Color::Indexed(235)),
        btn_area,
    );
}

// ---------------------------------------------------------------------------
// Body editor
// ---------------------------------------------------------------------------

fn draw_body_editor(frame: &mut Frame, app: &App, area: Rect) {
    let draft = app.current_draft();
    let is_editing = app.input_mode && app.input_target == InputTarget::Tab(EditorTab::Body);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);

    draw_body_type_selector(frame, app, chunks[0]);

    let block = Block::default()
        .title(if is_editing {
            " Body [Editing] "
        } else {
            " Body [i] "
        })
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(if is_editing {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        });

    if draft.body_type == BodyType::None {
        frame.render_widget(
            Paragraph::new(Span::styled(
                " Select a body type with ←/→, then press i to edit",
                Style::default().fg(Color::Indexed(240)),
            ))
            .block(block),
            chunks[1],
        );
        return;
    }

    let content = if is_editing {
        render_multiline_with_cursor(&draft.body)
    } else if draft.body.text.is_empty() {
        vec![Line::from(Span::styled(
            " i: start editing",
            Style::default().fg(Color::Indexed(240)),
        ))]
    } else {
        draft
            .body
            .text
            .lines()
            .map(|l| Line::from(l.to_string()))
            .collect()
    };

    frame.render_widget(
        Paragraph::new(content)
            .block(block)
            .wrap(Wrap { trim: false }),
        chunks[1],
    );
}

fn draw_body_type_selector(frame: &mut Frame, app: &App, area: Rect) {
    let draft = app.current_draft();
    let mut spans = vec![Span::raw(" Type: ")];
    for btype in &BodyType::ALL {
        let selected = draft.body_type == *btype;
        let style = if selected {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Indexed(245))
        };
        spans.push(Span::styled(format!(" {} ", btype.label()), style));
        spans.push(Span::raw(" "));
    }
    // Hint: arrow keys only (no h/l)
    spans.push(Span::styled(
        "  ←/→ change",
        Style::default().fg(Color::Indexed(238)),
    ));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

// ---------------------------------------------------------------------------
// Response / Viewer pane
// ---------------------------------------------------------------------------

fn draw_viewer(frame: &mut Frame, app: &App, area: Rect) {
    let block = pane_block(" 3. RESPONSE ", app.focus == FocusPane::Viewer);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Min(0)])
        .split(inner);

    // ---- Metrics header ----
    let res = &app.response;
    let status_color = match res.status_code {
        Some(c) if c < 300 => Color::Green,
        Some(c) if c < 400 => Color::Yellow,
        Some(_) => Color::Red,
        None => Color::DarkGray,
    };

    let metrics = vec![
        Line::from(vec![
            Span::raw(" Status: "),
            Span::styled(
                res.status_code.map(|c| c.to_string()).unwrap_or("-".into()),
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::raw(" Time:   "),
            Span::raw(
                res.latency_ms
                    .map(|l| format!("{}ms", l))
                    .unwrap_or("-".into()),
            ),
        ]),
        Line::from(vec![
            Span::raw(" Size:   "),
            Span::raw(crate::app::human_bytes(res.size_bytes)),
        ]),
        Line::from(vec![Span::raw(" Type:   "), Span::raw(&res.content_type)]),
    ];
    frame.render_widget(
        Paragraph::new(metrics).block(Block::default().borders(Borders::BOTTOM)),
        chunks[0],
    );

    // ---- Scrollable body ----
    let body_area = chunks[1];

    if res.highlighted.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                " No response — press r to send request",
                Style::default().fg(Color::DarkGray),
            )),
            body_area,
        );
        return;
    }

    let total_lines = res.highlighted.len() as u16;
    let visible_lines = body_area.height;
    // Clamp scroll so we never scroll past the last line
    let max_scroll = total_lines.saturating_sub(visible_lines);
    let scroll = app.viewer_scroll.min(max_scroll);

    // Body paragraph with scroll offset
    // Reserve 1 column on the right for the scrollbar
    let para_area = Rect {
        width: body_area.width.saturating_sub(1),
        ..body_area
    };
    frame.render_widget(
        Paragraph::new(res.highlighted.clone())
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        para_area,
    );

    // Vertical scrollbar
    if total_lines > visible_lines {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("▲"))
            .end_symbol(Some("▼"))
            .track_symbol(Some("│"))
            .thumb_symbol("█");
        let mut scrollbar_state =
            ScrollbarState::new(max_scroll as usize).position(scroll as usize);
        frame.render_stateful_widget(scrollbar, body_area, &mut scrollbar_state);
    }

    // Scroll hint (only when focused and there is content to scroll)
    if app.focus == FocusPane::Viewer && total_lines > visible_lines {
        let hint_area = Rect::new(
            body_area.x,
            body_area.y + body_area.height - 1,
            body_area.width,
            1,
        );
        frame.render_widget(
            Paragraph::new(Span::styled(
                " j/k or ↑/↓ scroll  PgUp/PgDn skip",
                Style::default().fg(Color::Indexed(238)),
            )),
            hint_area,
        );
    }
}

// ---------------------------------------------------------------------------
// Loader overlay
// ---------------------------------------------------------------------------

fn draw_loader(frame: &mut Frame, app: &App, viewer_area: Rect) {
    // Spinner frames — simple braille rotation
    const FRAMES: [&str; 8] = ["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];
    let frame_idx = (app.loader_tick as usize / 3) % FRAMES.len();
    let spinner = FRAMES[frame_idx];

    // Small centred overlay — 3 rows tall, 28 cols wide
    let w: u16 = 30;
    let h: u16 = 3;
    let x = viewer_area.x + (viewer_area.width.saturating_sub(w)) / 2;
    let y = viewer_area.y + (viewer_area.height.saturating_sub(h)) / 2;
    let overlay = Rect::new(x, y, w, h);

    frame.render_widget(Clear, overlay);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(overlay);
    frame.render_widget(block, overlay);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {} ", spinner),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Waiting for response…", Style::default().fg(Color::White)),
        ]))
        .alignment(Alignment::Left),
        inner,
    );
}

// ---------------------------------------------------------------------------
// URL history dropdown overlay
// ---------------------------------------------------------------------------

fn draw_url_history_dropdown(frame: &mut Frame, app: &App, editor_area: Rect) {
    let dropdown_y = editor_area.y + 4;
    let max_visible = 6usize;
    let visible = app.url_history.len().min(max_visible);
    if dropdown_y + visible as u16 + 2 > frame.area().height {
        return;
    }

    let dropdown_area = Rect::new(
        editor_area.x + 1,
        dropdown_y,
        editor_area.width.saturating_sub(2),
        visible as u16 + 2,
    );

    frame.render_widget(Clear, dropdown_area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(" URL History (↑/↓) ");
    let inner = block.inner(dropdown_area);
    frame.render_widget(block, dropdown_area);

    let items: Vec<ListItem> = app
        .url_history
        .iter()
        .take(max_visible)
        .enumerate()
        .map(|(i, url)| {
            let selected = app.url_history_index == Some(i);
            let style = if selected {
                Style::default()
                    .bg(Color::Cyan)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(Span::styled(format!(" {}", url), style))
        })
        .collect();

    frame.render_widget(List::new(items), inner);
}

// ---------------------------------------------------------------------------
// Custom route dialog
// ---------------------------------------------------------------------------

fn draw_custom_route_dialog(frame: &mut Frame, app: &App) {
    let Some(d) = &app.custom_route_dialog else {
        return;
    };

    let area = centered_rect(70, 50, frame.area());
    frame.render_widget(Clear, area);

    let outer_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Add Custom Route ")
        .border_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    let inner = outer_block.inner(area);
    frame.render_widget(outer_block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // top padding
            Constraint::Length(1), // method label
            Constraint::Length(3), // method pills
            Constraint::Length(1), // gap
            Constraint::Length(1), // path label
            Constraint::Length(3), // path input
            Constraint::Length(1), // gap
            Constraint::Min(1),    // hints
        ])
        .split(inner);

    // ---- Method label ----
    let method_focused = d.active_field == CustomRouteField::Method;
    frame.render_widget(
        Paragraph::new(Span::styled(
            " Method  (← / → to select, Tab to move to Path)",
            if method_focused {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Indexed(245))
            },
        )),
        rows[1],
    );

    // ---- Method pills ----
    let method_block = Block::default()
        .borders(Borders::ALL)
        .border_type(if method_focused {
            BorderType::Rounded
        } else {
            BorderType::Plain
        })
        .border_style(if method_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::Indexed(238))
        });
    let pill_inner = method_block.inner(rows[2]);
    frame.render_widget(method_block, rows[2]);

    let method_count = HttpMethod::ALL.len() as u32;
    let pill_constraints: Vec<Constraint> = (0..method_count)
        .map(|_| Constraint::Ratio(1, method_count))
        .collect();
    let pill_cells = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(pill_constraints)
        .split(pill_inner);

    for (i, m) in HttpMethod::ALL.iter().enumerate() {
        let is_selected = *m == d.method;
        let (bg, fg, modifier) = if is_selected {
            (method_pill_color(m.as_str()), Color::Black, Modifier::BOLD)
        } else {
            (Color::Indexed(236), Color::Indexed(245), Modifier::empty())
        };
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!(" {} ", m.as_str()),
                Style::default().fg(fg).add_modifier(modifier),
            ))
            .bg(bg)
            .alignment(Alignment::Center),
            pill_cells[i],
        );
    }

    // ---- Path label ----
    let path_focused = d.active_field == CustomRouteField::Path;
    frame.render_widget(
        Paragraph::new(Span::styled(
            " Path  (type route path, Enter to confirm)",
            if path_focused {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Indexed(245))
            },
        )),
        rows[4],
    );

    // ---- Path input ----
    let path_block = Block::default()
        .borders(Borders::ALL)
        .border_type(if path_focused {
            BorderType::Rounded
        } else {
            BorderType::Plain
        })
        .border_style(if path_focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::Indexed(238))
        });
    let path_content = if path_focused {
        cursor_line(&d.path, " ")
    } else {
        Line::from(format!(" {}", d.path.text))
    };
    frame.render_widget(Paragraph::new(path_content).block(path_block), rows[5]);

    // ---- Hints ----
    let key = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(" ←/→ ", key),
            Span::raw("method  "),
            Span::styled(" Tab ", key),
            Span::raw("next field  "),
            Span::styled(" Enter ", key),
            Span::raw("confirm  "),
            Span::styled(" Esc ", key),
            Span::raw("cancel"),
        ])),
        rows[7],
    );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn cursor_line<'a>(buf: &crate::app::TextBuffer, prefix: &str) -> Line<'a> {
    let (before, at, after) = buf.split_at_cursor();
    Line::from(vec![
        Span::raw(format!("{}{}", prefix, before)),
        Span::styled(
            at.to_string(),
            Style::default()
                .bg(Color::White)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(after.to_string()),
    ])
}

/// Cursor-highlighted line for KV cell editing (with leading space).
fn cursor_line_styled<'a>(buf: &crate::app::TextBuffer) -> Line<'a> {
    let (before, at, after) = buf.split_at_cursor();
    Line::from(vec![
        Span::raw(format!(" {}", before)),
        Span::styled(
            at.to_string(),
            Style::default()
                .bg(Color::White)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(after.to_string()),
    ])
}

fn render_multiline_with_cursor(buf: &crate::app::TextBuffer) -> Vec<Line<'static>> {
    let text_before = &buf.text[..buf.cursor.min(buf.text.len())];
    let cursor_line_idx = text_before.chars().filter(|c| *c == '\n').count();
    buf.text
        .lines()
        .enumerate()
        .map(|(i, line)| {
            if i == cursor_line_idx {
                let line_start: usize = buf.text.lines().take(i).map(|l| l.len() + 1).sum();
                let col = buf.cursor.saturating_sub(line_start).min(line.len());
                let (before, at, after) = if col >= line.len() {
                    (line, " ", "")
                } else {
                    let end = line[col..]
                        .char_indices()
                        .nth(1)
                        .map(|(j, _)| col + j)
                        .unwrap_or(line.len());
                    (&line[..col], &line[col..end], &line[end..])
                };
                Line::from(vec![
                    Span::raw(before.to_string()),
                    Span::styled(
                        at.to_string(),
                        Style::default()
                            .bg(Color::White)
                            .fg(Color::Black)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(after.to_string()),
                ])
            } else {
                Line::from(line.to_string())
            }
        })
        .collect()
}

fn pane_block(title: &str, focused: bool) -> Block {
    let color = if focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };
    let border_type = if focused {
        BorderType::Thick
    } else {
        BorderType::Plain
    };
    Block::default()
        .borders(Borders::ALL)
        .border_type(border_type)
        .border_style(Style::default().fg(color))
        .title(Span::styled(
            title,
            Style::default().add_modifier(Modifier::BOLD).fg(color),
        ))
}

fn method_color(m: &str) -> Color {
    match m {
        "GET" => Color::Green,
        "POST" => Color::Blue,
        "PUT" => Color::Yellow,
        "PATCH" => Color::Magenta,
        "DELETE" => Color::Red,
        _ => Color::Cyan,
    }
}

fn method_pill_color(m: &str) -> Color {
    match m {
        "GET" => Color::Green,
        "POST" => Color::Blue,
        "PUT" => Color::Yellow,
        "PATCH" => Color::Magenta,
        "DELETE" => Color::Red,
        "OPTIONS" => Color::Cyan,
        "HEAD" => Color::Indexed(208),
        _ => Color::Cyan,
    }
}
