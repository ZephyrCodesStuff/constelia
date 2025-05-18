fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .file_descriptor_set_path("runner_descriptor.bin")
        .compile_protos(&["../../proto/runner.proto"], &["../../proto"])?;

    tonic_build::configure()
        .include_file("../../runner.proto")
        .file_descriptor_set_path("scheduler_descriptor.bin")
        .compile_protos(&["../../proto/scheduler.proto"], &["../../proto"])?;

    tonic_build::configure()
        .file_descriptor_set_path("heartbeat_descriptor.bin")
        .compile_protos(&["../../proto/heartbeat.proto"], &["../../proto"])?;

    tonic_build::configure()
        .file_descriptor_set_path("submitter_descriptor.bin")
        .compile_protos(&["../../proto/submitter.proto"], &["../../proto"])?;

    Ok(())
}
