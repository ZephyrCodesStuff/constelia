use anyhow::Result;
use clap::Parser;
use tonic::Request;
use tracing::info;

use crate::scheduler::{GetJobResultRequest, scheduler_client::SchedulerClient};

#[derive(Debug, Parser)]
pub struct Job {
    /// ID of the job to get the result of
    #[clap(short, long)]
    pub job_id: String,
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
