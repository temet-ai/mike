use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::error::LaunchError;

pub use crate::config::launch::DEFAULT_BRIEF_INLINE_MAX;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptSource {
    File(PathBuf),
    Stdin,
}

impl PromptSource {
    pub fn parse(value: &str) -> Self {
        if value == "-" {
            PromptSource::Stdin
        } else {
            PromptSource::File(PathBuf::from(value))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BriefMode {
    Inline,
    File,
    Note,
}

impl BriefMode {
    pub fn as_str(self) -> &'static str {
        match self {
            BriefMode::Inline => "inline",
            BriefMode::File => "file",
            BriefMode::Note => "note",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedBrief {
    pub mode: BriefMode,
    pub delivered: String,
    pub path: Option<PathBuf>,
    /// The note_uuid the brief becomes in workspace.session_notes. Set
    /// whenever the launch's session is known and the ledger is reachable,
    /// for inline briefs too, so every spawned prompt is findable as a note.
    pub note: Option<Uuid>,
}

#[derive(Debug, Clone, Copy)]
pub struct BriefStaging<'a> {
    pub briefs_dir: &'a Path,
    pub inline_max: usize,
    pub notes: bool,
    pub by_mail: bool,
    pub who: Option<&'a str>,
}

pub fn read_prompt(source: &PromptSource, stdin: &mut dyn Read) -> Result<String, LaunchError> {
    let mut text = String::new();
    match source {
        PromptSource::Stdin => {
            stdin
                .read_to_string(&mut text)
                .map_err(|e| LaunchError::io("reading the prompt from stdin", e))?;
        }
        PromptSource::File(path) => {
            text = std::fs::read_to_string(path).map_err(|e| {
                LaunchError::io(format!("reading --prompt-file {}", path.display()), e)
            })?;
        }
    }
    let trimmed = text.trim_end_matches(['\n', '\r']);
    if trimmed.is_empty() {
        return Err(LaunchError::Usage(match source {
            PromptSource::Stdin => "--prompt-file -: stdin was empty".to_string(),
            PromptSource::File(p) => format!("--prompt-file {}: the file is empty", p.display()),
        }));
    }
    Ok(trimmed.to_string())
}

pub fn briefs_dir(
    xdg_state_home: Option<&str>,
    home: Option<&str>,
) -> Result<PathBuf, LaunchError> {
    let base = match xdg_state_home.filter(|v| !v.is_empty()) {
        Some(xdg) => PathBuf::from(xdg),
        None => PathBuf::from(home.filter(|v| !v.is_empty()).ok_or_else(|| {
            LaunchError::Failed(
                "neither XDG_STATE_HOME nor HOME is set; cannot place the brief".into(),
            )
        })?)
        .join(".local")
        .join("state"),
    };
    Ok(base.join("mike").join("briefs"))
}

pub fn default_briefs_dir() -> Result<PathBuf, LaunchError> {
    let dir = briefs_dir(
        crate::config::env_text("XDG_STATE_HOME").as_deref(),
        crate::config::env_text("HOME").as_deref(),
    )?;
    crate::config::paths::live_install_guard::refuse(&dir);
    Ok(dir)
}

/// Stage a brief for delivery. `session` is the launch's session uuid when
/// it is known; `file_id` names the fallback file when it is not.
pub fn stage(
    prompt: &str,
    session: Option<Uuid>,
    file_id: Uuid,
    staging: &BriefStaging<'_>,
) -> Result<StagedBrief, LaunchError> {
    let note = session.filter(|_| staging.notes).map(|_| Uuid::new_v4());
    let chars = prompt.chars().count();
    if chars <= staging.inline_max {
        return Ok(StagedBrief {
            mode: BriefMode::Inline,
            delivered: prompt.to_string(),
            path: None,
            note,
        });
    }
    match (session, note) {
        (Some(session), Some(note)) if staging.by_mail => Ok(StagedBrief {
            mode: BriefMode::Note,
            delivered: note_pointer(staging.who, session, note),
            path: None,
            note: Some(note),
        }),
        (Some(session), Some(note)) if super::seat::brief_is_delivered(prompt) => Ok(StagedBrief {
            mode: BriefMode::Inline,
            delivered: read_pointer(staging.who, session, note),
            path: None,
            note: Some(note),
        }),
        _ => Ok(StagedBrief {
            note,
            ..stage_file(prompt, file_id, staging.briefs_dir, staging.who)?
        }),
    }
}

pub struct FileFallback<'a> {
    pub file_id: Uuid,
    pub briefs_dir: &'a Path,
    pub who: Option<&'a str>,
}

pub fn demote_to_file(
    staged: &StagedBrief,
    prompt: &str,
    fallback: &FileFallback<'_>,
) -> Result<StagedBrief, LaunchError> {
    match staged.mode {
        BriefMode::Note => stage_file(prompt, fallback.file_id, fallback.briefs_dir, fallback.who),
        BriefMode::Inline if super::seat::brief_is_delivered(prompt) => {
            stage_file(prompt, fallback.file_id, fallback.briefs_dir, fallback.who)
        }
        BriefMode::Inline | BriefMode::File => Ok(StagedBrief {
            note: None,
            ..staged.clone()
        }),
    }
}

fn ascii_lead(who: Option<&str>) -> String {
    match who.map(str::trim) {
        Some(name) if !name.is_empty() && name.is_ascii() => format!("{name}. "),
        _ => String::new(),
    }
}

fn note_pointer(who: Option<&str>, session: Uuid, note: Uuid) -> String {
    format!(
        "{}Your brief is your mike session note {note}. Read it now in full with the mike tool \
         list_session_notes (session_id={session}, kind=\"brief\") and follow it exactly. mike \
         also mails it to you as message_uuid={note}; that mail counts as read once it reaches \
         you, so there is nothing to acknowledge. Do not wait for confirmation; begin as soon as \
         you have read it.",
        ascii_lead(who)
    )
}

fn read_pointer(who: Option<&str>, session: Uuid, note: Uuid) -> String {
    format!(
        "{}Your brief is your mike session note {note}. Read it now in full with the mike tool \
         list_session_notes (session_id={session}, kind=\"brief\") and follow it exactly. Do not \
         wait for confirmation; begin as soon as you have read it.",
        ascii_lead(who)
    )
}

fn stage_file(
    prompt: &str,
    file_id: Uuid,
    briefs_dir: &Path,
    who: Option<&str>,
) -> Result<StagedBrief, LaunchError> {
    let path = briefs_dir.join(format!("{file_id}.md"));
    write_private(briefs_dir, &path, prompt)?;
    Ok(StagedBrief {
        mode: BriefMode::File,
        delivered: format!(
            "{}Your brief is saved at {} - read that file now in full with your Read tool and \
             follow it exactly. Do not wait for confirmation; begin as soon as you have read it.",
            ascii_lead(who),
            path.display()
        ),
        path: Some(path),
        note: None,
    })
}

fn write_private(dir: &Path, path: &Path, text: &str) -> Result<(), LaunchError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(dir)
        .map_err(|e| LaunchError::io(format!("creating {}", dir.display()), e))?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| LaunchError::io(format!("securing {}", dir.display()), e))?;
    std::fs::write(path, text)
        .map_err(|e| LaunchError::io(format!("writing {}", path.display()), e))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| LaunchError::io(format!("securing {}", path.display()), e))
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
mod tests {
    use std::io::Cursor;
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[test]
    fn read_prompt_stdin_strips_trailing_newlines() {
        let mut stdin = Cursor::new("fix it\n\n");
        assert_eq!(
            read_prompt(&PromptSource::Stdin, &mut stdin).unwrap(),
            "fix it"
        );
    }

    #[test]
    fn read_prompt_empty_stdin_is_usage_error() {
        let mut stdin = Cursor::new("\n");
        let err = read_prompt(&PromptSource::Stdin, &mut stdin).unwrap_err();
        assert!(matches!(err, LaunchError::Usage(_)), "{err}");
    }

    #[test]
    fn read_prompt_missing_file_is_io_error() {
        let mut stdin = Cursor::new("");
        let err = read_prompt(
            &PromptSource::File("/nonexistent/brief.md".into()),
            &mut stdin,
        )
        .unwrap_err();
        assert!(matches!(err, LaunchError::Io { .. }), "{err}");
        assert_eq!(err.exit_code(), 1);
    }

    #[test]
    fn read_prompt_file_reads_contents() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("b.md");
        std::fs::write(&file, "hello {{session_id}}\n").unwrap();
        let mut stdin = Cursor::new("ignored");
        assert_eq!(
            read_prompt(&PromptSource::File(file), &mut stdin).unwrap(),
            "hello {{session_id}}"
        );
    }

    #[test]
    fn prompt_source_dash_is_stdin() {
        assert_eq!(PromptSource::parse("-"), PromptSource::Stdin);
        assert_eq!(
            PromptSource::parse("x.md"),
            PromptSource::File("x.md".into())
        );
    }

    #[test]
    fn briefs_dir_prefers_xdg_then_home_then_errors() {
        assert_eq!(
            briefs_dir(Some("/xdg"), Some("/h")).unwrap(),
            PathBuf::from("/xdg/mike/briefs")
        );
        assert_eq!(
            briefs_dir(None, Some("/h")).unwrap(),
            PathBuf::from("/h/.local/state/mike/briefs")
        );
        assert_eq!(
            briefs_dir(Some(""), Some("/h")).unwrap(),
            PathBuf::from("/h/.local/state/mike/briefs")
        );
        assert!(briefs_dir(None, None).is_err());
    }

    fn staging(dir: &Path, notes: bool) -> BriefStaging<'_> {
        BriefStaging {
            briefs_dir: dir,
            inline_max: DEFAULT_BRIEF_INLINE_MAX,
            notes,
            by_mail: notes,
            who: Some("sim-developer-T03"),
        }
    }

    #[test]
    fn stage_799_chars_is_inline_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let prompt = "x".repeat(799);
        let id = Uuid::new_v4();
        let staged = stage(&prompt, Some(id), id, &staging(dir.path(), false)).unwrap();
        assert_eq!(staged.mode, BriefMode::Inline);
        assert_eq!(staged.delivered, prompt);
        assert_eq!(staged.path, None);
        assert_eq!(staged.note, None);
        assert!(!dir.path().join("mike").exists());
    }

    #[test]
    fn stage_inline_with_a_known_session_and_ledger_still_mints_a_note() {
        let dir = tempfile::tempdir().unwrap();
        let id = Uuid::new_v4();
        let staged = stage("fix it", Some(id), id, &staging(dir.path(), true)).unwrap();
        assert_eq!(staged.mode, BriefMode::Inline);
        assert_eq!(staged.delivered, "fix it");
        assert!(staged.note.is_some());
    }

    #[test]
    fn stage_long_brief_with_a_known_session_is_a_note_pointer_and_writes_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let briefs = dir.path().join("briefs");
        let prompt = "y".repeat(801);
        let session = Uuid::new_v4();
        let staged = stage(&prompt, Some(session), session, &staging(&briefs, true)).unwrap();
        assert_eq!(staged.mode, BriefMode::Note);
        assert_eq!(staged.path, None);
        assert!(!briefs.exists(), "note mode must not write a brief file");
        let note = staged.note.unwrap();
        assert!(
            staged
                .delivered
                .starts_with("sim-developer-T03. Your brief is your mike session note")
        );
        assert!(!staged.delivered.contains("characters"));
        assert!(staged.delivered.contains(&format!("message_uuid={note}")));
        assert!(staged.delivered.contains(&format!("session_id={session}")));
        assert!(!staged.delivered.contains("ack_session_message"));
        assert!(staged.delivered.contains("list_session_notes"));
        assert!(staged.delivered.is_ascii(), "{}", staged.delivered);
    }

    #[test]
    fn stage_801_chars_without_a_ledger_writes_0600_file_in_0700_dir_and_points_to_it() {
        let dir = tempfile::tempdir().unwrap();
        let briefs = dir.path().join("briefs");
        let prompt = "y".repeat(801);
        let session = Uuid::new_v4();
        let staged = stage(&prompt, Some(session), session, &staging(&briefs, false)).unwrap();
        assert_eq!(staged.mode, BriefMode::File);
        assert_eq!(staged.note, None);
        let path = staged.path.clone().unwrap();
        assert_eq!(path, briefs.join(format!("{session}.md")));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), prompt);
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(&briefs).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert!(
            staged
                .delivered
                .starts_with("sim-developer-T03. Your brief is saved at ")
        );
        assert!(!staged.delivered.contains("characters"));
        assert!(staged.delivered.contains(&path.display().to_string()));
        assert!(staged.delivered.contains("read that file now in full"));
        assert!(staged.delivered.is_ascii(), "{}", staged.delivered);
    }

    #[test]
    fn stage_long_brief_with_the_mail_toggle_off_is_a_file_but_still_mints_a_note() {
        let dir = tempfile::tempdir().unwrap();
        let session = Uuid::new_v4();
        let staging = BriefStaging {
            by_mail: false,
            ..staging(dir.path(), true)
        };
        let staged = stage(&"t".repeat(900), Some(session), session, &staging).unwrap();
        assert_eq!(staged.mode, BriefMode::File);
        let note = staged.note.unwrap();
        assert!(
            !staged.delivered.contains(&note.to_string()),
            "a file pointer names no mail"
        );
    }

    #[test]
    fn stage_long_brief_with_an_unknown_session_falls_back_to_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_id = Uuid::new_v4();
        let staged = stage(&"q".repeat(900), None, file_id, &staging(dir.path(), true)).unwrap();
        assert_eq!(staged.mode, BriefMode::File);
        assert_eq!(staged.note, None);
        assert_eq!(staged.path, Some(dir.path().join(format!("{file_id}.md"))));
    }

    #[test]
    fn demote_to_file_turns_a_note_brief_into_a_file_and_drops_the_note() {
        let dir = tempfile::tempdir().unwrap();
        let prompt = "n".repeat(900);
        let session = Uuid::new_v4();
        let staged = stage(&prompt, Some(session), session, &staging(dir.path(), true)).unwrap();
        let fallback = FileFallback {
            file_id: session,
            briefs_dir: dir.path(),
            who: Some("observer"),
        };
        let demoted = demote_to_file(&staged, &prompt, &fallback).unwrap();
        assert_eq!(demoted.mode, BriefMode::File);
        assert_eq!(demoted.note, None);
        assert_eq!(
            std::fs::read_to_string(demoted.path.unwrap()).unwrap(),
            prompt
        );

        let inline = stage("short", Some(session), session, &staging(dir.path(), true)).unwrap();
        let demoted = demote_to_file(&inline, "short", &fallback).unwrap();
        assert_eq!(demoted.mode, BriefMode::Inline);
        assert_eq!(demoted.delivered, "short");
        assert_eq!(demoted.note, None);
    }

    #[test]
    fn stage_counts_characters_not_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let prompt = "é".repeat(800);
        let id = Uuid::new_v4();
        let staged = stage(&prompt, Some(id), id, &staging(dir.path(), true)).unwrap();
        assert_eq!(staged.mode, BriefMode::Inline);
    }

    #[test]
    fn a_session_with_no_name_or_a_name_the_terminal_would_mangle_gets_no_lead() {
        let dir = tempfile::tempdir().unwrap();
        let session = Uuid::new_v4();
        for who in [None, Some("  "), Some("développeur")] {
            let nameless = BriefStaging {
                who,
                ..staging(dir.path(), false)
            };
            let staged = stage(&"z".repeat(900), Some(session), session, &nameless).unwrap();
            assert!(
                staged.delivered.starts_with("Your brief is saved at "),
                "{}",
                staged.delivered
            );
        }
    }

    #[test]
    fn stage_unwritable_dir_is_io_error() {
        let err = stage(
            &"z".repeat(900),
            None,
            Uuid::new_v4(),
            &staging(Path::new("/proc/nonexistent/briefs"), false),
        )
        .unwrap_err();
        assert!(matches!(err, LaunchError::Io { .. }), "{err}");
    }
}
