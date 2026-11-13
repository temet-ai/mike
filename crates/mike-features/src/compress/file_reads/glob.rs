pub fn glob_match(pattern: &str, path: &str) -> bool {
    let pattern_segments: Vec<&str> = pattern.split('/').collect();
    let path_segments: Vec<&str> = path.split('/').collect();
    match_segments(&pattern_segments, &path_segments)
}

fn match_segments(pattern: &[&str], path: &[&str]) -> bool {
    if pattern.is_empty() {
        return path.is_empty();
    }
    if pattern[0] == "**" {
        return (0..=path.len()).any(|i| match_segments(&pattern[1..], &path[i..]));
    }
    !path.is_empty()
        && segment_match(pattern[0], path[0])
        && match_segments(&pattern[1..], &path[1..])
}

fn segment_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star_p: Option<usize> = None;
    let mut star_t = 0usize;
    while ti < t.len() {
        if pi < p.len() && p[pi] == '*' {
            star_p = Some(pi);
            star_t = ti;
            pi += 1;
        } else if pi < p.len() && p[pi] == t[ti] {
            pi += 1;
            ti += 1;
        } else if let Some(sp) = star_p {
            pi = sp + 1;
            star_t += 1;
            ti = star_t;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

#[cfg(test)]
mod tests {
    use super::glob_match;

    #[test]
    fn glob_match_double_star_matches_zero_or_more_segments() {
        assert!(glob_match("**/CLAUDE.md", "CLAUDE.md"));
        assert!(glob_match("**/CLAUDE.md", "/a/b/CLAUDE.md"));
        assert!(!glob_match("**/CLAUDE.md", "/a/b/NOT_CLAUDE.md"));
    }

    #[test]
    fn glob_match_single_star_does_not_cross_segment_boundary() {
        assert!(glob_match("**/BRIEF*.md", "/a/BRIEF.md"));
        assert!(glob_match("**/BRIEF*.md", "/a/BRIEF-123.md"));
        assert!(!glob_match("**/BRIEF*.md", "/a/BRIEF/extra.md"));
    }

    #[test]
    fn glob_match_double_star_matches_nested_directory_contents() {
        assert!(glob_match("**/.claude/**", "/repo/.claude/agents/x.md"));
        assert!(glob_match("**/.claude/**", "/repo/nested/.claude/x.md"));
        assert!(!glob_match("**/.claude/**", "/repo/not-claude/x.md"));
    }
}
