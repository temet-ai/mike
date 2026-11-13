const EPHEMERAL_PREFIXES: [&str; 6] = [
    "/tmp/",
    "/private/tmp/",
    "/var/folders/",
    "/private/var/folders/",
    "/dev/shm/",
    "/run/user/",
];

const EPHEMERAL_SEGMENTS: [&str; 6] = [
    "/scratchpad/",
    "/selftest",
    "-selftest",
    "vsp-verify-scratch",
    "dbgtest-",
    "scratch-repo",
];

pub fn ephemeral_project_path(project_path: &str) -> bool {
    let path = project_path.trim();
    if path.is_empty() {
        return false;
    }
    let lowered = path.to_lowercase();
    if EPHEMERAL_PREFIXES
        .iter()
        .any(|prefix| lowered.starts_with(prefix))
    {
        return true;
    }
    EPHEMERAL_SEGMENTS
        .iter()
        .any(|segment| lowered.contains(segment))
}

#[cfg(test)]
pub mod tests {
    use super::ephemeral_project_path;

    #[test]
    fn real_repo_path_is_not_ephemeral() {
        assert!(!ephemeral_project_path(
            "/mnt/data/projects/temet/temet-proxy"
        ));
        assert!(!ephemeral_project_path(
            "/Users/vimes/projects/temet/temet-proxy"
        ));
        assert!(!ephemeral_project_path("projects/temet/temet-proxy"));
    }

    #[test]
    fn empty_path_is_not_ephemeral() {
        assert!(!ephemeral_project_path(""));
        assert!(!ephemeral_project_path("   "));
    }

    #[test]
    fn macos_tempdir_is_ephemeral() {
        assert!(ephemeral_project_path(
            "/private/var/folders/xh/2hhvhg9s7fg5f80flxq139d80000gn/T/verifier-stop-producer-selftest-gw9t1mxz"
        ));
        assert!(ephemeral_project_path("/var/folders/xh/abc/T/probe"));
    }

    #[test]
    fn unix_tempdir_is_ephemeral() {
        assert!(ephemeral_project_path("/tmp/vsp-verify-scratch2"));
        assert!(ephemeral_project_path("/private/tmp/vsp-verify-scratch"));
        assert!(ephemeral_project_path("/dev/shm/probe-repo"));
        assert!(ephemeral_project_path("/run/user/1000/probe-repo"));
    }

    #[test]
    fn harness_scratch_segment_is_ephemeral() {
        assert!(ephemeral_project_path(
            "/home/vimes/projects/x/scratchpad/a5-demo-repo"
        ));
        assert!(ephemeral_project_path("/home/vimes/work/a5-scratch-repo"));
        assert!(ephemeral_project_path("/home/vimes/work/dbgtest-q1h13fpg"));
    }

    #[test]
    fn selftest_named_project_is_ephemeral_in_any_case() {
        assert!(ephemeral_project_path("/home/vimes/SelfTest-run"));
        assert!(ephemeral_project_path("/home/vimes/repo/selftest"));
    }
}
