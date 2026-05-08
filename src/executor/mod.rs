// Executor abstraction layer - the most critical design decision
// This allows Volt to execute requests locally OR remotely WITHOUT UI/config changes

use log::warn;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

// ============================================================================
// PHASE 9: Retry Policy - Control retry behavior
// ============================================================================

/// Configuration for exponential backoff retry logic
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts (includes initial attempt)
    pub max_attempts: u32,
    /// Initial backoff duration in milliseconds
    pub initial_backoff_ms: u64,
    /// Maximum backoff duration in milliseconds
    pub max_backoff_ms: u64,
    /// Exponential backoff multiplier
    pub backoff_multiplier: f64,
}
#[allow(dead_code)]
impl RetryPolicy {
    /// Create a custom retry policy
    pub fn new(
        max_attempts: u32,
        initial_backoff_ms: u64,
        max_backoff_ms: u64,
        backoff_multiplier: f64,
    ) -> Self {
        Self {
            max_attempts: max_attempts.max(1), // Ensure at least 1 attempt
            initial_backoff_ms,
            max_backoff_ms,
            backoff_multiplier,
        }
    }

    /// No retries - single attempt only
    pub fn no_retry() -> Self {
        Self {
            max_attempts: 1,
            initial_backoff_ms: 0,
            max_backoff_ms: 0,
            backoff_multiplier: 1.0,
        }
    }

    /// Conservative retry policy - 2 attempts with 500ms backoff
    pub fn conservative() -> Self {
        Self {
            max_attempts: 2,
            initial_backoff_ms: 500,
            max_backoff_ms: 5000,
            backoff_multiplier: 2.0,
        }
    }

    /// Aggressive retry policy - 5 attempts with 100ms backoff
    pub fn aggressive() -> Self {
        Self {
            max_attempts: 5,
            initial_backoff_ms: 100,
            max_backoff_ms: 5000,
            backoff_multiplier: 2.0,
        }
    }

    /// Calculate backoff duration for a given attempt (0-indexed)
    /// Uses exponential backoff: initial_backoff * (multiplier ^ attempt)
    pub fn backoff_duration(&self, attempt: u32) -> Duration {
        if attempt == 0 {
            Duration::from_millis(0)
        } else {
            let backoff_ms = (self.initial_backoff_ms as f64
                * self.backoff_multiplier.powi(attempt as i32 - 1))
            .min(self.max_backoff_ms as f64) as u64;
            Duration::from_millis(backoff_ms)
        }
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff_ms: 100,
            max_backoff_ms: 5000,
            backoff_multiplier: 2.0,
        }
    }
}

// ============================================================================
// PHASE 1: Execution Protocol - Define how local and remote talk
// ============================================================================

/// Request sent to executor (local or remote)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteRequest {
    pub method: String,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub query_params: HashMap<String, String>,
    pub body: Option<String>,
    pub timeout_ms: Option<u64>,
    /// Optional retry policy. If None, defaults to RetryPolicy::default()
    #[serde(default)]
    pub retry_policy: Option<RetryPolicy>,
}

/// Response from executor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub duration_ms: u128,
    pub size_bytes: usize,
}

/// Error that can occur during execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutorError {
    /// Transient error that can be retried (network issues, timeouts, 5xx errors)
    RetryableError(String),
    /// Permanent error that should not be retried (invalid URL, auth failure, 4xx errors)
    PermanentError(String),
    /// Legacy error types for backward compatibility
    NetworkError(String),
    TimeoutError,
    InvalidUrl(String),
    SerializationError(String),
    RemoteConnectionError(String),
    AuthenticationError(String),
}

impl ExecutorError {
    /// Check if this error is retryable (transient)
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::RetryableError(_)
                | Self::TimeoutError
                | Self::NetworkError(_)
                | Self::RemoteConnectionError(_)
        )
    }

    /// Check if this error is permanent (should not be retried)
    pub fn is_permanent(&self) -> bool {
        matches!(
            self,
            Self::PermanentError(_)
                | Self::InvalidUrl(_)
                | Self::AuthenticationError(_)
                | Self::SerializationError(_)
        )
    }
}

impl std::fmt::Display for ExecutorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RetryableError(e) => write!(f, "Retryable error: {}", e),
            Self::PermanentError(e) => write!(f, "Permanent error: {}", e),
            Self::NetworkError(e) => write!(f, "Network error: {}", e),
            Self::TimeoutError => write!(f, "Request timed out"),
            Self::InvalidUrl(e) => write!(f, "Invalid URL: {}", e),
            Self::SerializationError(e) => write!(f, "Serialization error: {}", e),
            Self::RemoteConnectionError(e) => write!(f, "Remote connection error: {}", e),
            Self::AuthenticationError(e) => write!(f, "Authentication error: {}", e),
        }
    }
}

impl std::error::Error for ExecutorError {}

pub type ExecutorResult<T> = Result<T, ExecutorError>;

// ============================================================================
// Core Executor Trait - Abstraction that powers everything
// ============================================================================

/// The core abstraction: local and remote executors implement this
/// This is what allows UI to not care about execution location
#[async_trait::async_trait]
#[allow(dead_code)]
pub trait Executor: Send + Sync {
    /// Execute an HTTP request
    async fn execute(&self, req: ExecuteRequest) -> ExecutorResult<ExecuteResponse>;

    /// Get human-readable executor name (e.g., "Local", "SSH:prod-box")
    fn name(&self) -> String;

    /// Check if executor is healthy/connected
    async fn healthcheck(&self) -> ExecutorResult<()>;

    /// Check if this executor supports retry logic
    /// Implementors can override to disable retries if needed
    async fn supports_retry(&self) -> bool {
        true
    }
}

// ============================================================================
// Local Executor Implementation
// ============================================================================

pub struct LocalExecutor {
    client: reqwest::Client,
}

impl LocalExecutor {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl Default for LocalExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Executor for LocalExecutor {
    async fn execute(&self, req: ExecuteRequest) -> ExecutorResult<ExecuteResponse> {
        // Determine retry policy - use provided or default
        let retry_policy = req
            .retry_policy
            .clone()
            .unwrap_or_else(RetryPolicy::default);

        // Execute with retry logic
        let mut last_error = None;

        for attempt in 0..retry_policy.max_attempts {
            // Log retry attempt (but not the first one unless it's a retry)
            if attempt > 0 {
                warn!(
                    "Retrying request to {} (attempt {}/{})",
                    &req.url,
                    attempt + 1,
                    retry_policy.max_attempts
                );
            }

            // Execute the request
            match self.execute_once(&req).await {
                Ok(response) => {
                    // Check HTTP status code for retryable errors
                    if self.is_retryable_status(response.status)
                        && attempt < retry_policy.max_attempts - 1
                    {
                        warn!(
                            "Retryable HTTP status {} for {}, will retry",
                            response.status, &req.url
                        );
                        // Sleep before retry
                        let backoff = retry_policy.backoff_duration(attempt);
                        if backoff.as_millis() > 0 {
                            tokio::time::sleep(backoff).await;
                        }
                        continue;
                    }
                    // Success or non-retryable status
                    return Ok(response);
                }
                Err(err) => {
                    last_error = Some(err.clone());

                    // Check if error is retryable and we have more attempts
                    if err.is_retryable() && attempt < retry_policy.max_attempts - 1 {
                        warn!(
                            "Retryable error on attempt {} for {}: {}",
                            attempt + 1,
                            &req.url,
                            err
                        );
                        // Sleep before retry
                        let backoff = retry_policy.backoff_duration(attempt);
                        if backoff.as_millis() > 0 {
                            tokio::time::sleep(backoff).await;
                        }
                        continue;
                    }

                    // Permanent error or last attempt
                    if err.is_permanent() {
                        return Err(err);
                    }
                }
            }
        }

        // All retries exhausted
        Err(last_error.unwrap_or_else(|| {
            ExecutorError::RetryableError("Max retry attempts exceeded".to_string())
        }))
    }

    fn name(&self) -> String {
        "Local".to_string()
    }

    async fn healthcheck(&self) -> ExecutorResult<()> {
        Ok(())
    }

    async fn supports_retry(&self) -> bool {
        true
    }
}

impl LocalExecutor {
    /// Execute a single HTTP request without retry logic
    async fn execute_once(&self, req: &ExecuteRequest) -> ExecutorResult<ExecuteResponse> {
        let method = reqwest::Method::from_bytes(req.method.as_bytes())
            .map_err(|e| ExecutorError::PermanentError(format!("Invalid HTTP method: {}", e)))?;

        // Validate URL
        let _parsed_url = reqwest::Url::parse(&req.url)
            .map_err(|e| ExecutorError::PermanentError(format!("Invalid URL: {}", e)))?;

        let mut request = self.client.request(method, &req.url);

        // Add headers
        for (key, value) in req.headers.iter() {
            if let Ok(header_name) = reqwest::header::HeaderName::from_bytes(key.as_bytes()) {
                if let Ok(header_value) = reqwest::header::HeaderValue::from_str(value) {
                    request = request.header(header_name, header_value);
                }
            }
        }

        // Add query parameters
        if !req.query_params.is_empty() {
            request = request.query(&req.query_params);
        }

        // Add body
        if let Some(body) = &req.body {
            request = request.body(body.clone());
        }

        // Set timeout
        if let Some(timeout_ms) = req.timeout_ms {
            request = request.timeout(Duration::from_millis(timeout_ms));
        }

        let start = std::time::Instant::now();
        let response = request.send().await.map_err(|e| {
            if e.is_timeout() {
                ExecutorError::RetryableError("Request timed out".to_string())
            } else if e.is_connect() {
                ExecutorError::RetryableError(format!("Connection error: {}", e))
            } else if e.is_request() || e.is_body() {
                ExecutorError::RetryableError(format!("Request error: {}", e))
            } else {
                ExecutorError::RetryableError(format!("Network error: {}", e))
            }
        })?;

        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();

        let body = response.text().await.unwrap_or_default();

        let duration_ms = start.elapsed().as_millis();
        let size_bytes = body.len();

        Ok(ExecuteResponse {
            status,
            headers,
            body,
            duration_ms,
            size_bytes,
        })
    }

    /// Check if an HTTP status code is retryable
    /// Retryable: 5xx errors, 429 (too many requests), 408 (request timeout)
    /// Not retryable: 4xx errors (except 408, 429), 2xx, 3xx
    fn is_retryable_status(&self, status: u16) -> bool {
        matches!(
            status,
            408 | 429 | 500 | 502 | 503 | 504 // Request timeout, rate limit, server errors
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_local_executor_health() {
        let executor = LocalExecutor::new();
        assert!(executor.healthcheck().await.is_ok());
    }

    // ========================================================================
    // Phase 9: Retry Policy Tests
    // ========================================================================

    #[test]
    fn test_retry_policy_default() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.max_attempts, 3);
        assert_eq!(policy.initial_backoff_ms, 100);
        assert_eq!(policy.max_backoff_ms, 5000);
        assert_eq!(policy.backoff_multiplier, 2.0);
    }

    #[test]
    fn test_retry_policy_no_retry() {
        let policy = RetryPolicy::no_retry();
        assert_eq!(policy.max_attempts, 1);
        assert_eq!(policy.initial_backoff_ms, 0);
    }

    #[test]
    fn test_retry_policy_conservative() {
        let policy = RetryPolicy::conservative();
        assert_eq!(policy.max_attempts, 2);
        assert_eq!(policy.initial_backoff_ms, 500);
    }

    #[test]
    fn test_retry_policy_aggressive() {
        let policy = RetryPolicy::aggressive();
        assert_eq!(policy.max_attempts, 5);
        assert_eq!(policy.initial_backoff_ms, 100);
    }

    #[test]
    fn test_backoff_duration_exponential() {
        let policy = RetryPolicy::default();

        // First attempt: no backoff
        assert_eq!(policy.backoff_duration(0).as_millis(), 0);

        // Second attempt: 100ms
        assert_eq!(policy.backoff_duration(1).as_millis(), 100);

        // Third attempt: 200ms (100 * 2^1)
        assert_eq!(policy.backoff_duration(2).as_millis(), 200);

        // Fourth attempt: 400ms (100 * 2^2)
        assert_eq!(policy.backoff_duration(3).as_millis(), 400);

        // Fifth attempt: 800ms (100 * 2^3)
        assert_eq!(policy.backoff_duration(4).as_millis(), 800);
    }

    #[test]
    fn test_backoff_duration_max_cap() {
        let policy = RetryPolicy {
            max_attempts: 10,
            initial_backoff_ms: 100,
            max_backoff_ms: 500,
            backoff_multiplier: 2.0,
        };

        // Backoff should not exceed max_backoff_ms
        assert_eq!(policy.backoff_duration(1).as_millis(), 100);
        assert_eq!(policy.backoff_duration(2).as_millis(), 200);
        assert_eq!(policy.backoff_duration(3).as_millis(), 400);
        assert_eq!(policy.backoff_duration(4).as_millis(), 500); // capped
        assert_eq!(policy.backoff_duration(5).as_millis(), 500); // still capped
    }

    #[test]
    fn test_retry_policy_custom() {
        let policy = RetryPolicy::new(5, 200, 3000, 1.5);
        assert_eq!(policy.max_attempts, 5);
        assert_eq!(policy.initial_backoff_ms, 200);
        assert_eq!(policy.max_backoff_ms, 3000);
        assert_eq!(policy.backoff_multiplier, 1.5);
    }

    #[test]
    fn test_retry_policy_min_attempts() {
        // Ensure max_attempts is at least 1
        let policy = RetryPolicy::new(0, 100, 5000, 2.0);
        assert_eq!(policy.max_attempts, 1);
    }

    #[test]
    fn test_executor_error_is_retryable() {
        assert!(ExecutorError::RetryableError("test".to_string()).is_retryable());
        assert!(ExecutorError::TimeoutError.is_retryable());
        assert!(ExecutorError::NetworkError("test".to_string()).is_retryable());
        assert!(ExecutorError::RemoteConnectionError("test".to_string()).is_retryable());

        assert!(!ExecutorError::PermanentError("test".to_string()).is_retryable());
        assert!(!ExecutorError::InvalidUrl("test".to_string()).is_retryable());
        assert!(!ExecutorError::AuthenticationError("test".to_string()).is_retryable());
        assert!(!ExecutorError::SerializationError("test".to_string()).is_retryable());
    }

    #[test]
    fn test_executor_error_is_permanent() {
        assert!(ExecutorError::PermanentError("test".to_string()).is_permanent());
        assert!(ExecutorError::InvalidUrl("test".to_string()).is_permanent());
        assert!(ExecutorError::AuthenticationError("test".to_string()).is_permanent());
        assert!(ExecutorError::SerializationError("test".to_string()).is_permanent());

        assert!(!ExecutorError::RetryableError("test".to_string()).is_permanent());
        assert!(!ExecutorError::TimeoutError.is_permanent());
        assert!(!ExecutorError::NetworkError("test".to_string()).is_permanent());
        assert!(!ExecutorError::RemoteConnectionError("test".to_string()).is_permanent());
    }

    #[test]
    fn test_local_executor_retryable_status() {
        let executor = LocalExecutor::new();

        // Retryable status codes
        assert!(executor.is_retryable_status(408)); // Request Timeout
        assert!(executor.is_retryable_status(429)); // Too Many Requests
        assert!(executor.is_retryable_status(500)); // Internal Server Error
        assert!(executor.is_retryable_status(502)); // Bad Gateway
        assert!(executor.is_retryable_status(503)); // Service Unavailable
        assert!(executor.is_retryable_status(504)); // Gateway Timeout

        // Non-retryable status codes
        assert!(!executor.is_retryable_status(200)); // OK
        assert!(!executor.is_retryable_status(201)); // Created
        assert!(!executor.is_retryable_status(300)); // Multiple Choices
        assert!(!executor.is_retryable_status(301)); // Moved Permanently
        assert!(!executor.is_retryable_status(400)); // Bad Request
        assert!(!executor.is_retryable_status(401)); // Unauthorized
        assert!(!executor.is_retryable_status(403)); // Forbidden
        assert!(!executor.is_retryable_status(404)); // Not Found
    }

    #[test]
    fn test_executor_error_display() {
        assert_eq!(
            ExecutorError::RetryableError("connection failed".to_string()).to_string(),
            "Retryable error: connection failed"
        );

        assert_eq!(
            ExecutorError::PermanentError("invalid auth".to_string()).to_string(),
            "Permanent error: invalid auth"
        );

        assert_eq!(ExecutorError::TimeoutError.to_string(), "Request timed out");

        assert_eq!(
            ExecutorError::InvalidUrl("bad url".to_string()).to_string(),
            "Invalid URL: bad url"
        );
    }
}
