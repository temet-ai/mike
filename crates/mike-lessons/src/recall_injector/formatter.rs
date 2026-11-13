use crate::store::persistence::lessons::LessonRow;

pub fn format_recall_reminder(topics: &[String], rows: &[LessonRow]) -> Option<String> {
    if rows.is_empty() {
        return None;
    }

    let mut out = String::from("<system-reminder>\nRecalled project memory:");

    let requested_topics: std::collections::HashSet<&str> =
        topics.iter().map(|s| s.as_str()).collect();

    for row in rows {
        let topic_label = row.topic.as_deref().unwrap_or("(unknown)").to_string();

        if row
            .topic
            .as_deref()
            .is_some_and(|t| requested_topics.contains(t))
        {
            out.push_str(&format!(
                "\n\n[topic: {}]\n({}) {}",
                topic_label, row.kind, row.text
            ));
        }
    }

    out.push_str("\n</system-reminder>");

    if out == "<system-reminder>\nRecalled project memory:\n</system-reminder>" {
        return None;
    }

    Some(out)
}

pub const MEMORY_TAG_GRAMMAR: &str = "Write <mem:recall topic=\"X\"/> in the last reply of a turn and topic X's stored text is added to your next turn; a topic made into a skill is also in this session's skill catalog. To record a rule for later sessions without a tool call, write <mem:update topic=\"X\" kind=\"correction|convention|warning|preference\">the rule</mem:update> in the last reply of a turn; it is stored as a lesson on topic X. Both are read only from that reply: text before a tool call does not carry them.";

pub fn format_topic_catalog(topics: &[String]) -> Option<String> {
    if topics.is_empty() {
        return None;
    }
    memory_note(topics, true)
}

pub fn memory_note(topics: &[String], explain: bool) -> Option<String> {
    let mut lines = Vec::new();
    if !topics.is_empty() {
        lines.push(format!(
            "Project memory topics relevant here: {}.",
            topics.join(", ")
        ));
    }
    if explain {
        lines.push(MEMORY_TAG_GRAMMAR.to_owned());
    }
    (!lines.is_empty()).then(|| {
        format!(
            "<system-reminder>\n{}\n</system-reminder>",
            lines.join("\n")
        )
    })
}

pub fn memory_note_for(session: &str, topics: &[String]) -> Option<String> {
    let fresh = crate::explained::new_topics(session, topics);
    let explain = crate::explained::first_time(session, crate::explained::MEMORY_TAGS);
    memory_note(&fresh, explain)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_row(kind: &str, text: &str, topic: Option<&str>) -> LessonRow {
        LessonRow {
            id: 1,
            project_id: Some(1),
            project_path: Some("/test".to_string()),
            kind: kind.to_string(),
            text: text.to_string(),
            agent_id: None,
            stage: "daily".to_string(),
            source_turn_id: None,
            created_at: Utc::now(),
            expires_at: None,
            topic: topic.map(str::to_string),
        }
    }

    #[test]
    fn format_recall_reminder_renders_full_text() -> anyhow::Result<()> {
        let topics = vec!["db-ports".to_string()];
        let rows = vec![
            make_row(
                "warning",
                "Use :6666, not the old bootstrap default :5440",
                Some("db-ports"),
            ),
            make_row(
                "convention",
                "Export MIKE_TEST_ADMIN_URL=.../postgres",
                Some("db-ports"),
            ),
        ];
        let result = format_recall_reminder(&topics, &rows)
            .ok_or_else(|| anyhow::anyhow!("format_recall_reminder returned None"))?;
        assert!(result.contains("Use :6666"));
        assert!(result.contains("Export MIKE_TEST_ADMIN_URL"));
        assert!(result.starts_with("<system-reminder>"));
        assert!(result.ends_with("</system-reminder>"));
        assert!(result.contains("Use :6666, not the old bootstrap default :5440"));
        Ok(())
    }

    #[test]
    fn format_recall_reminder_returns_none_for_empty_rows() {
        let topics = vec!["x".to_string()];
        assert!(format_recall_reminder(&topics, &[]).is_none());
    }

    #[test]
    fn format_recall_reminder_renders_extreme_length_full_text() -> anyhow::Result<()> {
        let long_text = "A".repeat(2000);
        let row = make_row("warning", &long_text, Some("big-topic"));
        let topics = vec!["big-topic".to_string()];
        let result = format_recall_reminder(&topics, &[row])
            .ok_or_else(|| anyhow::anyhow!("format_recall_reminder returned None"))?;
        assert!(
            result.contains(&long_text),
            "full 2000-char lesson text must be present verbatim — no truncation"
        );
        assert!(
            result.contains("[topic: big-topic]"),
            "must include topic label"
        );
        Ok(())
    }

    #[test]
    fn format_recall_reminder_skips_rows_not_matching_requested_topics() {
        let topics = vec!["alpha".to_string()];
        let rows = vec![make_row("convention", "beta content", Some("beta"))];
        assert!(format_recall_reminder(&topics, &rows).is_none());
    }

    #[test]
    fn format_topic_catalog_renders_keys() -> anyhow::Result<()> {
        let topics = vec!["a".to_string(), "b".to_string()];
        let result = format_topic_catalog(&topics)
            .ok_or_else(|| anyhow::anyhow!("format_topic_catalog returned None"))?;
        assert!(result.contains("a, b"));
        assert!(result.contains("mem:recall topic="));
        Ok(())
    }

    #[test]
    fn format_topic_catalog_none_when_empty() {
        assert!(format_topic_catalog(&[]).is_none());
    }

    #[test]
    fn memory_note_for_explains_the_tags_once_and_names_only_new_topics() {
        let topics = |t: &[&str]| t.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        let first = memory_note_for("memory-note-once", &topics(&["a"])).unwrap_or_default();
        assert!(first.contains("relevant here: a."));
        assert!(first.contains(MEMORY_TAG_GRAMMAR));
        let second = memory_note_for("memory-note-once", &topics(&["a", "b"])).unwrap_or_default();
        assert_eq!(
            second,
            "<system-reminder>\nProject memory topics relevant here: b.\n</system-reminder>"
        );
        assert!(memory_note_for("memory-note-once", &topics(&["b"])).is_none());
    }
}
