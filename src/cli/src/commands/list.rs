use anyhow::Result;
use clap::Parser;
use tonic::Request;
use tracing::info;

use crate::scheduler::{GetExploitsRequest, scheduler_client::SchedulerClient};

#[derive(Debug, Parser)]
pub struct List {}

pub async fn list_exploits(scheduler_addr: &str) -> Result<()> {
    let mut client = SchedulerClient::connect(scheduler_addr.to_string()).await?;
    let response = client
        .get_exploits(Request::new(GetExploitsRequest {}))
        .await?;
    let exploits = response.into_inner().exploit_names;

    info!("Exploits: {:?}", exploits);

    Ok(())
}
