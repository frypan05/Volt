use std::time::Instant;

use anyhow::Context;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};
use reqwest::{Method, Url};
use thiserror::Error;

use crate::app::{HttpMethod, RequestDraft};
use crate::scanner::RouteInfo;

pub type HttpResult = Result<HttpResponse, HttpError>;

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status_code: u16,
    pub latency_ms: u128,
    pub size_bytes: usize,
    pub content_type: String,
    pub body: String,
}

#[derive(Debug, Error, Clone)]
pub enum HttpError {
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    Transport(String),
}

pub async fn execute(route: RouteInfo, draft: RequestDraft) -> HttpResult {
    let url = build_url(&draft.base_url.text, &route.path)?;
    let client = reqwest::Client::new();
    let method = to_reqwest_method(route.method);
    let mut request = client.request(method, url);
    let headers = parse_headers(&draft.headers.text, &draft.auth.text)?;
    if !headers.is_empty() {
        request = request.headers(headers);
    }
    request = apply_params(request, &draft.params.text)?;
    request = apply_body(request, &draft.body.text)?;

    let started_at = Instant::now();
    let response = request
        .send()
        .await
        .with_context(|| format!("failed to execute {} {}", route.method, route.path))
        .map_err(|error| HttpError::Transport(error.to_string()))?;
    let latency_ms = started_at.elapsed().as_millis();
    let status_code = response.status().as_u16();
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("text/plain")
        .to_string();
    let body = response
        .text()
        .await
        .map_err(|error| HttpError::Transport(error.to_string()))?;
    let size_bytes = body.len();

    Ok(HttpResponse {
        status_code,
        latency_ms,
        size_bytes,
        content_type,
        body,
    })
}

fn build_url(base_url: &str, path: &str) -> Result<Url, HttpError> {
    let base = if base_url.trim().is_empty() {
        return Err(HttpError::Validation(
            "base URL cannot be empty".to_string(),
        ));
    } else {
        base_url.trim().to_string()
    };

    if path.starts_with("http://") || path.starts_with("https://") {
        return Url::parse(path).map_err(|error| HttpError::Validation(error.to_string()));
    }

    let normalized_base = format!("{}/", base.trim_end_matches('/'));
    let url =
        Url::parse(&normalized_base).map_err(|error| HttpError::Validation(error.to_string()))?;
    url.join(path.trim_start_matches('/'))
        .map_err(|error| HttpError::Validation(error.to_string()))
}

fn parse_headers(raw: &str, auth_raw: &str) -> Result<HeaderMap, HttpError> {
    let mut headers = HeaderMap::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err(HttpError::Validation(format!(
                "header must be in 'Key: Value' format: {line}"
            )));
        };
        let name = HeaderName::from_bytes(name.trim().as_bytes())
            .map_err(|error| HttpError::Validation(error.to_string()))?;
        let value = HeaderValue::from_str(value.trim())
            .map_err(|error| HttpError::Validation(error.to_string()))?;
        headers.insert(name, value);
    }

    if !auth_raw.trim().is_empty() && !headers.contains_key(AUTHORIZATION) {
        let value = HeaderValue::from_str(auth_raw.trim())
            .map_err(|error| HttpError::Validation(error.to_string()))?;
        headers.insert(AUTHORIZATION, value);
    }

    Ok(headers)
}

fn apply_params(
    mut request: reqwest::RequestBuilder,
    raw: &str,
) -> Result<reqwest::RequestBuilder, HttpError> {
    let mut params = Vec::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once('=').or_else(|| line.split_once(':')) else {
            return Err(HttpError::Validation(format!(
                "param must be in 'key=value' format: {line}"
            )));
        };
        params.push((key.trim().to_string(), value.trim().to_string()));
    }
    if !params.is_empty() {
        request = request.query(&params);
    }
    Ok(request)
}

fn apply_body(
    mut request: reqwest::RequestBuilder,
    raw: &str,
) -> Result<reqwest::RequestBuilder, HttpError> {
    let body = raw.trim();
    if body.is_empty() {
        return Ok(request);
    }

    if body.starts_with('{') || body.starts_with('[') {
        let value: serde_json::Value = serde_json::from_str(body)
            .map_err(|error| HttpError::Validation(format!("invalid JSON body: {error}")))?;
        request = request.json(&value);
    } else {
        request = request.body(raw.to_string());
    }

    Ok(request)
}

fn to_reqwest_method(method: HttpMethod) -> Method {
    match method {
        HttpMethod::Get => Method::GET,
        HttpMethod::Post => Method::POST,
        HttpMethod::Put => Method::PUT,
        HttpMethod::Patch => Method::PATCH,
        HttpMethod::Delete => Method::DELETE,
        HttpMethod::Options => Method::OPTIONS,
        HttpMethod::Head => Method::HEAD,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_base_url_and_path() {
        let url = build_url("http://localhost:3000", "/users").unwrap();
        assert_eq!(url.as_str(), "http://localhost:3000/users");
    }

    #[test]
    fn validates_json_bodies() {
        let request = reqwest::Client::new().get("http://localhost/");
        assert!(apply_body(request, "{bad json}").is_err());
    }
}
