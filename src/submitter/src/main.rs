use std::time::Duration;

use reqwest::{
    header::{HeaderMap, HeaderName},
    Client, ClientBuilder,
};
use serde_json::{json, Value};
use tonic::{transport::Server, Request, Response, Status};

pub mod submitter_proto {
    tonic::include_proto!("submitter");
    pub const FILE_DESCRIPTOR_SET: &[u8] =
        tonic::include_file_descriptor_set!("submitter_descriptor");
}

use submitter_proto::{
    submitter_server::{Submitter, SubmitterServer},
    SubmissionRequest, SubmissionResponse,
};
use tracing::info;
use tracing_subscriber::EnvFilter;

// TODO: Make this configurable
const TEAM_TOKEN: &str = "team-token";
const BASE_URL: &str = "http://10.10.0.1:8080/flags";

#[derive(Debug)]
pub struct SubmitterService {
    client: Client,
}

impl SubmitterService {
    pub fn new() -> Self {
        let headers = HeaderMap::from_iter(vec![
            (
                HeaderName::from_static("x-team-token"),
                TEAM_TOKEN.parse().unwrap(),
            ),
            (
                HeaderName::from_static("content-type"),
                "application/json".parse().unwrap(),
            ),
            (
                HeaderName::from_static("accept"),
                "application/json".parse().unwrap(),
            ),
            (
                HeaderName::from_static("user-agent"),
                "hzrd/submitter".parse().unwrap(),
            ),
        ]);

        let client = ClientBuilder::new()
            .timeout(Duration::from_secs(10))
            .default_headers(headers)
            .build()
            .unwrap();

        Self { client }
    }
}

#[tonic::async_trait]
impl Submitter for SubmitterService {
    async fn submit_flags(
        &self,
        request: Request<SubmissionRequest>,
    ) -> Result<Response<SubmissionResponse>, Status> {
        let req = request.into_inner();

        let flags: Value = json!(req.flags);

        info!("Submitting flags: {:?}", flags);

        // Submit flags
        let res = self
            .client
            .put(BASE_URL)
            .json(&flags)
            .send()
            .await
            .map_err(|e| Status::internal(format!("HTTP error: {}", e)))?;

        // TODO: Parse the response
        if res.status().is_success() {
            Ok(Response::new(SubmissionResponse {
                ok: true,
                message: "Flags submitted".to_string(),
            }))
        } else {
            let msg = res
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            Ok(Response::new(SubmissionResponse {
                ok: false,
                message: format!("Failed to submit flags: {}", msg),
            }))
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .pretty()
        .init();

    let addr = "[::]:50053".parse()?;
    let submitter = SubmitterService::new();

    info!("Starting submitter service at {}", addr);

    Server::builder()
        .add_service(SubmitterServer::new(submitter))
        .serve(addr)
        .await?;

    Ok(())
}
