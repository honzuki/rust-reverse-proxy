fn main() {
    tonic_build::configure()
        .protoc_arg("--experimental_allow_proto3_optional")
        .include_file("generated_proto.rs")
        .compile(&["proto/reverse-proxy.proto"], &["/proto"])
        .unwrap_or_else(|e| panic!("Failed to compile protos {:?}", e));
}
