use anyhow::Result;
use clap::Parser;
use common::Flag;
use reqwest::Client;
use std::collections::HashSet;
use tracing::{error, info, Level};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// CTFd API URL
    #[arg(short, long)]
    api_url: String,

    /// CTFd API token
    #[arg(short, long)]
    api_token: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    let cli = Cli::parse();
    info!("Starting flag submitter");

    // TODO: Connect to job queue to receive flags
    // For now, we'll just simulate receiving some flags
    let flags = simulate_flags();

    if let Err(e) = submit_flags(&cli, &flags).await {
        error!("Failed to submit flags: {}", e);
    }

    Ok(())
}

fn simulate_flags() -> Vec<Flag> {
    // This is just for testing - in reality, we'd get these from the queue
    vec![
        Flag {
            value: "flag{test1}".to_string(),
            target_id: "target1".to_string(),
            exploit_name: "exploit1.py".to_string(),
            timestamp: chrono::Utc::now(),
        },
        Flag {
            value: "flag{test2}".to_string(),
            target_id: "target2".to_string(),
            exploit_name: "exploit2.py".to_string(),
            timestamp: chrono::Utc::now(),
        },
    ]
}

async fn submit_flags(cli: &Cli, flags: &[Flag]) -> Result<()> {
    let client = Client::new();
    let mut seen = HashSet::new();

    for flag in flags {
        // Deduplicate flags
        if !seen.insert(flag.value.clone()) {
            info!("Skipping duplicate flag: {}", flag.value);
            continue;
        }

        // Submit flag to CTFd
        let response = client
            .post(&format!("{}/api/v1/challenges/attempt", cli.api_url))
            .header("Authorization", format!("Token {}", cli.api_token))
            .json(&serde_json::json!({
                "challenge_id": 1, // TODO: Map target_id to challenge_id
                "submission": flag.value
            }))
            .send()
            .await?;

        if response.status().is_success() {
            info!("Successfully submitted flag: {}", flag.value);
        } else {
            error!(
                "Failed to submit flag {}: {}",
                flag.value,
                response.text().await?
            );
        }
    }

    Ok(())
}
