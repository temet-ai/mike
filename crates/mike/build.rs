use std::path::PathBuf;

#[path = "../build_identity.rs"]
mod build_identity;
#[path = "src/migrations_digest.rs"]
mod migrations_digest;

const MIGRATIONS_DIR: &str = "../mike-base/src/store/persistence/migrations";

fn main() {
    emit_migrations_digest();

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

fn emit_migrations_digest() {
    println!("cargo:rerun-if-changed={MIGRATIONS_DIR}");
    match migrations_digest::digest_dir(std::path::Path::new(MIGRATIONS_DIR)) {
        Ok(digest) => println!("cargo::rustc-env=MIKE_MIGRATIONS_DIGEST={digest}"),
        Err(err) => panic!("cannot read the migrations in {MIGRATIONS_DIR}: {err}"),
    }
}
