use std::sync::{Arc, RwLock};

use crate::config::Config;
use crate::heartbeat::{heartbeat_client::HeartbeatClient, HeartbeatRequest};
use crate::runner::{RunnerInfo, RunnerStatus};

use chrono::Utc;
use tokio::time::Duration;
use tonic::Status;
use tracing::{debug, info, warn};

pub fn spawn_heartbeat_task(config: Config, status: Arc<RwLock<RunnerStatus>>) {
    tokio::spawn(async move {
        loop {
            let status = {
                let status = status.read().unwrap();
                status.clone()
            };

            match send_heartbeat(&config, status).await {
                Ok(_) => debug!("Heartbeat sent to scheduler"),
                Err(e) => warn!("Failed to send heartbeat: {}", e),
            }

            tokio::time::sleep(Duration::from_secs(config.heartbeat.interval)).await;
        }
    });
}

pub async fn send_heartbeat(config: &Config, status: RunnerStatus) -> Result<(), Status> {
    let mut client = HeartbeatClient::connect(format!("http://{}:{}", config.scheduler.host, config.scheduler.port))
        .await
        .map_err(|e| Status::internal(format!("Failed to connect to scheduler: {}", e)))?;

    client
        .send_heartbeat(HeartbeatRequest {
            runner_info: Some(RunnerInfo {
                id: config.runner.id.clone(),
                addr: format!("http://{}:{}", config.runner.host, config.runner.port),
                last_seen: Utc::now().to_rfc3339(),
                status: status.into(),
            }),
        })
        .await
        .map_err(|e| Status::internal(format!("Failed to send heartbeat: {}", e)))?;

    info!("Heartbeat sent to scheduler");

    Ok(())
}
