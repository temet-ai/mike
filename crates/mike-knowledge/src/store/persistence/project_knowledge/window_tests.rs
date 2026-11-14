use super::SearchWindow;

#[test]
fn window_with_dates_counts_the_whole_until_day() -> anyhow::Result<()> {
    let window =
        SearchWindow::parse(Some("2026-11-03"), Some("2026-11-04")).map_err(anyhow::Error::msg)?;
    assert_eq!(
        window.since.map(|t| t.to_rfc3339()),
        Some("2026-11-03T00:00:00+00:00".to_owned())
    );
    assert_eq!(
        window.until.map(|t| t.to_rfc3339()),
        Some("2026-11-05T00:00:00+00:00".to_owned())
    );
    Ok(())
}

#[test]
fn window_with_rfc3339_times_keeps_them() -> anyhow::Result<()> {
    let window =
        SearchWindow::parse(Some("2026-11-03T10:00:00+02:00"), None).map_err(anyhow::Error::msg)?;
    assert_eq!(
        window.since.map(|t| t.to_rfc3339()),
        Some("2026-11-03T08:00:00+00:00".to_owned())
    );
    assert_eq!(window.until, None);
    Ok(())
}

#[test]
fn window_with_a_misspelt_date_names_the_input_and_position() -> anyhow::Result<()> {
    let Err(message) = SearchWindow::parse(Some("2026/11/03"), None) else {
        anyhow::bail!("a slash date must be refused");
    };
    assert!(message.contains("\"2026/11/03\""), "{message}");
    assert!(message.contains("character 5"), "{message}");
    Ok(())
}

#[test]
fn window_with_since_after_until_is_refused() {
    assert!(SearchWindow::parse(Some("2026-11-05"), Some("2026-11-04")).is_err());
}

#[test]
fn window_with_blank_edges_is_open() {
    assert_eq!(
        SearchWindow::parse(Some(" "), None),
        Ok(SearchWindow::default())
    );
}
