use std::path::{Path, PathBuf};
use std::process::Command;

const STANDING_EXCLUDES: &[&str] = &["target", "node_modules", ".git", ".sqlx", "artifacts"];

const PAGE_SIZE: usize = 50;

const RENDER_BYTE_BUDGET: usize = 6 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum LiteralResolverError {
    #[error("rg binary not found on PATH: {0}")]
    RgNotFound(std::io::Error),
    #[error("rg exited with non-search error (status {status}): {stderr}")]
    RgFailed { status: i32, stderr: String },
    #[error("rg output was not valid UTF-8")]
    InvalidUtf8,
}

#[derive(Debug, Clone)]
pub struct LiteralQuery {
    pub pattern: String,
    pub scope: PathBuf,
    pub extra_excludes: Vec<String>,
    pub want_dead_code_tags: bool,
    pub page: usize,
}

impl LiteralQuery {
    pub fn new(pattern: impl Into<String>, scope: impl Into<PathBuf>) -> Self {
        Self {
            pattern: pattern.into(),
            scope: scope.into(),
            extra_excludes: Vec::new(),
            want_dead_code_tags: false,
            page: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitKind {
    StateDeclaration,
    MarkupUsage,
    CssRule,
    ConfigAssignment,
    TestFixture,
    Unclear,
}

impl HitKind {
    fn as_str(self) -> &'static str {
        match self {
            HitKind::StateDeclaration => "state-declaration",
            HitKind::MarkupUsage => "markup-usage",
            HitKind::CssRule => "css-rule",
            HitKind::ConfigAssignment => "config-assignment",
            HitKind::TestFixture => "test-fixture",
            HitKind::Unclear => "unclear",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeadCodeTag {
    ThirdPartyExample,
    FallbackDefault,
    CiRequired,
    Unclear,
}

impl DeadCodeTag {
    fn as_str(self) -> &'static str {
        match self {
            DeadCodeTag::ThirdPartyExample => "third-party-example",
            DeadCodeTag::FallbackDefault => "fallback-default",
            DeadCodeTag::CiRequired => "ci-required",
            DeadCodeTag::Unclear => "unclear",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Hit {
    pub file: PathBuf,
    pub line_no: usize,
    pub context: String,
    pub kind: HitKind,
    pub dead_code_tag: Option<DeadCodeTag>,
}

#[derive(Debug, Clone)]
pub struct FileGroup {
    pub file: PathBuf,
    pub hits: Vec<Hit>,
}

#[derive(Debug, Clone)]
pub struct LiteralAnswer {
    pub groups: Vec<FileGroup>,
    pub total_hits: usize,
    pub page: usize,
    pub page_size: usize,
}

impl LiteralAnswer {
    pub fn has_more(&self) -> bool {
        (self.page + 1) * self.page_size < self.total_hits
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "literal where-used: {} hit(s) total, page {} of {}\n",
            self.total_hits,
            self.page + 1,
            self.total_pages(),
        ));

        for group in &self.groups {
            let header = format!(
                "\n{} ({} hit(s)):\n",
                group.file.display(),
                group.hits.len()
            );
            if out.len() + header.len() > RENDER_BYTE_BUDGET {
                out.push_str("\n... truncated to fit size budget; narrow scope or page further\n");
                return out;
            }
            out.push_str(&header);

            for hit in &group.hits {
                let tag = hit
                    .dead_code_tag
                    .map(|t| format!(" dead-code={}", t.as_str()))
                    .unwrap_or_default();
                let line = format!(
                    "  L{} [{}{}]:\n{}\n",
                    hit.line_no,
                    hit.kind.as_str(),
                    tag,
                    indent(&hit.context)
                );
                if out.len() + line.len() > RENDER_BYTE_BUDGET {
                    out.push_str(
                        "\n... truncated to fit size budget; narrow scope or page further\n",
                    );
                    return out;
                }
                out.push_str(&line);
            }
        }

        if self.has_more() {
            out.push_str(&format!(
                "\n... {} more hit(s) beyond this page. Re-run with page={} to continue.\n",
                self.total_hits - (self.page + 1) * self.page_size,
                self.page + 1
            ));
        }

        out
    }

    fn total_pages(&self) -> usize {
        if self.page_size == 0 {
            1
        } else {
            self.total_hits.div_ceil(self.page_size).max(1)
        }
    }
}

fn indent(s: &str) -> String {
    s.lines()
        .map(|l| format!("    {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn classify_kind(context: &str) -> HitKind {
    let trimmed = context.trim_start();
    if trimmed.starts_with('<') || context.contains("</") {
        HitKind::MarkupUsage
    } else if trimmed.starts_with("let ")
        || trimmed.starts_with("const ")
        || trimmed.starts_with("static ")
        || trimmed.starts_with("var ")
    {
        HitKind::StateDeclaration
    } else if context.contains("#[test]")
        || context.contains("fn test_")
        || context.contains("#[cfg(test)]")
    {
        HitKind::TestFixture
    } else if context.contains('{')
        && (context.contains(':') || context.contains("--"))
        && !context.contains('=')
        && (context.contains('.') || context.contains('#'))
    {
        HitKind::CssRule
    } else if context.contains('=')
        && (context.contains("::") || context.contains('.') || context.contains('_'))
    {
        HitKind::ConfigAssignment
    } else {
        HitKind::Unclear
    }
}

fn classify_dead_code(file: &Path, context: &str) -> DeadCodeTag {
    let path_str = file.to_string_lossy();
    if path_str.contains("test") || path_str.contains("fixture") || path_str.contains("example") {
        return DeadCodeTag::ThirdPartyExample;
    }
    if path_str.contains(".github")
        || path_str.contains("ci/")
        || path_str.ends_with(".yml")
        || path_str.ends_with(".yaml")
    {
        return DeadCodeTag::CiRequired;
    }
    if context.contains(":-") {
        return DeadCodeTag::FallbackDefault;
    }
    DeadCodeTag::Unclear
}

pub fn resolve(query: &LiteralQuery) -> Result<LiteralAnswer, LiteralResolverError> {
    let mut cmd = Command::new("rg");
    cmd.arg("-n")
        .arg("-C3")
        .arg("--heading")
        .arg("--no-messages");

    for dir in STANDING_EXCLUDES {
        cmd.arg("--glob").arg(format!("!{dir}"));
    }
    for extra in &query.extra_excludes {
        cmd.arg("--glob").arg(format!("!{extra}"));
    }

    cmd.arg("--").arg(&query.pattern).arg(&query.scope);

    let output = cmd.output().map_err(LiteralResolverError::RgNotFound)?;

    if !output.status.success() && output.status.code() != Some(1) {
        return Err(LiteralResolverError::RgFailed {
            status: output.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }

    let stdout = String::from_utf8(output.stdout).map_err(|_| LiteralResolverError::InvalidUtf8)?;
    let hits = parse_rg_heading_output(&stdout, query.want_dead_code_tags);

    let total_hits = hits.len();
    let start = query.page * PAGE_SIZE;
    let page_hits: Vec<Hit> = hits.into_iter().skip(start).take(PAGE_SIZE).collect();

    Ok(LiteralAnswer {
        groups: group_by_file(page_hits),
        total_hits,
        page: query.page,
        page_size: PAGE_SIZE,
    })
}

struct BufLine {
    line_no: usize,
    is_match: bool,
    text: String,
}

fn parse_rg_heading_output(stdout: &str, want_dead_code_tags: bool) -> Vec<Hit> {
    let mut hits = Vec::new();
    let mut current_file: Option<PathBuf> = None;
    let mut block: Vec<BufLine> = Vec::new();

    let flush = |hits: &mut Vec<Hit>, file: &Option<PathBuf>, block: &mut Vec<BufLine>| {
        let Some(file) = file else {
            block.clear();
            return;
        };
        for (idx, line) in block.iter().enumerate() {
            if !line.is_match {
                continue;
            }
            let lo = idx.saturating_sub(3);
            let hi = (idx + 4).min(block.len());
            let context = block[lo..hi]
                .iter()
                .map(|l| l.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            let kind = classify_kind(&context);
            let dead_code_tag = want_dead_code_tags.then(|| classify_dead_code(file, &context));
            hits.push(Hit {
                file: file.clone(),
                line_no: line.line_no,
                context,
                kind,
                dead_code_tag,
            });
        }
        block.clear();
    };

    for raw_line in stdout.lines() {
        if raw_line.is_empty() {
            continue;
        }
        if raw_line == "--" {
            flush(&mut hits, &current_file, &mut block);
            continue;
        }
        if let Some((num_part, sep, rest)) = split_rg_line(raw_line) {
            if let Ok(line_no) = num_part.parse() {
                block.push(BufLine {
                    line_no,
                    is_match: sep == ':',
                    text: rest.to_string(),
                });
            }
        } else {
            flush(&mut hits, &current_file, &mut block);
            current_file = Some(PathBuf::from(raw_line));
        }
    }
    flush(&mut hits, &current_file, &mut block);

    hits
}

fn split_rg_line(line: &str) -> Option<(&str, char, &str)> {
    let bytes = line.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        idx += 1;
    }
    if idx == 0 || idx >= bytes.len() {
        return None;
    }
    let sep = bytes[idx] as char;
    if sep != ':' && sep != '-' {
        return None;
    }
    Some((&line[..idx], sep, &line[idx + 1..]))
}

fn group_by_file(hits: Vec<Hit>) -> Vec<FileGroup> {
    let mut groups: Vec<FileGroup> = Vec::new();
    for hit in hits {
        if let Some(last) = groups.last_mut()
            && last.file == hit.file
        {
            last.hits.push(hit);
            continue;
        }
        groups.push(FileGroup {
            file: hit.file.clone(),
            hits: vec![hit],
        });
    }
    groups
}

#[cfg(test)]
#[path = "literal_tests.rs"]
mod literal_tests;
