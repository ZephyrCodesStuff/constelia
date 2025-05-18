use anyhow::Result;
use bollard::{
    container::{
        AttachContainerOptions, Config, CreateContainerOptions, RemoveContainerOptions,
        StartContainerOptions, WaitContainerOptions,
    },
    secret::HostConfig,
    Docker,
};
use chrono::Utc;
use futures::StreamExt;
use std::{
    io::{BufReader, Write},
    sync::Arc,
};
use tempfile::TempDir;
use tokio::sync::{mpsc, Semaphore};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use tracing::{error, info};

use crate::runner::{runner_server::Runner, Flag, RunJobRequest, RunJobResponse};

#[derive(Debug)]
pub struct RunnerService {
    docker: Docker,
    semaphore: Arc<Semaphore>,
}

// TODO: Make this configurable
const MAX_CONCURRENT_JOBS: usize = 10;

impl Default for RunnerService {
    fn default() -> Self {
        Self {
            docker: Docker::connect_with_local_defaults().unwrap(),
            semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_JOBS)),
        }
    }
}

#[tonic::async_trait]
impl Runner for RunnerService {
    async fn run_job(
        &self,
        request: Request<RunJobRequest>,
    ) -> Result<Response<RunJobResponse>, Status> {
        // Acquire a permit from the semaphore
        let _permit = self.semaphore.acquire().await;

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

        let target = req.target.as_ref().expect("Target is required");

        let env = vec![
            format!("TARGET_HOST={}", target.host),
            format!("TARGET_PORT={}", target.port),
        ];

        info!("Exploit folder (temp): {}", exploit_folder.display());

        let config = Config::<&str> {
            image: Some("python:3.9-slim"),
            working_dir: Some("/app"),
            entrypoint: Some(vec!["/bin/bash", "/app/docker-entrypoint.sh"]),
            env: Some(env.iter().map(|s| s.as_str()).collect()),
            host_config: Some(HostConfig {
                binds: Some(vec![format!("{}:/app:ro", exploit_folder.display())]),
                ..Default::default()
            }),
            ..Default::default()
        };

        let container_name = format!("hzrd-{}", req.job_id);

        let options = Some(CreateContainerOptions {
            name: &container_name,
            platform: None, // Inherit from host
        });

        let id = self
            .docker
            .create_container(options, config)
            .await
            .map_err(|e| Status::internal(format!("Docker create_container failed: {}", e)))?;
        self.docker
            .start_container(&id.id, None::<StartContainerOptions<String>>)
            .await
            .map_err(|e| Status::internal(format!("Docker start_container failed: {}", e)))?;

        let mut attach = self
            .docker
            .attach_container(
                &id.id,
                Some(AttachContainerOptions::<String> {
                    stream: Some(true),
                    stdout: Some(true),
                    stderr: Some(true),
                    ..Default::default()
                }),
            )
            .await
            .map_err(|e| Status::internal(format!("Docker attach_container failed: {}", e)))?;

        let output_task = tokio::spawn(async move {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();

            while let Some(Ok(line)) = attach.output.next().await {
                match line {
                    bollard::container::LogOutput::StdOut { message } => {
                        stdout.write_all(&message).unwrap();
                        info!("[+] {}", String::from_utf8_lossy(&message));
                    }
                    bollard::container::LogOutput::StdErr { message } => {
                        stderr.write_all(&message).unwrap();
                        error!("[!] {}", String::from_utf8_lossy(&message));
                    }
                    _ => {}
                }
            }

            (stdout, stderr)
        });

        let wait_result = self
            .docker
            .wait_container(
                &id.id,
                Some(WaitContainerOptions {
                    condition: "not-running",
                }),
            )
            .next()
            .await
            .transpose()
            .map_err(|e| Status::internal(format!("Docker wait_container failed: {}", e)))?;

        let exit_code = wait_result
            .and_then(|status| Some(status.status_code))
            .unwrap_or(-1);

        let (stdout, stderr) = output_task
            .await
            .map_err(|e| Status::internal(format!("Join error: {}", e)))?;

        let flags = parse_flags(&stdout, &stderr, &req.clone());

        info!("Container exited with code {}", exit_code);

        self.docker
            .remove_container(
                &id.id,
                Some(RemoveContainerOptions {
                    v: true,
                    force: true,
                    link: false,
                }),
            )
            .await
            .map_err(|e| Status::internal(format!("Docker remove_container failed: {}", e)))?;

        let response = RunJobResponse {
            job_id: req.job_id,
            exit_code: exit_code as i32,
            stdout: String::from_utf8_lossy(&stdout).to_string(),
            stderr: String::from_utf8_lossy(&stderr).to_string(),
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

        let mut stream = request.into_inner();
        let (tx, rx) = mpsc::channel(4);

        let docker = self.docker.clone();
        tokio::spawn(async move {
            while let Some(req) = stream.next().await {
                let semaphore = semaphore.clone();
                let tx = tx.clone();
                let docker = docker.clone();

                tokio::spawn(async move {
                    let _permit = semaphore.acquire().await;

                    match req {
                        Ok(req) => match run_job(&docker, &req).await {
                            Ok((exit_code, stdout, stderr, flags)) => {
                                let response = RunJobResponse {
                                    job_id: req.job_id,
                                    exit_code,
                                    stdout,
                                    stderr,
                                    flags: flags.into_iter().collect(),
                                };
                                if tx.send(Ok(response)).await.is_err() {
                                    return;
                                }
                            }
                            Err(e) => {
                                error!("Job failed: {}", e);
                                if tx.send(Err(Status::internal(e.to_string()))).await.is_err() {
                                    return;
                                }
                            }
                        },
                        Err(e) => {
                            error!("Stream error: {}", e);
                            return;
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

async fn run_job(docker: &Docker, job: &RunJobRequest) -> Result<(i32, String, String, Vec<Flag>)> {
    info!(
        "Running job {} for target {}",
        job.job_id,
        job.target.as_ref().unwrap().id
    );

    let Some(target) = job.target.clone() else {
        return Err(anyhow::anyhow!("Target is required"));
    };

    let env = vec![
        format!("TARGET_HOST={}", target.host),
        format!("TARGET_PORT={}", target.port),
    ];

    // Create a tempdir for the exploit
    let tempdir =
        TempDir::new().map_err(|e| Status::internal(format!("Failed to create tempdir: {}", e)))?;
    let exploit_folder = tempdir.path();

    // Unpack the exploit bundle into the tempdir
    let reader = BufReader::new(&job.exploit_bundle[..]);
    let mut archive = tar::Archive::new(reader);
    archive
        .unpack(&exploit_folder)
        .map_err(|e| Status::internal(e.to_string()))?;

    let config = Config::<&str> {
        image: Some("python:3.9-slim"),
        working_dir: Some("/app"),
        entrypoint: Some(vec!["/bin/bash", "/app/docker-entrypoint.sh"]),
        env: Some(env.iter().map(|s| s.as_str()).collect()),
        host_config: Some(HostConfig {
            binds: Some(vec![format!("{}:/app:ro", exploit_folder.display())]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let container_name = format!("hzrd-{}", job.job_id);

    let options = Some(CreateContainerOptions {
        name: &container_name,
        platform: None, // Inherit from host
    });

    let id = docker.create_container(options, config).await?;
    docker
        .start_container(&id.id, None::<StartContainerOptions<String>>)
        .await?;

    let mut attach = docker
        .attach_container(
            &id.id,
            Some(AttachContainerOptions::<String> {
                stream: Some(true),
                stdout: Some(true),
                stderr: Some(true),
                ..Default::default()
            }),
        )
        .await?;

    let output_task = tokio::spawn(async move {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        while let Some(Ok(line)) = attach.output.next().await {
            match line {
                bollard::container::LogOutput::StdOut { message } => {
                    stdout.write_all(&message).unwrap();
                    info!("[+] {}", String::from_utf8_lossy(&message));
                }
                bollard::container::LogOutput::StdErr { message } => {
                    stderr.write_all(&message).unwrap();
                    error!("[!] {}", String::from_utf8_lossy(&message));
                }
                _ => {}
            }
        }

        (stdout, stderr)
    });

    let wait_result = docker
        .wait_container(
            &id.id,
            Some(WaitContainerOptions {
                condition: "not-running",
            }),
        )
        .next()
        .await
        .transpose()?;

    let exit_code = wait_result
        .and_then(|status| Some(status.status_code))
        .unwrap_or(-1);

    let (stdout, stderr) = output_task.await.unwrap();

    let flags = parse_flags(&stdout, &stderr, &job);

    info!("Container exited with code {}", exit_code);

    docker
        .remove_container(
            &id.id,
            Some(RemoveContainerOptions {
                v: true,
                force: true,
                link: false,
            }),
        )
        .await?;

    Ok((
        exit_code.try_into().unwrap(),
        String::from_utf8_lossy(&stdout).to_string(),
        String::from_utf8_lossy(&stderr).to_string(),
        flags,
    ))
}
