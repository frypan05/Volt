use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RouteConfig {
    pub headers: String,
    pub body: String,
    pub params: String,
    pub auth: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    pub theme: String,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            theme: "vesper".to_string(),
        }
    }
}

impl GlobalConfig {
    pub fn load() -> Self {
        let path = Self::path();
        if !path.exists() {
            return Self::default();
        }
        let content = fs::read_to_string(&path).unwrap_or_default();
        toml::from_str(&content).unwrap_or_default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    fn path() -> PathBuf {
        if let Some(proj_dirs) = ProjectDirs::from("com", "volt", "volt") {
            proj_dirs.config_dir().join("config.toml")
        } else {
            // Fallback for systems without a home/config directory
            PathBuf::from(".volt_global.toml")
        }
    }
}

// ============================================================================
// Phase 4: Remote Configuration Support
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteConfigEntry {
    pub host: String,
    pub user: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub identity: Option<String>,
}

fn default_port() -> u16 {
    22
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub base_url: String,
    pub last_selected_route: Option<String>,
    #[serde(default)]
    pub drafts: HashMap<String, RouteConfig>,
    #[serde(default)]
    pub remote: Option<HashMap<String, RemoteConfigEntry>>,
    #[serde(default)]
    pub current_remote: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:3000".to_string(),
            last_selected_route: None,
            drafts: HashMap::new(),
            remote: None,
            current_remote: None,
        }
    }
}

impl AppConfig {
    pub fn create_template_if_missing() -> anyhow::Result<bool> {
        let path = Self::path();
        if path.exists() {
            return Ok(false);
        }

        let template = r#"base_url = "http://localhost:3000"

[remote.production]
host = "prod-bastion.example.com"
user = "ubuntu"
port = 22
identity = "~/.ssh/volt_prod"

[remote.staging]
host = "staging-internal.company.com"
user = "deploy"
port = 22
identity = "~/.ssh/staging_key"
"#;

        fs::write(&path, template)?;
        Ok(true)
    }
    pub fn load() -> anyhow::Result<Self> {
        let path = Self::path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(&path)?;
        Ok(toml::from_str(&content).unwrap_or_else(|_| Self::default()))
    }
    #[allow(dead_code)]
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    fn path() -> PathBuf {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(".volt.toml")
    }

    /// Get a remote profile by name
    #[allow(dead_code)]
    pub fn get_remote(&self, name: &str) -> Option<RemoteConfigEntry> {
        self.remote.as_ref()?.get(name).cloned()
    }

    /// Set current remote executor
    #[allow(dead_code)]
    pub fn set_current_remote(&mut self, remote_name: Option<String>) {
        self.current_remote = remote_name;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_config_default() {
        let config = AppConfig::default();
        assert_eq!(config.base_url, "http://localhost:3000");
        assert!(config.remote.is_none());
        assert!(config.current_remote.is_none());
    }

    #[test]
    fn test_remote_config_entry() {
        let entry = RemoteConfigEntry {
            host: "bastion.example.com".to_string(),
            user: "ubuntu".to_string(),
            port: 22,
            identity: Some("~/.ssh/id_ed25519".to_string()),
        };
        assert_eq!(entry.host, "bastion.example.com");
        assert_eq!(entry.user, "ubuntu");
    }
}
