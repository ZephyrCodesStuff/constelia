use std::net::Ipv4Addr;

use clap::{Parser, Subcommand};

use crate::commands::{
    attack::Attack, job::Job, jobs::Jobs, list::List, pull::Pull, run::Run, runners::Runners,
    stream::Stream, targets::Targets, upload::Upload,
};

#[derive(Debug, Parser)]
pub struct Cli {
    /// IPv4 address of the scheduler to connect to.
    ///
    /// Defaults to the configured `HOST` environment variable, or `127.0.0.1` if unspecified.
    #[clap(short, long)]
    pub host: Option<Ipv4Addr>,

    /// Port of the scheduler to connect to.
    ///
    /// Defaults to the configured `PORT` environment variable, or `50051` if unspecified.
    #[clap(short, long)]
    pub port: Option<u16>,

    /// Action to perform.
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

    /// TODO: add an `Attack` command that inquires for info and
    /// queues all of the necessary jobs for the given target(s)
    Attack(Attack),

    /// Get a job's result
    Job(Job),

    /// List jobs
    Jobs(Jobs),

    /// Ask runners to pull jobs
    Pull(Pull),
}
