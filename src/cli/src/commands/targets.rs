use anyhow::Result;
use clap::Parser;
use tonic::Request;
use tracing::info;

use crate::scheduler::{GetTargetsRequest, scheduler_client::SchedulerClient};

#[derive(Debug, Parser)]
pub struct Targets {}

pub async fn list_targets(scheduler_addr: &str) -> Result<()> {
    let mut client = SchedulerClient::connect(scheduler_addr.to_string()).await?;
    let response = client
        .get_targets(Request::new(GetTargetsRequest {}))
        .await?;
    let targets = response.into_inner().targets;

    info!("Targets: {:?}", targets);
    Ok(())
}
