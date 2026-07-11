fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    std::env::set_var("PROTOC", protoc);
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR")?);
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .file_descriptor_set_path(out_dir.join("astravector_descriptor.bin"))
        .compile_protos(&["proto/astravector_embedding.proto"], &["proto"])?;
    println!("cargo:rerun-if-changed=proto/astravector_embedding.proto");
    println!("cargo:rerun-if-changed=migrations");
    Ok(())
}
