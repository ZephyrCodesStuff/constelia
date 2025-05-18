use std::net::Ipv4Addr;

use figment::{Figment, providers::{Env, Format, Toml}};
use serde::Deserialize;

use crate::services::runner::Target as RunnerTarget;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub heartbeat: HeartbeatConfig,
    pub submitter: SubmitterConfig,

    #[serde(default)]
    pub targets: Vec<Target>,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub host: Ipv4Addr,
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self { host: Ipv4Addr::new(0, 0, 0, 0), port: 50052 }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct HeartbeatConfig {
    /// How long to wait for a heartbeat before considering a runner dead
    pub timeout: u64,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self { timeout: 60 }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct SubmitterConfig {
    #[serde(default)]
    pub enabled: bool,

    pub host: Ipv4Addr,
    pub port: u16,

    /// Regex to match flags
    /// 
    /// Even if the submitter is disabled, this regex is used to validate flags
    pub flag_regex: String,
}

impl Default for SubmitterConfig {
    fn default() -> Self {
        Self { enabled: false, host: Ipv4Addr::new(0, 0, 0, 0), port: 50053, flag_regex: "[A-Z0-9]{31}=".to_string() }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Target {
    /// ID of the target
    ///
    /// IDs aren't always required so this is optional
    pub id: Option<String>,

    /// Host of the target
    pub host: Ipv4Addr,

    /// Port of the target
    pub port: u16,
}

// Boilerplate to avoid implementing `Deserialize` for `RunnerTarget`
impl From<Target> for RunnerTarget {
    fn from(target: Target) -> Self {
        RunnerTarget { id: target.id, host: target.host.to_string(), port: target.port as u32 }
    }
}

impl Config {
    pub fn new() -> Result<Self, figment::Error> {
        Figment::new()
            .merge(Env::prefixed("SCHEDULER_"))
            .merge(Toml::file("config.toml"))
            .merge(Toml::file("targets.toml")) // This file can get quite large
            .extract()
    }
}


