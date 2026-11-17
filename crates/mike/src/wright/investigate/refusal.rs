use std::path::Path;

use serde_json::Value;

pub(crate) const STDOUT_FILE: &str = "stdout.log";

pub(crate) fn api_refusal(stdout: &Path) -> Option<String> {
    let text = std::fs::read_to_string(stdout).ok()?;
    let result: Value = text
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .and_then(|line| serde_json::from_str(line).ok())?;
    let status = result.get("api_error_status")?.as_i64()?;
    let kind = result
        .get("api_error")
        .or_else(|| result.get("api_error_type"))
        .and_then(Value::as_str)
        .unwrap_or("api_error");
    let said = result.get("result").and_then(Value::as_str).unwrap_or("");
    Some(format!(
        "the Claude API refused the run ({status} {kind}): {said}"
    ))
}

#[cfg(test)]
mod tests {
    use super::api_refusal;

    fn written(text: &str) -> anyhow::Result<(tempfile::TempDir, std::path::PathBuf)> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("stdout.log");
        std::fs::write(&path, text)?;
        Ok((dir, path))
    }

    #[test]
    fn api_refusal_usage_limit_result_names_the_status_and_message() -> anyhow::Result<()> {
        let (_dir, path) = written(
            r#"{"type":"result","is_error":true,"api_error_status":429,"api_error_type":"rate_limit_error","api_error":"usage_limit_reached","result":"You've hit your weekly limit"}"#,
        )?;
        assert_eq!(
            api_refusal(&path).as_deref(),
            Some(
                "the Claude API refused the run (429 usage_limit_reached): You've hit your weekly limit"
            )
        );
        Ok(())
    }

    #[test]
    fn api_refusal_ordinary_result_or_missing_log_is_none() -> anyhow::Result<()> {
        let (_dir, path) = written(r#"{"type":"result","is_error":false,"result":"done"}"#)?;
        assert_eq!(api_refusal(&path), None);
        assert_eq!(api_refusal(&path.with_file_name("absent.log")), None);
        Ok(())
    }
}
