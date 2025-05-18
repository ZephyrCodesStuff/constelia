use crate::config::{Config, Target};
use crate::{config::SubmitterConfig};
use crate::services::scheduler_proto::{
    scheduler_server::Scheduler, GetExploitsRequest, GetExploitsResponse, RunExploitRequest,
    RunExploitResponse, UploadExploitRequest, UploadExploitResponse,
};

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::{fs, sync::Arc};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tonic::{Request, Response, Status};
use tracing::{error, info, warn};
use uuid::Uuid;

use super::runner::{RunJobRequest, RunJobResponse, RunnerInfo};
use super::scheduler_proto::{
    GetJobResultRequest, GetJobResultResponse, GetJobsRequest, GetJobsResponse, GetRunnersRequest,
    GetRunnersResponse, GetTargetsRequest, GetTargetsResponse,
};

#[derive(Debug, Default, Clone)]
pub struct SchedulerService {
    pub submitter: SubmitterConfig,
    pub state: Arc<RwLock<SchedulerState>>,
}

#[derive(Debug)]
pub struct SchedulerState {
    pub exploits: HashMap<String, PathBuf>,
    pub runners: HashMap<String, RunnerInfo>,
    pub dispatchers: HashMap<String, mpsc::Sender<RunJobRequest>>, // runner_id -> job sender
    pub job_results: HashMap<String, Option<RunJobResponse>>, // job_id -> result (None if not finished)
    pub targets: Vec<Target>,
}

impl SchedulerState {
    pub fn new(config: &Config) -> Self {
        let targets = config.targets.clone();
        Self {
            targets,
            ..Default::default()
        }
    }
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
            dispatchers: HashMap::new(),
            job_results: HashMap::new(),
            targets: vec![],
        }
    }
}

impl SchedulerService {
    // Call this when a new runner is registered (e.g., in heartbeat or at startup)
    pub async fn ensure_dispatcher(&self, runner: &RunnerInfo) {
        let mut state = self.state.write().unwrap();
        
        // Clean up old dispatcher if it exists
        if state.dispatchers.contains_key(&runner.id) {
            info!("Runner {} reconnected, cleaning up old dispatcher", runner.id);
            state.dispatchers.remove(&runner.id);
        }

        // Create new dispatcher
        let (tx, rx) = mpsc::channel::<RunJobRequest>(100);
        state.dispatchers.insert(runner.id.clone(), tx);
        let runner_addr = runner.addr.clone();
        let state = self.state.clone();
        let submitter = self.submitter.clone();

        tokio::spawn(async move {
            run_streaming_dispatcher(state, submitter, runner_addr, rx).await;
        });
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
            state.exploits.get(&req.exploit_name).cloned()
        };
        let exploit_path = exploit_path
            .ok_or_else(|| Status::not_found(format!("Exploit {} not found", req.exploit_name)))?;

        let runner = {
            let state = self.state.read().unwrap();
            state
                .runners
                .values()
                .next()
                .cloned()
                .ok_or_else(|| Status::unavailable("No runners available"))?
        };

        // Ensure dispatcher is running for this runner
        self.ensure_dispatcher(&runner).await;

        let job_id = Uuid::new_v4().to_string();

        // `req.target` is either the target ID or the host
        // We can try to find it from our config
        let Some(target) = ({
            let state = self.state.read().unwrap();
            state.targets.iter().find(|t| t.id == Some(req.target.clone()) || t.host.to_string() == req.target).cloned()
        }) else {
            return Err(Status::not_found(format!("Target {} not found", req.target)));
        };

        let request = RunJobRequest {
            job_id: job_id.clone(),
            target: Some(target.into()),
            exploit_name: req.exploit_name,
            flag_regex: self.submitter.flag_regex.clone(),
            exploit_bundle: fs::read(&exploit_path)
                .map_err(|e| Status::internal(format!("Failed to read exploit bundle: {}", e)))?,
        };

        // Register job as pending
        {
            let mut state = self.state.write().unwrap();
            state.job_results.insert(job_id.clone(), None);
        }

        // Enqueue the job for the runner's dispatcher
        let dispatchers = {
            let state = self.state.read().unwrap();
            state
                .dispatchers
                .get(&runner.id)
                .ok_or_else(|| Status::unavailable("No dispatcher for runner"))?
                .clone()
        };

        dispatchers
            .send(request)
            .await
            .map_err(|e| Status::internal(format!("Failed to send job to runner: {}", e)))?;

        Ok(Response::new(RunExploitResponse {
            ok: true,
            message: job_id, // Return job_id as message
        }))
    }

    async fn get_targets(
        &self,
        _request: Request<GetTargetsRequest>,
    ) -> Result<Response<GetTargetsResponse>, Status> {
        let state = self.state.read().unwrap();
        let targets = state.targets.clone();
        Ok(Response::new(GetTargetsResponse { targets: targets.into_iter().map(|t| t.into()).collect() }))
    }

    async fn get_runners(
        &self,
        _request: Request<GetRunnersRequest>,
    ) -> Result<Response<GetRunnersResponse>, Status> {
        let state = self.state.read().unwrap();
        let runners = state.runners.values().cloned().collect();
        Ok(Response::new(GetRunnersResponse { runners }))
    }

    async fn get_job_result(
        &self,
        request: Request<GetJobResultRequest>,
    ) -> Result<Response<GetJobResultResponse>, Status> {
        let state = self.state.read().unwrap();
        let job_id = request.into_inner().job_id;

        let result = state.job_results.get(&job_id).cloned();
        Ok(Response::new(GetJobResultResponse {
            finished: result.is_some(),
            result: result.unwrap_or_default(),
        }))
    }

    async fn get_jobs(
        &self,
        _request: Request<GetJobsRequest>,
    ) -> Result<Response<GetJobsResponse>, Status> {
        let state = self.state.read().unwrap();
        let jobs = state
            .job_results
            .values()
            .filter_map(|r| r.clone())
            .collect();
        Ok(Response::new(GetJobsResponse { jobs }))
    }
}

// Dispatcher task for a runner
async fn run_streaming_dispatcher(
    scheduler_service: Arc<RwLock<SchedulerState>>,
    submitter: SubmitterConfig,
    runner_addr: String,
    mut rx: mpsc::Receiver<RunJobRequest>,
) {
    use super::runner::runner_client::RunnerClient;
    use super::submitter_proto::submitter_client::SubmitterClient;
    use super::submitter_proto::SubmissionRequest;
    use tonic::Request;
    use tracing::{error, info, warn};

    let mut client = match RunnerClient::connect(runner_addr.clone()).await {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to connect to runner {}: {}", runner_addr, e);
            return;
        }
    };

    let (job_tx, job_rx) = mpsc::channel::<RunJobRequest>(100);
    let outbound = ReceiverStream::new(job_rx);

    // Forward jobs from rx to job_tx
    tokio::spawn(async move {
        while let Some(job) = rx.recv().await {
            if job_tx.send(job).await.is_err() {
                warn!("Failed to forward job to runner, channel closed");
                break;
            }
        }
    });

    let response_stream = match client.stream_jobs(Request::new(outbound)).await {
        Ok(r) => r.into_inner(),
        Err(e) => {
            error!("Failed to start stream_jobs: {}", e);
            return;
        }
    };
    tokio::pin!(response_stream);

    while let Some(result) = response_stream.next().await {
        match result {
            Ok(response) => {
                info!("Received job result: {:?}", response);
                // Store the result in job_results
                {
                    let mut state = scheduler_service.write().unwrap();
                    state.job_results.insert(
                        response.job_id.clone(),
                        Some(RunJobResponse {
                            job_id: response.job_id.clone(),
                            exit_code: response.exit_code,
                            stdout: response.stdout,
                            stderr: response.stderr,
                            flags: response.flags.clone(),
                        }),
                    );
                }

                if !submitter.enabled {
                    continue;
                }

                // Submit the flags to the submitter
                if !response.flags.is_empty() {
                    let addr = SocketAddr::new(submitter.host.into(), submitter.port);
                    match SubmitterClient::connect(addr.to_string()).await {
                        Ok(mut submitter) => {
                            if let Err(e) = submitter
                                .submit_flags(Request::new(SubmissionRequest {
                                    flags: response.flags.iter().map(|f| f.value.clone()).collect(),
                                }))
                                .await
                            {
                                error!("Failed to submit flags: {}", e);
                            }
                        }
                        Err(e) => error!("Failed to connect to submitter: {}", e),
                    }
                }
            }
            Err(e) => {
                error!("Error receiving job result: {}", e);
                break;
            }
        }
    }

    // Clean up the dispatcher when the stream ends
    {
        let mut state = scheduler_service.write().unwrap();
        state.dispatchers.remove(&runner_addr);
        info!("Dispatcher for runner {} cleaned up", runner_addr);
    }
}
