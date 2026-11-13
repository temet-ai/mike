use crate::store::persistence::session_action_list::SessionActionRow;

pub fn build_session_action_list(rows: &[SessionActionRow]) -> String {
    if rows.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity(rows.len() * 48);
    for (i, row) in rows.iter().enumerate() {
        let detail = row
            .command
            .as_deref()
            .or(row.file_path.as_deref())
            .unwrap_or_default();
        let outcome = if !row.has_result {
            "no result"
        } else if row.result_is_error == Some(true) {
            "error"
        } else {
            "ok"
        };
        if detail.is_empty() {
            out.push_str(&format!("{}. {} → {outcome}\n", i + 1, row.tool_name));
        } else {
            out.push_str(&format!(
                "{}. {} — {detail} → {outcome}\n",
                i + 1,
                row.tool_name
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_action_row(
        tool_name: &str,
        file_path: Option<&str>,
        command: Option<&str>,
        has_result: bool,
        result_is_error: Option<bool>,
    ) -> SessionActionRow {
        SessionActionRow {
            tool_name: tool_name.to_string(),
            file_path: file_path.map(str::to_string),
            command: command.map(str::to_string),
            has_result,
            result_is_error,
            operation: None,
        }
    }

    #[test]
    fn build_session_action_list_empty_for_no_rows() {
        assert_eq!(build_session_action_list(&[]), "");
    }

    #[test]
    fn build_session_action_list_renders_every_row_no_cap() {
        let rows: Vec<SessionActionRow> = (0..40)
            .map(|i| {
                let path = format!("/src/file_{i}.rs");
                make_action_row("Read", Some(&path), None, true, Some(false))
            })
            .collect();
        let out = build_session_action_list(&rows);
        assert_eq!(
            out.lines().count(),
            40,
            "every row rendered, no row-count cap"
        );
        assert!(
            !out.contains("(truncated)"),
            "no truncation marker anywhere in the path"
        );
        assert!(out.contains("40. Read"));
    }

    #[test]
    fn build_session_action_list_shows_bash_command_and_error_outcome() {
        let rows = vec![make_action_row(
            "Bash",
            None,
            Some("cargo check"),
            true,
            Some(true),
        )];
        let out = build_session_action_list(&rows);
        assert!(out.contains("Bash — cargo check → error"));
    }

    #[test]
    fn build_session_action_list_shows_file_path_and_no_result_outcome() {
        let rows = vec![make_action_row(
            "Read",
            Some("/src/lib.rs"),
            None,
            false,
            None,
        )];
        let out = build_session_action_list(&rows);
        assert!(out.contains("Read — /src/lib.rs → no result"));
    }
}
