use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use ignore::WalkBuilder;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::app::HttpMethod;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct RouteInfo {
    pub method: HttpMethod,
    pub path: String,
    pub framework: String,
    pub source: PathBuf,
    pub line: usize,
}

impl RouteInfo {
    pub fn id(&self) -> String {
        format!(
            "{}:{}:{}:{}",
            self.framework,
            self.method.as_str(),
            self.path,
            self.source.display()
        )
    }
}

#[derive(Debug, Clone)]
pub struct ScannerReport {
    pub routes: Vec<RouteInfo>,
    pub files_scanned: usize,
    pub duration_ms: u128,
}

pub fn scan_current_dir() -> anyhow::Result<ScannerReport> {
    scan_dir(&std::env::current_dir()?)
}

pub fn scan_dir(root: &Path) -> anyhow::Result<ScannerReport> {
    let started_at = Instant::now();
    let mut routes = BTreeSet::new();
    let mut files_scanned = 0usize;

    let walker = WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_exclude(true)
        .parents(true)
        .build();

    for entry in walker.flatten() {
        if !entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
        {
            continue;
        }
        let path = entry.path();
        if !is_supported(path) {
            continue;
        }
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        files_scanned += 1;
        for route in extract_routes(path, &content) {
            routes.insert(route);
        }
    }

    Ok(ScannerReport {
        routes: routes.into_iter().collect(),
        files_scanned,
        duration_ms: started_at.elapsed().as_millis(),
    })
}

fn is_supported(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("rs" | "js" | "ts" | "py")
    )
}

fn extract_routes(path: &Path, content: &str) -> Vec<RouteInfo> {
    let mut routes = Vec::new();
    routes.extend(extract_axum(path, content));
    routes.extend(extract_actix(path, content));
    routes.extend(extract_express(path, content));
    routes.extend(extract_fastapi(path, content));
    routes
}

fn extract_axum(path: &Path, content: &str) -> Vec<RouteInfo> {
    if !content.contains("axum") && !content.contains("Router::new") {
        return Vec::new();
    }
    let route_re = Regex::new(r#"\.route\(\s*\"([^\"]+)\"\s*,\s*([^;\n]+)"#).unwrap();
    let method_re = Regex::new(r#"\b(get|post|put|patch|delete|options|head)\b"#).unwrap();
    let mut routes = Vec::new();
    for captures in route_re.captures_iter(content) {
        let path_value = captures.get(1).map(|m| m.as_str()).unwrap_or_default();
        let handler_expr = captures.get(2).map(|m| m.as_str()).unwrap_or_default();
        let line = line_number(content, captures.get(0).map(|m| m.start()).unwrap_or(0));
        for method_cap in method_re.captures_iter(handler_expr) {
            if let Ok(method) = HttpMethod::try_from(method_cap.get(1).unwrap().as_str()) {
                routes.push(RouteInfo {
                    method,
                    path: path_value.to_string(),
                    framework: "axum".to_string(),
                    source: path.to_path_buf(),
                    line,
                });
            }
        }
    }
    routes
}

fn extract_actix(path: &Path, content: &str) -> Vec<RouteInfo> {
    if !content.contains("actix_web") && !content.contains("web::") {
        return Vec::new();
    }
    let attr_re =
        Regex::new(r#"#\[(get|post|put|patch|delete|head|options)\(\"([^\"]+)\""#).unwrap();
    let route_re = Regex::new(
        r#"\.route\(\s*\"([^\"]+)\"\s*,\s*web::(get|post|put|patch|delete|head|options)\(\)"#,
    )
    .unwrap();
    let mut routes = Vec::new();
    for captures in attr_re.captures_iter(content) {
        if let Ok(method) = HttpMethod::try_from(captures.get(1).unwrap().as_str()) {
            routes.push(RouteInfo {
                method,
                path: captures.get(2).unwrap().as_str().to_string(),
                framework: "actix-web".to_string(),
                source: path.to_path_buf(),
                line: line_number(content, captures.get(0).unwrap().start()),
            });
        }
    }
    for captures in route_re.captures_iter(content) {
        if let Ok(method) = HttpMethod::try_from(captures.get(2).unwrap().as_str()) {
            routes.push(RouteInfo {
                method,
                path: captures.get(1).unwrap().as_str().to_string(),
                framework: "actix-web".to_string(),
                source: path.to_path_buf(),
                line: line_number(content, captures.get(0).unwrap().start()),
            });
        }
    }
    routes
}

fn extract_express(path: &Path, content: &str) -> Vec<RouteInfo> {
    if !content.contains("express")
        && !content.contains("fastify")
        && !content.contains("router.")
        && !content.contains("app.")
    {
        return Vec::new();
    }
    let route_re = Regex::new(
        r#"\b(?:app|router|server|fastify)\.(get|post|put|patch|delete|options|head)\(\s*[\"']([^\"']+)[\"']"#,
    )
    .unwrap();
    let mut routes = Vec::new();
    let framework = if content.contains("fastify") {
        "fastify"
    } else {
        "express"
    };
    for captures in route_re.captures_iter(content) {
        if let Ok(method) = HttpMethod::try_from(captures.get(1).unwrap().as_str()) {
            routes.push(RouteInfo {
                method,
                path: captures.get(2).unwrap().as_str().to_string(),
                framework: framework.to_string(),
                source: path.to_path_buf(),
                line: line_number(content, captures.get(0).unwrap().start()),
            });
        }
    }
    routes
}

fn extract_fastapi(path: &Path, content: &str) -> Vec<RouteInfo> {
    if !content.contains("FastAPI") && !content.contains("@app.") && !content.contains("@router.") {
        return Vec::new();
    }
    let route_re = Regex::new(
        r#"@(app|router)\.(get|post|put|patch|delete|options|head)\(\s*[\"']([^\"']+)[\"']"#,
    )
    .unwrap();
    let mut routes = Vec::new();
    for captures in route_re.captures_iter(content) {
        if let Ok(method) = HttpMethod::try_from(captures.get(2).unwrap().as_str()) {
            routes.push(RouteInfo {
                method,
                path: captures.get(3).unwrap().as_str().to_string(),
                framework: "fastapi".to_string(),
                source: path.to_path_buf(),
                line: line_number(content, captures.get(0).unwrap().start()),
            });
        }
    }
    routes
}

fn line_number(content: &str, byte_offset: usize) -> usize {
    content[..byte_offset]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> (PathBuf, String) {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        let content = fs::read_to_string(&path).unwrap();
        (path, content)
    }

    #[test]
    fn detects_routes_across_supported_frameworks() {
        let fixture_names = [
            "axum.rs",
            "actix.rs",
            "express.ts",
            "fastify.ts",
            "fastapi.py",
        ];
        let mut routes = Vec::new();
        for name in fixture_names {
            let (path, content) = fixture(name);
            routes.extend(extract_routes(&path, &content));
        }

        assert!(
            routes
                .iter()
                .any(|route| route.framework == "axum" && route.path == "/v1/login")
        );
        assert!(
            routes
                .iter()
                .any(|route| route.framework == "actix-web" && route.path == "/health")
        );
        assert!(
            routes
                .iter()
                .any(|route| route.framework == "express" && route.path == "/users/:id")
        );
        assert!(
            routes
                .iter()
                .any(|route| route.framework == "fastify" && route.path == "/teams")
        );
        assert!(
            routes
                .iter()
                .any(|route| route.framework == "fastapi" && route.path == "/items")
        );
    }
}
