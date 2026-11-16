use super::check_tests::{candidate_files, names_in};

#[test]
fn candidate_files_take_test_files_from_the_command_and_the_changed_tests() {
    let files = candidate_files(
        "mise exec -- bun test test/classify.test.ts test/version.test.ts --reporter=dots",
        &[
            "\"none\"".to_owned(),
            "crates/a/src/lib.rs (added three tests)".to_owned(),
        ],
    );
    assert_eq!(
        files,
        vec![
            "test/classify.test.ts".to_owned(),
            "test/version.test.ts".to_owned(),
            "crates/a/src/lib.rs".to_owned(),
        ]
    );
}

#[test]
fn names_in_finds_quoted_cases_and_marked_functions() {
    let source = r#"
describe("status", () => {
  test("names the release and next version", async () => {});
  it.skip('skips nothing', () => {});
});
#[test]
fn config_rejects_bad_input() {}
fn helper() {}
pub fn parses_web_test() {}
def test_counts_rows():
"#;
    assert_eq!(
        names_in(source),
        vec![
            "status",
            "names the release and next version",
            "skips nothing",
            "config_rejects_bad_input",
            "parses_web_test",
            "test_counts_rows",
        ]
    );
}
