use std::{env, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    tonic_build::configure()
        .file_descriptor_set_path(out_dir.join("scheduler_descriptor.bin"))
        .compile_protos(&["../../proto/scheduler.proto"], &["../../proto"])?;

    Ok(())
}
