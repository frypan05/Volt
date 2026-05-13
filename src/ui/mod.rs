pub mod highlight;

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation,
    ScrollbarState, Tabs, Wrap,
};
use ratatui::Frame;

use crate::app::{
    App, AuthDialogField, AuthType, BodyType, CustomRouteField, EditorTab, FocusPane, HttpMethod,
    InputTarget,
};
use crate::ui::highlight::ResponseView;

// ---------------------------------------------------------------------------
// Themes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub primary: Color,
    pub fg: Color,
    pub comment: Color,
    pub orange: Color,
    pub yellow: Color,
    pub teal: Color,
    pub blue: Color,
    pub pink: Color,
}

impl Theme {
    pub fn get(name: &str) -> Self {
        match name.to_lowercase().as_str() {
            "dracula" => Self {
                primary: Color::Rgb(0x8b, 0xe9, 0xfd),
                fg: Color::Rgb(0xf8, 0xf8, 0xf2),
                comment: Color::Rgb(0x62, 0x72, 0xa4),
                orange: Color::Rgb(0xff, 0xb8, 0x6c),
                yellow: Color::Rgb(0xf1, 0xfa, 0x8c),
                teal: Color::Rgb(0x8b, 0xe9, 0xfd),
                blue: Color::Rgb(0xbd, 0x93, 0xf9),
                pink: Color::Rgb(0xff, 0x79, 0xc6),
            },
            "gruvbox" => Self {
                primary: Color::Rgb(0x8e, 0xc0, 0x7c),
                fg: Color::Rgb(0xeb, 0xdb, 0xb2),
                comment: Color::Rgb(0x92, 0x83, 0x74),
                orange: Color::Rgb(0xfe, 0x80, 0x19),
                yellow: Color::Rgb(0xfa, 0xbd, 0x2f),
                teal: Color::Rgb(0x8e, 0xc0, 0x7c),
                blue: Color::Rgb(0x83, 0xa5, 0x98),
                pink: Color::Rgb(0xd3, 0x86, 0x9b),
            },
            "tokyo-night" => Self {
                primary: Color::Rgb(0x73, 0xdc, 0xad),
                fg: Color::Rgb(0xa9, 0xb1, 0xd6),
                comment: Color::Rgb(0x56, 0x5f, 0x89),
                orange: Color::Rgb(0xff, 0x9e, 0x64),
                yellow: Color::Rgb(0xe0, 0xaf, 0x68),
                teal: Color::Rgb(0x73, 0xdc, 0xad),
                blue: Color::Rgb(0x7a, 0xa2, 0xf7),
                pink: Color::Rgb(0xbb, 0x9a, 0xfe),
            },
            "catppuccin" => Self {
                primary: Color::Rgb(0xa6, 0xe3, 0xa1),
                fg: Color::Rgb(0xcd, 0xe6, 0xf6),
                comment: Color::Rgb(0x6c, 0x70, 0x86),
                orange: Color::Rgb(0xf5, 0xa9, 0x7f),
                yellow: Color::Rgb(0xf9, 0xe2, 0xaf),
                teal: Color::Rgb(0x94, 0xe2, 0xd5),
                blue: Color::Rgb(0x89, 0xb4, 0xfa),
                pink: Color::Rgb(0xf5, 0xc2, 0xe7),
            },
            _ => Self {
                // Vesper (Default)
                primary: Color::Rgb(0x5c, 0xb8, 0xb2),
                fg: Color::Rgb(0xcc, 0xc9, 0xc2),
                comment: Color::Rgb(0x4d, 0x4d, 0x4d),
                orange: Color::Rgb(0xff, 0x98, 0x57),
                yellow: Color::Rgb(0xe5, 0xc0, 0x7b),
                teal: Color::Rgb(0x5c, 0xb8, 0xb2),
                blue: Color::Rgb(0x5b, 0xa2, 0xd0),
                pink: Color::Rgb(0xd6, 0x7a, 0x9c),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Top-level draw
// ---------------------------------------------------------------------------

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    app.last_area = area;
    app.tick_loader();
    let theme = Theme::get(&app.theme);

    if area.width < 40 || area.height < 8 {
        frame.render_widget(
            Paragraph::new("Terminal too small — please resize")
                .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
                .alignment(Alignment::Center),
            area,
        );
        return;
    }

    let main = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    draw_header(frame, app, main[0], theme);
    draw_footer(frame, app, main[2], theme);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(app.pane_widths[0]),
            Constraint::Percentage(app.pane_widths[1]),
            Constraint::Percentage(app.pane_widths[2]),
        ])
        .split(main[1]);

    let res_01 = app.resize_target == crate::app::ResizeTarget::Split01;
    let res_12 = app.resize_target == crate::app::ResizeTarget::Split12;

    // Draw the main panes — always draw all three, even when heatmap is open
    draw_explorer(frame, app, cols[0], theme, res_01);
    draw_editor(frame, app, cols[1], theme, res_01, res_12);
    draw_viewer(frame, app, cols[2], theme, res_12);

    // Draw overlays and popups
    if app.show_heatmap {
        draw_heatmap(frame, app, theme);
    } else if app.url_history_open && app.url_history.len() > 1 {
        draw_url_history_dropdown(frame, app, cols[1], theme);
    }
    if app.custom_route_dialog.is_some() {
        draw_custom_route_dialog(frame, app, theme);
    }
    if app.auth_dialog.is_some() {
        draw_auth_dialog(frame, app, theme);
    }
    if app.pending_request {
        draw_loader(frame, app, cols[2], theme);
    }
    if app.view_picker_open {
        draw_view_picker_popup(frame, app, theme);
    }
}

// ---------------------------------------------------------------------------
// Header / Footer
// ---------------------------------------------------------------------------

fn draw_header(frame: &mut Frame, app: &App, area: Rect, theme: Theme) {
    let hl = if app.response.highlight_pending {
        Span::styled("  [hl…]", Style::default().fg(theme.comment))
    } else {
        Span::raw("")
    };
    let text = Line::from(vec![
        Span::styled(
            " VOLT ",
            Style::default()
                .bg(theme.primary)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" | "),
        Span::styled(
            format!("{} env vars", app.env_vars.len()),
            Style::default().fg(theme.yellow),
        ),
        hl,
    ]);
    frame.render_widget(Paragraph::new(text), area);
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect, theme: Theme) {
    let version_str = format!(" Donate  v{} ", env!("CARGO_PKG_VERSION"));
    let right_w = version_str.len() as u16;

    let (left_area, right_area) = if area.width > right_w + 20 {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(right_w)])
            .split(area);
        (chunks[0], Some(chunks[1]))
    } else {
        (area, None)
    };

    let key = Style::default().fg(theme.teal).add_modifier(Modifier::BOLD);
    let extra = if app.body_type_focused {
        Span::styled(
            "  ←/→ body type  Esc/← back",
            Style::default().fg(theme.yellow),
        )
    } else if app.view_picker_open {
        Span::styled(
            "  j/k select  Enter confirm  Esc cancel  1-5 quick pick",
            Style::default().fg(theme.yellow),
        )
    } else {
        Span::raw("")
    };
    let left_text = Line::from(vec![
        Span::styled(" q ", key),
        Span::raw("quit "),
        Span::styled(" tab ", key),
        Span::raw("pane "),
        Span::styled(" u ", key),
        Span::raw("url "),
        Span::styled(" i ", key),
        Span::raw("edit "),
        Span::styled(" n ", key),
        Span::raw("row "),
        Span::styled(" r ", key),
        Span::raw("run "),
        Span::styled(" / ", key),
        Span::raw("view "),
        Span::styled(" y ", key),
        Span::raw("copy response "),
        Span::raw(" | "),
        Span::raw(&app.status_message),
        Span::raw(format!(
            " | Executor: {}",
            app.executor_name.as_deref().unwrap_or("Local")
        )),
        extra,
    ]);
    frame.render_widget(Paragraph::new(left_text).bg(Color::Indexed(234)), left_area);

    if let Some(r) = right_area {
        let right_line = Line::from(vec![
            Span::raw(" "),
            Span::styled("  ", Style::default().fg(theme.comment)),
            Span::styled(
                format!("v{}", env!("CARGO_PKG_VERSION")),
                Style::default().fg(theme.comment),
            ),
            Span::raw(" "),
        ]);
        frame.render_widget(
            Paragraph::new(right_line)
                .bg(Color::Indexed(234))
                .alignment(Alignment::Right),
            r,
        );
    }
}

// ---------------------------------------------------------------------------
// Heatmap popup
// ---------------------------------------------------------------------------

fn draw_heatmap(frame: &mut Frame, app: &App, theme: Theme) {
    let fa = frame.area();

    // Calculate the number of rows: header + routes + 2 borders
    let route_count = app.filtered_routes.len() as u16;
    // popup height: 1 header row + route rows + 2 border rows, capped at 80% of screen
    let content_rows = route_count.max(1);
    let ideal_h = content_rows + 3; // 1 header + 2 borders
    let max_h = (fa.height as f32 * 0.80) as u16;
    let popup_h = ideal_h.min(max_h).max(6);

    // popup width: METHOD(6) + space(1) + PATH(20) + space(1) + HITS(6) + borders(2) + padding
    // roughly 40 chars of content + 2 borders = ~44, but give a bit more for readability
    let popup_w: u16 = 48.min((fa.width as f32 * 0.80) as u16).max(40);

    let x = (fa.width.saturating_sub(popup_w)) / 2;
    let y = (fa.height.saturating_sub(popup_h)) / 2;
    let popup_area = Rect::new(x, y, popup_w, popup_h);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Span::styled(
            " Route Hitmap  (h to close, j/k to scroll) ",
            Style::default().fg(theme.teal).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.teal))
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    if app.filtered_routes.is_empty() {
        frame.render_widget(
            Paragraph::new("No routes found.")
                .style(Style::default().fg(Color::DarkGray))
                .alignment(Alignment::Center),
            inner,
        );
        return;
    }

    // Create header row
    let header = Line::from(vec![
        Span::styled(
            format!("{:<6} ", "METHOD"),
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{:<20} ", "PATH"),
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "HITS",
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ),
    ]);

    // Create rows for each route — numbers only, no bars
    let mut rows = vec![header];

    for route in &app.filtered_routes {
        let hits = route.hit_count;
        let method_col = method_color(route.method.as_str(), theme);

        let hits_color = if hits == 0 {
            theme.comment
        } else {
            theme.yellow
        };

        rows.push(Line::from(vec![
            Span::styled(
                format!("{:<6} ", route.method.as_str()),
                Style::default().fg(method_col),
            ),
            Span::styled(
                format!("{:<20} ", truncate(&route.path, 19)),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!("{:>5}", hits),
                Style::default().fg(hits_color).add_modifier(Modifier::BOLD),
            ),
        ]));
    }

    let total = rows.len() as u16;
    let visible = inner.height.saturating_sub(1); // 1 for header
    let max_sc = total.saturating_sub(visible);
    let scroll = app.viewer_scroll.min(max_sc);

    // Split inner into header row and scrollable content
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)].as_ref())
        .split(inner);

    // Render header
    frame.render_widget(Paragraph::new(rows[0].clone()), chunks[0]);

    // Render content rows with scrolling
    let content_rows_slice = &rows[1..];
    frame.render_widget(
        Paragraph::new(content_rows_slice.to_vec())
            .scroll((scroll, 0))
            .style(Style::default().fg(theme.fg)),
        chunks[1],
    );

    // Scrollbar only when content overflows
    if total > visible + 1 {
        let sb = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("▲"))
            .end_symbol(Some("▼"))
            .track_symbol(Some("│"))
            .thumb_symbol("█")
            .style(Style::default().fg(theme.teal));
        let mut ss = ScrollbarState::new(max_sc as usize).position(scroll as usize);
        frame.render_stateful_widget(sb, inner, &mut ss);
    }
}

fn draw_explorer(frame: &mut Frame, app: &App, area: Rect, theme: Theme, res_right: bool) {
    let block = pane_block(
        " 1. EXPLORER ",
        app.focus == FocusPane::Explorer,
        theme,
        false,
        res_right,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    // Create a layout with list and footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)].as_ref())
        .split(inner);

    let list_area = chunks[0];
    let footer_area = chunks[1];

    // Footer button - highlight when heatmap is active
    let btn_style = if app.show_heatmap {
        Style::default()
            .fg(Color::Black)
            .bg(theme.teal)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.teal).add_modifier(Modifier::BOLD)
    };

    let btn_text = if app.show_heatmap {
        " Hit Rate View Active (h to close) "
    } else {
        " h  View Hit Rate "
    };

    frame.render_widget(
        Paragraph::new(Span::styled(btn_text, btn_style))
            .alignment(Alignment::Center)
            .bg(Color::Indexed(235)),
        footer_area,
    );

    let mut items: Vec<ListItem> = app
        .filtered_routes
        .iter()
        .map(|r| {
            let color = method_color(r.method.as_str(), theme);
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {:<7}", r.method),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!(" {}", r.path)),
            ]))
        })
        .collect();

    if app.filtered_routes.is_empty() {
        if app.is_too_broad {
            items.push(ListItem::new(vec![
                Line::from(Span::styled(
                    " Too broad directory!",
                    Style::default()
                        .fg(theme.orange)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    " Please open a project",
                    Style::default().fg(theme.yellow),
                )),
                Line::from(Span::styled(
                    " directory for scanning.",
                    Style::default().fg(theme.yellow),
                )),
            ]));
        } else {
            items.push(ListItem::new(Span::styled(
                " No routes found. Press Enter to add one.",
                Style::default().fg(theme.comment),
            )));
        }
    }

    let ss = if app.selected_is_add_custom() {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(theme.comment)
    };
    items.push(ListItem::new(Span::styled(" + Add custom route", ss)));

    let list = List::new(items).highlight_style(Style::default().bg(Color::Indexed(237)));
    let mut state = ratatui::widgets::ListState::default();
    state.select(Some(app.selected_route));
    frame.render_stateful_widget(list, list_area, &mut state);
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        &s[..max]
    }
}

// ---------------------------------------------------------------------------
// Editor
// ---------------------------------------------------------------------------

fn draw_editor(
    frame: &mut Frame,
    app: &App,
    area: Rect,
    theme: Theme,
    res_left: bool,
    res_right: bool,
) {
    let block = pane_block(
        " 2. EDITOR ",
        app.focus == FocusPane::Editor,
        theme,
        res_left,
        res_right,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height < 4 {
        return;
    }

    let url_h = 3u16.min(inner.height.saturating_sub(2));
    let tab_h = if inner.height > url_h { 2u16 } else { 0 };
    let content_h = inner.height.saturating_sub(url_h + tab_h);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(url_h),
            Constraint::Length(tab_h),
            Constraint::Min(content_h),
        ])
        .split(inner);

    draw_base_url(frame, app, chunks[0], theme);
    if tab_h > 0 {
        draw_editor_tabs(frame, app, chunks[1], theme);
    }
    if content_h > 0 {
        if app.editor_tab == EditorTab::Body {
            draw_body_editor(frame, app, chunks[2], theme);
        } else {
            draw_kv_table(frame, app, chunks[2], theme);
        }
    }
}

fn draw_base_url(frame: &mut Frame, app: &App, area: Rect, theme: Theme) {
    if area.height == 0 {
        return;
    }
    let draft = app.current_draft();
    let editing = app.input_mode && app.input_target == InputTarget::BaseUrl;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(if editing {
            " Base URL [Editing] (↑/↓ history, Esc done) "
        } else {
            " Base URL [u] "
        })
        .border_style(if editing {
            Style::default().fg(theme.teal)
        } else {
            Style::default().fg(theme.comment)
        });
    let content = if editing {
        cursor_line(&draft.base_url, " ")
    } else {
        Line::from(format!(" {}", draft.base_url.text))
    };
    frame.render_widget(Paragraph::new(content).block(block), area);
}

fn draw_editor_tabs(frame: &mut Frame, app: &App, area: Rect, theme: Theme) {
    if area.height == 0 {
        return;
    }
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
            .fg(theme.primary)
            .add_modifier(Modifier::BOLD),
    )
    .divider("|");
    frame.render_widget(tabs, area);
}

fn draw_kv_table(frame: &mut Frame, app: &App, area: Rect, theme: Theme) {
    if area.height == 0 {
        return;
    }
    let draft = app.current_draft();
    let tab = app.editor_tab;

    // -----------------------------------------------------------------------
    // Auth tab: show status + open-dialog hint
    // -----------------------------------------------------------------------
    if tab == EditorTab::Auth {
        let mut configured: Vec<&str> = Vec::new();
        if !draft.auth_username.text.is_empty() || !draft.auth_password.text.is_empty() {
            configured.push("Basic Auth");
        }
        if !draft.auth_token.text.is_empty() {
            configured.push("Bearer Token");
        }
        if !draft.auth_header_name.text.is_empty() || !draft.auth_header_value.text.is_empty() {
            configured.push("API Key");
        }

        let total_lines = 1 + configured.len() as u16;
        let pad_top = area.height.saturating_sub(total_lines) / 2;

        let mut constraints = vec![Constraint::Length(pad_top), Constraint::Length(1)];
        for _ in &configured {
            constraints.push(Constraint::Length(1));
        }
        constraints.push(Constraint::Min(0));

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(area);

        frame.render_widget(
            Paragraph::new("Press 'i' to open authentication dialog")
                .style(Style::default().fg(theme.comment))
                .alignment(Alignment::Center),
            chunks[1],
        );

        for (i, label) in configured.iter().enumerate() {
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("Auth set: ", Style::default().fg(theme.comment)),
                    Span::styled(
                        *label,
                        Style::default().fg(theme.teal).add_modifier(Modifier::BOLD),
                    ),
                ]))
                .alignment(Alignment::Center),
                chunks[2 + i],
            );
        }
        return;
    }

    let rows = match tab {
        EditorTab::Headers => &draft.headers,
        EditorTab::Params => &draft.params,
        EditorTab::Auth => unreachable!(),
        EditorTab::Body => return,
    };
    let tbl_h = area.height.saturating_sub(1);
    let constr = [
        Constraint::Length(5),
        Constraint::Percentage(47),
        Constraint::Percentage(47),
    ];

    let hdr = Rect::new(area.x, area.y, area.width, 1);
    let hcols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constr)
        .split(hdr);
    frame.render_widget(Paragraph::new("").bg(Color::Indexed(236)), hdr);
    frame.render_widget(
        Paragraph::new(Span::styled(
            " name",
            Style::default()
                .fg(Color::Indexed(245))
                .add_modifier(Modifier::BOLD),
        )),
        hcols[1],
    );
    frame.render_widget(
        Paragraph::new(Span::styled(
            " value",
            Style::default()
                .fg(Color::Indexed(245))
                .add_modifier(Modifier::BOLD),
        )),
        hcols[2],
    );

    if area.height > 1 {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "─".repeat(area.width as usize),
                Style::default().fg(Color::Indexed(238)),
            )),
            Rect::new(area.x, area.y + 1, area.width, 1),
        );
    }

    for (i, row) in rows.iter().enumerate() {
        let row_y = area.y + 2 + i as u16;
        if row_y >= area.y + tbl_h {
            break;
        }
        let row_area = Rect::new(area.x, row_y, area.width, 1);
        let bg = if i % 2 == 0 {
            Color::Reset
        } else {
            Color::Indexed(234)
        };
        frame.render_widget(Paragraph::new("").bg(bg), row_area);

        let cells = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(constr)
            .split(row_area);
        let focused =
            app.input_mode && app.input_target == InputTarget::Tab(tab) && draft.active_row == i;

        let cb = if row.enabled {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(theme.comment)
        };
        frame.render_widget(
            Paragraph::new(Span::styled(if row.enabled { " [x]" } else { " [ ]" }, cb)),
            cells[0],
        );

        if focused && draft.active_col == 0 {
            frame.render_widget(
                Paragraph::new(cursor_line_styled(&row.key)).bg(Color::Indexed(236)),
                cells[1],
            );
        } else {
            let (t, s) = if row.key.text.is_empty() {
                (" key".into(), Style::default().fg(theme.comment))
            } else {
                (format!(" {}", row.key.text), Style::default().fg(theme.fg))
            };
            frame.render_widget(Paragraph::new(Span::styled(t, s)), cells[1]);
        }
        if focused && draft.active_col == 1 {
            frame.render_widget(
                Paragraph::new(cursor_line_styled(&row.value)).bg(Color::Indexed(236)),
                cells[2],
            );
        } else {
            let (t, s) = if row.value.text.is_empty() {
                (" value".into(), Style::default().fg(theme.comment))
            } else {
                (
                    format!(" {}", row.value.text),
                    Style::default().fg(theme.fg),
                )
            };
            frame.render_widget(Paragraph::new(Span::styled(t, s)), cells[2]);
        }
    }

    let btn_label = match tab {
        EditorTab::Headers => " + Add header",
        EditorTab::Params => " + Add param",
        EditorTab::Auth => unreachable!(),
        _ => "",
    };
    frame.render_widget(
        Paragraph::new(Span::styled(
            btn_label,
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ))
        .bg(Color::Indexed(235)),
        Rect::new(area.x, area.y + area.height - 1, area.width, 1),
    );
}

fn draw_body_editor(frame: &mut Frame, app: &App, area: Rect, theme: Theme) {
    if area.height == 0 {
        return;
    }
    let draft = app.current_draft();
    let editing = app.input_mode && app.input_target == InputTarget::Tab(EditorTab::Body);
    let sel_h = if area.height > 1 { 1u16 } else { 0 };
    let body_h = area.height.saturating_sub(sel_h);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(sel_h), Constraint::Min(body_h)])
        .split(area);

    if sel_h > 0 {
        draw_body_type_selector(frame, app, chunks[0], theme);
    }
    if body_h == 0 {
        return;
    }

    let bc = if editing {
        theme.teal
    } else if app.body_type_focused {
        theme.teal
    } else {
        theme.comment
    };
    let block = Block::default()
        .title(if editing {
            " Body [Editing] "
        } else {
            " Body [i] "
        })
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(bc));

    if draft.body_type == BodyType::None {
        frame.render_widget(
            Paragraph::new(Span::styled(
                " Select a body type with ←/→, then press i to edit",
                Style::default().fg(theme.comment),
            ))
            .block(block),
            chunks[1],
        );
        return;
    }

    let content = if editing {
        render_multiline_with_cursor(&draft.body)
    } else if draft.body.text.is_empty() {
        vec![Line::from(Span::styled(
            " i: start editing",
            Style::default().fg(theme.comment),
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

fn draw_body_type_selector(frame: &mut Frame, app: &App, area: Rect, theme: Theme) {
    let draft = app.current_draft();
    let mut spans: Vec<Span> = vec![Span::styled(" Type: ", Style::default().fg(theme.comment))];
    for btype in &BodyType::ALL {
        let sel = draft.body_type == *btype;
        let style = if sel && app.body_type_focused {
            Style::default()
                .fg(Color::Black)
                .bg(theme.primary)
                .add_modifier(Modifier::BOLD)
        } else if sel {
            Style::default()
                .fg(Color::Black)
                .bg(theme.teal)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.comment)
        };
        spans.push(Span::styled(format!(" {} ", btype.label()), style));
        spans.push(Span::raw(" "));
    }
    if app.body_type_focused {
        spans.push(Span::styled(
            "  ←/→ change  Esc/← back",
            Style::default().fg(theme.yellow),
        ));
    } else {
        spans.push(Span::styled(
            "  ←/→ to select",
            Style::default().fg(theme.comment),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

// ---------------------------------------------------------------------------
// Viewer / Response pane
// ---------------------------------------------------------------------------

fn draw_viewer(frame: &mut Frame, app: &App, area: Rect, theme: Theme, res_left: bool) {
    // Always draw the viewer — heatmap overlays on top of it
    let block = pane_block(
        " 3. RESPONSE ",
        app.focus == FocusPane::Viewer,
        theme,
        res_left,
        false,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height < 3 {
        return;
    }

    let metrics_h = 5u16.min(inner.height.saturating_sub(2));
    let view_sel_h = if inner.height > metrics_h { 1u16 } else { 0 };
    let body_h = inner.height.saturating_sub(metrics_h + view_sel_h);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(metrics_h),
            Constraint::Length(view_sel_h),
            Constraint::Min(body_h),
        ])
        .split(inner);

    let res = &app.response;
    let status_color = match res.status_code {
        Some(c) if c < 300 => Color::Green,
        Some(c) if c < 400 => theme.yellow,
        Some(_) => Color::Red,
        None => theme.comment,
    };

    let metrics = vec![
        Line::from(vec![
            Span::styled(" Status: ", Style::default().fg(theme.comment)),
            Span::styled(
                res.status_code.map(|c| c.to_string()).unwrap_or("-".into()),
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled(" Time:   ", Style::default().fg(theme.comment)),
            Span::styled(
                res.latency_ms
                    .map(|l| format!("{}ms", l))
                    .unwrap_or("-".into()),
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(" Size:   ", Style::default().fg(theme.comment)),
            Span::styled(
                crate::app::human_bytes(res.size_bytes),
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(" Type:   ", Style::default().fg(theme.comment)),
            Span::styled(res.content_type.clone(), Style::default().fg(theme.blue)),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(metrics).block(Block::default().borders(Borders::BOTTOM)),
        chunks[0],
    );

    if view_sel_h > 0 {
        draw_response_view_bar(frame, app, chunks[1], theme);
    }

    if body_h == 0 {
        return;
    }
    let body_area = chunks[2];

    if res.highlighted.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                " No response — press r to send  |  / to set view format",
                Style::default().fg(theme.comment),
            )),
            body_area,
        );
        return;
    }

    let total = res.highlighted.len() as u16;
    let visible = body_area.height;
    let max_sc = total.saturating_sub(visible);
    let scroll = app.viewer_scroll.min(max_sc);

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

    if total > visible {
        let sb = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("▲"))
            .end_symbol(Some("▼"))
            .track_symbol(Some("│"))
            .thumb_symbol("█");
        let mut ss = ScrollbarState::new(max_sc as usize).position(scroll as usize);
        frame.render_stateful_widget(sb, body_area, &mut ss);
    }

    if app.focus == FocusPane::Viewer && total > visible {
        let hint = Rect::new(
            body_area.x,
            body_area.y + body_area.height - 1,
            body_area.width,
            1,
        );
        frame.render_widget(
            Paragraph::new(Span::styled(
                " j/k scroll  PgUp/Dn  / view  y copy response",
                Style::default().fg(theme.comment),
            )),
            hint,
        );
    }
}

fn draw_response_view_bar(frame: &mut Frame, app: &App, area: Rect, theme: Theme) {
    let mut spans: Vec<Span> = vec![Span::styled(" View: ", Style::default().fg(theme.comment))];
    for v in &ResponseView::ALL {
        let sel = app.response.view == *v;
        let style = if sel {
            Style::default()
                .fg(Color::Black)
                .bg(theme.teal)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.comment)
        };
        spans.push(Span::styled(format!(" {} ", v.label()), style));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(
        " / to change",
        Style::default().fg(theme.comment),
    ));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

// ---------------------------------------------------------------------------
// View picker popup
// ---------------------------------------------------------------------------

fn draw_view_picker_popup(frame: &mut Frame, app: &App, theme: Theme) {
    let frame_area = frame.area();

    let popup_w: u16 = 38;
    let popup_h: u16 = (ResponseView::ALL.len() as u16) + 4;

    let x = (frame_area.width.saturating_sub(popup_w)) / 2;
    let y = (frame_area.height.saturating_sub(popup_h)) / 2;
    let popup_area = Rect::new(
        x,
        y,
        popup_w.min(frame_area.width),
        popup_h.min(frame_area.height),
    );

    frame.render_widget(Clear, popup_area);

    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.teal).add_modifier(Modifier::BOLD))
        .title(Span::styled(
            " Response View ",
            Style::default().fg(theme.teal).add_modifier(Modifier::BOLD),
        ));
    let inner = outer.inner(popup_area);
    frame.render_widget(outer, popup_area);

    if inner.height == 0 {
        return;
    }

    let option_h = ResponseView::ALL.len() as u16;
    let hint_h = inner.height.saturating_sub(option_h);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(option_h), Constraint::Min(hint_h)])
        .split(inner);

    let descriptions: [&str; 5] = [
        "detect from Content-Type",
        "pretty-print JSON",
        "syntax highlight HTML",
        "plain text, no markup",
        "raw bytes, no processing",
    ];

    let items: Vec<ListItem> = ResponseView::ALL
        .iter()
        .zip(descriptions.iter())
        .enumerate()
        .map(|(i, (v, desc))| {
            let is_cur = app.response.view == *v;
            let num = Span::styled(format!(" {} ", i + 1), Style::default().fg(theme.comment));
            let label = if is_cur {
                Span::styled(
                    format!("{:<5}", v.label()),
                    Style::default()
                        .fg(Color::Black)
                        .bg(theme.teal)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled(format!("{:<5}", v.label()), Style::default().fg(theme.fg))
            };
            let sep = Span::styled("  ", Style::default());
            let desc = Span::styled(*desc, Style::default().fg(theme.comment));
            ListItem::new(Line::from(vec![num, label, sep, desc]))
        })
        .collect();

    let list = List::new(items).highlight_style(Style::default().bg(Color::Indexed(237)));

    let mut state = ratatui::widgets::ListState::default();
    let cur_idx = ResponseView::ALL
        .iter()
        .position(|v| *v == app.response.view)
        .unwrap_or(0);
    state.select(Some(cur_idx));

    frame.render_stateful_widget(list, chunks[0], &mut state);

    if hint_h > 0 {
        let key = Style::default().fg(theme.teal).add_modifier(Modifier::BOLD);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(" "),
                Span::styled("j/k", key),
                Span::raw(" move  "),
                Span::styled("Enter", key),
                Span::raw(" pick  "),
                Span::styled("1-5", key),
                Span::raw(" quick  "),
                Span::styled("Esc", key),
                Span::raw(" close"),
            ])),
            chunks[1],
        );
    }
}

// ---------------------------------------------------------------------------
// Loader overlay
// ---------------------------------------------------------------------------

fn draw_loader(frame: &mut Frame, app: &App, viewer_area: Rect, theme: Theme) {
    const FRAMES: [&str; 8] = ["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];
    let spinner = FRAMES[(app.loader_tick as usize / 2) % FRAMES.len()];
    let w: u16 = 30;
    let h: u16 = 3;
    if viewer_area.width < w || viewer_area.height < h {
        return;
    }
    let x = viewer_area.x + (viewer_area.width.saturating_sub(w)) / 2;
    let y = viewer_area.y + (viewer_area.height.saturating_sub(h)) / 2;
    let overlay = Rect::new(x, y, w, h);
    frame.render_widget(Clear, overlay);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.teal));
    let inner = block.inner(overlay);
    frame.render_widget(block, overlay);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {} ", spinner),
                Style::default().fg(theme.teal).add_modifier(Modifier::BOLD),
            ),
            Span::styled("Waiting for response…", Style::default().fg(theme.fg)),
        ]))
        .alignment(Alignment::Left),
        inner,
    );
}

// ---------------------------------------------------------------------------
// URL history dropdown
// ---------------------------------------------------------------------------

fn draw_url_history_dropdown(frame: &mut Frame, app: &App, editor_area: Rect, theme: Theme) {
    let dy = editor_area.y + 4;
    let max = 6usize;
    let vis = app.url_history.len().min(max);
    if dy + vis as u16 + 2 > frame.area().height || editor_area.width < 4 {
        return;
    }
    let area = Rect::new(
        editor_area.x + 1,
        dy,
        editor_area.width.saturating_sub(2),
        vis as u16 + 2,
    );
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.teal))
        .title(" URL History (↑/↓) ");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let items: Vec<ListItem> = app
        .url_history
        .iter()
        .take(max)
        .enumerate()
        .map(|(i, url)| {
            let sel = app.url_history_index == Some(i);
            let style = if sel {
                Style::default()
                    .bg(theme.teal)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.fg)
            };
            ListItem::new(Span::styled(format!(" {}", url), style))
        })
        .collect();
    frame.render_widget(List::new(items), inner);
}

// ---------------------------------------------------------------------------
// Custom route dialog
// ---------------------------------------------------------------------------

fn draw_custom_route_dialog(frame: &mut Frame, app: &App, theme: Theme) {
    let Some(d) = &app.custom_route_dialog else {
        return;
    };
    let fa = frame.area();
    let px = if fa.width < 60 { 95u16 } else { 70 };
    let py = if fa.height < 20 { 90u16 } else { 50 };
    let area = centered_rect(px, py, fa);
    frame.render_widget(Clear, area);

    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Add Custom Route ")
        .border_style(Style::default().fg(theme.teal).add_modifier(Modifier::BOLD));
    let inner = outer.inner(area);
    frame.render_widget(outer, area);
    if inner.height < 4 {
        return;
    }

    let av = inner.height;
    let ml = if av >= 2 { 1u16 } else { 0 };
    let mp = if av >= ml + 3 { 3u16 } else { 0 };
    let g1 = if av > ml + mp { 1u16 } else { 0 };
    let pl = if av > ml + mp + g1 { 1u16 } else { 0 };
    let pi = if av > ml + mp + g1 + pl { 3u16 } else { 0 };
    let ht = av.saturating_sub(ml + mp + g1 + pl + pi);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(ml),
            Constraint::Length(mp),
            Constraint::Length(g1),
            Constraint::Length(pl),
            Constraint::Length(pi),
            Constraint::Min(ht),
        ])
        .split(inner);

    let mf = d.active_field == CustomRouteField::Method;
    let pf = d.active_field == CustomRouteField::Path;

    if ml > 0 {
        frame.render_widget(
            Paragraph::new(Span::styled(
                " Method  (← / → to select, Tab to move to Path)",
                if mf {
                    Style::default().fg(theme.teal).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.comment)
                },
            )),
            rows[0],
        );
    }

    if mp > 0 {
        let mb = Block::default()
            .borders(Borders::ALL)
            .border_type(if mf {
                BorderType::Rounded
            } else {
                BorderType::Plain
            })
            .border_style(if mf {
                Style::default().fg(theme.teal)
            } else {
                Style::default().fg(Color::Indexed(238))
            });
        let pill_inner = mb.inner(rows[1]);
        frame.render_widget(mb, rows[1]);
        let mc = HttpMethod::ALL.len() as u32;
        let pc = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(
                (0..mc)
                    .map(|_| Constraint::Ratio(1, mc))
                    .collect::<Vec<_>>(),
            )
            .split(pill_inner);
        for (i, m) in HttpMethod::ALL.iter().enumerate() {
            let is_sel = *m == d.method;
            let (bg, fg, mo) = if is_sel {
                (
                    method_pill_color(m.as_str(), theme),
                    Color::Black,
                    Modifier::BOLD,
                )
            } else {
                (Color::Indexed(236), theme.comment, Modifier::empty())
            };
            frame.render_widget(
                Paragraph::new(Span::styled(
                    format!(" {} ", m.as_str()),
                    Style::default().fg(fg).add_modifier(mo),
                ))
                .bg(bg)
                .alignment(Alignment::Center),
                pc[i],
            );
        }
    }

    if pl > 0 {
        frame.render_widget(
            Paragraph::new(Span::styled(
                " Path  (type route path, Enter to confirm)",
                if pf {
                    Style::default().fg(theme.teal).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.comment)
                },
            )),
            rows[3],
        );
    }

    if pi > 0 {
        let pb = Block::default()
            .borders(Borders::ALL)
            .border_type(if pf {
                BorderType::Rounded
            } else {
                BorderType::Plain
            })
            .border_style(if pf {
                Style::default().fg(theme.yellow)
            } else {
                Style::default().fg(Color::Indexed(238))
            });
        let pc = if pf {
            cursor_line(&d.path, " ")
        } else {
            Line::from(format!(" {}", d.path.text))
        };
        frame.render_widget(Paragraph::new(pc).block(pb), rows[4]);
    }

    if ht > 0 {
        let key = Style::default().fg(theme.teal).add_modifier(Modifier::BOLD);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(" "),
                Span::styled(" ←/→ ", key),
                Span::raw("method  "),
                Span::styled(" Tab ", key),
                Span::raw("next  "),
                Span::styled(" Enter ", key),
                Span::raw("confirm  "),
                Span::styled(" Esc ", key),
                Span::raw("cancel"),
            ])),
            rows[5],
        );
    }
}

// ---------------------------------------------------------------------------
// Auth dialog
// ---------------------------------------------------------------------------

fn draw_auth_dialog(frame: &mut Frame, app: &App, theme: Theme) {
    let Some(dialog) = &app.auth_dialog else {
        return;
    };

    let fa = frame.area();
    let px = if fa.width < 60 { 95u16 } else { 70 };
    let py = if fa.height < 20 { 90u16 } else { 50 };
    let area = centered_rect(px, py, fa);
    frame.render_widget(Clear, area);

    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Authentication ")
        .border_style(Style::default().fg(theme.teal).add_modifier(Modifier::BOLD));
    let inner = outer.inner(area);
    frame.render_widget(outer, area);
    if inner.height < 4 {
        return;
    }

    let draft = app.current_draft();
    let is_type_focused = dialog.active_field == AuthDialogField::TypeSelector;

    let av = inner.height;
    let ml = if av >= 2 { 1u16 } else { 0 };
    let mp = if av >= ml + 3 { 3u16 } else { 0 };
    let g1 = if av > ml + mp { 1u16 } else { 0 };
    let pl = if av > ml + mp + g1 { 1u16 } else { 0 };
    let fixed_rows = ml + mp + g1 + pl;
    let ht = if av > fixed_rows + 1 { 1u16 } else { 0 };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(ml),
            Constraint::Length(mp),
            Constraint::Length(g1),
            Constraint::Length(pl),
            Constraint::Min(0), // input fields: expands to fill all remaining space
            Constraint::Length(ht), // keybinding hint: pinned to 1 row
        ])
        .split(inner);

    if ml > 0 {
        frame.render_widget(
            Paragraph::new(Span::styled(
                " Auth Type  (← / → to select, Tab to move to inputs)",
                if is_type_focused {
                    Style::default().fg(theme.teal).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.comment)
                },
            )),
            rows[0],
        );
    }

    if mp > 0 {
        let mb = Block::default()
            .borders(Borders::ALL)
            .border_type(if is_type_focused {
                BorderType::Rounded
            } else {
                BorderType::Plain
            })
            .border_style(if is_type_focused {
                Style::default().fg(theme.teal)
            } else {
                Style::default().fg(Color::Indexed(238))
            });
        let pill_inner = mb.inner(rows[1]);
        frame.render_widget(mb, rows[1]);
        let mc = AuthType::ALL.len() as u32;
        let pc = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(
                (0..mc)
                    .map(|_| Constraint::Ratio(1, mc))
                    .collect::<Vec<_>>(),
            )
            .split(pill_inner);
        for (i, t) in AuthType::ALL.iter().enumerate() {
            let is_sel = *t == draft.auth_type;
            let (bg, fg, mo) = if is_sel {
                (
                    method_pill_color(t.label(), theme),
                    Color::Black,
                    Modifier::BOLD,
                )
            } else {
                (Color::Indexed(236), theme.comment, Modifier::empty())
            };
            frame.render_widget(
                Paragraph::new(Span::styled(
                    format!(" {} ", t.label()),
                    Style::default().fg(fg).add_modifier(mo),
                ))
                .bg(bg)
                .alignment(Alignment::Center),
                pc[i],
            );
        }
    }

    if pl > 0 {
        frame.render_widget(
            Paragraph::new(Span::styled(
                " Type a credential below when a mode is selected",
                Style::default().fg(theme.comment),
            )),
            rows[3],
        );
    }

    if rows[4].height > 0 {
        if draft.auth_type == AuthType::None {
            let pb = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Plain)
                .border_style(Style::default().fg(Color::Indexed(238)));
            frame.render_widget(
                Paragraph::new(Span::styled(
                    " No Authentication ",
                    Style::default()
                        .fg(theme.comment)
                        .add_modifier(Modifier::BOLD),
                ))
                .block(pb),
                rows[4],
            );
        } else {
            draw_auth_dialog_fields(frame, app, rows[4], theme, is_type_focused);
        }
    }

    if ht > 0 {
        let key = Style::default().fg(theme.teal).add_modifier(Modifier::BOLD);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(" "),
                Span::styled(" ←/→ ", key),
                Span::raw("select  "),
                Span::styled(" Tab ", key),
                Span::raw("inputs  "),
                Span::styled(" Enter ", key),
                Span::raw("confirm  "),
                Span::styled(" Esc ", key),
                Span::raw("close"),
            ])),
            rows[5],
        );
    }
}

fn draw_auth_dialog_fields(
    frame: &mut Frame,
    app: &App,
    area: Rect,
    theme: Theme,
    _is_type_focused: bool,
) {
    if area.height < 2 {
        return;
    }

    let draft = app.current_draft();
    let in_input_mode = app.input_mode;
    let dialog = app.auth_dialog.as_ref().unwrap();
    let is_field_focused = dialog.active_field == AuthDialogField::InputFields;

    match draft.auth_type {
        // -----------------------------------------------------------------
        // Basic Auth  — needs: label(1) + input(3) + label(1) + input(3) = 8
        // -----------------------------------------------------------------
        AuthType::BasicAuth => {
            if area.height < 5 {
                frame.render_widget(
                    Paragraph::new(Span::styled(
                        " ↕ Resize terminal to see fields",
                        Style::default()
                            .fg(theme.orange)
                            .add_modifier(Modifier::BOLD),
                    ))
                    .alignment(Alignment::Center),
                    area,
                );
                return;
            }

            let label_rows: u16 = 2;
            let hint_row: u16 = if area.height >= 7 { 1 } else { 0 };
            let input_space = area.height.saturating_sub(label_rows + hint_row);
            let input1_h = (input_space / 2).max(1);
            let input2_h = input_space.saturating_sub(input1_h).max(1);

            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),        // username label
                    Constraint::Length(input1_h), // username input
                    Constraint::Length(1),        // password label
                    Constraint::Length(input2_h), // password input
                    Constraint::Length(hint_row), // inline hint
                    Constraint::Min(0),           // leftover
                ])
                .split(area);

            frame.render_widget(
                Paragraph::new(Span::styled(
                    " Username",
                    Style::default()
                        .fg(theme.comment)
                        .add_modifier(Modifier::BOLD),
                )),
                rows[0],
            );

            let username_focused = in_input_mode && is_field_focused && draft.active_col == 0;
            let username_block = Block::default()
                .borders(Borders::ALL)
                .border_type(if username_focused {
                    BorderType::Rounded
                } else {
                    BorderType::Plain
                })
                .border_style(if username_focused {
                    Style::default().fg(theme.yellow)
                } else {
                    Style::default().fg(Color::Indexed(238))
                });
            let username_line = if username_focused {
                let len = draft.auth_username.text.len();
                let cursor = draft.auth_username.cursor.min(len);
                let before = &draft.auth_username.text[..cursor];
                let after = &draft.auth_username.text[cursor..];
                Line::from(vec![
                    Span::raw(format!(" {}", before)),
                    Span::styled(" ", Style::default().bg(Color::Indexed(238))),
                    Span::raw(after.to_string()),
                ])
            } else {
                let visible = if draft.auth_username.text.is_empty() {
                    " username".to_string()
                } else {
                    format!(" {}", draft.auth_username.text)
                };
                Line::from(Span::styled(
                    visible,
                    if draft.auth_username.text.is_empty() {
                        Style::default().fg(theme.comment)
                    } else {
                        Style::default().fg(theme.fg)
                    },
                ))
            };
            frame.render_widget(Paragraph::new(username_line).block(username_block), rows[1]);

            frame.render_widget(
                Paragraph::new(Span::styled(
                    " Password",
                    Style::default()
                        .fg(theme.comment)
                        .add_modifier(Modifier::BOLD),
                )),
                rows[2],
            );

            let password_focused = in_input_mode && is_field_focused && draft.active_col == 1;
            let password_block = Block::default()
                .borders(Borders::ALL)
                .border_type(if password_focused {
                    BorderType::Rounded
                } else {
                    BorderType::Plain
                })
                .border_style(if password_focused {
                    Style::default().fg(theme.yellow)
                } else {
                    Style::default().fg(Color::Indexed(238))
                });
            let password_line = if password_focused {
                let len = draft.auth_password.text.len();
                let cursor = draft.auth_password.cursor.min(len);
                let before = "*".repeat(cursor);
                let after = "*".repeat(len.saturating_sub(cursor));
                Line::from(vec![
                    Span::raw(format!(" {}", before)),
                    Span::styled(" ", Style::default().bg(Color::Indexed(238))),
                    Span::raw(after),
                ])
            } else {
                let visible = if draft.auth_password.text.is_empty() {
                    " password".to_string()
                } else {
                    format!(" {}", "*".repeat(draft.auth_password.text.len()))
                };
                Line::from(Span::styled(
                    visible,
                    if draft.auth_password.text.is_empty() {
                        Style::default().fg(theme.comment)
                    } else {
                        Style::default().fg(theme.fg)
                    },
                ))
            };
            frame.render_widget(Paragraph::new(password_line).block(password_block), rows[3]);

            if hint_row > 0 {
                frame.render_widget(
                    Paragraph::new(Span::styled(
                        " Tab move between fields   Esc close",
                        Style::default().fg(theme.comment),
                    )),
                    rows[4],
                );
            }
        }

        // -----------------------------------------------------------------
        // Bearer Token  — needs: label(1) + input(3) = 4
        // -----------------------------------------------------------------
        AuthType::BearerToken => {
            if area.height < 3 {
                frame.render_widget(
                    Paragraph::new(Span::styled(
                        " ↕ Resize terminal to see fields",
                        Style::default()
                            .fg(theme.orange)
                            .add_modifier(Modifier::BOLD),
                    ))
                    .alignment(Alignment::Center),
                    area,
                );
                return;
            }

            let hint_row: u16 = if area.height >= 5 { 1 } else { 0 };
            let input_h = area.height.saturating_sub(1 + hint_row).max(1);

            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),        // label
                    Constraint::Length(input_h),  // input
                    Constraint::Length(hint_row), // hint
                    Constraint::Min(0),
                ])
                .split(area);

            frame.render_widget(
                Paragraph::new(Span::styled(
                    " Token",
                    Style::default()
                        .fg(theme.comment)
                        .add_modifier(Modifier::BOLD),
                )),
                rows[0],
            );

            let token_focused = in_input_mode && is_field_focused;
            let token_block = Block::default()
                .borders(Borders::ALL)
                .border_type(if token_focused {
                    BorderType::Rounded
                } else {
                    BorderType::Plain
                })
                .border_style(if token_focused {
                    Style::default().fg(theme.yellow)
                } else {
                    Style::default().fg(Color::Indexed(238))
                });
            let token_line = if token_focused {
                let len = draft.auth_token.text.len();
                let cursor = draft.auth_token.cursor.min(len);
                let before = &draft.auth_token.text[..cursor];
                let after = &draft.auth_token.text[cursor..];
                Line::from(vec![
                    Span::raw(format!(" {}", before)),
                    Span::styled(" ", Style::default().bg(Color::Indexed(238))),
                    Span::raw(after.to_string()),
                ])
            } else {
                let visible = if draft.auth_token.text.is_empty() {
                    " token".to_string()
                } else {
                    format!(" {}", draft.auth_token.text)
                };
                Line::from(Span::styled(
                    visible,
                    if draft.auth_token.text.is_empty() {
                        Style::default().fg(theme.comment)
                    } else {
                        Style::default().fg(theme.fg)
                    },
                ))
            };
            frame.render_widget(Paragraph::new(token_line).block(token_block), rows[1]);

            if hint_row > 0 {
                frame.render_widget(
                    Paragraph::new(Span::styled(
                        " Esc close",
                        Style::default().fg(theme.comment),
                    )),
                    rows[2],
                );
            }
        }

        // -----------------------------------------------------------------
        // API Key  — needs: label(1) + input(3) + label(1) + input(3) = 8
        // -----------------------------------------------------------------
        AuthType::ApiKey => {
            if area.height < 5 {
                frame.render_widget(
                    Paragraph::new(Span::styled(
                        " ↕ Resize terminal to see fields",
                        Style::default()
                            .fg(theme.orange)
                            .add_modifier(Modifier::BOLD),
                    ))
                    .alignment(Alignment::Center),
                    area,
                );
                return;
            }

            let label_rows: u16 = 2;
            let hint_row: u16 = if area.height >= 7 { 1 } else { 0 };
            let input_space = area.height.saturating_sub(label_rows + hint_row);
            let input1_h = (input_space / 2).max(1);
            let input2_h = input_space.saturating_sub(input1_h).max(1);

            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),        // header name label
                    Constraint::Length(input1_h), // header name input
                    Constraint::Length(1),        // header value label
                    Constraint::Length(input2_h), // header value input
                    Constraint::Length(hint_row), // hint
                    Constraint::Min(0),
                ])
                .split(area);

            frame.render_widget(
                Paragraph::new(Span::styled(
                    " Header Name",
                    Style::default()
                        .fg(theme.comment)
                        .add_modifier(Modifier::BOLD),
                )),
                rows[0],
            );

            let header_focused = in_input_mode && is_field_focused && draft.active_col == 0;
            let header_block = Block::default()
                .borders(Borders::ALL)
                .border_type(if header_focused {
                    BorderType::Rounded
                } else {
                    BorderType::Plain
                })
                .border_style(if header_focused {
                    Style::default().fg(theme.yellow)
                } else {
                    Style::default().fg(Color::Indexed(238))
                });
            let header_line = if header_focused {
                let len = draft.auth_header_name.text.len();
                let cursor = draft.auth_header_name.cursor.min(len);
                let before = &draft.auth_header_name.text[..cursor];
                let after = &draft.auth_header_name.text[cursor..];
                Line::from(vec![
                    Span::raw(format!(" {}", before)),
                    Span::styled(" ", Style::default().bg(Color::Indexed(238))),
                    Span::raw(after.to_string()),
                ])
            } else {
                let visible = if draft.auth_header_name.text.is_empty() {
                    " header name".to_string()
                } else {
                    format!(" {}", draft.auth_header_name.text)
                };
                Line::from(Span::styled(
                    visible,
                    if draft.auth_header_name.text.is_empty() {
                        Style::default().fg(theme.comment)
                    } else {
                        Style::default().fg(theme.fg)
                    },
                ))
            };
            frame.render_widget(Paragraph::new(header_line).block(header_block), rows[1]);

            frame.render_widget(
                Paragraph::new(Span::styled(
                    " Header Value",
                    Style::default()
                        .fg(theme.comment)
                        .add_modifier(Modifier::BOLD),
                )),
                rows[2],
            );

            let value_focused = in_input_mode && is_field_focused && draft.active_col == 1;
            let value_block = Block::default()
                .borders(Borders::ALL)
                .border_type(if value_focused {
                    BorderType::Rounded
                } else {
                    BorderType::Plain
                })
                .border_style(if value_focused {
                    Style::default().fg(theme.yellow)
                } else {
                    Style::default().fg(Color::Indexed(238))
                });
            let value_line = if value_focused {
                let len = draft.auth_header_value.text.len();
                let cursor = draft.auth_header_value.cursor.min(len);
                let before = &draft.auth_header_value.text[..cursor];
                let after = &draft.auth_header_value.text[cursor..];
                Line::from(vec![
                    Span::raw(format!(" {}", before)),
                    Span::styled(" ", Style::default().bg(Color::Indexed(238))),
                    Span::raw(after.to_string()),
                ])
            } else {
                let visible = if draft.auth_header_value.text.is_empty() {
                    " header value".to_string()
                } else {
                    format!(" {}", draft.auth_header_value.text)
                };
                Line::from(Span::styled(
                    visible,
                    if draft.auth_header_value.text.is_empty() {
                        Style::default().fg(theme.comment)
                    } else {
                        Style::default().fg(theme.fg)
                    },
                ))
            };
            frame.render_widget(Paragraph::new(value_line).block(value_block), rows[3]);

            if hint_row > 0 {
                frame.render_widget(
                    Paragraph::new(Span::styled(
                        " Tab move between fields   Esc close",
                        Style::default().fg(theme.comment),
                    )),
                    rows[4],
                );
            }
        }

        AuthType::None => {}
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn centered_rect(px: u16, py: u16, r: Rect) -> Rect {
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - py) / 2),
            Constraint::Percentage(py),
            Constraint::Percentage((100 - py) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - px) / 2),
            Constraint::Percentage(px),
            Constraint::Percentage((100 - px) / 2),
        ])
        .split(v[1])[1]
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
    let tb = &buf.text[..buf.cursor.min(buf.text.len())];
    let ci = tb.chars().filter(|c| *c == '\n').count();
    buf.text
        .lines()
        .enumerate()
        .map(|(i, line)| {
            if i == ci {
                let ls: usize = buf.text.lines().take(i).map(|l| l.len() + 1).sum();
                let col = buf.cursor.saturating_sub(ls).min(line.len());
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

fn pane_block(
    title: &str,
    focused: bool,
    theme: Theme,
    res_left: bool,
    res_right: bool,
) -> Block<'_> {
    let (color, bt) = if res_left || res_right {
        (theme.teal, BorderType::Thick)
    } else if focused {
        (theme.teal, BorderType::Thick)
    } else {
        (theme.comment, BorderType::Plain)
    };
    Block::default()
        .borders(Borders::ALL)
        .border_type(bt)
        .border_style(Style::default().fg(color))
        .title(Span::styled(
            title,
            Style::default().add_modifier(Modifier::BOLD).fg(color),
        ))
}

fn method_color(m: &str, theme: Theme) -> Color {
    match m {
        "GET" => Color::Green,
        "POST" => theme.blue,
        "PUT" => theme.yellow,
        "PATCH" => theme.pink,
        "DELETE" => Color::Red,
        _ => theme.teal,
    }
}

fn method_pill_color(m: &str, theme: Theme) -> Color {
    match m {
        "GET" => Color::Green,
        "POST" => theme.blue,
        "PUT" => theme.yellow,
        "PATCH" => theme.pink,
        "DELETE" => Color::Red,
        "OPTIONS" => theme.teal,
        "HEAD" => Color::Indexed(208),
        _ => theme.teal,
    }
}
