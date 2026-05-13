// Phase 4: Remote Configuration
// Handles loading remote profiles from .volt.toml

use serde::{Deserialize, Serialize};

/// A remote profile that can be selected via CLI
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct RemoteProfile {
    pub name: String,
    pub host: String,
    pub user: String,
    pub port: u16,
    pub identity: Option<String>,
}
#[allow(dead_code)]
impl RemoteProfile {
    /// Create a new remote profile
    pub fn new(name: impl Into<String>, host: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            host: host.into(),
            user: user.into(),
            port: 22,
            identity: None,
        }
    }

    /// Set the SSH key identity
    pub fn with_identity(mut self, identity: impl Into<String>) -> Self {
        self.identity = Some(identity.into());
        self
    }

    /// Set the SSH port
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remote_profile_creation() {
        let profile = RemoteProfile::new("prod", "bastion.example.com", "ubuntu")
            .with_port(2222)
            .with_identity("~/.ssh/prod_key");

        assert_eq!(profile.name, "prod");
        assert_eq!(profile.host, "bastion.example.com");
        assert_eq!(profile.user, "ubuntu");
        assert_eq!(profile.port, 2222);
        assert_eq!(profile.identity, Some("~/.ssh/prod_key".to_string()));
    }
}
