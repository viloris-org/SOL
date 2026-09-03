fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    unsafe { std::env::set_var("PROTOC", protoc) };

    let schema = "../protocols/scp/v2/scp.proto";
    prost_build::Config::new().compile_protos(&[schema], &["../protocols"])?;
    println!("cargo:rerun-if-changed={schema}");
    Ok(())
}
