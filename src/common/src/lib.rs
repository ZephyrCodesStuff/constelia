use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Invalid target: {0}")]
    InvalidTarget(String),
    #[error("Invalid exploit: {0}")]
    InvalidExploit(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub service: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExploitMetadata {
    pub name: String,
    pub description: String,
    pub author: String,
    pub tags: Vec<String>,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flag {
    pub value: String,
    pub target_id: String,
    pub exploit_name: String,
    pub timestamp: DateTime<Utc>,
}
