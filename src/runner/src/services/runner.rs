use anyhow::Result;
use bollard::{
    container::{
        AttachContainerOptions, Config as ContainerConfig, CreateContainerOptions,
        RemoveContainerOptions, StartContainerOptions, WaitContainerOptions,
    },
    image::CreateImageOptions,
    secret::HostConfig,
    Docker,
};
use chrono::Utc;
use futures::StreamExt;
use std::{
    io::BufReader,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, RwLock,
    },
    path::Path,
};
use tempfile::TempDir;
use tokio::sync::{mpsc, Semaphore};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use tracing::{error, info};

use crate::{
    config::Config,
    runner::{runner_server::Runner, Flag, RunJobRequest, RunJobResponse, RunnerStatus},
    services::heartbeat,
};

#[derive(Debug)]
pub struct RunnerService {
    docker: Docker,
    semaphore: Arc<Semaphore>,
    config: Config,
    status: Arc<RwLock<RunnerStatus>>,
}

impl RunnerService {
    pub fn new(config: Config, status: Arc<RwLock<RunnerStatus>>) -> Self {
        Self {
            docker: Docker::connect_with_local_defaults().unwrap(),
            semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_JOBS)),
            config,
            status,
        }
    }
}

// TODO: Make this configurable
const MAX_CONCURRENT_JOBS: usize = 10;
const IMAGE_NAME: &str = "python";
const IMAGE_TAG: &str = "3.12-slim";

async fn run_job_internal(
    docker: &Docker,
    req: &RunJobRequest,
    exploit_folder: &Path,
) -> Result<(i32, String, String, Vec<Flag>), Status> {
    let target = req.target.as_ref().expect("Target is required");

    let image = docker.create_image(Some(CreateImageOptions::<&str> {
        from_image: IMAGE_NAME,
        tag: IMAGE_TAG,
        ..Default::default()
    }), None, None).next().await.unwrap().unwrap();

    let image_id = image.id;

    if let Some(error) = image.error {
        error!("Failed to create image: {}", error);
        return Err(Status::internal(format!("Failed to create image: {}", error)));
    }

    let env = vec![
        format!("TARGET_HOST={}", target.host),
        format!("TARGET_PORT={}", target.port),
    ];

    let container_name = format!("exploit-{}", req.job_id);
    let container_config = ContainerConfig {
        image: Some(format!("{}:{}", IMAGE_NAME, IMAGE_TAG)),
        working_dir: Some("/app".to_string()),
        entrypoint: Some(vec!["/bin/bash".to_string(), "/app/docker-entrypoint.sh".to_string()]),
        env: Some(env),
        host_config: Some(HostConfig {
            binds: Some(vec![format!("{}:/app", exploit_folder.display())]),
            ..Default::default()
        }),
        ..Default::default()
    };

    info!("Starting container with config: {:?}", container_config);

    let container = docker
        .create_container(
            Some(CreateContainerOptions {
                name: &container_name,
                platform: None,
            }),
            container_config,
        )
        .await
        .map_err(|e| Status::internal(format!("Docker create_container failed: {}", e)))?;

    docker
        .start_container(&container_name, None::<StartContainerOptions<String>>)
        .await
        .map_err(|e| Status::internal(format!("Docker start_container failed: {}", e)))?;

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let mut stream = docker
        .attach_container(
            &container_name,
            Some(AttachContainerOptions::<String> {
                stdout: Some(true),
                stderr: Some(true),
                stream: Some(true),
                ..Default::default()
            }),
        )
        .await
        .map_err(|e| Status::internal(format!("Docker attach_container failed: {}", e)))?;

    while let Some(Ok(output)) = stream.output.next().await {
        match output {
            bollard::container::LogOutput::StdOut { message } => {
                info!("[STDOUT] {}", String::from_utf8_lossy(&message));
                stdout.extend_from_slice(&message);
            }
            bollard::container::LogOutput::StdErr { message } => {
                error!("[STDERR] {}", String::from_utf8_lossy(&message));
                stderr.extend_from_slice(&message);
            }
            _ => {}
        }
    }

    let exit_code = docker
        .wait_container(
            &container_name,
            Some(WaitContainerOptions {
                condition: "not-running",
            }),
        )
        .next()
        .await
        .transpose()
        .map_err(|e| Status::internal(format!("Docker wait_container failed: {}", e)))?
        .map(|status| status.status_code as i32)
        .unwrap_or(-1);

    docker
        .remove_container(
            &container_name,
            Some(RemoveContainerOptions {
                force: true,
                ..Default::default()
            }),
        )
        .await
        .map_err(|e| Status::internal(format!("Docker remove_container failed: {}", e)))?;

    let flags = parse_flags(&stdout, &stderr, req);

    Ok((exit_code, String::from_utf8_lossy(&stdout).to_string(), String::from_utf8_lossy(&stderr).to_string(), flags))
}

#[tonic::async_trait]
impl Runner for RunnerService {
    async fn run_job(
        &self,
        request: Request<RunJobRequest>,
    ) -> Result<Response<RunJobResponse>, Status> {
        // Acquire a permit from the semaphore
        let _permit = self.semaphore.acquire().await;

        // Set the status to running
        let status = {
            let mut status = self.status.write().unwrap();
            *status = RunnerStatus::Running;
            status.clone()
        };

        // Send a heartbeat to the scheduler
        heartbeat::send_heartbeat(&self.config, status)
            .await
            .unwrap();

        let req = request.into_inner();
        info!("Received job request: {}", req.job_id);

        let tempdir = TempDir::new()
            .map_err(|e| Status::internal(format!("Failed to create tempdir: {}", e)))?;
        let exploit_folder = tempdir.path();

        if req.exploit_name.is_empty() {
            return Err(Status::invalid_argument("Exploit name is required"));
        }

        if req.exploit_bundle.is_empty() {
            return Err(Status::invalid_argument("Exploit bundle is required"));
        }

        let reader = BufReader::new(&req.exploit_bundle[..]);
        let mut archive = tar::Archive::new(reader);
        archive
            .unpack(&exploit_folder)
            .map_err(|e| Status::internal(e.to_string()))?;

        let (exit_code, stdout, stderr, flags) = run_job_internal(&self.docker, &req, exploit_folder).await?;

        // Set the status to completed or failed
        let status = {
            let mut status = self.status.write().unwrap();
            *status = if exit_code == 0 {
                RunnerStatus::Completed
            } else {
                RunnerStatus::Failed
            };

            status.clone()
        };

        // Send a heartbeat to the scheduler
        heartbeat::send_heartbeat(&self.config, status)
            .await
            .unwrap();

        let response = RunJobResponse {
            job_id: req.job_id,
            exit_code: exit_code as i32,
            stdout,
            stderr,
            flags: flags.into_iter().collect(),
        };

        Ok(Response::new(response))
    }

    type StreamJobsStream = ReceiverStream<Result<RunJobResponse, Status>>;

    async fn stream_jobs(
        &self,
        request: Request<tonic::Streaming<RunJobRequest>>,
    ) -> Result<Response<Self::StreamJobsStream>, Status> {
        let semaphore = self.semaphore.clone();
        let status_lock = self.status.clone();
        let config = self.config.clone();

        let status = {
            let mut status = status_lock.write().unwrap();
            *status = RunnerStatus::Running;
            status.clone()
        };

        // Send a heartbeat to the scheduler
        heartbeat::send_heartbeat(&self.config, status)
            .await
            .unwrap();

        let mut stream = request.into_inner();
        let (tx, rx) = mpsc::channel(4);

        let docker = self.docker.clone();

        // Keep track of running jobs
        let running_jobs = Arc::new(AtomicUsize::new(0));

        tokio::spawn(async move {
            while let Some(req) = stream.next().await {
                // Clone these because they don't implement `Copy`
                let semaphore = semaphore.clone();
                let tx = tx.clone();
                let docker = docker.clone();
                let running_jobs = running_jobs.clone();
                let status_lock = status_lock.clone();
                let config = config.clone();

                running_jobs.fetch_add(1, Ordering::SeqCst);

                tokio::spawn(async move {
                    let _permit = semaphore.acquire().await;

                    match req {
                        Ok(req) => {
                            let tempdir = match TempDir::new() {
                                Ok(t) => t,
                                Err(e) => {
                                    error!("Failed to create tempdir: {}", e);
                                    running_jobs.fetch_sub(1, Ordering::SeqCst);
                                    return;
                                }
                            };
                            let exploit_folder = tempdir.path();

                            if req.exploit_name.is_empty() {
                                error!("Exploit name is required");
                                running_jobs.fetch_sub(1, Ordering::SeqCst);
                                return;
                            }

                            if req.exploit_bundle.is_empty() {
                                error!("Exploit bundle is required");
                                running_jobs.fetch_sub(1, Ordering::SeqCst);
                                return;
                            }

                            let reader = BufReader::new(&req.exploit_bundle[..]);
                            let mut archive = tar::Archive::new(reader);
                            if let Err(e) = archive.unpack(&exploit_folder) {
                                error!("Failed to unpack exploit bundle: {}", e);
                                running_jobs.fetch_sub(1, Ordering::SeqCst);
                                return;
                            }

                            match run_job_internal(&docker, &req, exploit_folder).await {
                                Ok((exit_code, stdout, stderr, flags)) => {
                                    let response = RunJobResponse {
                                        job_id: req.job_id,
                                        exit_code,
                                        stdout,
                                        stderr,
                                        flags: flags.into_iter().collect(),
                                    };

                                    if tx.send(Ok(response)).await.is_err() {
                                        running_jobs.fetch_sub(1, Ordering::SeqCst);
                                        return;
                                    }
                                }
                                Err(e) => {
                                    error!("Job failed: {}", e);
                                    if tx.send(Err(Status::internal(e.to_string()))).await.is_err() {
                                        running_jobs.fetch_sub(1, Ordering::SeqCst);
                                        return;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!("Stream error: {}", e);
                            running_jobs.fetch_sub(1, Ordering::SeqCst);
                            return;
                        }
                    }

                    let remaining = running_jobs.fetch_sub(1, Ordering::SeqCst) - 1;
                    if remaining == 0 {
                        let status = {
                            let mut status = status_lock.write().unwrap();
                            *status = RunnerStatus::Idle;
                            status.clone()
                        };
                        if let Err(e) = heartbeat::send_heartbeat(&config, status).await {
                            error!("Failed to send idle status heartbeat: {}", e);
                        }
                    }
                });
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

fn parse_flags(stdout: &[u8], stderr: &[u8], job: &RunJobRequest) -> Vec<Flag> {
    let mut flags = Vec::new();
    let flag_pattern = regex::Regex::new(&job.flag_regex).unwrap();

    if let Ok(stdout_str) = String::from_utf8(stdout.to_vec()) {
        for cap in flag_pattern.captures_iter(&stdout_str) {
            flags.push(Flag {
                value: cap[0].to_string(),
                target_id: job.target.as_ref().unwrap().id.clone(),
                exploit_name: job.exploit_name.clone(),
                timestamp: Utc::now().to_rfc3339(),
            });
        }
    }

    if let Ok(stderr_str) = String::from_utf8(stderr.to_vec()) {
        for cap in flag_pattern.captures_iter(&stderr_str) {
            flags.push(Flag {
                value: cap[0].to_string(),
                target_id: job.target.as_ref().unwrap().id.clone(),
                exploit_name: job.exploit_name.clone(),
                timestamp: Utc::now().to_rfc3339(),
            });
        }
    }

    flags
}
