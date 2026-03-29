use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub llm: LlmConfig,
    pub embedding: EmbeddingConfig,
    pub database: DatabaseConfig,
    pub memory: MemoryConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LlmConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub summarize_prompt: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EmbeddingConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub embedding_dim: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseConfig {
    pub db_path: String,
    pub collection_name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryConfig {
    pub max_memory_results: usize,
    pub idle_timeout_minutes: u64,
    pub overlap_threshold: f32,
    pub enable_dedup: bool,
    pub similarity_weight: f32,
    pub importance_weight: f32,
    #[serde(default = "default_domains")]
    pub domains: Vec<String>,
}

fn default_domains() -> Vec<String> {
    vec![
        "frontend_dev".to_string(),
        "backend_dev".to_string(),
        "daily_life".to_string(),
    ]
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoggingConfig {
    pub log_file: String,
}

impl Config {
    pub fn from_yaml(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    pub fn expand_paths(&mut self) {
        if self.database.db_path.starts_with("~") {
            if let Some(home) = std::env::var("HOME").ok() {
                self.database.db_path = self.database.db_path.replacen("~", &home, 1);
            }
        }
        if self.logging.log_file.starts_with("~") {
            if let Some(home) = std::env::var("HOME").ok() {
                self.logging.log_file = self.logging.log_file.replacen("~", &home, 1);
            }
        }
    }
}
