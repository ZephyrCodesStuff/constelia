use crate::services::scheduler_proto::{
    scheduler_server::Scheduler, GetExploitsRequest, GetExploitsResponse, RunExploitRequest,
    RunExploitResponse, UploadExploitRequest, UploadExploitResponse,
};
use crate::services::submitter_proto::submitter_client::SubmitterClient;
use crate::services::submitter_proto::SubmissionRequest;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::{fs, sync::Arc};
use tonic::{Request, Response, Status};
use tracing::{error, info};
use uuid::Uuid;

use super::runner_proto::runner_client::RunnerClient;
use super::runner_proto::{RunJobRequest, Target};
use super::scheduler_proto::{GetRunnersRequest, GetRunnersResponse, Runner};

// TODO: Make this configurable
const SUBMITTER_ADDR: &str = "http://127.0.0.1:50053";

#[derive(Debug, Default)]
pub struct SchedulerService {
    pub state: Arc<RwLock<SchedulerState>>,
}

#[derive(Debug)]
pub struct SchedulerState {
    pub exploits: HashMap<String, PathBuf>,
    pub runners: HashMap<String, Runner>,
}

impl Default for SchedulerState {
    fn default() -> Self {
        // Read the static/exploits directory
        let exploit_dir = Path::new("static/exploits");
        if !exploit_dir.exists() {
            fs::create_dir_all(&exploit_dir)
                .map_err(|e| Status::internal(e.to_string()))
                .expect("Failed to create static/exploits directory");
        }

        // Read the exploits directory and get the folders inside
        let mut exploits = HashMap::new();
        for entry in fs::read_dir(exploit_dir).unwrap() {
            let Ok(entry) = entry else {
                continue;
            };

            let path = entry.path();

            if !path.is_file() {
                continue;
            }

            // Check if the file is a tar archive
            if path.extension().unwrap_or_default() != "tar" {
                continue;
            }

            let name = path.file_name().unwrap().to_str().unwrap().to_string();
            exploits.insert(name.replace(".tar", ""), path);
        }

        Self {
            exploits,
            runners: HashMap::new(),
        }
    }
}

#[tonic::async_trait]
impl Scheduler for SchedulerService {
    async fn upload_exploit(
        &self,
        request: Request<UploadExploitRequest>,
    ) -> Result<Response<UploadExploitResponse>, Status> {
        let req = request.into_inner();

        // Make sure the exploit directory exists
        let exploit_dir = Path::new("static/exploits");
        if !exploit_dir.exists() {
            fs::create_dir_all(&exploit_dir).map_err(|e| Status::internal(e.to_string()))?;
        }

        // Write the exploit bundle to the static/exploits directory
        let exploit_dir = exploit_dir.join(&req.exploit_name).with_extension("tar");
        fs::write(&exploit_dir, &req.exploit_bundle)
            .map_err(|e| Status::internal(e.to_string()))?;

        let mut state = self.state.write().unwrap();
        state.exploits.insert(req.exploit_name, exploit_dir);

        Ok(Response::new(UploadExploitResponse {
            ok: true,
            message: "Bundle received".to_string(),
        }))
    }

    async fn get_exploits(
        &self,
        _request: Request<GetExploitsRequest>,
    ) -> Result<Response<GetExploitsResponse>, Status> {
        let state = self.state.read().unwrap();
        let exploit_names = state.exploits.keys().cloned().collect();

        Ok(Response::new(GetExploitsResponse { exploit_names }))
    }

    async fn run_exploit(
        &self,
        request: Request<RunExploitRequest>,
    ) -> Result<Response<RunExploitResponse>, Status> {
        info!("Running exploit: {:?}", request);

        let req = request.into_inner();

        let exploit_path = {
            let state = self.state.read().unwrap();
            state
                .exploits
                .get(&req.exploit_name)
                .ok_or_else(|| {
                    Status::not_found(format!("Exploit {} not found", req.exploit_name))
                })?
                .clone()
        };

        let runner = {
            let state = self.state.read().unwrap();
            state
                .runners
                .values()
                .next()
                .cloned()
                .ok_or_else(|| Status::unavailable("No runners available"))?
        };

        let request = RunJobRequest {
            job_id: Uuid::new_v4().to_string(),
            target: Some(Target {
                id: "1".to_string(),
                host: "127.0.0.1".to_string(),
                port: 80,
                service: "http".to_string(),
                tags: vec![],
            }),
            exploit_name: req.exploit_name,
            flag_regex: "[A-Z0-9]{31}=".to_string(),
            exploit_bundle: fs::read(&exploit_path)
                .map_err(|e| Status::internal(format!("Failed to read exploit bundle: {}", e)))?,
        };

        let mut client = RunnerClient::connect(runner.addr.clone())
            .await
            .map_err(|e| {
                error!("Failed to connect to runner {:?}: {}", runner, e);

                // Remove the runner from the state
                let mut state = self.state.write().unwrap();
                state.runners.remove(&runner.id);

                Status::internal(e.to_string())
            })?;

        let response = client
            .run_job(Request::new(request))
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let response = response.into_inner();
        info!("Exploit run result: {:?}", response);

        // Submit the flags to the submitter
        let mut submitter = SubmitterClient::connect(SUBMITTER_ADDR.to_string())
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let submitter_response = submitter
            .submit_flags(Request::new(SubmissionRequest {
                flags: response.flags.iter().map(|f| f.value.clone()).collect(),
            }))
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        info!("Submitter response: {:?}", submitter_response);

        Ok(Response::new(RunExploitResponse {
            ok: true,
            message: "Exploit ran".to_string(),
        }))
    }

    async fn get_runners(
        &self,
        _request: Request<GetRunnersRequest>,
    ) -> Result<Response<GetRunnersResponse>, Status> {
        let state = self.state.read().unwrap();
        let runners = state.runners.values().cloned().collect();
        Ok(Response::new(GetRunnersResponse { runners }))
    }
}
