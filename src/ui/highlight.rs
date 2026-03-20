/// Highlighting module.
///
/// Design goals:
/// 1. ZERO latency on the HTTP response path — highlighting is never on the
///    critical path between "bytes received" and "body visible on screen".
/// 2. Fast path for Text/Raw/large bodies: simple line-split, Vesper fg colour,
///    no syntect at all.
/// 3. Syntect only for JSON/HTML/XML bodies under 200 KB — still fast because
///    the statics are pre-warmed at startup (awaited before the event loop).
/// 4. Viewport-aware: the draw function only renders the visible window, so
///    even a 10 000-line body costs nothing extra.
use once_cell::sync::Lazy;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use syntect::easy::HighlightLines;
use syntect::highlighting::Color as SColor;
use syntect::highlighting::{FontStyle, Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

// ---------------------------------------------------------------------------
// Statics
// ---------------------------------------------------------------------------

static SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(SyntaxSet::load_defaults_newlines);
static THEME: Lazy<Theme> = Lazy::new(|| {
    ThemeSet::load_defaults()
        .themes
        .get("base16-ocean.dark")
        .cloned()
        .unwrap_or_default()
});

/// Block until both statics are initialised.  Call this with
/// `tokio::task::spawn_blocking(highlight::prewarm).await` before the event
/// loop starts so the first HTTP response is rendered instantly.
pub fn prewarm() {
    let _ = &*SYNTAX_SET;
    let _ = &*THEME;
}

// Bodies larger than this use the fast plain-text path even for JSON/HTML.
const HIGHLIGHT_SIZE_LIMIT: usize = 200 * 1024; // 200 KB

// ---------------------------------------------------------------------------
// Vesper palette
// ---------------------------------------------------------------------------

pub const V_FG: Color = Color::Rgb(0xcc, 0xc9, 0xc2);
pub const V_COMMENT: Color = Color::Rgb(0x4d, 0x4d, 0x4d);
pub const V_ORANGE: Color = Color::Rgb(0xff, 0x98, 0x57);
pub const V_YELLOW: Color = Color::Rgb(0xe5, 0xc0, 0x7b);
pub const V_TEAL: Color = Color::Rgb(0x5c, 0xb8, 0xb2);
pub const V_BLUE: Color = Color::Rgb(0x5b, 0xa2, 0xd0);
pub const V_PINK: Color = Color::Rgb(0xd6, 0x7a, 0x9c);

// ---------------------------------------------------------------------------
// ResponseView
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseView {
    Auto,
    Json,
    Html,
    Text,
    Raw,
}

impl Default for ResponseView {
    fn default() -> Self {
        Self::Auto
    }
}

impl ResponseView {
    pub const ALL: [ResponseView; 5] = [Self::Auto, Self::Json, Self::Html, Self::Text, Self::Raw];

    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Json => "JSON",
            Self::Html => "HTML",
            Self::Text => "Text",
            Self::Raw => "Raw",
        }
    }

    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|v| *v == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Self {
        let i = Self::ALL.iter().position(|v| *v == self).unwrap_or(0);
        Self::ALL[(i + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

// ---------------------------------------------------------------------------
// Main render entry point
// ---------------------------------------------------------------------------

/// Convert a raw response body into highlighted ratatui Lines.
///
/// This function is called either:
///   a) when the user changes the view mode (body already in RAM → instant), or
///   b) as a blocking task spawned AFTER the raw body has already been shown.
///
/// It is NEVER called on the critical path between "bytes received" and
/// "first render of the response".
pub fn render_body(content_type: &str, body: &str, view: ResponseView) -> Vec<Line<'static>> {
    if body.is_empty() {
        return vec![Line::from(Span::styled(
            "No response body",
            Style::default().fg(V_COMMENT),
        ))];
    }

    let effective = match view {
        ResponseView::Auto => detect(content_type),
        ResponseView::Json => "json",
        ResponseView::Html => "html",
        ResponseView::Text => "text",
        ResponseView::Raw => "raw",
    };

    // Fast path — no syntect
    if effective == "raw" || effective == "text" || body.len() > HIGHLIGHT_SIZE_LIMIT {
        return plain_lines(body);
    }

    // Pretty-print JSON
    let normalized: String = if effective == "json" {
        serde_json::from_str::<serde_json::Value>(body)
            .map(|v| serde_json::to_string_pretty(&v).unwrap_or_else(|_| body.to_string()))
            .unwrap_or_else(|_| body.to_string())
    } else {
        body.to_string()
    };

    let syntax = match effective {
        "json" => SYNTAX_SET.find_syntax_by_extension("json"),
        "html" => SYNTAX_SET.find_syntax_by_extension("html"),
        "xml" => SYNTAX_SET.find_syntax_by_extension("xml"),
        _ => Some(SYNTAX_SET.find_syntax_plain_text()),
    }
    .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text());

    let mut hl = HighlightLines::new(syntax, &THEME);
    let mut lines: Vec<Line<'static>> = Vec::with_capacity(normalized.lines().count());

    for line in LinesWithEndings::from(&normalized) {
        match hl.highlight_line(line, &SYNTAX_SET) {
            Ok(regions) => {
                let spans: Vec<Span> = regions
                    .into_iter()
                    .filter_map(|(style, text)| {
                        let clean = text.trim_end_matches('\n');
                        if clean.is_empty() {
                            return None;
                        }
                        Some(Span::styled(clean.to_string(), to_ratatui(style)))
                    })
                    .collect();
                lines.push(Line::from(spans));
            }
            Err(_) => {
                lines.push(Line::from(Span::styled(
                    line.trim_end_matches('\n').to_string(),
                    Style::default().fg(V_FG),
                )));
            }
        }
    }
    lines
}

/// Cheap plain-text render — no syntect, O(n) in line count.
pub fn plain_lines(body: &str) -> Vec<Line<'static>> {
    body.lines()
        .map(|l| Line::from(Span::styled(l.to_string(), Style::default().fg(V_FG))))
        .collect()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn detect(content_type: &str) -> &'static str {
    if content_type.contains("json") {
        "json"
    } else if content_type.contains("html") {
        "html"
    } else if content_type.contains("xml") {
        "xml"
    } else {
        "text"
    }
}

fn to_ratatui(style: syntect::highlighting::Style) -> Style {
    let mut s = Style::default().fg(map_color(style.foreground));
    if style.font_style.contains(FontStyle::BOLD) {
        s = s.add_modifier(Modifier::BOLD);
    }
    if style.font_style.contains(FontStyle::ITALIC) {
        s = s.add_modifier(Modifier::ITALIC);
    }
    s
}

fn map_color(c: SColor) -> Color {
    let r = c.r as i32;
    let g = c.g as i32;
    let b = c.b as i32;
    let lum = r + g + b;
    if lum < 120 {
        return V_COMMENT;
    }
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    if max - min < 30 {
        return V_FG;
    }
    if r >= g && r >= b {
        if g >= b { V_ORANGE } else { V_PINK }
    } else if g >= r && g >= b {
        V_YELLOW
    } else {
        if g >= r { V_TEAL } else { V_BLUE }
    }
}
