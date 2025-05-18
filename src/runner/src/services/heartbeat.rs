use std::sync::{Arc, RwLock};

use crate::heartbeat::{heartbeat_client::HeartbeatClient, HeartbeatRequest};
use crate::runner::{RunnerInfo, RunnerStatus};

use chrono::Utc;
use tokio::time::Duration;
use tonic::Status;
use tracing::warn;

// TODO: Make these configurable
pub const SCHEDULER_ADDR: &str = "http://127.0.0.1:50052";
pub const RUNNER_ADDR: &str = "http://127.0.0.1:50051";
pub const HEARTBEAT_INTERVAL_SECS: u64 = 30;
pub const RUNNER_ID: &str = "runner-1";

pub fn spawn_heartbeat_task(status: Arc<RwLock<RunnerStatus>>) {
    tokio::spawn(async move {
        loop {
            let status = {
                let status = status.read().unwrap();
                status.clone()
            };

            match send_heartbeat(status).await {
                Ok(_) => tracing::debug!("Heartbeat sent to scheduler"),
                Err(e) => warn!("Failed to send heartbeat: {}", e),
            }

            tokio::time::sleep(Duration::from_secs(HEARTBEAT_INTERVAL_SECS)).await;
        }
    });
}

pub async fn send_heartbeat(status: RunnerStatus) -> Result<(), Status> {
    let mut client = HeartbeatClient::connect(SCHEDULER_ADDR.to_string())
        .await
        .map_err(|e| Status::internal(format!("Failed to connect to scheduler: {}", e)))?;

    client
        .send_heartbeat(HeartbeatRequest {
            runner_info: Some(RunnerInfo {
                id: RUNNER_ID.to_string(),
                addr: RUNNER_ADDR.to_string(),
                last_seen: Utc::now().to_rfc3339(),
                status: status.into(),
            }),
        })
        .await
        .map_err(|e| Status::internal(format!("Failed to send heartbeat: {}", e)))?;

    Ok(())
}
