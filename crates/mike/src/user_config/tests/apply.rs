use super::*;

#[test]
fn the_report_names_the_file_and_the_source_of_each_variable_without_values() {
    let provenance = Provenance {
        file: Some(PathBuf::from("/home/ada/.config/mike/config.toml")),
        state: FileState::Loaded { keys: 2 },
        origins: BTreeMap::from([
            ("MIKE_DATABASE_URL".to_string(), Origin::Environment),
            ("MIKE_LISTEN_ADDR".to_string(), Origin::Environment),
        ]),
    };
    let report = render_provenance(&provenance);
    assert!(
        report.contains("/home/ada/.config/mike/config.toml (2 keys)"),
        "{report}"
    );
    assert!(
        report.contains("MIKE_DATABASE_URL: environment"),
        "{report}"
    );
    assert!(report.contains("MIKE_LISTEN_ADDR: environment"), "{report}");
    assert!(report.lines().all(|l| l.starts_with('#')), "{report}");
}

#[test]
fn a_multi_line_parse_error_stays_on_one_comment_line() {
    let provenance = Provenance {
        file: Some(PathBuf::from("/c.toml")),
        state: FileState::Unusable("line one\nline two".to_string()),
        origins: BTreeMap::new(),
    };
    let report = render_provenance(&provenance);
    assert!(report.lines().all(|l| l.starts_with('#')), "{report}");
}
