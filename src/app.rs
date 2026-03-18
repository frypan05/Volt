#![allow(dead_code)]

use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

use arboard::Clipboard;
use ratatui::text::Line;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::config::AppConfig;
use crate::http::{self, HttpResult};
use crate::scanner::{RouteInfo, ScannerReport};
use crate::ui::highlight;

// ---------------------------------------------------------------------------
// HttpMethod
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Options,
    Head,
}

impl HttpMethod {
    pub const ALL: [HttpMethod; 7] = [
        Self::Get,
        Self::Post,
        Self::Put,
        Self::Patch,
        Self::Delete,
        Self::Options,
        Self::Head,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Options => "OPTIONS",
            Self::Head => "HEAD",
        }
    }

    pub fn cycle_next(self) -> Self {
        let idx = Self::ALL.iter().position(|m| *m == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    pub fn cycle_prev(self) -> Self {
        let idx = Self::ALL.iter().position(|m| *m == self).unwrap_or(0);
        Self::ALL[(idx + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

impl fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl TryFrom<&str> for HttpMethod {
    type Error = ();
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.to_ascii_lowercase().as_str() {
            "get" => Ok(Self::Get),
            "post" => Ok(Self::Post),
            "put" => Ok(Self::Put),
            "patch" => Ok(Self::Patch),
            "delete" => Ok(Self::Delete),
            "options" => Ok(Self::Options),
            "head" => Ok(Self::Head),
            _ => Err(()),
        }
    }
}

// ---------------------------------------------------------------------------
// UI state enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusPane {
    Explorer,
    Editor,
    Viewer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EditorTab {
    Params,
    Headers,
    Auth,
    Body,
}

impl EditorTab {
    pub const ALL: [EditorTab; 4] = [Self::Params, Self::Headers, Self::Auth, Self::Body];
    pub fn title(self) -> &'static str {
        match self {
            Self::Params => "Params",
            Self::Headers => "Headers",
            Self::Auth => "Auth",
            Self::Body => "Body",
        }
    }
    pub fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }
    pub fn prev(self) -> Self {
        let idx = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        Self::ALL[(idx + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputTarget {
    BaseUrl,
    Tab(EditorTab),
}

// ---------------------------------------------------------------------------
// TextBuffer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct TextBuffer {
    pub text: String,
    pub cursor: usize,
}

impl TextBuffer {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let cursor = text.len();
        Self { text, cursor }
    }
    pub fn insert_char(&mut self, ch: char) {
        self.text.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
    }
    pub fn insert_newline(&mut self) {
        self.insert_char('\n');
    }
    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        if let Some(prev) = self.text[..self.cursor]
            .char_indices()
            .last()
            .map(|(idx, _)| idx)
        {
            self.text.drain(prev..self.cursor);
            self.cursor = prev;
        }
    }
    pub fn delete(&mut self) {
        if self.cursor >= self.text.len() {
            return;
        }
        let next = self.text[self.cursor..]
            .char_indices()
            .nth(1)
            .map(|(idx, _)| self.cursor + idx)
            .unwrap_or(self.text.len());
        self.text.drain(self.cursor..next);
    }
    pub fn move_left(&mut self) {
        if let Some(prev) = self.text[..self.cursor]
            .char_indices()
            .last()
            .map(|(idx, _)| idx)
        {
            self.cursor = prev;
        }
    }
    pub fn move_right(&mut self) {
        if self.cursor >= self.text.len() {
            return;
        }
        if let Some((idx, ch)) = self.text[self.cursor..].char_indices().next() {
            self.cursor += idx + ch.len_utf8();
        }
    }
    pub fn move_home(&mut self) {
        self.cursor = 0;
    }
    pub fn move_end(&mut self) {
        self.cursor = self.text.len();
    }
    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }
    pub fn split_at_cursor(&self) -> (&str, &str, &str) {
        if self.cursor >= self.text.len() {
            (&self.text, " ", "")
        } else {
            let ch_end = self.text[self.cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor + i)
                .unwrap_or(self.text.len());
            (
                &self.text[..self.cursor],
                &self.text[self.cursor..ch_end],
                &self.text[ch_end..],
            )
        }
    }
}

// ---------------------------------------------------------------------------
// KVRow / BodyType / RequestDraft
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct KVRow {
    pub key: TextBuffer,
    pub value: TextBuffer,
    pub enabled: bool,
}

impl KVRow {
    pub fn new() -> Self {
        Self {
            key: TextBuffer::default(),
            value: TextBuffer::default(),
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyType {
    Json,
    Text,
    FormUrlEncoded,
    None,
}

impl BodyType {
    pub const ALL: [BodyType; 4] = [Self::None, Self::Json, Self::Text, Self::FormUrlEncoded];
    pub fn label(self) -> &'static str {
        match self {
            Self::Json => "JSON",
            Self::Text => "Text",
            Self::FormUrlEncoded => "Form",
            Self::None => "None",
        }
    }
    pub fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }
    pub fn prev(self) -> Self {
        let idx = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        Self::ALL[(idx + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Clone)]
pub struct RequestDraft {
    pub base_url: TextBuffer,
    pub headers: Vec<KVRow>,
    pub body: TextBuffer,
    pub body_type: BodyType,
    pub params: Vec<KVRow>,
    pub auth: Vec<KVRow>,
    pub active_row: usize,
    pub active_col: usize,
}

impl RequestDraft {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: TextBuffer::new(base_url),
            headers: vec![KVRow::new()],
            body: TextBuffer::default(),
            body_type: BodyType::None,
            params: vec![KVRow::new()],
            auth: vec![KVRow::new()],
            active_row: 0,
            active_col: 0,
        }
    }

    pub fn active_buffer_mut(&mut self, target: InputTarget) -> &mut TextBuffer {
        match target {
            InputTarget::BaseUrl => &mut self.base_url,
            InputTarget::Tab(EditorTab::Body) => &mut self.body,
            InputTarget::Tab(tab) => {
                let rows = match tab {
                    EditorTab::Headers => &mut self.headers,
                    EditorTab::Params => &mut self.params,
                    EditorTab::Auth => &mut self.auth,
                    EditorTab::Body => unreachable!(),
                };
                while rows.len() <= self.active_row {
                    rows.push(KVRow::new());
                }
                if self.active_col == 0 {
                    &mut rows[self.active_row].key
                } else {
                    &mut rows[self.active_row].value
                }
            }
        }
    }

    pub fn add_row(&mut self, tab: EditorTab) {
        match tab {
            EditorTab::Headers => self.headers.push(KVRow::new()),
            EditorTab::Params => self.params.push(KVRow::new()),
            EditorTab::Auth => self.auth.push(KVRow::new()),
            EditorTab::Body => return,
        }
        let len = match tab {
            EditorTab::Headers => self.headers.len(),
            EditorTab::Params => self.params.len(),
            EditorTab::Auth => self.auth.len(),
            EditorTab::Body => return,
        };
        self.active_row = len - 1;
        self.active_col = 0;
    }
}

// ---------------------------------------------------------------------------
// ResponseState
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct ResponseState {
    pub status_code: Option<u16>,
    pub latency_ms: Option<u128>,
    pub size_bytes: usize,
    pub content_type: String,
    pub body: String,
    pub highlighted: Vec<Line<'static>>,
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

pub struct App {
    pub routes: Vec<RouteInfo>,
    pub filtered_routes: Vec<RouteInfo>,
    pub scanner_report: ScannerReport,
    pub selected_route: usize,
    pub focus: FocusPane,
    pub editor_tab: EditorTab,
    pub input_mode: bool,
    pub input_target: InputTarget,
    pub drafts: HashMap<String, RequestDraft>,
    pub url_history: Vec<String>,
    pub url_history_index: Option<usize>,
    pub url_history_open: bool,
    pub viewer_scroll: u16,
    pub response: ResponseState,
    pub should_quit: bool,
    pub pending_request: bool,
    pub loader_tick: u8,
    pub status_message: String,
    pub env_vars: HashMap<String, String>,
    pub clipboard: Option<Clipboard>,
    pub request_tx: mpsc::UnboundedSender<HttpResult>,
    pub custom_route_dialog: Option<CustomRouteDialog>,
    pub last_area: ratatui::layout::Rect,
    pub http_client: reqwest::Client,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustomRouteField {
    Method,
    Path,
}

#[derive(Debug, Clone)]
pub struct CustomRouteDialog {
    pub method: HttpMethod,
    pub path: TextBuffer,
    pub active_field: CustomRouteField,
}

impl App {
    pub fn new(
        report: ScannerReport,
        config: AppConfig,
        request_tx: mpsc::UnboundedSender<HttpResult>,
    ) -> Self {
        let routes = report.routes.clone();
        let _ = dotenvy::dotenv();
        let env_vars: HashMap<String, String> = std::env::vars().collect();

        let default_base_url = "http://localhost:3000".to_string();
        let mut url_history: Vec<String> = vec![default_base_url.clone()];
        if !config.base_url.is_empty() && config.base_url != default_base_url {
            url_history.insert(0, config.base_url.clone());
        }

        // Built once; cloning is an Arc clone — shares the connection pool.
        let http_client = reqwest::Client::builder()
            .tcp_nodelay(true)
            .pool_max_idle_per_host(8)
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .expect("failed to build reqwest client");

        Self {
            routes: routes.clone(),
            filtered_routes: routes,
            scanner_report: report,
            selected_route: 0,
            focus: FocusPane::Explorer,
            editor_tab: EditorTab::Params,
            input_mode: false,
            input_target: InputTarget::Tab(EditorTab::Params),
            drafts: HashMap::new(),
            url_history,
            url_history_index: None,
            url_history_open: false,
            viewer_scroll: 0,
            response: ResponseState::default(),
            should_quit: false,
            pending_request: false,
            loader_tick: 0,
            status_message: "Ready".to_string(),
            env_vars,
            clipboard: Clipboard::new().ok(),
            request_tx,
            custom_route_dialog: None,
            last_area: ratatui::layout::Rect::default(),
            http_client,
        }
    }

    // -- URL history -------------------------------------------------------

    pub fn cycle_url_history(&mut self, up: bool) {
        if self.url_history.is_empty() {
            return;
        }
        let len = self.url_history.len();
        let idx = match self.url_history_index {
            None => {
                if up {
                    0
                } else {
                    len - 1
                }
            }
            Some(i) => {
                if up {
                    (i + 1) % len
                } else {
                    (i + len - 1) % len
                }
            }
        };
        self.url_history_index = Some(idx);
        let url = self.url_history[idx].clone();
        self.current_draft_mut().base_url = TextBuffer::new(url);
    }

    // -- Viewer scroll -----------------------------------------------------

    pub fn scroll_viewer(&mut self, up: bool) {
        if up {
            self.viewer_scroll = self.viewer_scroll.saturating_sub(1);
        } else {
            self.viewer_scroll = self.viewer_scroll.saturating_add(1);
        }
    }

    pub fn scroll_viewer_page(&mut self, up: bool) {
        if up {
            self.viewer_scroll = self.viewer_scroll.saturating_sub(10);
        } else {
            self.viewer_scroll = self.viewer_scroll.saturating_add(10);
        }
    }

    // -- Mouse -------------------------------------------------------------

    pub fn handle_mouse_click(&mut self, col: u16, row: u16) {
        let area = self.last_area;
        let explorer_width = (area.width as f32 * 0.25) as u16;
        let editor_width = (area.width as f32 * 0.35) as u16;

        if col < explorer_width {
            self.focus = FocusPane::Explorer;
            if row >= 2 {
                let idx = (row - 2) as usize;
                if idx <= self.filtered_routes.len() {
                    self.selected_route = idx;
                }
            }
        } else if col < explorer_width + editor_width {
            self.focus = FocusPane::Editor;
            if row >= 2 && row <= 4 {
                self.start_editing(InputTarget::BaseUrl);
            } else if row == 5 || row == 6 {
                let rel_x = col.saturating_sub(explorer_width);
                let tab_w = editor_width.max(4) / 4;
                let idx = (rel_x / tab_w).min(3) as usize;
                self.editor_tab = EditorTab::ALL[idx];
            } else if row >= 9 {
                let data_row = (row - 9) as usize;
                let inner_x = col.saturating_sub(explorer_width + 1);
                let key_end = 5 + (editor_width as f32 * 0.47) as u16;
                if inner_x < 5 {
                    let tab = self.editor_tab;
                    let draft = self.current_draft_mut();
                    let rows = match tab {
                        EditorTab::Headers => &mut draft.headers,
                        EditorTab::Params => &mut draft.params,
                        EditorTab::Auth => &mut draft.auth,
                        EditorTab::Body => return,
                    };
                    if let Some(r) = rows.get_mut(data_row) {
                        r.enabled = !r.enabled;
                    }
                } else {
                    self.start_editing(InputTarget::Tab(self.editor_tab));
                    let d = self.current_draft_mut();
                    d.active_row = data_row;
                    d.active_col = if inner_x < key_end { 0 } else { 1 };
                }
            }
        } else {
            self.focus = FocusPane::Viewer;
        }
    }

    // -- Draft helpers -----------------------------------------------------

    pub fn current_route(&self) -> Option<&RouteInfo> {
        self.filtered_routes.get(self.selected_route)
    }

    pub fn current_draft_mut(&mut self) -> &mut RequestDraft {
        let key = self
            .current_route()
            .map(|r| r.id())
            .unwrap_or_else(|| "default".to_string());
        let base = self
            .url_history
            .last()
            .cloned()
            .unwrap_or_else(|| "http://localhost:3000".to_string());
        self.drafts
            .entry(key)
            .or_insert_with(|| RequestDraft::new(base))
    }

    pub fn current_draft(&self) -> RequestDraft {
        let key = self
            .current_route()
            .map(|r| r.id())
            .unwrap_or_else(|| "default".to_string());
        self.drafts.get(&key).cloned().unwrap_or_else(|| {
            RequestDraft::new(
                self.url_history
                    .last()
                    .cloned()
                    .unwrap_or_else(|| "http://localhost:3000".to_string()),
            )
        })
    }

    pub fn active_buffer_mut(&mut self) -> &mut TextBuffer {
        let target = self.input_target;
        self.current_draft_mut().active_buffer_mut(target)
    }

    pub fn add_kv_row(&mut self) {
        let tab = self.editor_tab;
        if tab == EditorTab::Body {
            return;
        }
        self.current_draft_mut().add_row(tab);
        self.input_mode = true;
        self.input_target = InputTarget::Tab(tab);
    }

    // -- Row navigation ----------------------------------------------------

    pub fn move_row(&mut self, up: bool) {
        let tab = self.editor_tab;
        if tab == EditorTab::Body {
            return;
        }
        let d = self.current_draft_mut();
        if up {
            d.active_row = d.active_row.saturating_sub(1);
        } else {
            d.active_row += 1;
            match tab {
                EditorTab::Headers => {
                    while d.headers.len() <= d.active_row {
                        d.headers.push(KVRow::new());
                    }
                }
                EditorTab::Params => {
                    while d.params.len() <= d.active_row {
                        d.params.push(KVRow::new());
                    }
                }
                EditorTab::Auth => {
                    while d.auth.len() <= d.active_row {
                        d.auth.push(KVRow::new());
                    }
                }
                _ => {}
            }
        }
    }

    // -- Focus -------------------------------------------------------------

    pub fn focus_next(&mut self) {
        self.focus = match self.focus {
            FocusPane::Explorer => FocusPane::Editor,
            FocusPane::Editor => FocusPane::Viewer,
            FocusPane::Viewer => FocusPane::Explorer,
        };
    }

    pub fn focus_prev(&mut self) {
        self.focus = match self.focus {
            FocusPane::Explorer => FocusPane::Viewer,
            FocusPane::Editor => FocusPane::Explorer,
            FocusPane::Viewer => FocusPane::Editor,
        };
    }

    // -- Editing -----------------------------------------------------------

    pub fn start_editing(&mut self, target: InputTarget) {
        self.input_mode = true;
        self.input_target = target;
        if target == InputTarget::BaseUrl {
            self.url_history_open = true;
            self.url_history_index = None;
        }
    }

    pub fn stop_editing(&mut self) {
        self.input_mode = false;
        self.url_history_open = false;
        self.url_history_index = None;
        let url = self.current_draft().base_url.text.clone();
        if !url.is_empty() && !self.url_history.contains(&url) {
            self.url_history.insert(0, url);
        }
    }

    // -- HTTP --------------------------------------------------------------

    pub async fn execute_current_request(&mut self) {
        let Some(route) = self.current_route().cloned() else {
            return;
        };
        self.pending_request = true;
        self.loader_tick = 0;
        self.viewer_scroll = 0;
        let draft = self.current_draft();
        let tx = self.request_tx.clone();
        let client = self.http_client.clone();
        tokio::spawn(async move {
            let res = http::execute(client, route, draft).await;
            // Highlighting is CPU work — do it here on a background thread
            let res = res.map(|mut r| {
                r.highlighted = highlight::render_body(&r.content_type, &r.body);
                r
            });
            let _ = tx.send(res);
        });
    }

    pub fn apply_http_result(&mut self, result: HttpResult) {
        self.pending_request = false;
        match result {
            Ok(res) => {
                self.response.status_code = Some(res.status_code);
                self.response.latency_ms = Some(res.latency_ms);
                self.response.size_bytes = res.size_bytes;
                self.response.content_type = res.content_type;
                self.response.body = res.body;
                self.response.highlighted = res.highlighted;
                self.status_message = format!("{} in {}ms", res.status_code, res.latency_ms);
            }
            Err(e) => {
                self.status_message = format!("Error: {}", e);
            }
        }
    }

    // -- Loader ------------------------------------------------------------

    pub fn tick_loader(&mut self) {
        if self.pending_request {
            self.loader_tick = self.loader_tick.wrapping_add(1);
        }
    }

    // -- Custom route dialog -----------------------------------------------

    pub fn open_custom_route_dialog(&mut self) {
        self.custom_route_dialog = Some(CustomRouteDialog {
            method: HttpMethod::Get,
            path: TextBuffer::new("/"),
            active_field: CustomRouteField::Method,
        });
    }

    pub fn confirm_custom_route(&mut self) {
        if let Some(d) = self.custom_route_dialog.take() {
            self.routes.push(RouteInfo {
                method: d.method,
                path: d.path.text,
                framework: "custom".into(),
                source: std::path::PathBuf::from("custom"),
                line: 0,
            });
            self.filtered_routes = self.routes.clone();
            self.selected_route = self.filtered_routes.len() - 1;
        }
    }

    pub fn selected_is_add_custom(&self) -> bool {
        self.selected_route == self.filtered_routes.len()
    }
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

pub fn human_bytes(size: usize) -> String {
    if size < 1024 {
        format!("{} B", size)
    } else {
        format!("{:.1} KB", size as f64 / 1024.0)
    }
}
