use super::*;

#[test]
fn extract_project_display_name_skips_home_prefix_only() {
    let prompt = "Contents of /a/b/c/d/e/f/CLAUDE.md (instructions):";
    assert_eq!(
        extract_project(sig_from_prompt(prompt)),
        Some("/a/b/c/d/e/f".to_string())
    );
}

#[test]
fn extract_project_display_name_exactly_three_segments_kept() {
    let prompt = "Contents of /Users/user/proj/CLAUDE.md (instructions):";
    assert_eq!(
        extract_project(sig_from_prompt(prompt)),
        Some("/Users/user/proj".to_string())
    );
}

#[test]
fn extract_project_display_name_four_segments() {
    let prompt = "Contents of /Users/user/dev/proj/CLAUDE.md (instructions):";
    assert_eq!(
        extract_project(sig_from_prompt(prompt)),
        Some("/Users/user/dev/proj".to_string())
    );
}

#[test]
fn extract_project_display_name_linux_home_shallow_path() {
    let prompt = "Contents of /home/user/proj/CLAUDE.md (instructions):";
    assert_eq!(
        extract_project(sig_from_prompt(prompt)),
        Some("/home/user/proj".to_string())
    );
}

#[test]
fn extract_project_two_segment_root_is_filtered() {
    let prompt = "Contents of /org/project/CLAUDE.md (instructions):";
    assert_eq!(extract_project(sig_from_prompt(prompt)), None);
}

#[test]
fn extract_project_segment_boundary_table() {
    let cases: &[(&str, Option<&str>)] = &[
        ("Contents of /a/CLAUDE.md (instructions):", None),
        ("Contents of /a/b/CLAUDE.md (instructions):", None),
        (
            "Contents of /a/b/c/CLAUDE.md (instructions):",
            Some("/a/b/c"),
        ),
        (
            "Contents of /a/b/c/d/CLAUDE.md (instructions):",
            Some("/a/b/c/d"),
        ),
    ];
    for (prompt, expected) in cases {
        assert_eq!(
            extract_project(sig_from_prompt(prompt)),
            expected.map(str::to_string),
            "failed for prompt: {prompt}"
        );
    }
}
