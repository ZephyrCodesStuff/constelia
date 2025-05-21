use anyhow::Result;
use clap::Parser;
use tonic::Request;
use tracing::info;

use crate::scheduler::{GetJobsRequest, scheduler_client::SchedulerClient};

#[derive(Debug, Parser)]
pub struct Jobs {}

pub async fn get_jobs(scheduler_addr: &str) -> Result<()> {
    let mut client = SchedulerClient::connect(scheduler_addr.to_string()).await?;
    let response = client.get_jobs(Request::new(GetJobsRequest {})).await?;
    let response = response.into_inner();

    info!("Jobs: {:?}", response.jobs);

    Ok(())
}
