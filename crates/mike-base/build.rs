use std::path::PathBuf;

#[path = "../build_identity.rs"]
mod build_identity;

fn main() {
    println!("cargo:rerun-if-changed=src/store/persistence/migrations");
    let Some(manifest_dir) = std::env::var_os("CARGO_MANIFEST_DIR") else {
        return;
    };
    build_identity::emit_build_identity(&PathBuf::from(manifest_dir));
}
