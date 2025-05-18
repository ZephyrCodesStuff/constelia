use anyhow::Result;
use services::{
    heartbeat::{check_runners, HeartbeatService, HEARTBEAT_CHECK_INTERVAL_SECS},
    heartbeat_proto,
    scheduler::{SchedulerService, SchedulerState},
    scheduler_proto,
};
use std::{
    sync::{Arc, RwLock},
    time::Duration,
};
use tonic::transport::Server;
use tracing::{debug, info};
use tracing_subscriber::EnvFilter;

use heartbeat_proto::heartbeat_server::HeartbeatServer;
use scheduler_proto::scheduler_server::SchedulerServer;

mod services;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .pretty()
        .init();

    // Start Heartbeat gRPC server
    let addr = "0.0.0.0:50052".parse().unwrap();

    let state = SchedulerState::default();
    let state_arc = Arc::new(RwLock::new(state));

    let heartbeat_service = HeartbeatService {
        state: state_arc.clone(),
    };
    let scheduler_service = SchedulerService { state: state_arc };

    // Periodic task to check for alive runners
    let state_for_checker = heartbeat_service.state.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(HEARTBEAT_CHECK_INTERVAL_SECS)).await;
            let mut state = state_for_checker.write().unwrap();

            // Make sure that runners are still alive
            check_runners(&mut state.runners);

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
