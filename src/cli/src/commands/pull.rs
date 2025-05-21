use anyhow::Result;
use clap::Parser;
use tonic::Request;
use tracing::info;

use crate::scheduler::{PollRunnersRequest, scheduler_client::SchedulerClient};

#[derive(Debug, Parser)]
pub struct Pull {}

pub async fn pull_jobs(scheduler_addr: &str) -> Result<()> {
    let mut client = SchedulerClient::connect(scheduler_addr.to_string()).await?;
    let response = client
        .poll_runners(Request::new(PollRunnersRequest {}))
        .await?;
    let response = response.into_inner();
    info!("Pulled jobs: {:?}", response.runners);
    Ok(())
}
