use crate::app::{BodyType, HttpMethod, RequestDraft};
use crate::scanner::RouteInfo;
use reqwest::{
    Client, Method,
    header::{CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue},
};
use std::time::Instant;

pub type HttpResult = Result<HttpResponse, String>;

/// Raw HTTP response — no highlighting, no post-processing.
/// The UI thread applies highlighting lazily after this arrives.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status_code: u16,
    /// Wall-clock ms: request sent → last body byte received.
    pub latency_ms: u128,
    pub size_bytes: usize,
    pub content_type: String,
    pub body: String,
}

pub async fn execute(client: Client, route: RouteInfo, draft: RequestDraft) -> HttpResult {
    let base = draft.base_url.text.trim_end_matches('/');
    let path = route.path.trim_start_matches('/');
    let url = if path.is_empty() {
        base.to_string()
    } else {
        format!("{}/{}", base, path)
    };

    let mut req = client.request(to_reqwest_method(route.method), &url);

    // Headers
    let mut headers = HeaderMap::new();
    for row in draft
        .headers
        .iter()
        .filter(|r| r.enabled && !r.key.text.is_empty())
    {
        if let (Ok(n), Ok(v)) = (
            HeaderName::from_bytes(row.key.text.as_bytes()),
            HeaderValue::from_str(&row.value.text),
        ) {
            headers.insert(n, v);
        }
    }
    for row in draft
        .auth
        .iter()
        .filter(|r| r.enabled && !r.key.text.is_empty())
    {
        if let (Ok(n), Ok(v)) = (
            HeaderName::from_bytes(row.key.text.as_bytes()),
            HeaderValue::from_str(&row.value.text),
        ) {
            headers.insert(n, v);
        }
    }

    let params: Vec<(&str, &str)> = draft
        .params
        .iter()
        .filter(|r| r.enabled && !r.key.text.is_empty())
        .map(|r| (r.key.text.as_str(), r.value.text.as_str()))
        .collect();

    req = req.headers(headers).query(&params);

    if !draft.body.text.is_empty() {
        match draft.body_type {
            BodyType::Json => {
                req = req
                    .header(CONTENT_TYPE, "application/json")
                    .body(draft.body.text.clone());
            }
            BodyType::Text => {
                req = req
                    .header(CONTENT_TYPE, "text/plain")
                    .body(draft.body.text.clone());
            }
            BodyType::FormUrlEncoded => {
                req = req
                    .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(draft.body.text.clone());
            }
            BodyType::None => {}
        }
    }

    // Timer: request sent → all bytes received.
    let start = Instant::now();

    let res = req.send().await.map_err(|e| e.to_string())?;
    let status_code = res.status().as_u16();
    let content_type = res
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("text/plain")
        .to_string();

    // bytes() skips charset detection — faster than text()
    let bytes = res.bytes().await.map_err(|e| e.to_string())?;
    let latency_ms = start.elapsed().as_millis();
    let size_bytes = bytes.len();
    let body = String::from_utf8_lossy(&bytes).into_owned();

    Ok(HttpResponse {
        status_code,
        latency_ms,
        size_bytes,
        content_type,
        body,
    })
}

fn to_reqwest_method(m: HttpMethod) -> Method {
    match m {
        HttpMethod::Get => Method::GET,
        HttpMethod::Post => Method::POST,
        HttpMethod::Put => Method::PUT,
        HttpMethod::Patch => Method::PATCH,
        HttpMethod::Delete => Method::DELETE,
        HttpMethod::Options => Method::OPTIONS,
        HttpMethod::Head => Method::HEAD,
    }
}
