use super::{digest_dir, digest_files};

fn file(name: &str, bytes: &str) -> (String, Vec<u8>) {
    (name.to_owned(), bytes.as_bytes().to_vec())
}

#[test]
fn a_new_migration_file_changes_the_digest() {
    let before = vec![file("1_a.sql", "select 1;")];
    let after = vec![file("1_a.sql", "select 1;"), file("2_b.sql", "")];
    assert_ne!(digest_files(&before), digest_files(&after));
}

#[test]
fn changed_bytes_or_a_renamed_file_change_the_digest() {
    let base = digest_files(&[file("1_a.sql", "select 1;")]);
    assert_ne!(base, digest_files(&[file("1_a.sql", "select 2;")]));
    assert_ne!(base, digest_files(&[file("1_b.sql", "select 1;")]));
}

#[test]
fn name_and_bytes_are_framed_apart() {
    assert_ne!(
        digest_files(&[file("1_a.sql", "xselect")]),
        digest_files(&[file("1_a.sqlx", "select")])
    );
}

#[test]
fn the_digest_does_not_depend_on_listing_order() {
    let one = vec![file("1_a.sql", "a"), file("2_b.sql", "b")];
    let two = vec![file("2_b.sql", "b"), file("1_a.sql", "a")];
    assert_eq!(digest_files(&one), digest_files(&two));
}

#[test]
fn the_digest_is_sixteen_hex_digits() {
    let digest = digest_files(&[]);
    assert_eq!(digest.len(), 16);
    assert!(digest.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn the_compiled_digest_is_the_digest_of_the_migrations_directory() -> anyhow::Result<()> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../mike-base/src/store/persistence/migrations");
    assert_eq!(digest_dir(&dir)?, env!("MIKE_MIGRATIONS_DIGEST"));
    Ok(())
}

#[test]
fn a_missing_directory_is_an_error() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("no-such-migrations-dir");
    assert!(digest_dir(&dir).is_err());
}
