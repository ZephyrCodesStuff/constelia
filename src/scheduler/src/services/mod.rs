pub mod heartbeat;
pub mod scheduler;

pub mod heartbeat_proto {
    tonic::include_proto!("heartbeat");
    pub const FILE_DESCRIPTOR_SET: &[u8] =
        tonic::include_file_descriptor_set!("heartbeat_descriptor");
}

pub mod scheduler_proto {
    tonic::include_proto!("scheduler");
    pub const FILE_DESCRIPTOR_SET: &[u8] =
        tonic::include_file_descriptor_set!("scheduler_descriptor");
}

pub mod submitter_proto {
    tonic::include_proto!("submitter");
}

pub mod runner_proto {
    tonic::include_proto!("runner");
}
