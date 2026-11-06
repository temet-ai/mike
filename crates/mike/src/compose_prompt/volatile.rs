use std::sync::Arc;

use arc_swap::ArcSwap;
use regex::Regex;

use crate::store::persistence::PersistError;

pub(crate) use crate::store::persistence::volatile::VolatilePatternRow;

#[derive(Debug, Clone)]
pub(crate) struct VolatilePattern {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) pattern: Regex,
    pub(crate) applies_to_block_index: u8,
    pub(crate) enabled: bool,
}

pub(crate) fn strip_volatile_lines(
    text: &str,
    patterns: &[VolatilePattern],
    block_index: u8,
) -> String {
    if !patterns
        .iter()
        .any(|p| p.enabled && p.applies_to_block_index == block_index)
    {
        return text.to_string();
    }

    let mut kept: Vec<&str> = Vec::new();
    for raw_line in text.split('\n') {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        let drop = patterns.iter().any(|p| {
            p.enabled && p.applies_to_block_index == block_index && p.pattern.is_match(line)
        });
        if !drop {
            kept.push(line);
        }
    }
    kept.join("\n")
}

#[derive(Debug)]
pub(crate) struct VolatilePatternCache {
    inner: ArcSwap<Vec<VolatilePattern>>,
}

impl VolatilePatternCache {
    pub(crate) fn new(initial: Vec<VolatilePattern>) -> Self {
        Self {
            inner: ArcSwap::from_pointee(initial),
        }
    }

    pub(crate) fn empty() -> Self {
        Self::new(Vec::new())
    }

    pub(crate) fn load(&self) -> Arc<Vec<VolatilePattern>> {
        self.inner.load_full()
    }

    pub(crate) fn store(&self, new: Vec<VolatilePattern>) {
        self.inner.store(Arc::new(new));
    }
}

pub(crate) fn compile_row(row: &VolatilePatternRow) -> Result<VolatilePattern, PersistError> {
    let pattern = Regex::new(&row.pattern).map_err(|err| {
        PersistError::DataInvariant(format!(
            "volatile_line_patterns row id={id} name={name:?}: invalid regex {pat:?}: {err}",
            id = row.id,
            name = row.name,
            pat = row.pattern,
        ))
    })?;
    Ok(VolatilePattern {
        id: row.id,
        name: row.name.clone(),
        pattern,
        applies_to_block_index: row.applies_to_block_index,
        enabled: row.enabled,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pat(name: &str, regex: &str, block_index: u8, enabled: bool) -> Option<VolatilePattern> {
        Regex::new(regex).ok().map(|pattern| VolatilePattern {
            id: 0,
            name: name.to_string(),
            pattern,
            applies_to_block_index: block_index,
            enabled,
        })
    }

    fn pats(specs: &[(&str, &str, u8, bool)]) -> anyhow::Result<Vec<VolatilePattern>> {
        specs
            .iter()
            .map(|(name, regex, block, enabled)| {
                pat(name, regex, *block, *enabled)
                    .ok_or_else(|| anyhow::anyhow!("invalid test regex {regex:?}"))
            })
            .collect()
    }

    #[test]
    fn no_patterns_returns_text_unchanged() {
        let text = "line1\nline2";
        assert_eq!(strip_volatile_lines(text, &[], 2), text);
    }

    #[test]
    fn enabled_pattern_drops_matching_line() -> anyhow::Result<()> {
        let patterns = pats(&[("cc-version", r"^cc_version=", 2, true)])?;
        let text = "cc_version=abc; cch=def\nactual content\nmore content";
        let stripped = strip_volatile_lines(text, &patterns, 2);
        assert_eq!(stripped, "actual content\nmore content");
        Ok(())
    }

    #[test]
    fn disabled_pattern_does_not_drop() -> anyhow::Result<()> {
        let patterns = pats(&[("cc-version", r"^cc_version=", 2, false)])?;
        let text = "cc_version=abc\nbody";
        assert_eq!(strip_volatile_lines(text, &patterns, 2), text);
        Ok(())
    }

    #[test]
    fn pattern_targeting_other_block_does_not_apply() -> anyhow::Result<()> {
        let patterns = pats(&[("cc-version", r"^cc_version=", 0, true)])?;
        let text = "cc_version=abc\nbody";
        assert_eq!(strip_volatile_lines(text, &patterns, 2), text);
        Ok(())
    }

    #[test]
    fn handles_crlf_line_endings() -> anyhow::Result<()> {
        let patterns = pats(&[("cc-version", r"^cc_version=", 2, true)])?;
        let text = "cc_version=x\r\nactual\r\nmore";
        assert_eq!(strip_volatile_lines(text, &patterns, 2), "actual\nmore");
        Ok(())
    }

    #[test]
    fn multiple_enabled_patterns_drop_independently() -> anyhow::Result<()> {
        let patterns = pats(&[
            ("cc-version", r"^cc_version=", 2, true),
            ("build-id", r"^build_id=", 2, true),
        ])?;
        let text = "cc_version=a\nbuild_id=b\nactual";
        assert_eq!(strip_volatile_lines(text, &patterns, 2), "actual");
        Ok(())
    }

    #[test]
    fn empty_text_stays_empty() -> anyhow::Result<()> {
        let patterns = pats(&[("cc-version", r"^cc_version=", 2, true)])?;
        assert_eq!(strip_volatile_lines("", &patterns, 2), "");
        Ok(())
    }

    #[test]
    fn pattern_that_matches_every_line_drops_everything() -> anyhow::Result<()> {
        let patterns = pats(&[("any", r".*", 2, true)])?;
        let text = "a\nb\nc";
        assert_eq!(strip_volatile_lines(text, &patterns, 2), "");
        Ok(())
    }

    #[test]
    fn cache_empty_load_returns_empty_vec() {
        let cache = VolatilePatternCache::empty();
        let snap = cache.load();
        assert!(snap.is_empty());
    }

    #[test]
    fn cache_store_replaces_contents() -> anyhow::Result<()> {
        let cache = VolatilePatternCache::empty();
        let next = pats(&[("p1", r"^x=", 2, true)])?;
        cache.store(next);
        let snap = cache.load();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].name, "p1");
        Ok(())
    }

    #[test]
    fn fast_path_skips_when_no_pattern_targets_block() -> anyhow::Result<()> {
        let patterns = pats(&[("p0a", r"^a", 0, true), ("p0b", r"^b", 0, true)])?;
        let text = "a\nb\nc";
        assert_eq!(strip_volatile_lines(text, &patterns, 2), text);
        Ok(())
    }
}
