use std::net::Ipv4Addr;

use figment::{
    providers::{Env, Format, Toml},
    Figment,
};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub scheduler: SchedulerConfig,
    pub runner: RunnerConfig,
    pub heartbeat: HeartbeatConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SchedulerConfig {
    pub host: Ipv4Addr,
    pub port: u16,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            host: Ipv4Addr::new(127, 0, 0, 1),
            port: 50052,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct RunnerConfig {
    /// ID to identify the runner with the scheduler
    pub id: String,

    /// Host to listen on for incoming connections from the scheduler
    pub host: Ipv4Addr,

    /// Port to listen on for incoming connections from the scheduler
    pub port: u16,

    /// Maximum number of jobs to run in parallel
    ///
    /// Configure this based on your runner's resources.
    pub max_parallel_jobs: u32,

    /// Docker image to use for the runner
    pub docker_image: String,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            id: "runner-1".to_string(),
            host: Ipv4Addr::new(127, 0, 0, 1),
            port: 50051,
            max_parallel_jobs: 5,
            docker_image: "python:3.13-alpine".to_string(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct HeartbeatConfig {
    /// How often to send a heartbeat to the scheduler
    pub interval: u64,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self { interval: 30 }
    }
}

impl Config {
    pub fn new() -> Result<Self, figment::Error> {
        let config = Figment::new()
            .merge(Toml::file("config.toml"))
            .merge(Env::prefixed("RUNNER_"))
            .extract()?;
        Ok(config)
    }
}
