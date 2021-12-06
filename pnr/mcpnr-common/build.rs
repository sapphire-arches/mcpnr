use std::env;
use std::fs;
use std::path;

fn main() -> Result<(), std::io::Error> {
    let out_dir = path::PathBuf::from(env::var("OUT_DIR").unwrap());
    let out_dir = out_dir.join("protos");
    let proto_cache_dir = path::PathBuf::from(&out_dir).join("cache");

    fs::create_dir_all(&proto_cache_dir).expect("Failed to create proto file cache directory");

    let yosys_proto = {
        let yosys_proto = env::var("YOSYS_PROTO_PATH")
            .expect("YOSYS_PROTO_PATH must be set to the path to the Yosys Protobuf spec file");
        println!("cargo:rerun-if-env-changed=YOSYS_PROTO_PATH");
        println!("cargo:rerun-if-changed={}", yosys_proto);

        let yosys_proto_cached = proto_cache_dir.join("yosys.proto");

        if yosys_proto_cached.exists() {
            fs::remove_file(&yosys_proto_cached).expect("Could not remove stale yosys proto");
        }

        fs::copy(&yosys_proto, &yosys_proto_cached).expect(&format!(
            "Failed to copy {:?} to {:?}",
            yosys_proto, yosys_proto_cached
        ));

        yosys_proto_cached
    };

    prost_build::Config::new()
        .include_file("protos.rs")
        .compile_protos(&[yosys_proto], &[proto_cache_dir])?;
    Ok(())
}
