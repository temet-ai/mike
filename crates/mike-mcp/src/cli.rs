use crate::tools::{Profile, tools_for_profile};

pub fn render_tools_text(profile: Profile) -> String {
    let tools = tools_for_profile(profile);
    let mut out = String::new();
    for tool in &tools {
        out.push_str(&tool.name);
        if let Some(desc) = tool.description.as_deref() {
            out.push_str(" — ");
            out.push_str(first_sentence(desc));
        }
        out.push('\n');
    }
    out.push_str(&format!("total: {} tools\n", tools.len()));
    out
}

pub fn render_tools_json(profile: Profile) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&tools_for_profile(profile))
}

fn first_sentence(desc: &str) -> &str {
    let trimmed = desc.trim();
    let bytes = trimmed.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'.' {
            match bytes.get(i + 1) {
                None => return trimmed,
                Some(next) if next.is_ascii_whitespace() => return &trimmed[..=i],
                Some(_) => {}
            }
        }
    }
    trimmed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::all_tools;

    #[test]
    fn render_tools_text_count_matches_all_tools_len() -> anyhow::Result<()> {
        let text = render_tools_text(Profile::Full);
        let lines: Vec<&str> = text.lines().collect();
        let expected = all_tools().len();

        assert_eq!(lines.len(), expected + 1, "line count mismatch:\n{text}");
        let last = lines
            .last()
            .ok_or_else(|| anyhow::anyhow!("render_tools_text produced no lines"))?;
        assert_eq!(*last, format!("total: {expected} tools"));
        Ok(())
    }

    #[test]
    fn render_tools_text_core_is_smaller_than_full() {
        let full = render_tools_text(Profile::Full);
        let core = render_tools_text(Profile::Core);
        let full_lines = full.lines().count();
        let core_lines = core.lines().count();
        assert!(
            core_lines < full_lines,
            "core preview ({core_lines} lines) must be smaller than full ({full_lines} lines)"
        );
        assert!(core.ends_with(&format!(
            "total: {} tools\n",
            tools_for_profile(Profile::Core).len()
        )));
    }

    #[test]
    fn render_tools_json_emits_every_tool() -> anyhow::Result<()> {
        let json = render_tools_json(Profile::Full)?;
        let value: serde_json::Value = serde_json::from_str(&json)?;
        let arr = value
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("expected a JSON array of tools"))?;
        assert_eq!(arr.len(), all_tools().len());
        for entry in arr {
            assert!(entry.get("name").is_some(), "tool missing name: {entry}");
            assert!(
                entry.get("inputSchema").is_some() || entry.get("input_schema").is_some(),
                "tool missing input schema: {entry}"
            );
        }
        Ok(())
    }

    #[test]
    fn first_sentence_stops_at_boundary() {
        assert_eq!(
            first_sentence("Lists things. Returns rows."),
            "Lists things."
        );
        assert_eq!(
            first_sentence("No terminal period here"),
            "No terminal period here"
        );
        assert_eq!(first_sentence("Ends with period."), "Ends with period.");
        assert_eq!(
            first_sentence("Uses v1.2 wire format. Second."),
            "Uses v1.2 wire format."
        );
    }
}
