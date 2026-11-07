use std::path::{Path, PathBuf};

use super::declared::{DeclaredError, DeclaredSource, parse, read};

fn parse_text(text: &str) -> Result<Vec<DeclaredSource>, DeclaredError> {
    parse(
        Path::new("/repo"),
        Path::new("/repo/.mike/knowledge.json"),
        text,
    )
}

#[test]
fn parse_resolves_each_source_under_the_repository_with_its_excludes() -> anyhow::Result<()> {
    let sources = parse_text(
        r#"{"version":1,"project_uuid":"x","sources":[
            {"name":"docs","path":"docs","exclude":["knowledge"]},
            {"name":"artifacts","path":"artifacts"}]}"#,
    )?;
    assert_eq!(
        sources,
        vec![
            DeclaredSource {
                name: "docs".into(),
                root: PathBuf::from("/repo/docs"),
                excludes: vec!["knowledge".into()],
            },
            DeclaredSource {
                name: "artifacts".into(),
                root: PathBuf::from("/repo/artifacts"),
                excludes: Vec::new(),
            },
        ]
    );
    Ok(())
}

#[test]
fn parse_refuses_paths_that_leave_the_repository() {
    for text in [
        r#"{"sources":[{"name":"a","path":"/etc"}]}"#,
        r#"{"sources":[{"name":"a","path":"../elsewhere"}]}"#,
        r#"{"sources":[{"name":"a","path":"docs","exclude":["../x"]}]}"#,
    ] {
        assert!(matches!(
            parse_text(text),
            Err(DeclaredError::Invalid { .. })
        ));
    }
}

#[test]
fn parse_refuses_the_names_the_proxy_scan_owns() {
    for name in ["claude-memory", "skills", " "] {
        let text = format!(r#"{{"sources":[{{"name":"{name}","path":"docs"}}]}}"#);
        assert!(matches!(
            parse_text(&text),
            Err(DeclaredError::Invalid { .. })
        ));
    }
}

#[test]
fn parse_names_the_file_when_the_json_is_malformed() -> anyhow::Result<()> {
    let Err(error) = parse_text("{not json") else {
        anyhow::bail!("malformed JSON must be refused");
    };
    assert!(error.to_string().contains(".mike/knowledge.json"));
    Ok(())
}

#[tokio::test]
async fn read_returns_nothing_when_the_repository_declares_no_sources() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    assert!(read(&dir.path().to_string_lossy()).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn read_loads_the_declaration_from_the_repository() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    std::fs::create_dir_all(dir.path().join(".mike"))?;
    std::fs::write(
        dir.path().join(".mike/knowledge.json"),
        r#"{"sources":[{"name":"docs","path":"docs"}]}"#,
    )?;
    let sources = read(&dir.path().to_string_lossy()).await?;
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].root, dir.path().join("docs"));
    Ok(())
}
