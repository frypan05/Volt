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
    pub is_too_broad: bool,
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

fn is_too_broad_dir(path: &Path) -> bool {
    // Check for Home directory
    if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        if path == Path::new(&home) {
            return true;
        }
    }

    // Check for Root directory
    if path.parent().is_none() {
        return true;
    }

    // Check for common high-level dirs if we can't get home env
    #[cfg(windows)]
    {
        if let Some(s) = path.to_str() {
            if s.to_lowercase().starts_with("c:\\users") && s.split('\\').count() <= 3 {
                return true;
            }
        }
    }
    #[cfg(unix)]
    {
        if let Some(s) = path.to_str() {
            if s.starts_with("/home/") && s.split('/').count() <= 3 {
                return true;
            }
        }
    }

    false
}

pub fn scan_dir(root: &Path) -> anyhow::Result<ScannerReport> {
    let is_too_broad = is_too_broad_dir(root);
    let mut routes = BTreeSet::new();

    // If it's too broad, we skip the heavy recursive scan to avoid lag
    if !is_too_broad {
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
    }

    let mut persisted_base_urls = std::collections::HashMap::new();
    for pr in load_persisted_routes(root) {
        persisted_base_urls.insert(pr.route.id(), pr.base_url);
        routes.insert(pr.route);
    }

    Ok(ScannerReport {
        routes: routes.into_iter().collect(),
        persisted_base_urls,
        is_too_broad,
    })
}

fn is_supported(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("rs" | "js" | "ts" | "py" | "jsx" | "tsx" | "vue" | "svelte" | "go" | "java" | "cs")
    )
}

fn extract_routes(path: &Path, content: &str) -> Vec<RouteInfo> {
    let mut routes = Vec::new();
    // Original extractors
    routes.extend(extract_axum(path, content));
    routes.extend(extract_actix(path, content));
    routes.extend(extract_express(path, content));
    routes.extend(extract_fastapi(path, content));
    routes.extend(extract_nextjs(path, content));
    routes.extend(extract_react_router(path, content));
    routes.extend(extract_vue_router(path, content));
    routes.extend(extract_svelte_kit(path, content));
    routes.extend(extract_angular(path, content));
    // New extractors
    routes.extend(extract_gin(path, content));
    routes.extend(extract_spring(path, content));
    routes.extend(extract_django(path, content));
    routes.extend(extract_flask(path, content));
    routes.extend(extract_nuxt(path, content));
    routes.extend(extract_aspnet(path, content));
    routes.extend(extract_azurefunction(path, content));
    routes
}

// ---------------------------------------------------------------------------
// Existing extractors (unchanged)
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// New extractors
// ---------------------------------------------------------------------------

/// Gin (Go) — matches router method calls on any variable name.
///
/// Patterns detected:
///   r.GET("/path", handler)
///   router.POST("/path", handler)
///   v1.PUT("/path", handler)          (group variables)
///   engine.DELETE("/path", handler)
///
/// Gate: file must be a .go file and import "github.com/gin-gonic/gin".
fn extract_gin(path: &Path, content: &str) -> Vec<RouteInfo> {
    if path.extension().and_then(|e| e.to_str()) != Some("go") {
        return Vec::new();
    }
    if !content.contains("gin-gonic/gin")
        && !content.contains("gin.Default")
        && !content.contains("gin.New")
    {
        return Vec::new();
    }

    // Matches: <ident>.(GET|POST|...)(  "<path>"
    // The variable name before the dot is deliberately not captured — any
    // identifier is valid (r, router, v1, api, engine, …).
    let route_re =
        Regex::new(r#"\w+\.(GET|POST|PUT|PATCH|DELETE|OPTIONS|HEAD)\(\s*"([^"]+)""#).unwrap();

    let mut routes = Vec::new();
    for cap in route_re.captures_iter(content) {
        if let Ok(method) =
            HttpMethod::try_from(cap.get(1).unwrap().as_str().to_lowercase().as_str())
        {
            // Gin uses :param notation natively — no conversion needed.
            routes.push(RouteInfo {
                method,
                path: cap.get(2).unwrap().as_str().to_string(),
                framework: "gin".to_string(),
                source: path.to_path_buf(),
                line: line_number(content, cap.get(0).unwrap().start()),
            });
        }
    }
    routes
}

/// Spring Boot / Quarkus / Micronaut (Java) — annotation-based routing.
///
/// Patterns detected:
///   @GetMapping("/path")
///   @PostMapping("/path")
///   @PutMapping, @PatchMapping, @DeleteMapping
///   @RequestMapping(value = "/path", method = RequestMethod.GET)
///   @RequestMapping(method = RequestMethod.POST, value = "/path")
///   @Path("/path") with @GET / @POST / @PUT / @PATCH / @DELETE (JAX-RS)
///
/// Gate: file must be a .java file containing a mapping annotation.
fn extract_spring(path: &Path, content: &str) -> Vec<RouteInfo> {
    if path.extension().and_then(|e| e.to_str()) != Some("java") {
        return Vec::new();
    }

    // Quick gate — must have at least one recognisable annotation.
    let has_spring = content.contains("Mapping(") || content.contains("RequestMapping");
    let has_jaxrs =
        content.contains("@GET") || content.contains("@POST") || content.contains("@Path");
    if !has_spring && !has_jaxrs {
        return Vec::new();
    }

    let mut routes = Vec::new();

    // Spring-style shorthand: @GetMapping("/path")  @PostMapping("/path") etc.
    let shorthand_re = Regex::new(
        r#"@(Get|Post|Put|Patch|Delete)Mapping\(\s*(?:value\s*=\s*)?[{"']([^"'}]+)[}'"]"#,
    )
    .unwrap();
    for cap in shorthand_re.captures_iter(content) {
        let verb = cap.get(1).unwrap().as_str().to_lowercase();
        if let Ok(method) = HttpMethod::try_from(verb.as_str()) {
            routes.push(RouteInfo {
                method,
                path: cap.get(2).unwrap().as_str().to_string(),
                framework: "spring".to_string(),
                source: path.to_path_buf(),
                line: line_number(content, cap.get(0).unwrap().start()),
            });
        }
    }

    // @RequestMapping(value = "/path", method = RequestMethod.GET)
    // Also handles method listed before value.
    let req_map_re = Regex::new(r#"@RequestMapping\(([^)]{0,512})\)"#).unwrap();
    let value_re = Regex::new(r#"value\s*=\s*[{"']([^"'}]+)[}'"]"#).unwrap();
    let method_re = Regex::new(r#"method\s*=\s*RequestMethod\.(\w+)"#).unwrap();
    for cap in req_map_re.captures_iter(content) {
        let block = cap.get(1).unwrap().as_str();
        let route_path = match value_re.captures(block) {
            Some(c) => c.get(1).unwrap().as_str().to_string(),
            None => continue,
        };
        if let Some(mc) = method_re.captures(block) {
            if let Ok(method) =
                HttpMethod::try_from(mc.get(1).unwrap().as_str().to_lowercase().as_str())
            {
                routes.push(RouteInfo {
                    method,
                    path: route_path,
                    framework: "spring".to_string(),
                    source: path.to_path_buf(),
                    line: line_number(content, cap.get(0).unwrap().start()),
                });
            }
        } else {
            // No explicit method — default to GET (class-level @RequestMapping).
            routes.push(RouteInfo {
                method: HttpMethod::Get,
                path: route_path,
                framework: "spring".to_string(),
                source: path.to_path_buf(),
                line: line_number(content, cap.get(0).unwrap().start()),
            });
        }
    }

    // JAX-RS (@Path + @GET/@POST etc.) — used by Quarkus and Micronaut too.
    // Strategy: find every @Path("…") annotation, then look at the next
    // HTTP-verb annotation within a short window (500 chars) after it.
    let path_re = Regex::new(r#"@Path\(\s*"([^"]+)"\s*\)"#).unwrap();
    let verb_re = Regex::new(r#"@(GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS)\b"#).unwrap();
    for path_cap in path_re.captures_iter(content) {
        let route_path = path_cap.get(1).unwrap().as_str().to_string();
        let after_start = path_cap.get(0).unwrap().end();
        let window_end = (after_start + 500).min(content.len());
        let window = &content[after_start..window_end];
        if let Some(verb_cap) = verb_re.captures(window) {
            if let Ok(method) =
                HttpMethod::try_from(verb_cap.get(1).unwrap().as_str().to_lowercase().as_str())
            {
                routes.push(RouteInfo {
                    method,
                    path: route_path,
                    framework: "spring".to_string(),
                    source: path.to_path_buf(),
                    line: line_number(content, path_cap.get(0).unwrap().start()),
                });
            }
        }
    }

    routes
}

/// Django (Python) — urlpatterns list.
///
/// Patterns detected:
///   path("route/", view)
///   path("route/<int:pk>/", view)
///   re_path(r"^route/$", view)
///   url(r"^route/$", view)       (Django <2)
///
/// Gate: file must be a .py file that looks like a urls module.
/// Django routes are always GET by default (the view itself handles methods),
/// so we emit GET for every detected path.
fn extract_django(path: &Path, content: &str) -> Vec<RouteInfo> {
    if path.extension().and_then(|e| e.to_str()) != Some("py") {
        return Vec::new();
    }

    // Gate: must reference urlpatterns and at least one path/re_path/url call.
    if !content.contains("urlpatterns") {
        return Vec::new();
    }
    if !content.contains("path(") && !content.contains("re_path(") && !content.contains("url(") {
        return Vec::new();
    }

    // Skip files that look like FastAPI — they use @app.get / @router.get
    // which extract_fastapi already handles.
    if content.contains("FastAPI") || content.contains("@app.") || content.contains("@router.") {
        return Vec::new();
    }

    let mut routes = Vec::new();

    // path("route/", ...) and re_path(r"^route/$", ...)
    let path_re = Regex::new(r#"(?:re_path|path)\(\s*r?["']([^"']+)["']"#).unwrap();
    for cap in path_re.captures_iter(content) {
        let raw = cap.get(1).unwrap().as_str();
        let route_path = normalise_django_path(raw);
        routes.push(RouteInfo {
            method: HttpMethod::Get,
            path: route_path,
            framework: "django".to_string(),
            source: path.to_path_buf(),
            line: line_number(content, cap.get(0).unwrap().start()),
        });
    }

    // Legacy url(r"^route/$", ...) — Django <2
    let url_re = Regex::new(r#"\burl\(\s*r?["']([^"']+)["']"#).unwrap();
    for cap in url_re.captures_iter(content) {
        let raw = cap.get(1).unwrap().as_str();
        let route_path = normalise_django_path(raw);
        routes.push(RouteInfo {
            method: HttpMethod::Get,
            path: route_path,
            framework: "django".to_string(),
            source: path.to_path_buf(),
            line: line_number(content, cap.get(0).unwrap().start()),
        });
    }

    routes
}

/// Normalise a Django URL pattern string into a clean route path.
///
/// Strips regex anchors (^ $), trailing slashes on non-root paths,
/// converts Django typed converters <int:pk> -> :pk and
/// named groups (?P<name>...) -> :name.
fn normalise_django_path(raw: &str) -> String {
    // Strip leading ^ and trailing $
    let s = raw.trim_start_matches('^').trim_end_matches('$');
    // Convert Django typed converters: <int:pk> -> :pk, <str:slug> -> :slug
    let s = Regex::new(r#"<\w+:(\w+)>"#)
        .unwrap()
        .replace_all(s, ":$1")
        .to_string();
    // Convert plain angle-bracket params: <pk> -> :pk
    let s = Regex::new(r#"<(\w+)>"#)
        .unwrap()
        .replace_all(&s, ":$1")
        .to_string();
    // Convert named regex groups: (?P<name>[^/]+) -> :name
    let s = Regex::new(r#"\(\?P<(\w+)>[^)]+\)"#)
        .unwrap()
        .replace_all(&s, ":$1")
        .to_string();
    // Ensure leading slash
    let s = if s.starts_with('/') {
        s.to_string()
    } else {
        format!("/{}", s)
    };
    // Strip trailing slash unless it's the root
    if s.len() > 1 && s.ends_with('/') {
        s.trim_end_matches('/').to_string()
    } else {
        s
    }
}

/// Flask (Python) — decorator-based routing.
///
/// Patterns detected:
///   @app.route("/path")
///   @app.route("/path", methods=["GET", "POST"])
///   @blueprint.route("/path", methods=["DELETE"])
///
/// Gate: .py file containing flask import and @app.route or @<name>.route.
///
/// Note: extract_fastapi already gates on FastAPI/`@app.` so we check that
/// this file actually imports Flask to avoid double-counting.
fn extract_flask(path: &Path, content: &str) -> Vec<RouteInfo> {
    if path.extension().and_then(|e| e.to_str()) != Some("py") {
        return Vec::new();
    }

    // Must import Flask (or Blueprint) and use .route(
    if !content.contains("flask") && !content.contains("Flask") {
        return Vec::new();
    }
    if !content.contains(".route(") {
        return Vec::new();
    }

    // @<ident>.route("/path")  or  @<ident>.route("/path", methods=[...])
    let route_re =
        Regex::new(r#"@\w+\.route\(\s*["']([^"']+)["'](?:[^)]*methods\s*=\s*\[([^\]]*)\])?"#)
            .unwrap();
    let method_item_re = Regex::new(r#"["'](GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS)["']"#).unwrap();

    let mut routes = Vec::new();
    for cap in route_re.captures_iter(content) {
        let route_path = cap.get(1).unwrap().as_str().to_string();
        let line = line_number(content, cap.get(0).unwrap().start());

        if let Some(methods_str) = cap.get(2) {
            // Explicit methods list — emit one RouteInfo per method.
            let mut found = false;
            for mc in method_item_re.captures_iter(methods_str.as_str()) {
                if let Ok(method) =
                    HttpMethod::try_from(mc.get(1).unwrap().as_str().to_lowercase().as_str())
                {
                    routes.push(RouteInfo {
                        method,
                        path: route_path.clone(),
                        framework: "flask".to_string(),
                        source: path.to_path_buf(),
                        line,
                    });
                    found = true;
                }
            }
            if !found {
                // methods list present but nothing matched — fall back to GET
                routes.push(RouteInfo {
                    method: HttpMethod::Get,
                    path: route_path,
                    framework: "flask".to_string(),
                    source: path.to_path_buf(),
                    line,
                });
            }
        } else {
            // No methods list — Flask default is GET (and HEAD).
            routes.push(RouteInfo {
                method: HttpMethod::Get,
                path: route_path,
                framework: "flask".to_string(),
                source: path.to_path_buf(),
                line,
            });
        }
    }
    routes
}

/// Nuxt.js — file-based server routing.
///
/// Nuxt 3 places server routes under:
///   server/api/**   -> /api/<stem>
///   server/routes/**  -> /<stem>
///
/// The HTTP method can be encoded in the filename:
///   users.get.ts    -> GET  /api/users
///   users.post.ts   -> POST /api/users
///   users.ts        -> GET  /api/users   (default)
///   [id].delete.ts  -> DELETE /api/:id
///
/// Gate: path must contain /server/api/ or /server/routes/.
fn extract_nuxt(path: &Path, content: &str) -> Vec<RouteInfo> {
    let path_str = path.to_string_lossy();

    let in_api = path_str.contains("/server/api/") || path_str.contains("\\server\\api\\");
    let in_routes = path_str.contains("/server/routes/") || path_str.contains("\\server\\routes\\");

    if !in_api && !in_routes {
        return Vec::new();
    }

    // Only handle JS/TS files
    match path.extension().and_then(|e| e.to_str()) {
        Some("js" | "ts" | "mjs" | "mts") => {}
        _ => return Vec::new(),
    }

    let route_path = nuxt_route_path(path, in_api);
    let method = nuxt_method_from_filename(path);

    // If the file also exports named HTTP handlers we respect those instead
    // of the filename convention (Nuxt 3 also supports this pattern).
    let export_re =
        Regex::new(r#"export\s+(?:default\s+)?defineEventHandler|export\s+default\s+eventHandler"#)
            .unwrap();

    if export_re.is_match(content) {
        vec![RouteInfo {
            method,
            path: route_path,
            framework: "nuxt".to_string(),
            source: path.to_path_buf(),
            line: 1,
        }]
    } else {
        Vec::new()
    }
}

/// Derive the Nuxt route path from the file path.
///
/// server/api/users/index.get.ts  -> /api/users
/// server/api/[id].ts             -> /api/:id
/// server/routes/health.ts        -> /health
fn nuxt_route_path(path: &Path, in_api: bool) -> String {
    let path_str = path.to_string_lossy();

    let marker = if in_api {
        if path_str.contains("/server/api/") {
            "/server/api/"
        } else {
            "\\server\\api\\"
        }
    } else {
        if path_str.contains("/server/routes/") {
            "/server/routes/"
        } else {
            "\\server\\routes\\"
        }
    };

    let after = match path_str.splitn(2, marker).nth(1) {
        Some(s) => s.to_string(),
        None => return "/".to_string(),
    };

    let p = std::path::Path::new(&after);
    // Strip the method suffix and extension from the filename.
    // e.g.  users.get.ts -> users,  [id].delete.ts -> [id],  index.ts -> index
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("index");
    // Strip trailing .get/.post/… if present
    let stem = if let Some(pos) = stem.rfind('.') {
        let suffix = &stem[pos + 1..];
        if ["get", "post", "put", "patch", "delete", "head", "options"].contains(&suffix) {
            &stem[..pos]
        } else {
            stem
        }
    } else {
        stem
    };

    let parent = p
        .parent()
        .map(|pp| pp.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();

    let combined = if stem == "index" {
        if parent.is_empty() {
            String::new()
        } else {
            parent.clone()
        }
    } else {
        if parent.is_empty() {
            stem.to_string()
        } else {
            format!("{}/{}", parent, stem)
        }
    };

    // Convert [param] -> :param
    let combined = combined.replace('[', ":").replace(']', "");
    // Normalise double slashes
    let combined = combined.trim_matches('/');

    let prefix = if in_api { "/api" } else { "" };
    if combined.is_empty() {
        if in_api {
            "/api".to_string()
        } else {
            "/".to_string()
        }
    } else {
        format!("{}/{}", prefix, combined)
    }
}

/// Extract the HTTP method from a Nuxt filename convention.
/// users.get.ts -> GET,  users.post.ts -> POST,  users.ts -> GET (default)
fn nuxt_method_from_filename(path: &Path) -> HttpMethod {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    if let Some(pos) = stem.rfind('.') {
        let suffix = &stem[pos + 1..];
        if let Ok(m) = HttpMethod::try_from(suffix) {
            return m;
        }
    }
    HttpMethod::Get
}

/// ASP.NET Core (.NET / C#) — minimal API and attribute routing.
///
/// Patterns detected (minimal API):
///   app.MapGet("/path", handler)
///   app.MapPost("/path", handler)
///   app.MapPut, MapPatch, MapDelete
///
/// Patterns detected (attribute routing):
///   [HttpGet("/path")]
///   [HttpPost("/path")]
///   [HttpPut], [HttpPatch], [HttpDelete]
///   [Route("/path")]  (combined with [HttpGet] etc. on the class)
///
/// Gate: .cs file containing MapGet/MapPost or Http* attributes.
fn extract_aspnet(path: &Path, content: &str) -> Vec<RouteInfo> {
    if path.extension().and_then(|e| e.to_str()) != Some("cs") {
        return Vec::new();
    }

    let has_minimal = content.contains("MapGet(")
        || content.contains("MapPost(")
        || content.contains("MapPut(")
        || content.contains("MapPatch(")
        || content.contains("MapDelete(");
    let has_attr = content.contains("[HttpGet")
        || content.contains("[HttpPost")
        || content.contains("[HttpPut")
        || content.contains("[HttpPatch")
        || content.contains("[HttpDelete");

    if !has_minimal && !has_attr {
        return Vec::new();
    }

    let mut routes = Vec::new();

    // Minimal API: app.Map<Verb>("/path", ...)
    let minimal_re =
        Regex::new(r#"\.Map(Get|Post|Put|Patch|Delete)\(\s*["']([^"']+)["']"#).unwrap();
    for cap in minimal_re.captures_iter(content) {
        if let Ok(method) =
            HttpMethod::try_from(cap.get(1).unwrap().as_str().to_lowercase().as_str())
        {
            routes.push(RouteInfo {
                method,
                path: cap.get(2).unwrap().as_str().to_string(),
                framework: "aspnet".to_string(),
                source: path.to_path_buf(),
                line: line_number(content, cap.get(0).unwrap().start()),
            });
        }
    }

    // Attribute routing: [HttpGet("/path")] or [HttpGet] (no inline path)
    // When no inline path is present, look for a [Route("...")] on the same
    // controller class (within 2000 chars before the attribute).
    let attr_with_path_re =
        Regex::new(r#"\[Http(Get|Post|Put|Patch|Delete)\(\s*["']([^"']+)["']\s*\)\]"#).unwrap();
    for cap in attr_with_path_re.captures_iter(content) {
        if let Ok(method) =
            HttpMethod::try_from(cap.get(1).unwrap().as_str().to_lowercase().as_str())
        {
            routes.push(RouteInfo {
                method,
                path: cap.get(2).unwrap().as_str().to_string(),
                framework: "aspnet".to_string(),
                source: path.to_path_buf(),
                line: line_number(content, cap.get(0).unwrap().start()),
            });
        }
    }

    // [HttpGet] without inline path — look backwards for [Route("...")] to
    // get the path, then combine with any [RoutePrefix] or just use it directly.
    let attr_bare_re = Regex::new(r#"\[Http(Get|Post|Put|Patch|Delete)\]"#).unwrap();
    let route_attr_re = Regex::new(r#"\[Route\(\s*["']([^"']+)["']\s*\)\]"#).unwrap();
    for cap in attr_bare_re.captures_iter(content) {
        let offset = cap.get(0).unwrap().start();
        // Search backwards within 2000 chars for the nearest [Route("...")]
        let look_start = offset.saturating_sub(2000);
        let window = &content[look_start..offset];
        let route_path = if let Some(rc) = route_attr_re.captures_iter(window).last() {
            rc.get(1).unwrap().as_str().to_string()
        } else {
            continue; // No path info available — skip
        };
        if let Ok(method) =
            HttpMethod::try_from(cap.get(1).unwrap().as_str().to_lowercase().as_str())
        {
            // ASP.NET uses {param} notation — convert to :param
            let route_path = Regex::new(r#"\{(\w+)(?::[^}]*)?\}"#)
                .unwrap()
                .replace_all(&route_path, ":$1")
                .to_string();
            routes.push(RouteInfo {
                method,
                path: if route_path.starts_with('/') {
                    route_path
                } else {
                    format!("/{}", route_path)
                },
                framework: "aspnet".to_string(),
                source: path.to_path_buf(),
                line: line_number(content, offset),
            });
        }
    }

    // Also convert {param} in paths from the minimal API and attr-with-path patterns above.
    // We do a post-pass on all routes collected so far.
    let param_re = Regex::new(r#"\{(\w+)(?::[^}]*)?\}"#).unwrap();
    for r in routes.iter_mut() {
        if r.framework == "aspnet" {
            r.path = param_re.replace_all(&r.path, ":$1").to_string();
        }
    }

    routes
}

fn extract_azurefunction(path: &Path, content: &str) -> Vec<RouteInfo> {
    if path.extension().and_then(|e| e.to_str()) != Some("cs") {
        return Vec::new();
    }

    if !content.contains("[HttpTrigger(") {
        return Vec::new();
    }

    let mut routes = Vec::new();

    let attr_default = Regex::new(
r#"\[HttpTrigger\(AuthorizationLevel\.(\w+)(?:\s*,\s*"(\w+)")?(?:\s*,\s*Route\s*=\s*"([^"]*)")?\s*\)\]"#    )
    .unwrap();
    for cap in attr_default.captures_iter(content) {
        let offset = cap.get(0).unwrap().start();
        let _auth_level = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let method = cap.get(2).map(|m| m.as_str()).unwrap_or("get");
        let route = cap.get(3).map(|m| m.as_str()).unwrap_or("");

        if let Ok(method) = HttpMethod::try_from(method) {
            routes.push(RouteInfo {
                method,
                path: format!("/{}", route),
                framework: "azurefunction".to_string(),
                source: path.to_path_buf(),
                line: line_number(content, offset),
            });
        }
    }

    let attr_with_methods = Regex::new(
        r#"\[HttpTrigger\(AuthorizationLevel\.(\w+)\s*,\s*((?:"[^"]+"\s*,?\s*)+)(?:,?\s*Route\s*=\s*"([^"]*)")?\s*\)\]"#,
    )
    .unwrap();
    let method_re = Regex::new(r#""([^"]+)""#).unwrap();

    for cap in attr_with_methods.captures_iter(content) {
        let offset = cap.get(0).unwrap().start();
        let _auth_level = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let methods_blob = cap.get(2).unwrap().as_str();
        let route = cap.get(3).map(|m| m.as_str()).unwrap_or("");

        for mc in method_re.captures_iter(methods_blob) {
            let method_str = mc.get(1).unwrap().as_str();
            if let Ok(method) = HttpMethod::try_from(method_str) {
                routes.push(RouteInfo {
                    method,
                    path: format!("/{}", route),
                    framework: "azurefunction".to_string(),
                    source: path.to_path_buf(),
                    line: line_number(content, offset),
                });
            }
        }
    }

    // Helper — extract individual method strings from captured methods_blob
    let _method_re = Regex::new(r#""([^"]+)""#).unwrap();

    routes
}

// ---------------------------------------------------------------------------
// Shared helper
// ---------------------------------------------------------------------------

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

    // Unit tests for new extractors — these run against inline content so
    // they do not require fixture files on disk.

    #[test]
    fn gin_detects_routes() {
        let path = PathBuf::from("main.go");
        let content = r#"
import "github.com/gin-gonic/gin"
func main() {
    r := gin.Default()
    r.GET("/users", listUsers)
    r.POST("/users", createUser)
    v1 := r.Group("/v1")
    v1.DELETE("/users/:id", deleteUser)
}
"#;
        let routes = extract_gin(&path, content);
        assert!(
            routes
                .iter()
                .any(|r| r.path == "/users" && r.method == HttpMethod::Get)
        );
        assert!(
            routes
                .iter()
                .any(|r| r.path == "/users" && r.method == HttpMethod::Post)
        );
        assert!(
            routes
                .iter()
                .any(|r| r.path == "/users/:id" && r.method == HttpMethod::Delete)
        );
    }

    #[test]
    fn spring_detects_shorthand_mappings() {
        let path = PathBuf::from("UserController.java");
        let content = r#"
@RestController
public class UserController {
    @GetMapping("/users")
    public List<User> list() { return null; }

    @PostMapping("/users")
    public User create(@RequestBody User u) { return null; }

    @DeleteMapping("/users/{id}")
    public void delete(@PathVariable Long id) {}
}
"#;
        let routes = extract_spring(&path, content);
        assert!(
            routes
                .iter()
                .any(|r| r.path == "/users" && r.method == HttpMethod::Get)
        );
        assert!(
            routes
                .iter()
                .any(|r| r.path == "/users" && r.method == HttpMethod::Post)
        );
        assert!(
            routes
                .iter()
                .any(|r| r.path == "/users/{id}" && r.method == HttpMethod::Delete)
        );
    }

    #[test]
    fn django_detects_urlpatterns() {
        let path = PathBuf::from("urls.py");
        let content = r#"
from django.urls import path, re_path
urlpatterns = [
    path("users/", views.UserList.as_view()),
    path("users/<int:pk>/", views.UserDetail.as_view()),
    re_path(r"^articles/(?P<slug>[a-z]+)/$", views.ArticleDetail.as_view()),
]
"#;
        let routes = extract_django(&path, content);
        assert!(routes.iter().any(|r| r.path == "/users"));
        assert!(routes.iter().any(|r| r.path == "/users/:pk"));
        assert!(routes.iter().any(|r| r.path == "/articles/:slug"));
    }

    #[test]
    fn flask_detects_routes() {
        let path = PathBuf::from("app.py");
        let content = r#"
from flask import Flask
app = Flask(__name__)

@app.route("/")
def index(): pass

@app.route("/users", methods=["GET", "POST"])
def users(): pass

@app.route("/users/<id>", methods=["DELETE"])
def delete_user(id): pass
"#;
        let routes = extract_flask(&path, content);
        assert!(
            routes
                .iter()
                .any(|r| r.path == "/" && r.method == HttpMethod::Get)
        );
        assert!(
            routes
                .iter()
                .any(|r| r.path == "/users" && r.method == HttpMethod::Get)
        );
        assert!(
            routes
                .iter()
                .any(|r| r.path == "/users" && r.method == HttpMethod::Post)
        );
        assert!(
            routes
                .iter()
                .any(|r| r.path == "/users/<id>" && r.method == HttpMethod::Delete)
        );
    }

    #[test]
    fn aspnet_minimal_api() {
        let path = PathBuf::from("Program.cs");
        let content = r#"
var app = builder.Build();
app.MapGet("/users", () => Results.Ok());
app.MapPost("/users", (User u) => Results.Created());
app.MapDelete("/users/{id}", (int id) => Results.NoContent());
"#;
        let routes = extract_aspnet(&path, content);
        assert!(
            routes
                .iter()
                .any(|r| r.path == "/users" && r.method == HttpMethod::Get)
        );
        assert!(
            routes
                .iter()
                .any(|r| r.path == "/users" && r.method == HttpMethod::Post)
        );
        assert!(
            routes
                .iter()
                .any(|r| r.path == "/users/:id" && r.method == HttpMethod::Delete)
        );
    }
}
