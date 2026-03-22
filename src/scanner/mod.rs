use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

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

/// On-disk representation of a user-created custom route.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedRoute {
    #[serde(flatten)]
    pub route: RouteInfo,
    pub base_url: String,
}

#[derive(Debug, Clone)]
pub struct ScannerReport {
    pub routes: Vec<RouteInfo>,
    pub persisted_base_urls: std::collections::HashMap<String, String>,
}

pub const CUSTOM_ROUTES_FILE: &str = ".volt_routes.json";

pub fn load_persisted_routes(root: &Path) -> Vec<PersistedRoute> {
    let path = root.join(CUSTOM_ROUTES_FILE);
    let Ok(content) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<PersistedRoute>>(&content).unwrap_or_default()
}

pub fn save_custom_routes(
    root: &Path,
    routes: &[RouteInfo],
    base_urls: &std::collections::HashMap<String, String>,
) {
    let persisted: Vec<PersistedRoute> = routes
        .iter()
        .filter(|r| r.framework == "custom")
        .map(|r| PersistedRoute {
            base_url: base_urls.get(&r.id()).cloned().unwrap_or_default(),
            route: r.clone(),
        })
        .collect();
    let path = root.join(CUSTOM_ROUTES_FILE);
    if let Ok(json) = serde_json::to_string_pretty(&persisted) {
        let _ = fs::write(path, json);
    }
}

pub fn scan_current_dir() -> anyhow::Result<ScannerReport> {
    scan_dir(&std::env::current_dir()?)
}

pub fn scan_dir(root: &Path) -> anyhow::Result<ScannerReport> {
    let mut routes = BTreeSet::new();

    let walker = WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_exclude(true)
        .parents(true)
        .max_depth(Some(9))
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
        for route in extract_routes(path, &content) {
            routes.insert(route);
        }
    }

    let mut persisted_base_urls = std::collections::HashMap::new();
    for pr in load_persisted_routes(root) {
        persisted_base_urls.insert(pr.route.id(), pr.base_url);
        routes.insert(pr.route);
    }

    Ok(ScannerReport {
        routes: routes.into_iter().collect(),
        persisted_base_urls,
    })
}

fn is_supported(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("rs" | "js" | "ts" | "py" | "jsx" | "tsx" | "vue" | "svelte")
    )
}

fn extract_routes(path: &Path, content: &str) -> Vec<RouteInfo> {
    let mut routes = Vec::new();
    routes.extend(extract_axum(path, content));
    routes.extend(extract_actix(path, content));
    routes.extend(extract_express(path, content));
    routes.extend(extract_fastapi(path, content));
    routes.extend(extract_nextjs(path, content));
    routes.extend(extract_react_router(path, content));
    routes.extend(extract_vue_router(path, content));
    routes.extend(extract_svelte_kit(path, content));
    routes.extend(extract_angular(path, content));
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

fn extract_nextjs(path: &Path, content: &str) -> Vec<RouteInfo> {
    let filename = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
    let path_str = path.to_string_lossy();
    let mut routes = Vec::new();

    // App Router — page.tsx / page.jsx / page.ts / page.js
    if matches!(filename, "page.tsx" | "page.jsx" | "page.ts" | "page.js") {
        if let Some(route_path) = nextjs_app_path(path) {
            let method_re = Regex::new(
                r#"export\s+(?:async\s+)?function\s+(GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS)"#,
            )
            .unwrap();
            let mut found_methods = false;
            for cap in method_re.captures_iter(content) {
                if let Ok(method) =
                    HttpMethod::try_from(cap.get(1).unwrap().as_str().to_lowercase().as_str())
                {
                    routes.push(RouteInfo {
                        method,
                        path: route_path.clone(),
                        framework: "next.js".to_string(),
                        source: path.to_path_buf(),
                        line: line_number(content, cap.get(0).unwrap().start()),
                    });
                    found_methods = true;
                }
            }
            if !found_methods {
                routes.push(RouteInfo {
                    method: HttpMethod::Get,
                    path: route_path,
                    framework: "next.js".to_string(),
                    source: path.to_path_buf(),
                    line: 1,
                });
            }
        }
    }

    // App Router API — route.ts / route.js
    if matches!(
        filename,
        "route.ts" | "route.js" | "route.tsx" | "route.jsx"
    ) {
        if let Some(route_path) = nextjs_app_path(path) {
            let method_re = Regex::new(
                r#"export\s+(?:async\s+)?function\s+(GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS)"#,
            )
            .unwrap();
            for cap in method_re.captures_iter(content) {
                if let Ok(method) =
                    HttpMethod::try_from(cap.get(1).unwrap().as_str().to_lowercase().as_str())
                {
                    routes.push(RouteInfo {
                        method,
                        path: route_path.clone(),
                        framework: "next.js".to_string(),
                        source: path.to_path_buf(),
                        line: line_number(content, cap.get(0).unwrap().start()),
                    });
                }
            }
        }
    }

    // Pages Router — files under /pages/ excluding special files
    if path_str.contains("/pages/") || path_str.contains("\\pages\\") {
        if !matches!(
            filename,
            "_app.tsx"
                | "_app.jsx"
                | "_app.js"
                | "_app.ts"
                | "_document.tsx"
                | "_document.jsx"
                | "_document.js"
                | "_document.ts"
                | "_error.tsx"
                | "_error.jsx"
                | "_error.js"
                | "_error.ts"
        ) {
            if let Some(route_path) = nextjs_pages_path(path) {
                let api_re =
                    Regex::new(r#"export\s+(?:default\s+)?(?:async\s+)?function\s+handler"#)
                        .unwrap();
                if api_re.is_match(content) {
                    let method_check_re = Regex::new(
                        r#"req\.method\s*[=!]=\s*['"](GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS)['"]"#,
                    )
                    .unwrap();
                    let mut found = false;
                    for cap in method_check_re.captures_iter(content) {
                        if let Ok(method) = HttpMethod::try_from(
                            cap.get(1).unwrap().as_str().to_lowercase().as_str(),
                        ) {
                            routes.push(RouteInfo {
                                method,
                                path: route_path.clone(),
                                framework: "next.js".to_string(),
                                source: path.to_path_buf(),
                                line: line_number(content, cap.get(0).unwrap().start()),
                            });
                            found = true;
                        }
                    }
                    if !found {
                        routes.push(RouteInfo {
                            method: HttpMethod::Get,
                            path: route_path,
                            framework: "next.js".to_string(),
                            source: path.to_path_buf(),
                            line: 1,
                        });
                    }
                } else {
                    routes.push(RouteInfo {
                        method: HttpMethod::Get,
                        path: route_path,
                        framework: "next.js".to_string(),
                        source: path.to_path_buf(),
                        line: 1,
                    });
                }
            }
        }
    }

    routes
}

/// Convert a Next.js App Router file path to a route string.
/// e.g. src/app/dashboard/settings/page.tsx -> /dashboard/settings
fn nextjs_app_path(path: &Path) -> Option<String> {
    let path_str = path.to_string_lossy();
    let marker = if path_str.contains("/app/") {
        "/app/"
    } else if path_str.contains("\\app\\") {
        "\\app\\"
    } else {
        return None;
    };
    let after_app = path_str.splitn(2, marker).nth(1)?;
    // Remove the filename to get just the directory segment
    let route_dir = std::path::Path::new(after_app).parent()?;
    let route = route_dir.to_string_lossy();
    // Convert Next.js dynamic segments [param] -> :param
    let route = route.replace('[', ":").replace(']', "");
    // Normalise Windows separators
    let route = route.replace('\\', "/");
    // Trim any accidental leading slash to avoid double-slash
    let route = route.trim_start_matches('/').to_string();
    if route.is_empty() {
        Some("/".to_string())
    } else {
        Some(format!("/{}", route))
    }
}

/// Convert a Next.js Pages Router file path to a route string.
/// e.g. pages/dashboard/index.tsx -> /dashboard
fn nextjs_pages_path(path: &Path) -> Option<String> {
    let path_str = path.to_string_lossy();
    let marker = if path_str.contains("/pages/") {
        "/pages/"
    } else if path_str.contains("\\pages\\") {
        "\\pages\\"
    } else {
        return None;
    };
    let after_pages = path_str.splitn(2, marker).nth(1)?;
    let p = std::path::Path::new(after_pages);
    let stem = p.file_stem()?.to_string_lossy();
    let parent = p
        .parent()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    let route = if stem == "index" {
        if parent.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", parent)
        }
    } else {
        let base = if parent.is_empty() {
            stem.to_string()
        } else {
            format!("{}/{}", parent, stem)
        };
        let base = base.replace('[', ":").replace(']', "");
        format!("/{}", base)
    };
    Some(route)
}

fn extract_react_router(path: &Path, content: &str) -> Vec<RouteInfo> {
    // Gate on explicit react-router import to avoid false positives from
    // any file that happens to contain a path: key in an object.
    if !content.contains("react-router") && !content.contains("react-router-dom") {
        return Vec::new();
    }
    let mut routes = Vec::new();

    // JSX form: <Route path="/foo" />
    let jsx_re = Regex::new(r#"<Route[^>]+path=["']([^"']+)["']"#).unwrap();
    for cap in jsx_re.captures_iter(content) {
        routes.push(RouteInfo {
            method: HttpMethod::Get,
            path: cap.get(1).unwrap().as_str().to_string(),
            framework: "react-router".to_string(),
            source: path.to_path_buf(),
            line: line_number(content, cap.get(0).unwrap().start()),
        });
    }

    // Object form — only inside a createBrowserRouter / createHashRouter /
    // createMemoryRouter call to avoid matching arbitrary objects.
    let factory_re = Regex::new(r#"create(?:Browser|Hash|Memory)Router\s*\(\s*\["#).unwrap();
    if let Some(factory_match) = factory_re.find(content) {
        let block_start = factory_match.end();
        let block = &content[block_start..(block_start + 8192).min(content.len())];
        let obj_path_re = Regex::new(r#"\bpath:\s*["']([^"']+)["']"#).unwrap();
        for cap in obj_path_re.captures_iter(block) {
            routes.push(RouteInfo {
                method: HttpMethod::Get,
                path: cap.get(1).unwrap().as_str().to_string(),
                framework: "react-router".to_string(),
                source: path.to_path_buf(),
                line: line_number(content, factory_match.start() + cap.get(0).unwrap().start()),
            });
        }
    }

    routes
}

fn extract_vue_router(path: &Path, content: &str) -> Vec<RouteInfo> {
    // Gate on explicit vue-router import or createRouter usage.
    if !content.contains("vue-router")
        && !content.contains("createRouter")
        && !content.contains("VueRouter")
    {
        return Vec::new();
    }

    // Only extract path values that sit inside a routes array definition.
    let anchor_re =
        Regex::new(r#"(?:routes\s*:\s*\[|(?:const|let|var)\s+routes\s*=\s*\[)"#).unwrap();

    let Some(anchor) = anchor_re.find(content) else {
        return Vec::new();
    };

    let block_start = anchor.end();
    let block = &content[block_start..(block_start + 16384).min(content.len())];
    let path_re = Regex::new(r#"\bpath:\s*["']([^"']+)["']"#).unwrap();
    let mut routes = Vec::new();

    for cap in path_re.captures_iter(block) {
        routes.push(RouteInfo {
            method: HttpMethod::Get,
            path: cap.get(1).unwrap().as_str().to_string(),
            framework: "vue-router".to_string(),
            source: path.to_path_buf(),
            line: line_number(content, anchor.start() + cap.get(0).unwrap().start()),
        });
    }

    routes
}

fn extract_svelte_kit(path: &Path, content: &str) -> Vec<RouteInfo> {
    let filename = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
    let path_str = path.to_string_lossy();

    if !path_str.contains("/routes/") && !path_str.contains("\\routes\\") {
        return Vec::new();
    }

    let mut routes = Vec::new();
    let route_path = sveltekit_route_path(path).unwrap_or_else(|| "/".to_string());

    if matches!(filename, "+page.svelte" | "+page.ts" | "+page.js") {
        routes.push(RouteInfo {
            method: HttpMethod::Get,
            path: route_path,
            framework: "sveltekit".to_string(),
            source: path.to_path_buf(),
            line: 1,
        });
    } else if matches!(filename, "+server.ts" | "+server.js") {
        let method_re = Regex::new(
            r#"export\s+(?:async\s+)?function\s+(GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS)"#,
        )
        .unwrap();
        for cap in method_re.captures_iter(content) {
            if let Ok(method) =
                HttpMethod::try_from(cap.get(1).unwrap().as_str().to_lowercase().as_str())
            {
                routes.push(RouteInfo {
                    method,
                    path: route_path.clone(),
                    framework: "sveltekit".to_string(),
                    source: path.to_path_buf(),
                    line: line_number(content, cap.get(0).unwrap().start()),
                });
            }
        }
    }

    routes
}

fn sveltekit_route_path(path: &Path) -> Option<String> {
    let path_str = path.to_string_lossy();
    let marker = if path_str.contains("/routes/") {
        "/routes/"
    } else {
        "\\routes\\"
    };
    let after = path_str.splitn(2, marker).nth(1)?;
    let dir = std::path::Path::new(after).parent()?;
    let route = dir.to_string_lossy().replace('\\', "/");
    // Strip SvelteKit (group) segments which are invisible in the URL
    let route = Regex::new(r"\([^)]+\)/?")
        .unwrap()
        .replace_all(&route, "")
        .to_string();
    // Convert [param] -> :param
    let route = route.replace('[', ":").replace(']', "");
    let route = route.trim_matches('/').to_string();
    if route.is_empty() {
        Some("/".to_string())
    } else {
        Some(format!("/{}", route))
    }
}

fn extract_angular(path: &Path, content: &str) -> Vec<RouteInfo> {
    if !content.contains("RouterModule")
        && !content.contains("Routes")
        && !content.contains("provideRouter")
    {
        return Vec::new();
    }

    // Only scan inside a typed Routes array or forRoot/forChild call
    // to avoid matching arbitrary objects with a path key.
    let anchor_re = Regex::new(r#"(?:const|let|var)\s+\w+\s*:\s*Routes\s*=\s*\["#).unwrap();

    let anchor = match anchor_re.find(content) {
        Some(m) => m,
        None => {
            let fallback = Regex::new(r#"(?:forRoot|forChild)\s*\(\s*\["#).unwrap();
            match fallback.find(content) {
                Some(m) => m,
                None => return Vec::new(),
            }
        }
    };

    let block_start = anchor.end();
    let block = &content[block_start..(block_start + 16384).min(content.len())];
    let path_re = Regex::new(r#"\bpath:\s*['"]([^'"]+)['"]"#).unwrap();
    let mut routes = Vec::new();

    for cap in path_re.captures_iter(block) {
        let p = cap.get(1).unwrap().as_str();
        if p == "**" {
            continue;
        }
        let route_path = if p.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", p)
        };
        routes.push(RouteInfo {
            method: HttpMethod::Get,
            path: route_path,
            framework: "angular".to_string(),
            source: path.to_path_buf(),
            line: line_number(content, anchor.start() + cap.get(0).unwrap().start()),
        });
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
