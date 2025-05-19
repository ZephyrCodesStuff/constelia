use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
pub struct Cli {
    #[clap(subcommand)]
    pub action: Action,
}

#[derive(Debug, Subcommand)]
pub enum Action {
    /// Upload an exploit
    Upload(Upload),

    /// List exploits
    List(List),

    /// List targets
    Targets(Targets),

    /// Run an exploit
    /// TODO: change this to `Enqueue`
    Run(Run),

    /// List runners
    Runners(Runners),

    /// Run jobs as a stream
    Stream(Stream),

    /// Get a job's result
    Job(Job),

    /// List jobs
    Jobs(Jobs),

    /// Ask runners to pull jobs
    Pull(Pull),
}

#[derive(Debug, Parser)]
pub struct Upload {
    /// Path to the exploit's folder containing:
    /// - `main.py`
    /// - `requirements.txt`
    /// - `docker-entrypoint.sh`
    #[clap(short, long)]
    pub exploit: PathBuf,
}

#[derive(Debug, Parser)]
pub struct List {}

#[derive(Debug, Parser)]
pub struct Targets {}

#[derive(Debug, Parser)]
pub struct Run {
    /// Name of the exploit to run
    #[clap(short, long)]
    pub exploit: String,

    /// Target ID or host
    #[clap(short, long)]
    pub target: String,
}

#[derive(Debug, Parser)]
pub struct Runners {}

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

#[derive(Debug, Parser)]
pub struct Job {
    /// ID of the job to get the result of
    #[clap(short, long)]
    pub job_id: String,
}

#[derive(Debug, Parser)]
pub struct Jobs {}

#[derive(Debug, Parser)]
pub struct Pull {}
