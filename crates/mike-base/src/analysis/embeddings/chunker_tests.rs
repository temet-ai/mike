use super::*;

#[test]
fn chunk_text_empty_returns_empty_vec() {
    assert!(chunk_text("", 1500).is_empty());
}

#[test]
fn chunk_text_short_input_returns_single_chunk() {
    let chunks = chunk_text("hello world", 1500);
    assert_eq!(chunks, vec!["hello world".to_string()]);
}

#[test]
fn chunk_text_exact_boundary_emits_single_chunk_no_trailing_empty() {
    let text = "x".repeat(1500);
    let chunks = chunk_text(&text, 1500);
    assert_eq!(
        chunks.len(),
        1,
        "exact-boundary text must NOT produce an empty trailing chunk"
    );
    assert_eq!(chunks[0].chars().count(), 1500);
}

#[test]
fn chunk_text_long_input_splits_to_under_max_chars() {
    let para = format!("{}\n\n", "word ".repeat(120));
    let text = para.repeat(15);
    let chunks = chunk_text(&text, 1500);
    assert!(
        chunks.len() >= 4,
        "expected ≥4 chunks, got {}",
        chunks.len()
    );
    for (i, c) in chunks.iter().enumerate() {
        assert!(
            c.chars().count() <= 1500,
            "chunk {i} exceeds cap: {} chars",
            c.chars().count()
        );
    }
}

#[test]
fn chunk_text_no_whitespace_falls_back_to_hard_split_at_max_chars() {
    let text = "a".repeat(3500);
    let chunks = chunk_text(&text, 1500);
    assert!(
        chunks.len() >= 2,
        "no-whitespace input must still split, got {} chunks",
        chunks.len()
    );
    for c in &chunks {
        assert!(
            c.chars().count() <= 1500,
            "hard-split chunk exceeds cap: {} chars",
            c.chars().count()
        );
    }
    let joined: String = chunks.concat();
    assert_eq!(joined, text);
}

#[test]
fn chunk_text_multibyte_utf8_never_splits_mid_codepoint() {
    let emoji = "🦀";
    assert_eq!(emoji.chars().count(), 1);
    assert_eq!(emoji.len(), 4);
    let text = emoji.repeat(2000);
    let chunks = chunk_text(&text, 1500);
    assert!(chunks.len() >= 2, "expected ≥2 chunks");
    for c in &chunks {
        assert!(c.chars().all(|ch| ch == '🦀'), "chunk corrupted: {c:?}");
        assert!(c.chars().count() <= 1500);
    }
    let joined: String = chunks.concat();
    assert_eq!(joined, text);
}

#[test]
fn chunk_text_paragraph_break_preferred_over_sentence_break() {
    let head = format!(
        "{}\n\n{}",
        "sentence one. sentence two. sentence three. ".repeat(30),
        "x".repeat(2200),
    );
    let chunks = chunk_text(&head, 1500);
    assert!(chunks.len() >= 2, "expected ≥2 chunks");
    assert!(
        !chunks[0].contains("xxxxx"),
        "first chunk leaked into the second paragraph: {:?}",
        chunks[0]
    );
}
