use std::collections::BTreeSet;
use std::sync::LazyLock;

use regex::Regex;

pub(crate) const REDUCTION_MARKER_TAG: &str = "[[mike:shell_output_reduced]]";

const SIGNAL_PATTERN: &str = r"(?i)(\berrors?\b|error\[|\bfailed\b|\bfailures?\b|\bFAIL\b|\bpanic|\bfatal\b|exception|traceback|\bwarning\b|assert|denied|not found|no such file|cannot |could not|undefined|unresolved|timed out|\bkilled\b|\baborted?\b|^test result:|\b\d+ (passed|failed)\b|^\s*--> |exit code|^\s*finished\b|^\s*summary)";

static SIGNAL: LazyLock<Regex> = LazyLock::new(|| match Regex::new(SIGNAL_PATTERN) {
    Ok(re) => re,
    Err(e) => panic!("invalid static signal pattern: {e}"),
});

pub(super) struct ReductionShape {
    pub(super) head_lines: usize,
    pub(super) tail_lines: usize,
    pub(super) signal_context_lines: usize,
    pub(super) signal_line_cap: usize,
}

pub(super) struct Reduction {
    pub(super) text: String,
    pub(super) result_lines: usize,
    pub(super) kept_lines: usize,
    pub(super) kept_ranges: String,
    pub(super) signal_lines: usize,
    pub(super) omitted_ranges: usize,
}

pub(crate) fn is_signal_line(line: &str) -> bool {
    SIGNAL.is_match(line)
}

fn kept_indices(lines: &[&str], shape: &ReductionShape) -> (BTreeSet<usize>, usize) {
    let count = lines.len();
    let tail_start = count
        .saturating_sub(shape.tail_lines)
        .max(shape.head_lines.min(count));
    let mut keep: BTreeSet<usize> = (0..shape.head_lines.min(count)).collect();
    keep.extend(tail_start..count);
    let mut signals = 0;
    for (index, line) in lines
        .iter()
        .enumerate()
        .take(tail_start)
        .skip(shape.head_lines)
    {
        if signals >= shape.signal_line_cap {
            break;
        }
        if is_signal_line(line) {
            keep.extend(index..(index + 1 + shape.signal_context_lines).min(count));
            signals += 1;
        }
    }
    (keep, signals)
}

fn gap_marker(first: usize, last: usize) -> String {
    format!(
        "{REDUCTION_MARKER_TAG} output lines {}-{} omitted ({} lines)",
        first + 1,
        last + 1,
        last - first + 1
    )
}

pub(super) fn build_reduction(content: &str, shape: &ReductionShape) -> Option<Reduction> {
    let lines: Vec<&str> = content.split('\n').collect();
    if lines.len() <= shape.head_lines + shape.tail_lines {
        return None;
    }
    let (keep, signal_lines) = kept_indices(&lines, shape);
    let mut out: Vec<String> = Vec::with_capacity(keep.len() + 8);
    let mut gap_start: Option<usize> = None;
    let mut omitted_ranges = 0;
    for (index, line) in lines.iter().enumerate() {
        if !keep.contains(&index) {
            gap_start.get_or_insert(index);
            continue;
        }
        if let Some(first) = gap_start.take() {
            out.push(gap_marker(first, index - 1));
            omitted_ranges += 1;
        }
        out.push((*line).to_string());
    }
    if let Some(first) = gap_start {
        out.push(gap_marker(first, lines.len() - 1));
        omitted_ranges += 1;
    }
    if omitted_ranges == 0 {
        return None;
    }
    Some(Reduction {
        text: out.join("\n"),
        result_lines: lines.len(),
        kept_lines: keep.len(),
        kept_ranges: kept_ranges(&keep),
        signal_lines,
        omitted_ranges,
    })
}

fn kept_ranges(keep: &BTreeSet<usize>) -> String {
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for &index in keep {
        match ranges.last_mut() {
            Some((_, last)) if *last + 1 == index => *last = index,
            _ => ranges.push((index, index)),
        }
    }
    ranges
        .iter()
        .map(|(first, last)| format!("{}-{}", first + 1, last + 1))
        .collect::<Vec<_>>()
        .join(",")
}
