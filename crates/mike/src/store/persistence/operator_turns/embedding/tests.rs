use super::*;

#[test]
fn vector_to_text_literal_roundtrip_shape() {
    let s = vector_to_text_literal(&[1.0, -2.5, 0.0]);
    assert_eq!(s, "[1,-2.5,0]");
}

#[test]
fn vector_to_text_literal_handles_empty_slice() {
    let s = vector_to_text_literal(&[]);
    assert_eq!(s, "[]");
}
