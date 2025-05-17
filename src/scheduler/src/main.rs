use anyhow::Result;
use chrono::Utc;
use clap::Parser;
use common::{ExploitMetadata, Job, Target};
use std::path::PathBuf;
use tokio::fs;
use tracing::{info, Level};
use uuid::Uuid;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Path to targets.toml
    #[arg(short, long, default_value = "targets.toml")]
    targets: PathBuf,

    /// Path to exploits directory
    #[arg(short, long, default_value = "exploits")]
    exploits: PathBuf,

    /// Run a single round of job dispatch
    #[arg(short, long)]
    run_once: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    let cli = Cli::parse();
    info!("Starting scheduler with targets: {:?}", cli.targets);

    // Load targets and exploits
    let targets = load_targets(&cli.targets).await?;
    let exploits = load_exploits(&cli.exploits).await?;

    // Generate jobs
    let jobs = generate_jobs(&targets, &exploits);
    info!("Generated {} jobs", jobs.len());

    // TODO: Send jobs to queue
    for job in jobs {
        info!("Job: {:?}", job);
    }

    Ok(())
}

async fn load_targets(path: &PathBuf) -> Result<Vec<Target>> {
    let contents = fs::read_to_string(path).await?;
    let targets: Vec<Target> = toml::from_str(&contents)?;
    Ok(targets)
}

async fn load_exploits(dir: &PathBuf) -> Result<Vec<ExploitMetadata>> {
    let mut exploits = Vec::new();
    let mut entries = fs::read_dir(dir).await?;

    while let Some(entry) = entries.next_entry().await? {
        if entry.path().extension().and_then(|s| s.to_str()) == Some("toml") {
            let contents = fs::read_to_string(entry.path()).await?;
            let metadata: ExploitMetadata = toml::from_str(&contents)?;
            exploits.push(metadata);
        }
    }

    Ok(exploits)
}

fn generate_jobs(targets: &[Target], exploits: &[ExploitMetadata]) -> Vec<Job> {
    let mut jobs = Vec::new();
    let now = Utc::now();

    for target in targets {
        for exploit in exploits {
            // TODO: Add matching logic based on tags
            let job = Job {
                id: Uuid::new_v4().to_string(),
                target: target.clone(),
                exploit_name: exploit.name.clone(),
                status: common::JobStatus::Pending,
                created_at: now,
                updated_at: now,
                result: None,
                flag_regex: r"[A-Z0-9]{31}=".to_string(),
            };
            jobs.push(job);
        }
    }

    jobs
}
