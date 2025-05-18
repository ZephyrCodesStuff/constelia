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
