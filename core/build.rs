fn main() {
    tonic_build::configure()
        .include_file("generated_proto.rs")
        .compile(&["proto/reverse-proxy.proto"], &["/proto"])
        .unwrap_or_else(|e| panic!("Failed to compile protos {:?}", e));
}
