// Phase 5: HTTP Execution with Executor Abstraction
// Updated to work with both LocalExecutor and RemoteExecutor

use crate::app::{AuthType, BodyType, RequestDraft};
use crate::executor::{ExecuteRequest, Executor};
use crate::scanner::RouteInfo;
use base64::Engine;
use std::collections::HashMap;
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

/// Execute an HTTP request using the configured executor (local or remote)
pub async fn execute(executor: &dyn Executor, route: RouteInfo, draft: RequestDraft) -> HttpResult {
    let base = draft.base_url.text.trim_end_matches('/');
    let path = route.path.trim_start_matches('/');
    let url = if path.is_empty() {
        base.to_string()
    } else {
        format!("{}/{}", base, path)
    };

    // Build headers map
    let mut headers = HashMap::new();

    // User-defined headers
    for row in draft
        .headers
        .iter()
        .filter(|r| r.enabled && !r.key.text.is_empty())
    {
        headers.insert(row.key.text.clone(), row.value.text.clone());
    }

    // Auth headers
    match draft.auth_type {
        AuthType::None => {}
        AuthType::BasicAuth => {
            if !draft.auth_username.text.is_empty() {
                let credentials = base64::engine::general_purpose::STANDARD.encode(format!(
                    "{}:{}",
                    draft.auth_username.text, draft.auth_password.text
                ));
                headers.insert(
                    "Authorization".to_string(),
                    format!("Basic {}", credentials),
                );
            }
        }
        AuthType::BearerToken => {
            if !draft.auth_token.text.is_empty() {
                headers.insert(
                    "Authorization".to_string(),
                    format!("Bearer {}", draft.auth_token.text),
                );
            }
        }
        AuthType::ApiKey => {
            if !draft.auth_header_name.text.is_empty() && !draft.auth_header_value.text.is_empty() {
                headers.insert(
                    draft.auth_header_name.text.clone(),
                    draft.auth_header_value.text.clone(),
                );
            }
        }
    }

    // Build query parameters
    let mut query_params = HashMap::new();
    for row in draft
        .params
        .iter()
        .filter(|r| r.enabled && !r.key.text.is_empty())
    {
        query_params.insert(row.key.text.clone(), row.value.text.clone());
    }

    // Build body
    let body = if draft.body.text.is_empty() {
        None
    } else {
        // Set content-type based on body type
        match draft.body_type {
            BodyType::Json => {
                headers.insert("Content-Type".to_string(), "application/json".to_string());
            }
            BodyType::Text => {
                headers.insert("Content-Type".to_string(), "text/plain".to_string());
            }
            BodyType::FormUrlEncoded => {
                headers.insert(
                    "Content-Type".to_string(),
                    "application/x-www-form-urlencoded".to_string(),
                );
            }
            BodyType::None => {}
        }
        Some(draft.body.text.clone())
    };

    // Create execution request
    let exec_req = ExecuteRequest {
        method: route.method.as_str().to_string(),
        url,
        headers,
        query_params,
        body,
        timeout_ms: Some(30000),
        retry_policy: None,
    };

    let start = Instant::now();

    // Execute using the provided executor (local or remote)
    let response = executor
        .execute(exec_req)
        .await
        .map_err(|e| format!("Execution error: {}", e))?;

    let latency_ms = start.elapsed().as_millis();

    Ok(HttpResponse {
        status_code: response.status,
        latency_ms,
        size_bytes: response.size_bytes,
        content_type: response
            .headers
            .get("content-type")
            .cloned()
            .unwrap_or_else(|| "text/plain".to_string()),
        body: response.body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_response_creation() {
        let response = HttpResponse {
            status_code: 200,
            latency_ms: 100,
            size_bytes: 1024,
            content_type: "application/json".to_string(),
            body: "{}".to_string(),
        };

        assert_eq!(response.status_code, 200);
        assert_eq!(response.size_bytes, 1024);
    }
}
