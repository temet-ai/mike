use std::path::PathBuf;

#[path = "../build_identity.rs"]
mod build_identity;

fn main() {
    println!("cargo:rerun-if-changed=../mike-base/src/store/persistence/migrations");

    println!("cargo::rustc-check-cfg=cfg(has_spa_build)");
    println!("cargo::rustc-check-cfg=cfg(mike_build_stamp)");
    println!("cargo::rustc-cfg=mike_build_stamp");
    let Some(manifest_dir) = std::env::var_os("CARGO_MANIFEST_DIR") else {
        return;
    };
    let manifest_dir = PathBuf::from(manifest_dir);
    let frontend_build = manifest_dir.join("../../frontend/build");
    let index_html = frontend_build.join("index.html");
    if index_html.is_file() {
        println!("cargo::rustc-cfg=has_spa_build");
    }
    if frontend_build.exists() {
        println!("cargo:rerun-if-changed={}", frontend_build.display());
    }

    build_identity::emit_build_identity(&manifest_dir);
}
