use crate::heartbeat::{heartbeat_client::HeartbeatClient, HeartbeatRequest};
use chrono::Utc;
use tokio::time::Duration;
use tracing::warn;

// TODO: Make these configurable
pub const SCHEDULER_ADDR: &str = "http://127.0.0.1:50052";
pub const RUNNER_ADDR: &str = "http://127.0.0.1:50051";
pub const HEARTBEAT_INTERVAL_SECS: u64 = 30;
pub const RUNNER_ID: &str = "runner-1";

pub fn spawn_heartbeat_task() {
    tokio::spawn(async move {
        loop {
            let mut client = match HeartbeatClient::connect(SCHEDULER_ADDR.to_string()).await {
                Ok(c) => c,
                Err(e) => {
                    warn!("Failed to connect to scheduler for heartbeat: {}", e);
                    tokio::time::sleep(Duration::from_secs(HEARTBEAT_INTERVAL_SECS)).await;
                    continue;
                }
            };
            let req = HeartbeatRequest {
                runner_id: RUNNER_ID.to_string(),
                timestamp: Utc::now().to_rfc3339(),
                runner_addr: RUNNER_ADDR.to_string(),
            };
            match client.send_heartbeat(req).await {
                Ok(_) => tracing::debug!("Heartbeat sent to scheduler"),
                Err(e) => warn!("Failed to send heartbeat: {}", e),
            }
            tokio::time::sleep(Duration::from_secs(HEARTBEAT_INTERVAL_SECS)).await;
        }
    });
}
