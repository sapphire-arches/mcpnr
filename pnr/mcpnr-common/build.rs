use std::env;
use std::path;

fn main() -> Result<(), std::io::Error> {
    let yosys_proto = {
        let yosys_proto = path::PathBuf::from(
            env::var_os("YOSYS_PROTO_PATH")
                .expect("YOSYS_PROTO_PATH must be set to the path to the Yosys Protobuf spec file"),
        );

        println!("cargo:rerun-if-env-changed=YOSYS_PROTO_PATH");
        println!("cargo:rerun-if-changed={}", yosys_proto.to_string_lossy());

        yosys_proto
    };

    let yosys_proto_dir = yosys_proto
        .parent()
        .expect(&format!(
            "Failed to get parent directory of yosys protobuf file {}",
            yosys_proto.to_string_lossy()
        ))
        .to_owned();

    prost_build::Config::new()
        .include_file("protos.rs")
        .compile_protos(&[yosys_proto], &[yosys_proto_dir])?;
    Ok(())
}
