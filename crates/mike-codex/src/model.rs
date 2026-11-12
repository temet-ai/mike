pub(crate) fn listed(models: &serde_json::Value, model: &str) -> bool {
    models["data"].as_array().is_some_and(|rows| {
        rows.iter().any(|row| {
            row["model"].as_str() == Some(model)
                || row["id"].as_str() == Some(model)
                || row["slug"].as_str() == Some(model)
        })
    })
}
