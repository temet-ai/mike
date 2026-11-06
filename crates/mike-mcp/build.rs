#[path = "../build_identity.rs"]
mod build_identity;

fn main() {
    if let Some(manifest_dir) = std::env::var_os("CARGO_MANIFEST_DIR") {
        build_identity::emit_build_identity(std::path::Path::new(&manifest_dir));
    }
}
