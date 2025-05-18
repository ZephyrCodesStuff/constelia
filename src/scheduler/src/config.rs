use std::net::Ipv4Addr;

use figment::{Figment, providers::{Env, Format, Toml}};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub heartbeat: HeartbeatConfig,
    pub submitter: SubmitterConfig,
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
}

impl Default for SubmitterConfig {
    fn default() -> Self {
        Self { enabled: false, host: Ipv4Addr::new(0, 0, 0, 0), port: 50053 }
    }
}


impl Config {
    pub fn new() -> Result<Self, figment::Error> {
        Figment::new()
            .merge(Env::prefixed("SCHEDULER_"))
            .merge(Toml::file("config.toml"))
            .extract()
    }
}


