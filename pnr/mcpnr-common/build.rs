use std::env;
use std::path::PathBuf;

fn main() -> Result<(), std::io::Error> {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let proto_files = [PathBuf::from("./src/protos/placed_design.proto")];

    prost_build::Config::new()
        .include_file("protos.rs")
        .file_descriptor_set_path(out_dir.join("file_descriptor_set.protobuf"))
        .compile_protos(&proto_files, &[PathBuf::from("./src/protos/")])?;
    Ok(())
}
