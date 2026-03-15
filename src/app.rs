use std::collections::HashMap;
use std::fmt;
use std::time::Instant;

use arboard::Clipboard;
use ratatui::text::{Line, Span};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::config::{AppConfig, RouteConfig};
use crate::http::{self, HttpError, HttpResult};
use crate::scanner::{RouteInfo, ScannerReport};
use crate::ui::highlight;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusPane {
    Explorer,
    Editor,
    Viewer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EditorTab {
    Headers,
    Body,
    Params,
    Auth,
}

impl EditorTab {
    pub const ALL: [EditorTab; 4] = [Self::Headers, Self::Body, Self::Params, Self::Auth];

    pub fn title(self) -> &'static str {
        match self {
            Self::Headers => "Headers",
            Self::Body => "Body",
            Self::Params => "Params",
            Self::Auth => "Auth",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Headers => Self::Body,
            Self::Body => Self::Params,
            Self::Params => Self::Auth,
            Self::Auth => Self::Headers,
        }
    }

    fn prev(self) -> Self {
        match self {
            Self::Headers => Self::Auth,
            Self::Body => Self::Headers,
            Self::Params => Self::Body,
            Self::Auth => Self::Params,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputTarget {
    BaseUrl,
    Tab(EditorTab),
}

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
        if let Some(next) = self.text[self.cursor..]
            .char_indices()
            .nth(1)
            .map(|(idx, _)| self.cursor + idx)
            .or(Some(self.text.len()))
        {
            self.text.drain(self.cursor..next);
        }
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
}

#[derive(Debug, Clone)]
pub struct RequestDraft {
    pub base_url: TextBuffer,
    pub headers: TextBuffer,
    pub body: TextBuffer,
    pub params: TextBuffer,
    pub auth: TextBuffer,
}

impl RequestDraft {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: TextBuffer::new(base_url),
            headers: TextBuffer::default(),
            body: TextBuffer::default(),
            params: TextBuffer::default(),
            auth: TextBuffer::default(),
        }
    }

    pub fn buffer_mut(&mut self, target: InputTarget) -> &mut TextBuffer {
        match target {
            InputTarget::BaseUrl => &mut self.base_url,
            InputTarget::Tab(EditorTab::Headers) => &mut self.headers,
            InputTarget::Tab(EditorTab::Body) => &mut self.body,
            InputTarget::Tab(EditorTab::Params) => &mut self.params,
            InputTarget::Tab(EditorTab::Auth) => &mut self.auth,
        }
    }

    pub fn buffer(&self, target: InputTarget) -> &TextBuffer {
        match target {
            InputTarget::BaseUrl => &self.base_url,
            InputTarget::Tab(EditorTab::Headers) => &self.headers,
            InputTarget::Tab(EditorTab::Body) => &self.body,
            InputTarget::Tab(EditorTab::Params) => &self.params,
            InputTarget::Tab(EditorTab::Auth) => &self.auth,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ResponseState {
    pub status_code: Option<u16>,
    pub latency_ms: Option<u128>,
    pub size_bytes: usize,
    pub content_type: String,
    pub body: String,
    pub highlighted: Vec<Line<'static>>,
    pub error: Option<String>,
}

pub struct App {
    pub routes: Vec<RouteInfo>,
    pub filtered_routes: Vec<RouteInfo>,
    pub search_query: String,
    pub search_active: bool,
    pub scanner_report: ScannerReport,
    pub selected_route: usize,
    pub focus: FocusPane,
    pub editor_tab: EditorTab,
    pub input_mode: bool,
    pub input_target: InputTarget,
    pub should_quit: bool,
    pub pending_request: bool,
    pub editor_scroll: u16,
    pub viewer_scroll: u16,
    pub response: ResponseState,
    pub status_message: String,
    pub drafts: HashMap<String, RequestDraft>,
    pub env_vars: HashMap<String, String>,
    pub clipboard: Option<Clipboard>,
    pub request_tx: mpsc::UnboundedSender<HttpResult>,
    pub config: AppConfig,
    pub launch_instant: Instant,
}

impl App {
    pub fn new(
        report: ScannerReport,
        config: AppConfig,
        request_tx: mpsc::UnboundedSender<HttpResult>,
    ) -> Self {
        let routes = report.routes.clone();
        let filtered_routes = routes.clone();
        
        // Load env variables
        let _ = dotenvy::dotenv();
        let env_vars: HashMap<String, String> = std::env::vars().collect();

        // Convert stored RouteConfigs to RequestDrafts
        let mut drafts = HashMap::new();
        for (id, route_cfg) in &config.drafts {
            let mut draft = RequestDraft::new(config.base_url.clone());
            draft.headers = TextBuffer::new(&route_cfg.headers);
            draft.body = TextBuffer::new(&route_cfg.body);
            draft.params = TextBuffer::new(&route_cfg.params);
            draft.auth = TextBuffer::new(&route_cfg.auth);
            drafts.insert(id.clone(), draft);
        }

        let status_message = if routes.is_empty() {
            "No routes detected in the current directory".to_string()
        } else {
            format!(
                "Detected {} route(s) across {} file(s) ({} env vars loaded)",
                routes.len(),
                report.files_scanned,
                env_vars.len()
            )
        };
        let selected_route = config
            .last_selected_route
            .as_ref()
            .and_then(|id| routes.iter().position(|route| route.id() == *id))
            .unwrap_or(0);
        Self {
            routes,
            filtered_routes,
            search_query: String::new(),
            search_active: false,
            scanner_report: report,
            selected_route,
            focus: FocusPane::Explorer,
            editor_tab: EditorTab::Headers,
            input_mode: false,
            input_target: InputTarget::Tab(EditorTab::Headers),
            should_quit: false,
            pending_request: false,
            editor_scroll: 0,
            viewer_scroll: 0,
            response: ResponseState::default(),
            status_message,
            drafts,
            env_vars,
            clipboard: Clipboard::new().ok(),
            request_tx,
            config,
            launch_instant: Instant::now(),
        }
    }

    pub fn resolve_variables(&self, input: &str) -> String {
        let mut result = input.to_string();
        for (key, value) in &self.env_vars {
            let placeholder = format!("{{{{{}}}}}", key);
            result = result.replace(&placeholder, value);
        }
        result
    }

    pub fn prettify_current_body(&mut self) {
        let target = InputTarget::Tab(EditorTab::Body);
        let body = self.current_draft().buffer(target).text.trim().to_string();
        if body.is_empty() {
            return;
        }

        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&body) {
            if let Ok(pretty) = serde_json::to_string_pretty(&value) {
                self.current_draft_mut().buffer_mut(target).text = pretty;
                self.current_draft_mut().buffer_mut(target).move_end();
                self.status_message = "Prettified JSON body".to_string();
            }
        } else {
            self.status_message = "Invalid JSON - cannot prettify".to_string();
        }
    }

    pub fn current_route(&self) -> Option<&RouteInfo> {
        self.filtered_routes.get(self.selected_route)
    }

    fn current_route_key(&self) -> String {
        self.current_route()
            .map(|route| route.id())
            .unwrap_or_else(|| "default".to_string())
    }

    pub fn current_draft_mut(&mut self) -> &mut RequestDraft {
        let route_key = self.current_route_key();
        self.drafts
            .entry(route_key)
            .or_insert_with(|| RequestDraft::new(self.config.base_url.clone()))
    }

    pub fn current_draft(&self) -> RequestDraft {
        self.drafts
            .get(&self.current_route_key())
            .cloned()
            .unwrap_or_else(|| RequestDraft::new(self.config.base_url.clone()))
    }

    pub fn active_buffer_mut(&mut self) -> &mut TextBuffer {
        let target = self.input_target;
        self.current_draft_mut().buffer_mut(target)
    }

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

    pub fn move_down(&mut self) {
        match self.focus {
            FocusPane::Explorer => {
                if !self.filtered_routes.is_empty() {
                    self.selected_route =
                        (self.selected_route + 1).min(self.filtered_routes.len() - 1);
                }
            }
            FocusPane::Editor => self.editor_scroll = self.editor_scroll.saturating_add(1),
            FocusPane::Viewer => self.viewer_scroll = self.viewer_scroll.saturating_add(1),
        }
    }

    pub fn move_up(&mut self) {
        match self.focus {
            FocusPane::Explorer => {
                if self.selected_route > 0 {
                    self.selected_route -= 1;
                }
            }
            FocusPane::Editor => self.editor_scroll = self.editor_scroll.saturating_sub(1),
            FocusPane::Viewer => self.viewer_scroll = self.viewer_scroll.saturating_sub(1),
        }
    }

    pub fn start_search(&mut self) {
        self.search_active = true;
        self.focus = FocusPane::Explorer;
        self.status_message = "Search mode: type to filter routes (Esc to stop)".to_string();
    }

    pub fn stop_search(&mut self) {
        self.search_active = false;
        if self.search_query.is_empty() {
            self.status_message = "Ready".to_string();
        } else {
            self.status_message = format!("Filtered {} routes", self.filtered_routes.len());
        }
    }

    pub fn on_search_input(&mut self, ch: char) {
        self.search_query.push(ch);
        self.update_filtered_routes();
    }

    pub fn on_search_backspace(&mut self) {
        self.search_query.pop();
        self.update_filtered_routes();
    }

    pub fn update_filtered_routes(&mut self) {
        if self.search_query.is_empty() {
            self.filtered_routes = self.routes.clone();
        } else {
            let query = self.search_query.to_lowercase();
            self.filtered_routes = self
                .routes
                .iter()
                .filter(|r| {
                    r.path.to_lowercase().contains(&query)
                        || r.method.as_str().to_lowercase().contains(&query)
                })
                .cloned()
                .collect();
        }
        self.selected_route = 0;
    }

    pub fn next_tab(&mut self) {
        self.editor_tab = self.editor_tab.next();
        self.editor_scroll = 0;
    }

    pub fn prev_tab(&mut self) {
        self.editor_tab = self.editor_tab.prev();
        self.editor_scroll = 0;
    }

    pub fn set_tab(&mut self, tab: EditorTab) {
        self.editor_tab = tab;
        self.editor_scroll = 0;
    }

    pub fn start_editing(&mut self, target: InputTarget) {
        self.input_mode = true;
        self.input_target = target;
        self.status_message = match target {
            InputTarget::BaseUrl => "Insert mode: editing base URL (Esc to stop)".to_string(),
            InputTarget::Tab(tab) => {
                format!(
                    "Insert mode: editing {} (Esc to stop)",
                    tab.title().to_lowercase()
                )
            }
        };
    }

    pub fn stop_editing(&mut self) {
        self.input_mode = false;
        self.status_message =
            format!("Ready - Tab focus, i edit tab, u edit base URL, r send, c copy, q quit");
        self.config.base_url = self.current_draft().base_url.text;
    }

    pub async fn execute_current_request(&mut self) {
        let Some(route) = self.current_route().cloned() else {
            self.status_message = "No route selected".to_string();
            return;
        };

        let draft = self.current_draft();
        
        // Resolve environment variables in the draft copy
        let mut resolved_draft = draft.clone();
        resolved_draft.base_url.text = self.resolve_variables(&draft.base_url.text);
        resolved_draft.headers.text = self.resolve_variables(&draft.headers.text);
        resolved_draft.body.text = self.resolve_variables(&draft.body.text);
        resolved_draft.params.text = self.resolve_variables(&draft.params.text);
        resolved_draft.auth.text = self.resolve_variables(&draft.auth.text);

        self.pending_request = true;
        self.response.error = None;
        self.status_message = format!("Running {} {}", route.method, route.path);
        let tx = self.request_tx.clone();
        tokio::spawn(async move {
            let result = http::execute(route, resolved_draft).await;
            let _ = tx.send(result);
        });
    }

    pub fn apply_http_result(&mut self, result: HttpResult) {
        self.pending_request = false;
        match result {
            Ok(response) => {
                self.response.status_code = Some(response.status_code);
                self.response.latency_ms = Some(response.latency_ms);
                self.response.size_bytes = response.size_bytes;
                self.response.content_type = response.content_type.clone();
                self.response.body = response.body.clone();
                self.response.highlighted =
                    highlight::render_body(&response.content_type, &response.body);
                self.response.error = None;
                self.status_message = format!(
                    "{} in {}ms ({} bytes)",
                    response.status_code, response.latency_ms, response.size_bytes
                );
            }
            Err(error) => {
                self.response.error = Some(error.to_string());
                self.response.status_code = None;
                self.response.latency_ms = None;
                self.response.size_bytes = 0;
                self.response.content_type.clear();
                self.response.body.clear();
                self.response.highlighted = vec![Line::from(error.to_string())];
                self.status_message = match error {
                    HttpError::Validation(message) => format!("Validation error: {message}"),
                    other => format!("Request failed: {other}"),
                };
            }
        }
    }

    pub fn copy_response_to_clipboard(&mut self) {
        if self.response.body.is_empty() {
            self.status_message = "No response body to copy".to_string();
            return;
        }
        let Some(clipboard) = &mut self.clipboard else {
            self.status_message = "Clipboard is not available in this environment".to_string();
            return;
        };

        match clipboard.set_text(self.response.body.clone()) {
            Ok(()) => self.status_message = "Copied response body to clipboard".to_string(),
            Err(error) => self.status_message = format!("Clipboard copy failed: {error}"),
        }
    }

    pub fn export_as_curl(&mut self) {
        let Some(route) = self.current_route().cloned() else {
            self.status_message = "No route selected to export".to_string();
            return;
        };
        let draft = self.current_draft();
        
        // Build cURL command
        let mut curl = format!("curl -X {}", route.method.as_str());
        
        let url = if draft.base_url.text.ends_with('/') || route.path.starts_with('/') {
            format!("{}{}", draft.base_url.text.trim_end_matches('/'), route.path)
        } else {
             format!("{}/{}", draft.base_url.text, route.path)
        };
        curl.push_str(&format!(" \"{}\"", url));

        // Headers
        for line in draft.headers.text.lines() {
            if let Some((key, value)) = line.split_once(':') {
                curl.push_str(&format!(" -H \"{}: {}\"", key.trim(), value.trim()));
            }
        }
        
        // Auth
        if !draft.auth.text.is_empty() {
             curl.push_str(&format!(" -H \"Authorization: {}\"", draft.auth.text.trim()));
        }

        // Body
        if !draft.body.text.is_empty() {
             // Escape single quotes for shell safety if needed, 
             // but for now simple escaping is a good start.
             let body_escaped = draft.body.text.replace("\"", "\\\"");
             curl.push_str(&format!(" -d \"{}\"", body_escaped));
        }

        let Some(clipboard) = &mut self.clipboard else {
            self.status_message = "Clipboard not available".to_string();
            return;
        };

        match clipboard.set_text(curl) {
            Ok(()) => self.status_message = "Copied cURL command to clipboard".to_string(),
            Err(e) => self.status_message = format!("Failed to copy cURL: {}", e),
        }
    }

    pub fn persist_config(&self) -> anyhow::Result<()> {
        let mut config = self.config.clone();
        config.base_url = self.current_draft().base_url.text;
        config.last_selected_route = self.current_route().map(|route| route.id());
        config.save()
    }

    pub fn route_details(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        if let Some(route) = self.current_route() {
            lines.push(Line::from(vec![
                Span::raw("Framework: "),
                Span::styled(
                    route.framework.clone(),
                    ratatui::style::Style::default().fg(ratatui::style::Color::Cyan),
                ),
            ]));
            lines.push(Line::from(format!(
                "Source: {}:{}",
                route.source.display(),
                route.line
            )));
        } else {
            lines.push(Line::from("No route selected"));
        }
        lines.push(Line::from(format!(
            "Scanner: {} files in {}ms",
            self.scanner_report.files_scanned, self.scanner_report.duration_ms
        )));
        lines.push(Line::from(format!(
            "Startup: {}ms",
            self.launch_instant.elapsed().as_millis()
        )));
        lines
    }
}

pub fn human_bytes(size: usize) -> String {
    if size < 1024 {
        return format!("{size} B");
    }
    if size < 1024 * 1024 {
        return format!("{:.1} KB", size as f64 / 1024.0);
    }
    format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn mock_route(method: HttpMethod, path: &str) -> RouteInfo {
        RouteInfo {
            method,
            path: path.to_string(),
            framework: "test".to_string(),
            source: PathBuf::from("test.rs"),
            line: 1,
        }
    }

    fn mock_app() -> App {
        let routes = vec![
            mock_route(HttpMethod::Get, "/users"),
            mock_route(HttpMethod::Post, "/users"),
            mock_route(HttpMethod::Get, "/posts"),
            mock_route(HttpMethod::Delete, "/posts/:id"),
        ];
        let report = ScannerReport {
            routes,
            files_scanned: 1,
            duration_ms: 10,
        };
        let (tx, _) = mpsc::unbounded_channel();
        App::new(report, AppConfig::default(), tx)
    }

    #[test]
    fn filters_routes_by_path_and_method() {
        let mut app = mock_app();

        // Initial state
        assert_eq!(app.filtered_routes.len(), 4);

        // Filter by path
        app.search_query = "users".to_string();
        app.update_filtered_routes();
        assert_eq!(app.filtered_routes.len(), 2);
        assert!(app.filtered_routes.iter().all(|r| r.path.contains("users")));

        // Filter by method
        app.search_query = "delete".to_string();
        app.update_filtered_routes();
        assert_eq!(app.filtered_routes.len(), 1);
        assert_eq!(app.filtered_routes[0].method, HttpMethod::Delete);
    }
    
    #[test]
    fn resolves_environment_variables() {
        let mut app = mock_app();
        app.env_vars.insert("BASE".to_string(), "http://api.local".to_string());
        app.env_vars.insert("TOKEN".to_string(), "secret123".to_string());

        let input = "{{BASE}}/users";
        assert_eq!(app.resolve_variables(input), "http://api.local/users");

        let input = "Bearer {{TOKEN}}";
        assert_eq!(app.resolve_variables(input), "Bearer secret123");

        let input = "No variables";
        assert_eq!(app.resolve_variables(input), "No variables");
        
        let input = "{{MISSING}}";
        assert_eq!(app.resolve_variables(input), "{{MISSING}}");
    }
}
