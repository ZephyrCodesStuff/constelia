use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
pub struct Cli {
    #[clap(subcommand)]
    pub action: Action,
}

#[derive(Debug, Subcommand)]
pub enum Action {
    Upload(Upload),
    List(List),
    Run(Run),
    Runners(Runners),
    Stream(Stream),
    Job(Job),
    Jobs(Jobs),
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
pub struct Run {
    /// Name of the exploit to run
    #[clap(short, long)]
    pub exploit: String,
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
    /// Optional target host
    #[clap(long)]
    pub target: Option<String>,
    /// Optional target port
    #[clap(long)]
    pub port: Option<u16>,
}

#[derive(Debug, Parser)]
pub struct Job {
    /// ID of the job to get the result of
    #[clap(short, long)]
    pub job_id: String,
}

#[derive(Debug, Parser)]
pub struct Jobs {}
