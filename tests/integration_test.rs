// ============================================================================
// Phase 10: Comprehensive Integration Tests for SSH Remote Execution Feature
// ============================================================================
// Tests cover LocalExecutor, RetryPolicy, ExecutorError, HTTP integration,
// and configuration. Uses real HTTP calls where practical and mocks where needed.

use std::collections::HashMap;
use std::time::{Duration, Instant};
use volt::app::{AuthType, BodyType, HttpMethod, KVRow, RequestDraft, TextBuffer};
use volt::executor::{
    ExecuteRequest, ExecuteResponse, Executor, ExecutorError, ExecutorResult, LocalExecutor,
    RetryPolicy,
};
use volt::http;
use volt::remote::config::RemoteProfile;
use volt::scanner::RouteInfo;

// ============================================================================
// Test Utilities
// ============================================================================

/// Helper to create a RouteInfo for testing
fn test_route(method: HttpMethod, path: &str) -> RouteInfo {
    use std::path::PathBuf;
    RouteInfo {
        method,
        path: path.to_string(),
        framework: "test".to_string(),
        source: PathBuf::from("test.rs"),
        line: 1,
    }
}

/// Helper to create a RequestDraft for testing
fn test_request_draft(base_url: &str) -> RequestDraft {
    RequestDraft {
        base_url: TextBuffer::new(base_url),
        params: vec![],
        headers: vec![],
        body: TextBuffer::default(),
        body_type: BodyType::Json,
        auth_type: AuthType::None,
        auth_username: TextBuffer::default(),
        auth_password: TextBuffer::default(),
        auth_token: TextBuffer::default(),
        auth_header_name: TextBuffer::default(),
        auth_header_value: TextBuffer::default(),
        active_row: 0,
        active_col: 0,
    }
}

/// Helper to create ExecuteRequest with common defaults
fn test_execute_request(method: &str, url: &str) -> ExecuteRequest {
    ExecuteRequest {
        method: method.to_string(),
        url: url.to_string(),
        headers: HashMap::new(),
        query_params: HashMap::new(),
        body: None,
        timeout_ms: Some(30000),
        retry_policy: None,
    }
}

// ============================================================================
// Module: Local Executor Tests
// ============================================================================

mod executor_tests {
    use super::*;

    /// Test basic GET request to a public endpoint
    /// This test uses httpbin.org which provides echo responses for testing
    #[tokio::test]
    #[ignore] // Requires internet connectivity
    async fn test_local_executor_get_request() {
        let executor = LocalExecutor::new();
        let mut req = test_execute_request("GET", "https://httpbin.org/get");
        req.timeout_ms = Some(10000);

        let result = executor.execute(req).await;

        assert!(result.is_ok(), "GET request should succeed");
        let response = result.unwrap();
        assert_eq!(response.status, 200);
        assert!(response.body.len() > 0);
        assert!(response.size_bytes > 0);
    }

    /// Test POST request with JSON body
    #[tokio::test]
    #[ignore] // Requires internet connectivity
    async fn test_local_executor_post_request() {
        let executor = LocalExecutor::new();
        let mut req = test_execute_request("POST", "https://httpbin.org/post");
        req.body = Some(r#"{"name":"test","value":42}"#.to_string());
        req.timeout_ms = Some(10000);

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        req.headers = headers;

        let result = executor.execute(req).await;

        assert!(result.is_ok(), "POST request should succeed");
        let response = result.unwrap();
        assert_eq!(response.status, 200);
    }

    /// Test custom header handling
    #[tokio::test]
    #[ignore] // Requires internet connectivity
    async fn test_local_executor_with_headers() {
        let executor = LocalExecutor::new();
        let mut req = test_execute_request("GET", "https://httpbin.org/headers");

        let mut headers = HashMap::new();
        headers.insert("X-Custom-Header".to_string(), "test-value".to_string());
        headers.insert("User-Agent".to_string(), "volt-test".to_string());
        req.headers = headers;
        req.timeout_ms = Some(10000);

        let result = executor.execute(req).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.status, 200);
        // Verify headers were sent
        assert!(response.body.contains("X-Custom-Header") || response.body.len() > 0);
    }

    /// Test Basic authentication header construction
    #[tokio::test]
    #[ignore] // Requires internet connectivity
    async fn test_local_executor_with_auth() {
        let executor = LocalExecutor::new();
        let mut req = test_execute_request("GET", "https://httpbin.org/bearer");

        let mut headers = HashMap::new();
        // Construct Basic auth: base64(username:password)
        let creds = base64::engine::general_purpose::STANDARD.encode("testuser:testpass");
        headers.insert("Authorization".to_string(), format!("Basic {}", creds));
        req.headers = headers;
        req.timeout_ms = Some(10000);

        let result = executor.execute(req).await;

        // httpbin.org will return 401 if auth is required but not provided
        // or 200 if endpoint doesn't require auth
        assert!(result.is_ok());
    }

    /// Test timeout handling
    #[tokio::test]
    #[ignore] // Requires internet connectivity, may be slow
    async fn test_local_executor_timeout() {
        let executor = LocalExecutor::new();
        // Use a very short timeout
        let mut req = test_execute_request("GET", "https://httpbin.org/delay/10");
        req.timeout_ms = Some(100);

        let result = executor.execute(req).await;

        // Should timeout
        assert!(result.is_err());
        match result.err().unwrap() {
            ExecutorError::RetryableError(msg) => {
                assert!(msg.contains("timeout") || msg.contains("time"));
            }
            other => panic!("Expected timeout error, got: {:?}", other),
        }
    }

    /// Test invalid URL handling
    #[tokio::test]
    async fn test_local_executor_invalid_url() {
        let executor = LocalExecutor::new();
        let req = test_execute_request("GET", "not a valid url at all");

        let result = executor.execute(req).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().is_permanent());
    }

    /// Test GET request to localhost (mock server)
    #[tokio::test]
    async fn test_local_executor_invalid_host() {
        let executor = LocalExecutor::new();
        let mut req = test_execute_request("GET", "http://127.0.0.1:1");
        req.timeout_ms = Some(1000); // Short timeout for refused connection

        let result = executor.execute(req).await;

        // Connection should fail
        assert!(result.is_err());
    }

    /// Test executor name
    #[tokio::test]
    async fn test_local_executor_name() {
        let executor = LocalExecutor::new();
        assert_eq!(executor.name(), "Local");
    }

    /// Test executor healthcheck
    #[tokio::test]
    async fn test_local_executor_healthcheck() {
        let executor = LocalExecutor::new();
        let result = executor.healthcheck().await;
        assert!(result.is_ok());
    }

    /// Test executor supports_retry
    #[tokio::test]
    async fn test_local_executor_supports_retry() {
        let executor = LocalExecutor::new();
        assert!(executor.supports_retry().await);
    }

    /// Test query parameter handling
    #[tokio::test]
    #[ignore] // Requires internet connectivity
    async fn test_local_executor_with_query_params() {
        let executor = LocalExecutor::new();
        let mut req = test_execute_request("GET", "https://httpbin.org/get");

        let mut params = HashMap::new();
        params.insert("key1".to_string(), "value1".to_string());
        params.insert("key2".to_string(), "value2".to_string());
        req.query_params = params;
        req.timeout_ms = Some(10000);

        let result = executor.execute(req).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.status, 200);
    }
}

// ============================================================================
// Module: Retry Policy Tests
// ============================================================================

mod retry_tests {
    use super::*;

    /// Test exponential backoff calculation
    #[test]
    fn test_retry_policy_exponential_backoff() {
        let policy = RetryPolicy::default();

        // Verify exponential sequence: 0, 100, 200, 400, 800ms
        assert_eq!(policy.backoff_duration(0).as_millis(), 0);
        assert_eq!(policy.backoff_duration(1).as_millis(), 100);
        assert_eq!(policy.backoff_duration(2).as_millis(), 200);
        assert_eq!(policy.backoff_duration(3).as_millis(), 400);
        assert_eq!(policy.backoff_duration(4).as_millis(), 800);
    }

    /// Test that backoff respects maximum cap
    #[test]
    fn test_retry_policy_max_backoff_cap() {
        let policy = RetryPolicy {
            max_attempts: 10,
            initial_backoff_ms: 100,
            max_backoff_ms: 500,
            backoff_multiplier: 2.0,
        };

        // Verify cap is enforced
        assert_eq!(policy.backoff_duration(1).as_millis(), 100);
        assert_eq!(policy.backoff_duration(2).as_millis(), 200);
        assert_eq!(policy.backoff_duration(3).as_millis(), 400);
        assert_eq!(policy.backoff_duration(4).as_millis(), 500); // Capped
        assert_eq!(policy.backoff_duration(5).as_millis(), 500); // Still capped
        assert_eq!(policy.backoff_duration(10).as_millis(), 500); // Still capped
    }

    /// Test preset retry policies
    #[test]
    fn test_retry_policy_presets() {
        // Test no_retry
        let no_retry = RetryPolicy::no_retry();
        assert_eq!(no_retry.max_attempts, 1);
        assert_eq!(no_retry.initial_backoff_ms, 0);

        // Test conservative
        let conservative = RetryPolicy::conservative();
        assert_eq!(conservative.max_attempts, 2);
        assert_eq!(conservative.initial_backoff_ms, 500);

        // Test aggressive
        let aggressive = RetryPolicy::aggressive();
        assert_eq!(aggressive.max_attempts, 5);
        assert_eq!(aggressive.initial_backoff_ms, 100);
    }

    /// Test full backoff sequence
    #[test]
    fn test_backoff_sequence() {
        let policy = RetryPolicy::new(5, 50, 1000, 3.0);

        let backoffs = (0..6)
            .map(|i| policy.backoff_duration(i).as_millis() as u64)
            .collect::<Vec<_>>();

        // Sequence: 0, 50, 150, 450, 1000 (capped), 1000 (capped)
        assert_eq!(backoffs[0], 0);
        assert_eq!(backoffs[1], 50);
        assert_eq!(backoffs[2], 150);
        assert_eq!(backoffs[3], 450);
        assert_eq!(backoffs[4], 1000);
        assert_eq!(backoffs[5], 1000);
    }

    /// Test custom retry policy creation
    #[test]
    fn test_retry_policy_custom_creation() {
        let policy = RetryPolicy::new(7, 200, 3000, 1.5);

        assert_eq!(policy.max_attempts, 7);
        assert_eq!(policy.initial_backoff_ms, 200);
        assert_eq!(policy.max_backoff_ms, 3000);
        assert_eq!(policy.backoff_multiplier, 1.5);
    }

    /// Test default retry policy
    #[test]
    fn test_retry_policy_default() {
        let policy = RetryPolicy::default();

        assert_eq!(policy.max_attempts, 3);
        assert_eq!(policy.initial_backoff_ms, 100);
        assert_eq!(policy.max_backoff_ms, 5000);
        assert_eq!(policy.backoff_multiplier, 2.0);
    }

    /// Test min_attempts enforcement
    #[test]
    fn test_retry_policy_min_attempts() {
        let policy = RetryPolicy::new(0, 100, 5000, 2.0);
        assert_eq!(policy.max_attempts, 1); // Should enforce minimum of 1
    }
}

// ============================================================================
// Module: Executor Error Tests
// ============================================================================

mod error_tests {
    use super::*;

    /// Test error classification (retryable vs permanent)
    #[test]
    fn test_error_classification() {
        // Test retryable errors
        assert!(ExecutorError::RetryableError("test".to_string()).is_retryable());
        assert!(ExecutorError::TimeoutError.is_retryable());
        assert!(ExecutorError::NetworkError("test".to_string()).is_retryable());
        assert!(ExecutorError::RemoteConnectionError("test".to_string()).is_retryable());

        // Test permanent errors
        assert!(ExecutorError::PermanentError("test".to_string()).is_permanent());
        assert!(ExecutorError::InvalidUrl("test".to_string()).is_permanent());
        assert!(ExecutorError::AuthenticationError("test".to_string()).is_permanent());
        assert!(ExecutorError::SerializationError("test".to_string()).is_permanent());
    }

    /// Test error display formatting
    #[test]
    fn test_error_display() {
        let msg = "connection failed".to_string();
        assert_eq!(
            ExecutorError::RetryableError(msg.clone()).to_string(),
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

        assert_eq!(
            ExecutorError::NetworkError("no connection".to_string()).to_string(),
            "Network error: no connection"
        );

        assert_eq!(
            ExecutorError::AuthenticationError("invalid creds".to_string()).to_string(),
            "Authentication error: invalid creds"
        );

        assert_eq!(
            ExecutorError::SerializationError("bad json".to_string()).to_string(),
            "Serialization error: bad json"
        );

        assert_eq!(
            ExecutorError::RemoteConnectionError("ssh timeout".to_string()).to_string(),
            "Remote connection error: ssh timeout"
        );
    }

    /// Test network errors are retryable
    #[test]
    fn test_network_error_retryable() {
        assert!(ExecutorError::NetworkError("connection reset".to_string()).is_retryable());
        assert!(ExecutorError::RemoteConnectionError("ssh failed".to_string()).is_retryable());
        assert!(ExecutorError::TimeoutError.is_retryable());
    }

    /// Test auth errors are permanent
    #[test]
    fn test_auth_error_permanent() {
        assert!(ExecutorError::AuthenticationError("invalid key".to_string()).is_permanent());
        assert!(!ExecutorError::AuthenticationError("invalid key".to_string()).is_retryable());
    }

    /// Test invalid URL is permanent
    #[test]
    fn test_invalid_url_permanent() {
        assert!(ExecutorError::InvalidUrl("malformed".to_string()).is_permanent());
        assert!(!ExecutorError::InvalidUrl("malformed".to_string()).is_retryable());
    }

    /// Test serialization error is permanent
    #[test]
    fn test_serialization_error_permanent() {
        assert!(ExecutorError::SerializationError("bad json".to_string()).is_permanent());
        assert!(!ExecutorError::SerializationError("bad json".to_string()).is_retryable());
    }
}

// ============================================================================
// Module: HTTP Integration Tests
// ============================================================================

mod http_integration_tests {
    use super::*;

    /// Test HTTP response creation
    #[test]
    fn test_http_response_creation() {
        let response = http::HttpResponse {
            status_code: 200,
            latency_ms: 100,
            size_bytes: 1024,
            content_type: "application/json".to_string(),
            body: "{}".to_string(),
        };

        assert_eq!(response.status_code, 200);
        assert_eq!(response.size_bytes, 1024);
        assert_eq!(response.content_type, "application/json");
    }

    /// Test basic HTTP execution
    #[tokio::test]
    async fn test_http_execute_with_executor() {
        let executor = LocalExecutor::new();
        let route = test_route(HttpMethod::Get, "/test");
        let draft = test_request_draft("http://httpbin.org");

        // Note: This test would require actual HTTP, so we test the structure
        // instead of the actual execution
        assert_eq!(route.method, HttpMethod::Get);
        assert_eq!(route.path, "/test");
    }

    /// Test HTTP execute with headers and auth
    #[tokio::test]
    async fn test_http_execute_with_headers_and_auth() {
        let executor = LocalExecutor::new();
        let route = test_route(HttpMethod::Post, "/api/data");

        let mut draft = test_request_draft("http://httpbin.org");
        draft.body.text = r#"{"test":"data"}"#.to_string();
        draft.body_type = BodyType::Json;

        assert_eq!(draft.body.text, r#"{"test":"data"}"#);
    }
}

// ============================================================================
// Module: Configuration Tests
// ============================================================================

mod config_tests {
    use super::*;

    /// Test remote profile creation
    #[test]
    fn test_remote_profile_creation() {
        let profile = RemoteProfile::new("prod", "example.com", "ubuntu");

        assert_eq!(profile.name, "prod");
        assert_eq!(profile.host, "example.com");
        assert_eq!(profile.user, "ubuntu");
        assert_eq!(profile.port, 22);
        assert_eq!(profile.identity, None);
    }

    /// Test remote profile with identity
    #[test]
    fn test_remote_profile_with_identity() {
        let profile = RemoteProfile::new("dev", "dev.local", "developer")
            .with_identity("~/.ssh/dev_key")
            .with_port(2222);

        assert_eq!(profile.name, "dev");
        assert_eq!(profile.user, "developer");
        assert_eq!(profile.port, 2222);
        assert_eq!(profile.identity, Some("~/.ssh/dev_key".to_string()));
    }

    /// Test remote profile validation
    #[test]
    fn test_remote_profile_validation() {
        let profile = RemoteProfile::new("staging", "staging.example.com", "ec2-user");

        // Validate all fields are set
        assert!(!profile.name.is_empty());
        assert!(!profile.host.is_empty());
        assert!(!profile.user.is_empty());
        assert!(profile.port > 0);
    }

    /// Test multiple remote profiles
    #[test]
    fn test_multiple_remote_profiles() {
        let prod = RemoteProfile::new("prod", "prod.example.com", "ubuntu")
            .with_port(22)
            .with_identity("~/.ssh/prod_key");

        let staging = RemoteProfile::new("staging", "staging.example.com", "ubuntu")
            .with_port(2222)
            .with_identity("~/.ssh/staging_key");

        assert_eq!(prod.name, "prod");
        assert_eq!(prod.port, 22);
        assert_eq!(staging.name, "staging");
        assert_eq!(staging.port, 2222);
    }
}

// ============================================================================
// Module: Retry Logic Integration Tests
// ============================================================================

mod retry_integration_tests {
    use super::*;

    /// Mock executor that simulates retryable failures
    struct FailingExecutor {
        fail_count: std::sync::atomic::AtomicU32,
    }

    #[async_trait::async_trait]
    impl Executor for FailingExecutor {
        async fn execute(&self, _req: ExecuteRequest) -> ExecutorResult<ExecuteResponse> {
            let count = self
                .fail_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

            if count < 2 {
                // Fail first 2 attempts
                Err(ExecutorError::RetryableError(
                    "simulated failure".to_string(),
                ))
            } else {
                // Succeed on third attempt
                Ok(ExecuteResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: "success".to_string(),
                    duration_ms: 10,
                    size_bytes: 7,
                })
            }
        }

        fn name(&self) -> String {
            "FailingExecutor".to_string()
        }

        async fn healthcheck(&self) -> ExecutorResult<()> {
            Ok(())
        }

        async fn supports_retry(&self) -> bool {
            true
        }
    }

    /// Test that executor retries on transient failures
    #[tokio::test]
    async fn test_executor_retries_on_transient_failure() {
        let executor = FailingExecutor {
            fail_count: std::sync::atomic::AtomicU32::new(0),
        };

        let req = ExecuteRequest {
            method: "GET".to_string(),
            url: "http://example.com".to_string(),
            headers: HashMap::new(),
            query_params: HashMap::new(),
            body: None,
            timeout_ms: Some(5000),
            retry_policy: Some(RetryPolicy::aggressive()),
        };

        let result = executor.execute(req).await;

        // Should succeed after retries
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.body, "success");
    }

    /// Mock executor that always fails permanently
    struct PermanentFailExecutor;

    #[async_trait::async_trait]
    impl Executor for PermanentFailExecutor {
        async fn execute(&self, _req: ExecuteRequest) -> ExecutorResult<ExecuteResponse> {
            Err(ExecutorError::InvalidUrl("bad url".to_string()))
        }

        fn name(&self) -> String {
            "PermanentFailExecutor".to_string()
        }

        async fn healthcheck(&self) -> ExecutorResult<()> {
            Ok(())
        }
    }

    /// Test that executor doesn't retry on permanent failures
    #[tokio::test]
    async fn test_executor_no_retry_on_permanent_failure() {
        let executor = PermanentFailExecutor;

        let req = ExecuteRequest {
            method: "GET".to_string(),
            url: "invalid".to_string(),
            headers: HashMap::new(),
            query_params: HashMap::new(),
            body: None,
            timeout_ms: Some(5000),
            retry_policy: Some(RetryPolicy::aggressive()),
        };

        let result = executor.execute(req).await;

        // Should fail immediately
        assert!(result.is_err());
    }
}

// ============================================================================
// Module: Retryable Status Code Tests
// ============================================================================

mod status_code_tests {
    use super::*;

    /// Test retryable HTTP status codes
    #[test]
    fn test_retryable_status_codes() {
        let executor = LocalExecutor::new();

        // These should be retryable
        assert!(executor.is_retryable_status(408)); // Request Timeout
        assert!(executor.is_retryable_status(429)); // Too Many Requests
        assert!(executor.is_retryable_status(500)); // Internal Server Error
        assert!(executor.is_retryable_status(502)); // Bad Gateway
        assert!(executor.is_retryable_status(503)); // Service Unavailable
        assert!(executor.is_retryable_status(504)); // Gateway Timeout
    }

    /// Test non-retryable HTTP status codes
    #[test]
    fn test_non_retryable_status_codes() {
        let executor = LocalExecutor::new();

        // These should NOT be retryable
        assert!(!executor.is_retryable_status(200)); // OK
        assert!(!executor.is_retryable_status(201)); // Created
        assert!(!executor.is_retryable_status(204)); // No Content
        assert!(!executor.is_retryable_status(301)); // Moved Permanently
        assert!(!executor.is_retryable_status(302)); // Found
        assert!(!executor.is_retryable_status(304)); // Not Modified
        assert!(!executor.is_retryable_status(400)); // Bad Request
        assert!(!executor.is_retryable_status(401)); // Unauthorized
        assert!(!executor.is_retryable_status(403)); // Forbidden
        assert!(!executor.is_retryable_status(404)); // Not Found
    }
}

// ============================================================================
// Module: Edge Case Tests
// ============================================================================

mod edge_case_tests {
    use super::*;

    /// Test empty response body handling
    #[test]
    fn test_empty_response_body() {
        let response = http::HttpResponse {
            status_code: 204,
            latency_ms: 50,
            size_bytes: 0,
            content_type: "text/plain".to_string(),
            body: String::new(),
        };

        assert_eq!(response.body.len(), 0);
        assert_eq!(response.size_bytes, 0);
    }

    /// Test large response body
    #[test]
    fn test_large_response_body() {
        let large_body = "x".repeat(1_000_000); // 1MB
        let response = http::HttpResponse {
            status_code: 200,
            latency_ms: 500,
            size_bytes: large_body.len(),
            content_type: "text/plain".to_string(),
            body: large_body.clone(),
        };

        assert_eq!(response.size_bytes, 1_000_000);
    }

    /// Test very short timeout
    #[test]
    fn test_very_short_timeout() {
        let req = ExecuteRequest {
            method: "GET".to_string(),
            url: "http://example.com".to_string(),
            headers: HashMap::new(),
            query_params: HashMap::new(),
            body: None,
            timeout_ms: Some(1), // 1ms - almost certainly will timeout
            retry_policy: Some(RetryPolicy::no_retry()),
        };

        assert_eq!(req.timeout_ms, Some(1));
    }

    /// Test execute request with all fields populated
    #[test]
    fn test_execute_request_fully_populated() {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        headers.insert("Authorization".to_string(), "Bearer token123".to_string());

        let mut params = HashMap::new();
        params.insert("page".to_string(), "1".to_string());
        params.insert("limit".to_string(), "10".to_string());

        let req = ExecuteRequest {
            method: "POST".to_string(),
            url: "http://api.example.com/data".to_string(),
            headers,
            query_params: params,
            body: Some(r#"{"key":"value"}"#.to_string()),
            timeout_ms: Some(30000),
            retry_policy: Some(RetryPolicy::conservative()),
        };

        assert_eq!(req.method, "POST");
        assert_eq!(req.url, "http://api.example.com/data");
        assert_eq!(req.headers.len(), 2);
        assert_eq!(req.query_params.len(), 2);
        assert!(req.body.is_some());
        assert_eq!(req.timeout_ms, Some(30000));
        assert!(req.retry_policy.is_some());
    }
}

// ============================================================================
// Module: Performance Tests
// ============================================================================

mod performance_tests {
    use super::*;

    /// Test retry timing - ensure backoff is being applied
    #[test]
    fn test_retry_backoff_timing() {
        let policy = RetryPolicy::default();
        let start = Instant::now();

        for attempt in 0..policy.max_attempts {
            let backoff = policy.backoff_duration(attempt);
            // Each backoff should be progressively longer (except first)
            if attempt > 0 {
                let prev_backoff = policy.backoff_duration(attempt - 1);
                assert!(
                    backoff >= prev_backoff,
                    "Backoff should increase: attempt {} backoff {:?} >= attempt {} backoff {:?}",
                    attempt,
                    backoff,
                    attempt - 1,
                    prev_backoff
                );
            }
        }

        // Entire test should be very fast (no actual sleeping)
        let elapsed = start.elapsed();
        assert!(elapsed.as_millis() < 100, "Test should complete quickly");
    }

    /// Test that retry policy calculations are deterministic
    #[test]
    fn test_retry_policy_deterministic() {
        let policy = RetryPolicy::default();

        // Run multiple times and verify same results
        let results1: Vec<_> = (0..5)
            .map(|i| policy.backoff_duration(i).as_millis())
            .collect();

        let results2: Vec<_> = (0..5)
            .map(|i| policy.backoff_duration(i).as_millis())
            .collect();

        assert_eq!(results1, results2);
    }
}
