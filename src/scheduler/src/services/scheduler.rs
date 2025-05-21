use crate::config::SubmitterConfig;
use crate::config::{Config, Target};
use crate::services::scheduler_proto::{
    scheduler_server::Scheduler, GetExploitsRequest, GetExploitsResponse, RunExploitRequest,
    RunExploitResponse, UploadExploitRequest, UploadExploitResponse,
};

use anyhow::Error;
use async_nats::jetstream::consumer::AckPolicy;
use async_nats::{
    jetstream, jetstream::consumer::pull::Config as JetStreamPullConfig,
    jetstream::Context as JetStreamContext,
};
use prost::Message;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::Duration;
use std::{fs, sync::Arc};
use tokio_stream::StreamExt;
use tonic::{Request, Response, Status};
use tracing::{error, info};
use uuid::Uuid;

use super::runner::runner_client::RunnerClient;
use super::runner::{Job, PullJobsRequest, RunnerInfo, Status as JobStatus};
use super::scheduler_proto::{
    GetJobResultRequest, GetJobResultResponse, GetJobsRequest, GetJobsResponse, GetRunnersRequest,
    GetRunnersResponse, GetTargetsRequest, GetTargetsResponse, PollRunnersRequest,
    PollRunnersResponse,
};

#[derive(Debug, Clone)]
pub struct SchedulerService {
    pub submitter: SubmitterConfig,
    pub state: Arc<RwLock<SchedulerState>>,
}

#[derive(Debug)]
pub struct SchedulerState {
    pub exploits: HashMap<String, PathBuf>,
    pub runners: HashMap<String, RunnerInfo>,
    pub targets: Vec<Target>,
    pub jetstream: JetStreamContext,
}

impl SchedulerState {
    pub async fn try_new(config: &Config) -> Result<Self, Error> {
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

        let nats = async_nats::connect(config.nats.url.clone()).await?;
        let jetstream = jetstream::new(nats.clone());

        // Ensure the stream exists (idempotent)
        let _ = jetstream
            .create_stream(async_nats::jetstream::stream::Config {
                name: "jobs".to_string(),
                subjects: vec!["jobs".to_string()],
                ..Default::default()
            })
            .await
            .map_err(|e| Status::internal(format!("Failed to create JetStream stream: {}", e)))?;

        // Create a `runner-shared` consumer for the `jobs` stream
        let _ = jetstream
            .create_consumer_on_stream(
                JetStreamPullConfig {
                    durable_name: Some("runner-shared".to_string()),
                    ack_policy: AckPolicy::Explicit,
                    ack_wait: Duration::from_secs(10),
                    ..Default::default()
                },
                "jobs",
            )
            .await?;

        Ok(Self {
            exploits,
            runners: HashMap::new(),
            targets: config.targets.clone(),
            jetstream,
        })
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

    // TODO: rename to `enqueue_job`
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
        let targets = {
            let state = self.state.read().unwrap();
            state.targets.clone()
        };
        let jetstream = {
            let state = self.state.read().unwrap();
            state.jetstream.clone()
        };

        let exploit_path = exploit_path
            .ok_or_else(|| Status::not_found(format!("Exploit {} not found", req.exploit_name)))?;

        let Some(target) = targets
            .iter()
            .find(|t| t.id == Some(req.target.clone()) || t.host.to_string() == req.target)
        else {
            return Err(Status::not_found(format!(
                "Target {} not found",
                req.target
            )));
        };
        let request = Job {
            job_id: Uuid::new_v4().to_string(),
            target: Some(target.to_owned().into()),
            exploit_name: req.exploit_name,
            flag_regex: self.submitter.flag_regex.clone(),
            exploit_bundle: fs::read(&exploit_path)
                .map_err(|e| Status::internal(format!("Failed to read exploit bundle: {}", e)))?,
            status: JobStatus::JobPending.into(),
        };
        // Publish job to JetStream
        let payload = request.encode_to_vec();
        jetstream
            .publish("jobs", payload.into())
            .await
            .map_err(|e| Status::internal(format!("Failed to publish job to JetStream: {}", e)))?;
        Ok(Response::new(RunExploitResponse {
            ok: true,
            message: request.job_id.clone(),
        }))
    }

    async fn get_targets(
        &self,
        _request: Request<GetTargetsRequest>,
    ) -> Result<Response<GetTargetsResponse>, Status> {
        let state = self.state.read().unwrap();
        let targets = state.targets.clone();
        Ok(Response::new(GetTargetsResponse {
            targets: targets.into_iter().map(|t| t.into()).collect(),
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

    async fn get_job_result(
        &self,
        request: Request<GetJobResultRequest>,
    ) -> Result<Response<GetJobResultResponse>, Status> {
        // TODO: Implement this

        // Job result not found
        Ok(Response::new(GetJobResultResponse {
            finished: false,
            result: None,
        }))
    }

    async fn get_jobs(
        &self,
        _request: Request<GetJobsRequest>,
    ) -> Result<Response<GetJobsResponse>, Status> {
        let jetstream = {
            let state = self.state.read().unwrap();
            state.jetstream.clone()
        };
        // Now drop the lock before any .await

        let consumer = jetstream
            .create_consumer_on_stream(
                JetStreamPullConfig {
                    ack_policy: AckPolicy::None,
                    ..Default::default()
                },
                "jobs",
            )
            .await
            .map_err(|e| Status::internal(format!("Failed to get JetStream consumer: {}", e)))?;

        // Fetch messages (peek, do not ack)
        let mut messages = consumer
            .fetch()
            .messages()
            .await
            .map_err(|e| Status::internal(format!("Failed to fetch jobs: {}", e)))?;

        let mut jobs = Vec::new();

        while let Some(Ok(msg)) = messages.next().await {
            if let Ok(job) = Job::decode(&*msg.payload) {
                jobs.push(job);
            }
        }
        Ok(Response::new(GetJobsResponse { jobs }))
    }

    async fn poll_runners(
        &self,
        _request: Request<PollRunnersRequest>,
    ) -> Result<Response<PollRunnersResponse>, Status> {
        let runners = {
            let state = self.state.read().unwrap();
            state.runners.values().cloned().collect::<Vec<_>>()
        };
        // Now drop the lock before any .await

        let mut handles = Vec::new();

        for runner in &runners {
            let runner = runner.clone();
            let handle = tokio::spawn(async move {
                let request = PullJobsRequest {};
                let mut client = match RunnerClient::connect(runner.addr.clone()).await {
                    Ok(client) => client,
                    Err(e) => {
                        error!("Failed to connect to runner {}: {}", runner.id, e);
                        return;
                    }
                };

                match client.pull_jobs(request).await {
                    Ok(response) => {
                        let response = response.into_inner();
                        for job in response.jobs {
                            info!("Runner {} pulled job: {:?}", runner.id, job);
                        }
                    }
                    Err(e) => {
                        error!("Failed to pull jobs from runner {}: {}", runner.id, e);
                    }
                }
            });
            handles.push(handle);
        }
        // Wait for all pulls to complete
        futures::future::join_all(handles).await;

        Ok(Response::new(PollRunnersResponse {
            runners: runners.into_iter().map(|r| r.into()).collect(),
        }))
    }
}
