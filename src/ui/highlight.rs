use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

pub fn render_body(content_type: &str, body: &str) -> Vec<Line<'static>> {
    let normalized = if content_type.contains("json") {
        serde_json::from_str::<serde_json::Value>(body)
            .map(|value| serde_json::to_string_pretty(&value).unwrap_or_else(|_| body.to_string()))
            .unwrap_or_else(|_| body.to_string())
    } else {
        body.to_string()
    };

    if normalized.is_empty() {
        return vec![Line::from("No response body")];
    }

    let syntax_set = SyntaxSet::load_defaults_newlines();
    let theme = theme();
    let syntax = if content_type.contains("json") {
        syntax_set.find_syntax_by_extension("json")
    } else if content_type.contains("html") {
        syntax_set.find_syntax_by_extension("html")
    } else if content_type.contains("xml") {
        syntax_set.find_syntax_by_extension("xml")
    } else {
        Some(syntax_set.find_syntax_plain_text())
    }
    .unwrap_or_else(|| syntax_set.find_syntax_plain_text());

    let mut highlighter = HighlightLines::new(syntax, &theme);
    let mut lines = Vec::new();
    for line in LinesWithEndings::from(&normalized) {
        let spans = highlighter
            .highlight_line(line, &syntax_set)
            .map(|regions| {
                regions
                    .into_iter()
                    .map(|(style, text)| Span::styled(text.to_string(), to_style(style.foreground)))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|_| vec![Span::raw(line.to_string())]);
        lines.push(Line::from(spans));
    }
    lines
}

fn to_style(color: syntect::highlighting::Color) -> Style {
    Style::default().fg(Color::Rgb(color.r, color.g, color.b))
}

fn theme() -> Theme {
    let themes = ThemeSet::load_defaults();
    themes
        .themes
        .get("InspiredGitHub")
        .cloned()
        .or_else(|| themes.themes.values().next().cloned())
        .unwrap_or_default()
}
