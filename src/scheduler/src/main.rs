use anyhow::Result;
use config::Config;
use services::{
    heartbeat::{check_runners, HeartbeatService},
    heartbeat_proto,
    scheduler::{SchedulerService, SchedulerState},
    scheduler_proto,
};
use std::{
    net::SocketAddr, sync::{Arc, RwLock}, time::Duration
};
use tonic::transport::Server;
use tracing::{debug, info};
use tracing_subscriber::EnvFilter;

use heartbeat_proto::heartbeat_server::HeartbeatServer;
use scheduler_proto::scheduler_server::SchedulerServer;

mod services;
mod config;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .pretty()
        .init();

    let config = Config::new()?;

    // Start Heartbeat gRPC server
    let addr = SocketAddr::from((config.server.host, config.server.port));

    let state = SchedulerState::default();
    let state_arc = Arc::new(RwLock::new(state));

    let heartbeat_service = HeartbeatService {
        config: config.heartbeat,
        state: state_arc.clone(),
    };
    let scheduler_service = SchedulerService {
        submitter: config.submitter,
        state: state_arc,
    };

    // Periodic task to check for alive runners
    let state_for_checker = heartbeat_service.state.clone();
    let config = heartbeat_service.config.clone();

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(config.timeout)).await;
            let mut state = state_for_checker.write().unwrap();

            // Make sure that runners are still alive
            check_runners(&mut state.runners, &config);

            debug!("{} runners are alive", state.runners.len());
        }
    });

    // Add reflection service
    let reflection_service = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(heartbeat_proto::FILE_DESCRIPTOR_SET)
        .register_encoded_file_descriptor_set(scheduler_proto::FILE_DESCRIPTOR_SET)
        .build_v1()
        .unwrap();

    info!("Scheduler server started on {}", addr);

    Server::builder()
        .add_service(HeartbeatServer::new(heartbeat_service))
        .add_service(SchedulerServer::new(scheduler_service))
        .add_service(reflection_service)
        .serve(addr)
        .await?;

    Ok(())
}
