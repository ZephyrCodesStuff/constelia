use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::Parser;
use tar::Builder;
use tonic::Request;
use tracing::error;

use crate::scheduler::{UploadExploitRequest, scheduler_client::SchedulerClient};

#[derive(Debug, Parser)]
pub struct Upload {
    /// Path to the exploit's folder containing:
    /// - `main.py`
    /// - `requirements.txt`
    /// - `docker-entrypoint.sh`
    #[clap(short, long)]
    pub exploit: PathBuf,
}

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
