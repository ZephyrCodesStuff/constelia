use anyhow::Result;
use clap::Parser;
use tonic::Request;
use tracing::{error, info};

use crate::scheduler::{RunExploitRequest, scheduler_client::SchedulerClient};

#[derive(Debug, Parser)]
pub struct Stream {
    /// Name of the exploit to run
    #[clap(short, long)]
    pub exploit: String,

    /// Number of jobs to run in parallel
    #[clap(short, long, default_value = "5")]
    pub count: usize,

    /// Target ID or host
    #[clap(long)]
    pub target: String,
}

pub async fn stream_exploits(
    scheduler_addr: &str,
    exploit_name: &str,
    count: usize,
    target: &str,
) -> Result<()> {
    let mut handles = Vec::new();
    for _ in 0..count {
        let exploit_name = exploit_name.to_string();
        let scheduler_addr = scheduler_addr.to_string();
        let target = target.to_string();

        handles.push(tokio::spawn(async move {
            let mut client = SchedulerClient::connect(scheduler_addr).await?;
            let response = client
                .run_exploit(Request::new(RunExploitRequest {
                    exploit_name: exploit_name.clone(),
                    target: target.to_string(),
                }))
                .await?;
            let response = response.into_inner();
            Ok::<_, anyhow::Error>(response)
        }));
    }
    let results = futures::future::join_all(handles).await;
    for (i, res) in results.into_iter().enumerate() {
        match res {
            Ok(Ok(resp)) => info!("[Job {i}] Response: {:?}", resp),
            Ok(Err(e)) => error!("[Job {i}] Error: {e}"),
            Err(e) => error!("[Job {i}] Join error: {e}"),
        }
    }
    Ok(())
}
