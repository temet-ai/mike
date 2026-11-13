pub fn if_none_match_contains(if_none_match: &str, etag: &str) -> bool {
    fn strip_weak(s: &str) -> &str {
        let s = s.trim();
        s.strip_prefix("W/").unwrap_or(s)
    }
    let want = strip_weak(etag);
    if_none_match
        .split(',')
        .map(str::trim)
        .any(|candidate| candidate == "*" || strip_weak(candidate) == want)
}
