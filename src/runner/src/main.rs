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
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .pretty()
        .init();

    info!("Starting exploit runner service");

    // TODO: Make this configurable
    let addr = SocketAddr::from(([0, 0, 0, 0], 50051));

    info!("Runner service listening on {}", addr);

    let reflection_service = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(runner::FILE_DESCRIPTOR_SET)
        .register_encoded_file_descriptor_set(heartbeat::FILE_DESCRIPTOR_SET)
        .build_v1()?;

    // Spawn heartbeat task
    let state = Arc::new(RwLock::new(RunnerStatus::Idle));
    services::heartbeat::spawn_heartbeat_task(state.clone());

    let runner = RunnerService::new(state.clone());

    Server::builder()
        .add_service(RunnerServer::new(runner))
        .add_service(reflection_service)
        .serve(addr)
        .await?;

    Ok(())
}
