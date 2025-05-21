use anyhow::Result;
use runner::{runner_server::RunnerServer, RunnerStatus};
use services::runner::RunnerService;
use std::{
    net::SocketAddr,
    sync::{Arc, RwLock},
};
use tonic::transport::Server;
use tracing::info;
use tracing_subscriber::EnvFilter;

mod config;
mod services;

pub mod runner {
    tonic::include_proto!("runner");
    pub const FILE_DESCRIPTOR_SET: &[u8] = tonic::include_file_descriptor_set!("runner_descriptor");
}

pub mod heartbeat {
    tonic::include_proto!("heartbeat");
    pub const FILE_DESCRIPTOR_SET: &[u8] =
        tonic::include_file_descriptor_set!("heartbeat_descriptor");
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .pretty()
        .init();

    info!("Starting exploit runner service");

    let config = config::Config::new()?;

    let host = config.runner.host;
    let port = config.runner.port;

    info!("Runner service listening on {}:{}", host, port);

    let reflection_service = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(runner::FILE_DESCRIPTOR_SET)
        .register_encoded_file_descriptor_set(heartbeat::FILE_DESCRIPTOR_SET)
        .build_v1()?;

    let state = Arc::new(RwLock::new(RunnerStatus::RunnerIdle));

    // Spawn heartbeat task
    services::heartbeat::spawn_heartbeat_task(config.clone(), state.clone());

    let runner = RunnerService::try_new(config, state.clone()).await?;

    Server::builder()
        .add_service(RunnerServer::new(runner))
        .add_service(reflection_service)
        .serve(SocketAddr::from((host, port)))
        .await?;

    Ok(())
}
