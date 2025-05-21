use anyhow::Result;
use clap::Parser;
use tonic::Request;
use tracing::{error, info};

use crate::scheduler::{RunExploitRequest, scheduler_client::SchedulerClient};

#[derive(Debug, Parser)]
pub struct Run {
    /// Name of the exploit to run
    #[clap(short, long)]
    pub exploit: String,

    /// Target ID or host
    #[clap(short, long)]
    pub target: String,
}

pub async fn run_exploit(scheduler_addr: &str, exploit_name: &str, target: &str) -> Result<()> {
    let mut client = SchedulerClient::connect(scheduler_addr.to_string()).await?;
    let response = client
        .run_exploit(Request::new(RunExploitRequest {
            exploit_name: exploit_name.to_string(),
            target: target.to_string(),
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
