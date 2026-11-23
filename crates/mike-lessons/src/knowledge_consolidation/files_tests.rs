use super::files::{EditError, remove_passage, reinsert, reinsert_into, removal_span};

fn offsets(body: &str, passage: &str) -> anyhow::Result<(usize, usize)> {
    let start = body
        .find(passage)
        .ok_or_else(|| anyhow::anyhow!("passage not in body"))?;
    Ok((start, start + passage.len()))
}

fn removed(body: &str, passage: &str) -> anyhow::Result<String> {
    let (start, end) = offsets(body, passage)?;
    let (from, to) = removal_span(body.as_bytes(), start, end, passage)
        .ok_or_else(|| anyhow::anyhow!("not removable"))?;
    Ok(format!("{}{}", &body[..from], &body[to..]))
}

#[test]
fn removing_a_whole_bullet_takes_its_line() -> anyhow::Result<()> {
    let body = "# Rules\n- Run clippy.\n- Use jj.\n- Test it.\n";
    assert_eq!(removed(body, "Use jj.")?, "# Rules\n- Run clippy.\n- Test it.\n");
    Ok(())
}

#[test]
fn removing_a_passage_inside_a_sentence_leaves_the_rest_of_the_line() -> anyhow::Result<()> {
    let body = "Run clippy. Use jj. Test it.\n";
    assert_eq!(removed(body, "Use jj.")?, "Run clippy.  Test it.\n");
    Ok(())
}

#[test]
fn bytes_that_differ_from_the_passage_are_never_removed() -> anyhow::Result<()> {
    let body = "- Use git.\n";
    let (start, end) = offsets(body, "Use git.")?;
    assert_eq!(removal_span(body.as_bytes(), start, end, "Use jj."), None);
    assert_eq!(removal_span(body.as_bytes(), 0, 999, "x"), None);
    Ok(())
}

#[test]
fn remove_passage_rewrites_the_file_atomically_and_refuses_a_changed_file() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("rules.md");
    let body = "- Keep.\n- Drop me.\n";
    std::fs::write(&path, body)?;
    let (start, end) = offsets(body, "Drop me.")?;

    remove_passage(&path, start, end, "Drop me.")?;
    assert_eq!(std::fs::read_to_string(&path)?, "- Keep.\n");

    std::fs::write(&path, "- Other text entirely.\n")?;
    let result = remove_passage(&path, start, end, "Drop me.");
    assert!(matches!(result, Err(EditError::Mismatch(_))));
    assert_eq!(std::fs::read_to_string(&path)?, "- Other text entirely.\n");

    let missing = remove_passage(&dir.path().join("none.md"), 0, 1, "x");
    assert!(matches!(missing, Err(EditError::Io(_, _))));
    Ok(())
}

#[test]
fn a_removal_is_reinserted_at_its_offset_only_while_the_surrounding_bytes_match() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("rules.md");
    let body = "# Rules\n- Keep.\n- Drop me.\n- Also keep.\n";
    std::fs::write(&path, body)?;
    let (start, end) = offsets(body, "Drop me.")?;
    let record = remove_passage(&path, start, end, "Drop me.")?;
    assert_eq!(record.removed, "- Drop me.\n");

    reinsert(&record)?;
    assert_eq!(std::fs::read_to_string(&path)?, body);

    remove_passage(&path, start, end, "Drop me.")?;
    std::fs::write(&path, "# Rules\n- Keep.\n- Something else now.\n")?;
    assert!(matches!(reinsert(&record), Err(EditError::Mismatch(_))));
    assert_eq!(std::fs::read_to_string(&path)?, "# Rules\n- Keep.\n- Something else now.\n");
    assert_eq!(reinsert_into(b"", &record), None);
    Ok(())
}
