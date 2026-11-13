use super::reduction::{REDUCTION_MARKER_TAG, ReductionShape, build_reduction, is_signal_line};

fn shape(head: usize, tail: usize) -> ReductionShape {
    ReductionShape {
        head_lines: head,
        tail_lines: tail,
        signal_context_lines: 1,
        signal_line_cap: 40,
    }
}

fn numbered(count: usize) -> Vec<String> {
    (1..=count)
        .map(|i| format!("plain output line {i}"))
        .collect()
}

#[test]
fn build_reduction_keeps_head_tail_and_signal_lines_with_context() -> anyhow::Result<()> {
    let mut lines = numbered(100);
    lines[49] = "error[E0308]: mismatched types".to_string();
    let content = lines.join("\n");
    let reduction = build_reduction(&content, &shape(5, 5))
        .ok_or_else(|| anyhow::anyhow!("a 100-line output must reduce"))?;
    let kept: Vec<&str> = reduction.text.split('\n').collect();
    assert_eq!(kept[0], "plain output line 1");
    assert_eq!(kept[4], "plain output line 5");
    assert!(kept[5].starts_with(REDUCTION_MARKER_TAG));
    assert!(kept[5].ends_with("output lines 6-49 omitted (44 lines)"));
    assert_eq!(kept[6], "error[E0308]: mismatched types");
    assert_eq!(kept[7], "plain output line 51");
    assert!(kept[8].ends_with("output lines 52-95 omitted (44 lines)"));
    assert_eq!(kept.last().copied(), Some("plain output line 100"));
    assert_eq!(reduction.signal_lines, 1);
    assert_eq!(reduction.omitted_ranges, 2);
    assert_eq!(reduction.kept_lines, 12);
    assert_eq!(reduction.kept_ranges, "1-5,50-51,96-100");
    Ok(())
}

#[test]
fn build_reduction_short_output_returns_none() {
    let content = numbered(10).join("\n");
    assert!(build_reduction(&content, &shape(5, 5)).is_none());
}

#[test]
fn build_reduction_signal_cap_bounds_the_kept_signal_lines() -> anyhow::Result<()> {
    let lines: Vec<String> = (1..=100).map(|i| format!("test case_{i} FAILED")).collect();
    let mut capped = shape(2, 2);
    capped.signal_line_cap = 3;
    capped.signal_context_lines = 0;
    let reduction = build_reduction(&lines.join("\n"), &capped)
        .ok_or_else(|| anyhow::anyhow!("must reduce"))?;
    assert_eq!(reduction.signal_lines, 3);
    assert_eq!(reduction.kept_lines, 7);
    Ok(())
}

#[test]
fn is_signal_line_matches_errors_failures_and_summaries_not_plain_text() {
    for line in [
        "error: could not compile `mike`",
        "test result: FAILED. 3 passed; 1 failed",
        "thread 'main' panicked at src/lib.rs:4:5",
        "  --> crates/mike/src/lib.rs:10:5",
        "Traceback (most recent call last):",
        "ls: cannot access 'x': No such file or directory",
        "warning: unused variable",
    ] {
        assert!(is_signal_line(line), "{line}");
    }
    for line in [
        "src/compress/mod.rs",
        "   Compiling mike v0.1.0",
        "M  crates/mike/Cargo.toml",
    ] {
        assert!(!is_signal_line(line), "{line}");
    }
}
