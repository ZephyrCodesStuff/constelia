use anyhow::Result;
use bollard::container::{
    AttachContainerOptions, Config, CreateContainerOptions, RemoveContainerOptions,
    StartContainerOptions, WaitContainerOptions,
};
use bollard::secret::HostConfig;
use bollard::Docker;
use chrono::Utc;
use common::{Job, JobStatus};
use futures::StreamExt;
use std::io::Write;
use std::net::SocketAddr;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{transport::Server, Request, Response, Status};
use tracing::{error, info, Level};

pub mod runner {
    tonic::include_proto!("runner");
    pub const FILE_DESCRIPTOR_SET: &[u8] = tonic::include_file_descriptor_set!("runner_descriptor");
}

use runner::{
    runner_server::{Runner, RunnerServer},
    Flag, RunJobRequest, RunJobResponse,
};

#[derive(Debug)]
pub struct RunnerService {
    docker: Docker,
}

#[tonic::async_trait]
impl Runner for RunnerService {
    async fn run_job(
        &self,
        request: Request<RunJobRequest>,
    ) -> Result<Response<RunJobResponse>, Status> {
        let req = request.into_inner();
        info!("Received job request: {}", req.job_id);

        let target = req.target.expect("Target is required");

        let job = Job {
            id: req.job_id,
            target: common::Target {
                id: target.id,
                host: target.host,
                port: target.port as u16,
                service: target.service,
                tags: target.tags,
            },
            exploit_name: req.exploit_name,
            status: JobStatus::Pending,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            result: None,
            flag_regex: req.flag_regex,
        };

        match run_job(&self.docker, &job).await {
            Ok((exit_code, stdout, stderr, flags)) => {
                let response = RunJobResponse {
                    job_id: job.id,
                    exit_code,
                    stdout,
                    stderr,
                    flags: flags
                        .into_iter()
                        .map(|f| Flag {
                            value: f.value,
                            target_id: f.target_id,
                            exploit_name: f.exploit_name,
                            timestamp: f.timestamp.to_rfc3339(),
                        })
                        .collect(),
                };
                Ok(Response::new(response))
            }
            Err(e) => {
                error!("Job failed: {}", e);
                Err(Status::internal(e.to_string()))
            }
        }
    }

    type StreamJobsStream = ReceiverStream<Result<RunJobResponse, Status>>;

    async fn stream_jobs(
        &self,
        request: Request<tonic::Streaming<RunJobRequest>>,
    ) -> Result<Response<Self::StreamJobsStream>, Status> {
        let mut stream = request.into_inner();
        let (tx, rx) = mpsc::channel(4);

        let docker = self.docker.clone();
        tokio::spawn(async move {
            while let Some(req) = stream.next().await {
                match req {
                    Ok(req) => {
                        let target = req.target.expect("Target is required");

                        let job = Job {
                            id: req.job_id,
                            target: common::Target {
                                id: target.id,
                                host: target.host,
                                port: target.port as u16,
                                service: target.service,
                                tags: target.tags,
                            },
                            exploit_name: req.exploit_name,
                            status: JobStatus::Pending,
                            created_at: Utc::now(),
                            updated_at: Utc::now(),
                            result: None,
                            flag_regex: req.flag_regex,
                        };

                        match run_job(&docker, &job).await {
                            Ok((exit_code, stdout, stderr, flags)) => {
                                let response = RunJobResponse {
                                    job_id: job.id,
                                    exit_code,
                                    stdout,
                                    stderr,
                                    flags: flags
                                        .into_iter()
                                        .map(|f| Flag {
                                            value: f.value,
                                            target_id: f.target_id,
                                            exploit_name: f.exploit_name,
                                            timestamp: f.timestamp.to_rfc3339(),
                                        })
                                        .collect(),
                                };
                                if tx.send(Ok(response)).await.is_err() {
                                    break;
                                }
                            }
                            Err(e) => {
                                error!("Job failed: {}", e);
                                if tx.send(Err(Status::internal(e.to_string()))).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Stream error: {}", e);
                        break;
                    }
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("Starting exploit runner service");

    let addr = SocketAddr::from(([0, 0, 0, 0], 50051));
    let docker = Docker::connect_with_local_defaults()?;
    let runner = RunnerService { docker };

    info!("Runner service listening on {}", addr);

    let reflection_service = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(runner::FILE_DESCRIPTOR_SET)
        .build_v1()?;

    Server::builder()
        .add_service(RunnerServer::new(runner))
        .add_service(reflection_service)
        .serve(addr)
        .await?;

    Ok(())
}

async fn run_job(docker: &Docker, job: &Job) -> Result<(i32, String, String, Vec<common::Flag>)> {
    info!("Running job {} for target {}", job.id, job.target.id);

    // Environment variables
    let env = vec![
        format!("TARGET_HOST={}", job.target.host),
        format!("TARGET_PORT={}", job.target.port),
    ];

    // Create container config
    let current_dir = std::env::current_dir().unwrap();
    let exploit_folder = current_dir.join("exploits").join(&job.exploit_name);

    info!("Exploit folder: {}", exploit_folder.display());

    if !exploit_folder.exists() {
        return Err(anyhow::anyhow!(
            "Exploit folder does not exist: {}",
            exploit_folder.display()
        ));
    }

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

    // Create and start container
    let container_name = format!("hzrd-{}", job.id);

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

    // Start a task to collect output
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

    // Wait for container to complete
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

    // Get the output
    let (stdout, stderr) = output_task.await.unwrap();

    // Parse stdout/stderr for flags
    let flags = parse_flags(&stdout, &stderr, job);

    info!("Container exited with code {}", exit_code);

    // Delete the container
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

fn parse_flags(stdout: &[u8], stderr: &[u8], job: &Job) -> Vec<common::Flag> {
    let mut flags = Vec::new();
    let flag_pattern = regex::Regex::new(&job.flag_regex).unwrap();

    // Parse stdout
    if let Ok(stdout_str) = String::from_utf8(stdout.to_vec()) {
        for cap in flag_pattern.captures_iter(&stdout_str) {
            flags.push(common::Flag {
                value: cap[0].to_string(),
                target_id: job.target.id.clone(),
                exploit_name: job.exploit_name.clone(),
                timestamp: Utc::now(),
            });
        }
    }

    // Parse stderr
    if let Ok(stderr_str) = String::from_utf8(stderr.to_vec()) {
        for cap in flag_pattern.captures_iter(&stderr_str) {
            flags.push(common::Flag {
                value: cap[0].to_string(),
                target_id: job.target.id.clone(),
                exploit_name: job.exploit_name.clone(),
                timestamp: Utc::now(),
            });
        }
    }

    flags
}
