use super::*;
use std::os::unix::ffi::OsStringExt;

fn args(arguments: &[&str]) -> Vec<OsString> {
    arguments.iter().map(OsString::from).collect()
}

pub fn narrow_queries_and_builtin_help_pass() {
    for arguments in [
        vec![],
        vec!["--version"],
        vec!["+nightly", "--offline", "metadata"],
        vec!["help", "test"],
        vec!["test", "--help"],
        vec!["--config", "a=b", "tree"],
    ] {
        assert_eq!(classify(&args(&arguments)), Decision::Query);
    }
}

pub fn heavy_aliases_and_test_harness_help_are_refused() {
    for arguments in [
        vec!["build"],
        vec!["run"],
        vec!["install", "helper"],
        vec!["clippy"],
        vec!["nextest", "run"],
        vec!["custom-alias"],
        vec!["test", "--", "--help"],
        vec!["help", "custom-alias"],
    ] {
        assert_eq!(classify(&args(&arguments)), Decision::HeavyOrUnknown);
    }
    assert_eq!(
        classify(&[OsString::from_vec(vec![0xff])]),
        Decision::HeavyOrUnknown
    );
}

pub fn formatter_is_an_explicit_separate_refusal() {
    assert_eq!(
        classify(&args(&["fmt", "--all", "--", "--check"])),
        Decision::Format
    );
    assert_eq!(classify(&args(&["help", "fmt"])), Decision::HeavyOrUnknown);
}
