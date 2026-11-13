use anyhow::Context;
use bytes::Bytes;
use serde_json::Value;

use super::CorrectionRequest;

const PROMPT_WITH_EXCERPT: &str =
    "Your previous response used \"{word}\" in: \"{excerpt}\". Rewrite it without \"{word}\".";

fn original_body() -> anyhow::Result<Bytes> {
    let body = serde_json::json!({
        "model": "claude-opus-4-5",
        "max_tokens": 1024,
        "messages": [{"role": "user", "content": "Hello"}],
    });
    Ok(Bytes::from(serde_json::to_vec(&body)?))
}

fn rendered_correction(prompt: &str, response: &str, word: &str) -> anyhow::Result<String> {
    let request = CorrectionRequest::new(
        original_body()?,
        response.to_owned(),
        prompt.to_owned(),
        word.to_owned(),
    );
    let json: Value = serde_json::from_slice(&request.build_body()?)?;
    json["messages"][2]["content"]
        .as_str()
        .map(str::to_owned)
        .context("correction content is not a string")
}

#[test]
fn build_body_prompt_with_excerpt_leaves_no_placeholder() -> anyhow::Result<()> {
    let text = rendered_correction(
        PROMPT_WITH_EXCERPT,
        "Intro line.\nThis is a comprehensive approach to it. Tail sentence.",
        "comprehensive",
    )?;
    assert!(
        !text.contains('{') && !text.contains('}'),
        "placeholder survived: {text}"
    );
    Ok(())
}

#[test]
fn build_body_excerpt_is_the_sentence_holding_the_match() -> anyhow::Result<()> {
    let text = rendered_correction(
        PROMPT_WITH_EXCERPT,
        "Intro line.\nThis is a comprehensive approach to it. Tail sentence.",
        "comprehensive",
    )?;
    assert_eq!(
        text,
        "Your previous response used \"comprehensive\" in: \"This is a comprehensive approach to it.\". Rewrite it without \"comprehensive\"."
    );
    Ok(())
}

#[test]
fn build_body_excerpt_keeps_original_casing_of_the_match() -> anyhow::Result<()> {
    let text = rendered_correction("{excerpt}", "A Comprehensive plan!", "comprehensive")?;
    assert_eq!(text, "A Comprehensive plan!");
    Ok(())
}

#[test]
fn build_body_excerpt_without_a_match_is_the_opening_sentence() -> anyhow::Result<()> {
    let text = rendered_correction("{excerpt}", "no trigger here", "comprehensive")?;
    assert_eq!(text, "no trigger here");
    Ok(())
}

#[test]
fn build_body_excerpt_is_bounded_around_the_match() -> anyhow::Result<()> {
    let response = format!("{}robust{}", "a".repeat(500), "b".repeat(500));
    let text = rendered_correction("{excerpt}", &response, "robust")?;
    assert_eq!(
        text,
        format!("{}robust{}", "a".repeat(200), "b".repeat(200))
    );
    Ok(())
}

#[test]
fn build_body_excerpt_handles_multibyte_text() -> anyhow::Result<()> {
    let response = format!("{}ROBUST{}", "é".repeat(300), "ü".repeat(300));
    let text = rendered_correction("{excerpt}", &response, "robust")?;
    assert_eq!(
        text,
        format!("{}ROBUST{}", "é".repeat(200), "ü".repeat(200))
    );
    Ok(())
}

#[test]
fn build_body_does_not_expand_placeholders_inside_the_excerpt() -> anyhow::Result<()> {
    let text = rendered_correction(
        "{word}: {excerpt}",
        "It is robust and uses {word} literally.",
        "robust",
    )?;
    assert_eq!(text, "robust: It is robust and uses {word} literally.");
    Ok(())
}

#[test]
fn build_body_prompt_with_word_only_still_renders() -> anyhow::Result<()> {
    let text = rendered_correction("Do not use '{word}'.", "text", "robust")?;
    assert_eq!(text, "Do not use 'robust'.");
    Ok(())
}

#[test]
fn build_body_rejects_unknown_placeholder_naming_it() -> anyhow::Result<()> {
    let request = CorrectionRequest::new(
        original_body()?,
        "text".to_owned(),
        "Fix {word} in {passage}.".to_owned(),
        "robust".to_owned(),
    );
    let Err(error) = request.build_body() else {
        anyhow::bail!("unknown placeholder was accepted");
    };
    assert!(
        format!("{error:#}").contains("{passage}"),
        "error does not name the placeholder: {error:#}"
    );
    Ok(())
}

#[test]
fn build_body_leaves_json_style_braces_alone() -> anyhow::Result<()> {
    let text = rendered_correction(
        "Reply as {\"fixed\": true} without {word}.",
        "text",
        "robust",
    )?;
    assert_eq!(text, "Reply as {\"fixed\": true} without robust.");
    Ok(())
}
