use std::ffi::OsString;

use super::{DIGEST, list_requested, listing, migrator};

fn args(words: &[&str]) -> Vec<OsString> {
    words.iter().map(OsString::from).collect()
}

#[test]
fn list_flag_is_accepted_only_alone() {
    assert!(list_requested(&args(&["--list-embedded-migrations"])));
    assert!(!list_requested(&args(&[])));
    assert!(!list_requested(&args(&[
        "--list-embedded-migrations",
        "serve"
    ])));
    assert!(!list_requested(&args(&[
        "serve",
        "--list-embedded-migrations"
    ])));
}

#[test]
fn listing_names_every_migration_file_by_version() -> anyhow::Result<()> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../mike-base/src/store/persistence/migrations");
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        let Some((version, _)) = name.split_once('_') else {
            anyhow::bail!("migration file {name} has no version prefix");
        };
        files.push(version.to_owned());
    }
    files.sort();
    let text = listing(&migrator());
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some(format!("# digest {DIGEST}").as_str()));
    let mut listed: Vec<String> = lines.map(str::to_owned).collect();
    listed.sort();
    assert_eq!(listed, files);
    Ok(())
}
