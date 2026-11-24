use super::scratchpad_parent_session;

const PARENT: &str = "96f346df-32ec-4e11-9157-e9e8b51f6244";

#[test]
fn scratchpad_parent_linux_cache_path_returns_parent() {
    let cwd = format!(
        "/home/unit/.claude/.cache/tmp/claude-1000/-mnt-data-projects-temet-ask-kit/{PARENT}/scratchpad/baseline/warm/cwd"
    );
    assert_eq!(
        scratchpad_parent_session(&cwd).map(|id| id.to_string()),
        Some(PARENT.to_string())
    );
}

#[test]
fn scratchpad_parent_mac_private_tmp_returns_parent() {
    let cwd = format!("/private/tmp/claude-501/-Users-dawiddutoit-projects-temet-tether/{PARENT}/scratchpad");
    assert!(scratchpad_parent_session(&cwd).is_some());
}

#[test]
fn scratchpad_parent_without_scratchpad_segment_returns_none() {
    let cwd = format!("/home/unit/.claude/.cache/tmp/claude-1000/-mnt-data-projects-temet-ask-kit/{PARENT}/tasks");
    assert_eq!(scratchpad_parent_session(&cwd), None);
}

#[test]
fn scratchpad_parent_non_numeric_owner_returns_none() {
    let cwd = format!("/tmp/claude-code/-x/{PARENT}/scratchpad");
    assert_eq!(scratchpad_parent_session(&cwd), None);
}

#[test]
fn scratchpad_parent_plain_repo_path_returns_none() {
    assert_eq!(
        scratchpad_parent_session("/mnt/data/projects/temet/temet-proxy"),
        None
    );
}
