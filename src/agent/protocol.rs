// Phase 2: Remote Agent Protocol
// This defines how the local controller talks to the remote agent

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// Protocol Definition: JSON messages over SSH stdio
// ============================================================================

/// Message sent from local controller to remote agent
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub enum ControllerMessage {
    Execute(ExecutionPayload),
    Health,
    Shutdown,
}

/// Message sent from remote agent back to local controller
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub enum AgentMessage {
    ExecutionResult(ExecutionResult),
    HealthOk,
    Error(String),
    Shutdown,
}

/// Execution payload sent to remote
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct ExecutionPayload {
    pub request_id: String,
    pub method: String,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub query_params: HashMap<String, String>,
    pub body: Option<String>,
    pub timeout_ms: Option<u64>,
}

/// Execution result from remote
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct ExecutionResult {
    pub request_id: String,
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub duration_ms: u128,
    pub size_bytes: usize,
    pub timestamp: i64,
}

// ============================================================================
// Utility Functions
// ============================================================================
#[allow(dead_code)]
impl ControllerMessage {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}
#[allow(dead_code)]
impl AgentMessage {
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_serialization() {
        let payload = ExecutionPayload {
            request_id: "req-1".to_string(),
            method: "GET".to_string(),
            url: "http://localhost:8080/api".to_string(),
            headers: Default::default(),
            query_params: Default::default(),
            body: None,
            timeout_ms: Some(5000),
        };

        let msg = ControllerMessage::Execute(payload);
        let json = msg.to_json().unwrap();

        // Should contain request_id
        assert!(json.contains("req-1"));
        assert!(json.contains("GET"));
    }
}
