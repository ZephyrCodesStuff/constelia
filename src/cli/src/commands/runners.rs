use anyhow::Result;
use clap::Parser;
use tonic::Request;
use tracing::info;

use crate::scheduler::{GetRunnersRequest, scheduler_client::SchedulerClient};

#[derive(Debug, Parser)]
pub struct Runners {}

pub async fn list_runners(scheduler_addr: &str) -> Result<()> {
    let mut client = SchedulerClient::connect(scheduler_addr.to_string()).await?;
    let response = client
        .get_runners(Request::new(GetRunnersRequest {}))
        .await?;
    let runners = response.into_inner().runners;
    info!("Runners: {:?}", runners);
    Ok(())
}
