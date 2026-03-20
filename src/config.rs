use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RouteConfig {
    pub headers: String,
    pub body: String,
    pub params: String,
    pub auth: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub base_url: String,
    pub last_selected_route: Option<String>,
    pub drafts: HashMap<String, RouteConfig>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:3000".to_string(),
            last_selected_route: None,
            drafts: HashMap::new(),
        }
    }
}

impl AppConfig {
    pub fn load() -> anyhow::Result<Self> {
        let path = Self::path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(&path)?;
        Ok(toml::from_str(&content).unwrap_or_default())
    }

    fn path() -> PathBuf {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(".volt.toml")
    }
}
