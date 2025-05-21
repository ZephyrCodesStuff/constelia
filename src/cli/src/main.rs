use std::net::Ipv4Addr;

use anyhow::Result;
use clap::Parser;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

pub mod runner {
    tonic::include_proto!("runner");
}

pub mod scheduler {
    tonic::include_proto!("scheduler");
}

mod cli;
mod commands;

/// Shorthand to try a command and handle errors gracefully.
macro_rules! try_command {
    ($command:expr) => {
        $command.await.map_err(|e| {
            error!("Failed to execute command: {}", e);
            anyhow::anyhow!("Failed to execute command: {}", e)
        })?;
    };
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .pretty()
        .init();

    let cli = cli::Cli::parse();

    let host = cli.host.unwrap_or_else(|| {
        let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        host.parse::<Ipv4Addr>().unwrap()
    });

    let port = cli.port.unwrap_or_else(|| {
        let port = std::env::var("PORT").unwrap_or_else(|_| "50051".to_string());
        port.parse::<u16>().unwrap()
    });

    let scheduler_addr = format!("http://{}:{}", host, port);

    match cli.action {
        cli::Action::Upload(upload) => {
            let exploit_name = upload.exploit.file_name().unwrap().to_str().unwrap();

            try_command!(commands::upload::upload_exploit(
                &scheduler_addr,
                exploit_name,
                &upload.exploit
            ));

            info!("Exploit uploaded successfully");
        }
        cli::Action::List(_) => {
            try_command!(commands::list::list_exploits(&scheduler_addr));
        }
        cli::Action::Targets(_) => {
            try_command!(commands::targets::list_targets(&scheduler_addr));
        }
        cli::Action::Run(run) => {
            try_command!(commands::run::run_exploit(
                &scheduler_addr,
                &run.exploit,
                &run.target
            ));
        }
        cli::Action::Runners(_) => {
            try_command!(commands::runners::list_runners(&scheduler_addr));
        }
        cli::Action::Stream(stream) => {
            try_command!(commands::stream::stream_exploits(
                &scheduler_addr,
                &stream.exploit,
                stream.count,
                &stream.target,
            ));
        }
        cli::Action::Job(job) => {
            try_command!(commands::job::get_job_result(&scheduler_addr, &job.job_id));
        }
        cli::Action::Jobs(_) => {
            try_command!(commands::jobs::get_jobs(&scheduler_addr));
        }
        cli::Action::Pull(_) => {
            try_command!(commands::pull::pull_jobs(&scheduler_addr));
        }
        cli::Action::Attack(_) => {
            try_command!(commands::attack::attack(&scheduler_addr));
        }
    }

    Ok(())
}
