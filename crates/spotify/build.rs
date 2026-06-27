fn main() {
  protobuf_codegen::Codegen::new()
    .pure()
    .cargo_out_dir("custom_protos")
    .include("proto")
    .input("proto/searchview.proto")
    .input("proto/casita_home.proto")
    .input("proto/recently_played.proto")
    .input("proto/collection.proto")
    .run_from_script();
}
