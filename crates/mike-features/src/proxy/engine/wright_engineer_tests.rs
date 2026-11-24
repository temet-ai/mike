use axum::http::{HeaderMap, HeaderValue};

use super::{RUN_HEADER, registration_path};

#[test]
fn registration_path_with_working_directory_keeps_it() {
    let mut headers = HeaderMap::new();
    headers.insert(RUN_HEADER, HeaderValue::from_static("7"));
    assert_eq!(registration_path(&headers, "/work/repo"), "/work/repo");
}

#[test]
fn registration_path_without_wright_header_stays_empty() {
    assert_eq!(registration_path(&HeaderMap::new(), ""), "");
}

#[test]
fn registration_path_wright_run_without_directory_names_a_mike_checkout_or_nothing() {
    let mut headers = HeaderMap::new();
    headers.insert(RUN_HEADER, HeaderValue::from_static("7"));
    let path = registration_path(&headers, "");
    assert!(
        path.is_empty() || std::path::Path::new(&path).join("crates/mike/Cargo.toml").is_file(),
        "{path} is neither empty nor a mike checkout"
    );
}
