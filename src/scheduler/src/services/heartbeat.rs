use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use chrono::{DateTime, Utc};
use tonic::{Request, Response, Status};
use tracing::info;

use crate::services::heartbeat_proto::{
    heartbeat_server::Heartbeat, HeartbeatRequest, HeartbeatResponse,
};

use super::runner::RunnerInfo;
use super::scheduler::SchedulerState;

#[derive(Debug, Default)]
pub struct HeartbeatService {
    pub state: Arc<RwLock<SchedulerState>>,
}

#[tonic::async_trait]
impl Heartbeat for HeartbeatService {
    async fn send_heartbeat(
        &self,
        request: Request<HeartbeatRequest>,
    ) -> Result<Response<HeartbeatResponse>, Status> {
        let req = request.into_inner();

        let Some(info) = req.runner_info else {
            return Err(Status::invalid_argument("Runner info is required"));
        };

        let mut state = self.state.write().unwrap();
        state.runners.insert(
            info.id.clone(),
            RunnerInfo {
                id: info.id.clone(),
                addr: info.addr.clone(),
                last_seen: Utc::now().to_rfc3339(),
                status: info.status,
            },
        );

        info!("Runner {} sent a heartbeat: {:?}", info.id, info);

        Ok(Response::new(HeartbeatResponse {
            ok: true,
            message: "Heartbeat received".to_string(),
        }))
    }
}

pub const HEARTBEAT_TIMEOUT_SECS: i64 = 60;
pub const HEARTBEAT_CHECK_INTERVAL_SECS: u64 = 10;

pub fn check_runners(runners: &mut HashMap<String, RunnerInfo>) {
    let now = Utc::now();
    let mut to_remove = Vec::new();
    for (runner_id, runner) in runners.iter() {
        if let Ok(dt) = DateTime::parse_from_rfc3339(runner.last_seen.as_str()) {
            let dt_utc = dt.with_timezone(&Utc);
            if (now - dt_utc).num_seconds() > HEARTBEAT_TIMEOUT_SECS {
                to_remove.push(runner_id.clone());
            }
        } else {
            to_remove.push(runner_id.clone());
        }
    }
    for runner_id in to_remove {
        runners.remove(&runner_id);
        tracing::warn!("Runner {} timed out and was removed", runner_id);
    }
}
