pub fn find_heading_span(text: &str, heading: &str) -> Option<(usize, usize)> {
    if heading.is_empty() {
        return None;
    }
    let mut offset = 0usize;
    let mut start: Option<usize> = None;
    for line in text.split_inclusive('\n') {
        let content = line.strip_suffix('\n').unwrap_or(line);
        let content = content.strip_suffix('\r').unwrap_or(content);
        match start {
            None => {
                if content == heading {
                    start = Some(offset);
                }
            }
            Some(s) => {
                if content.starts_with("# ") {
                    return Some((s, offset));
                }
            }
        }
        offset += line.len();
    }
    start.map(|s| (s, text.len()))
}

#[cfg(test)]
pub mod tests {
    use super::find_heading_span;

    const HEADING: &str = "# Claude in Chrome browser automation";

    #[test]
    fn span_found_mid_block_ends_at_next_heading() -> anyhow::Result<()> {
        let text = "preamble\n# Claude in Chrome browser automation\nbody line 1\nbody line 2\n# Next section\ntail";
        let (start, end) =
            find_heading_span(text, HEADING).ok_or_else(|| anyhow::anyhow!("expected a span"))?;
        assert_eq!(
            &text[start..end],
            "# Claude in Chrome browser automation\nbody line 1\nbody line 2\n"
        );
        assert!(text[end..].starts_with("# Next section"));
        Ok(())
    }

    #[test]
    fn span_at_block_end_runs_to_text_end() -> anyhow::Result<()> {
        let text = "intro\n# Claude in Chrome browser automation\nlast body line";
        let (start, end) =
            find_heading_span(text, HEADING).ok_or_else(|| anyhow::anyhow!("expected a span"))?;
        assert_eq!(end, text.len());
        assert_eq!(
            &text[start..end],
            "# Claude in Chrome browser automation\nlast body line"
        );
        Ok(())
    }

    #[test]
    fn heading_absent_returns_none() {
        assert!(find_heading_span("no such heading here\n# Other\nbody", HEADING).is_none());
    }

    #[test]
    fn empty_heading_returns_none() {
        assert!(find_heading_span("# A\nbody", "").is_none());
    }

    #[test]
    fn two_sections_only_first_matching_heading_spans() -> anyhow::Result<()> {
        let text = "# Alpha\nalpha body\n# Beta\nbeta body\n";
        let (start, end) =
            find_heading_span(text, "# Alpha").ok_or_else(|| anyhow::anyhow!("expected a span"))?;
        assert_eq!(&text[start..end], "# Alpha\nalpha body\n");
        Ok(())
    }

    #[test]
    fn duplicate_heading_first_occurrence_wins() -> anyhow::Result<()> {
        let text = "# Dup\nfirst body\n# Other\nmid\n# Dup\nsecond body\n";
        let (start, end) =
            find_heading_span(text, "# Dup").ok_or_else(|| anyhow::anyhow!("expected a span"))?;
        assert_eq!(start, 0);
        assert_eq!(&text[start..end], "# Dup\nfirst body\n");
        Ok(())
    }

    #[test]
    fn heading_must_match_full_line_not_prefix() {
        let text = "# Claude in Chrome browser automation extras\nbody";
        assert!(find_heading_span(text, HEADING).is_none());
    }

    #[test]
    fn subheadings_do_not_terminate_the_span() -> anyhow::Result<()> {
        let text = "# Top\nbody\n## Sub\nsub body\n# Next\n";
        let (start, end) =
            find_heading_span(text, "# Top").ok_or_else(|| anyhow::anyhow!("expected a span"))?;
        assert_eq!(&text[start..end], "# Top\nbody\n## Sub\nsub body\n");
        Ok(())
    }

    #[test]
    fn fenced_code_block_hash_line_terminates_span_documented_caveat() -> anyhow::Result<()> {
        let text = "# Section\nbody\n```bash\n# comment in fence\n```\ntail\n";
        let (_, end) = find_heading_span(text, "# Section")
            .ok_or_else(|| anyhow::anyhow!("expected a span"))?;
        assert!(text[end..].starts_with("# comment in fence"));
        Ok(())
    }

    #[test]
    fn crlf_heading_line_matches() -> anyhow::Result<()> {
        let text = "# Section\r\nbody\r\n# Next\r\n";
        let (start, end) = find_heading_span(text, "# Section")
            .ok_or_else(|| anyhow::anyhow!("expected a span"))?;
        assert_eq!(start, 0);
        assert_eq!(&text[start..end], "# Section\r\nbody\r\n");
        Ok(())
    }
}
