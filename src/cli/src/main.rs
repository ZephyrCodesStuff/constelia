use anyhow::Result;
use clap::Parser;
use futures::future::join_all;
use std::path::Path;
use tar::Builder;
use tonic::Request;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

pub mod runner {
    tonic::include_proto!("runner");
}

pub mod scheduler {
    tonic::include_proto!("scheduler");
}

use scheduler::{
    GetExploitsRequest, GetJobResultRequest, GetJobsRequest, GetRunnersRequest, RunExploitRequest,
    UploadExploitRequest, scheduler_client::SchedulerClient,
};

mod cli;

pub async fn upload_exploit(
    scheduler_addr: &str,
    exploit_name: &str,
    folder_path: &Path,
) -> Result<()> {
    // Tar the folder into memory
    let mut tar_data = Vec::new();
    {
        let mut tar_builder = Builder::new(&mut tar_data);
        tar_builder.append_dir_all(".", folder_path)?;
        tar_builder.finish()?;
    }

    // Connect to the scheduler
    let mut client = SchedulerClient::connect(scheduler_addr.to_string()).await?;

    // Build the request
    let request = UploadExploitRequest {
        exploit_name: exploit_name.to_string(),
        exploit_bundle: tar_data,
    };

    // Send the request
    let response = client.upload_exploit(Request::new(request)).await?;
    if !response.into_inner().ok {
        error!("Failed to upload exploit");
        return Err(anyhow::anyhow!("Failed to upload exploit"));
    }

    Ok(())
}

pub async fn list_exploits(scheduler_addr: &str) -> Result<()> {
    let mut client = SchedulerClient::connect(scheduler_addr.to_string()).await?;
    let response = client
        .get_exploits(Request::new(GetExploitsRequest {}))
        .await?;
    let exploits = response.into_inner().exploit_names;

    info!("Exploits: {:?}", exploits);

    Ok(())
}

pub async fn run_exploit(scheduler_addr: &str, exploit_name: &str) -> Result<()> {
    let mut client = SchedulerClient::connect(scheduler_addr.to_string()).await?;
    let response = client
        .run_exploit(Request::new(RunExploitRequest {
            exploit_name: exploit_name.to_string(),
        }))
        .await?;

    let response = response.into_inner();

    if !response.ok {
        error!("Failed to run exploit");
        return Err(anyhow::anyhow!("Failed to run exploit"));
    }

    info!("Exploit ran successfully: {:?}", response);

    Ok(())
}

pub async fn list_runners(scheduler_addr: &str) -> Result<()> {
    let mut client = SchedulerClient::connect(scheduler_addr.to_string()).await?;
    let response = client
        .get_runners(Request::new(GetRunnersRequest {}))
        .await?;
    let runners = response.into_inner().runners;
    info!("Runners: {:?}", runners);
    Ok(())
}

pub async fn stream_exploits(
    scheduler_addr: &str,
    exploit_name: &str,
    count: usize,
    target: Option<String>,
    port: Option<u16>,
) -> Result<()> {
    let mut handles = Vec::new();
    for _ in 0..count {
        let exploit_name = exploit_name.to_string();
        let scheduler_addr = scheduler_addr.to_string();
        let target = target.clone();
        let port = port.clone();
        handles.push(tokio::spawn(async move {
            let mut client = SchedulerClient::connect(scheduler_addr).await?;
            let response = client
                .run_exploit(Request::new(RunExploitRequest {
                    exploit_name: exploit_name.clone(),
                    // You can extend this to pass target/port if your proto supports it
                }))
                .await?;
            let response = response.into_inner();
            Ok::<_, anyhow::Error>(response)
        }));
    }
    let results = join_all(handles).await;
    for (i, res) in results.into_iter().enumerate() {
        match res {
            Ok(Ok(resp)) => info!("[Job {i}] Response: {:?}", resp),
            Ok(Err(e)) => error!("[Job {i}] Error: {e}"),
            Err(e) => error!("[Job {i}] Join error: {e}"),
        }
    }
    Ok(())
}

pub async fn get_job_result(scheduler_addr: &str, job_id: &str) -> Result<()> {
    let mut client = SchedulerClient::connect(scheduler_addr.to_string()).await?;
    let response = client
        .get_job_result(Request::new(GetJobResultRequest {
            job_id: job_id.to_string(),
        }))
        .await?;
    let response = response.into_inner();
    info!("Job result: {:?}", response);
    Ok(())
}

pub async fn get_jobs(scheduler_addr: &str) -> Result<()> {
    let mut client = SchedulerClient::connect(scheduler_addr.to_string()).await?;
    let response = client.get_jobs(Request::new(GetJobsRequest {})).await?;
    let response = response.into_inner();

    info!(
        "Jobs: {:?}",
        response
            .jobs
            .iter()
            .map(|j| (
                j.job_id.clone(),
                j.flags.iter().map(|f| f.value.clone()).collect::<Vec<_>>()
            ))
            .collect::<Vec<_>>()
    );

    Ok(())
}

// TODO: Make this configurable
const SCHEDULER_ADDR: &str = "http://localhost:50052";

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .pretty()
        .init();

    let cli = cli::Cli::parse();

    match cli.action {
        cli::Action::Upload(upload) => {
            let exploit_name = upload.exploit.file_name().unwrap().to_str().unwrap();

            upload_exploit(SCHEDULER_ADDR, exploit_name, &upload.exploit)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to upload exploit: {}", e))?;

            info!("Exploit uploaded successfully");
        }
        cli::Action::List(_) => {
            list_exploits(SCHEDULER_ADDR)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to list exploits: {}", e))?;
        }
        cli::Action::Run(run) => {
            run_exploit(SCHEDULER_ADDR, &run.exploit)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to run exploit: {}", e))?;
        }
        cli::Action::Runners(_) => {
            list_runners(SCHEDULER_ADDR)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to list runners: {}", e))?;
        }
        cli::Action::Stream(stream) => {
            stream_exploits(
                SCHEDULER_ADDR,
                &stream.exploit,
                stream.count,
                stream.target,
                stream.port,
            )
            .await
            .map_err(|e| anyhow::anyhow!("Failed to stream exploits: {}", e))?;
        }
        cli::Action::Job(job) => {
            get_job_result(SCHEDULER_ADDR, &job.job_id)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to get job result: {}", e))?;
        }
        cli::Action::Jobs(_) => {
            get_jobs(SCHEDULER_ADDR)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to get jobs: {}", e))?;
        }
    }

    Ok(())
}
