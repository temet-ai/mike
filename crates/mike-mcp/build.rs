#[path = "../build_identity.rs"]
mod build_identity;

fn main() {
    println!("cargo::rustc-check-cfg=cfg(mike_build_stamp)");
    if let Some(manifest_dir) = std::env::var_os("CARGO_MANIFEST_DIR") {
        build_identity::emit_build_identity(std::path::Path::new(&manifest_dir));
    }
}
