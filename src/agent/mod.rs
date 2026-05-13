// Phase 2: Remote Agent Protocol and Types
// This module defines the communication protocol between local controller and remote agent

pub mod protocol;

// Re-export for convenience
// pub use protocol::{AgentMessage, ControllerMessage, ExecutionPayload, ExecutionResult};

// ============================================================================
// Agent types and helpers
// ============================================================================

/// Represents a remote agent connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum AgentStatus {
    Disconnected,
    // Connecting,
    Connected,
    Error,
}

impl std::fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disconnected => write!(f, "Disconnected"),
            // Self::Connecting => write!(f, "Connecting..."),
            Self::Connected => write!(f, "Connected"),
            Self::Error => write!(f, "Error"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_status_display() {
        assert_eq!(AgentStatus::Connected.to_string(), "Connected");
        assert_eq!(AgentStatus::Disconnected.to_string(), "Disconnected");
    }
}
