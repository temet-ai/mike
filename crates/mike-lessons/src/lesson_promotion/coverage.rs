use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::model::{LessonRow, MARKER, STANDING_FILE};

pub(super) const MIN_UNIT_CHARS: usize = 30;
pub(super) const MAX_UNITS: usize = 400;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Unit {
    pub path: String,
    pub text: String,
}

fn list_item(line: &str) -> Option<&str> {
    let mut characters = line.char_indices();
    let (_, first) = characters.next()?;
    if matches!(first, '-' | '*' | '+') {
        return line[1..].strip_prefix(' ').map(str::trim);
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let rest = &line[digits..];
    rest.strip_prefix(['.', ')'])
        .and_then(|after| after.strip_prefix(' '))
        .map(str::trim)
}

fn flush(units: &mut Vec<String>, current: &mut String) {
    if current.chars().count() >= MIN_UNIT_CHARS {
        units.push(std::mem::take(current));
    } else {
        current.clear();
    }
}

pub(super) fn split_units(text: &str) -> Vec<String> {
    let mut units = Vec::new();
    let mut current = String::new();
    let mut fenced = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            flush(&mut units, &mut current);
            fenced = !fenced;
        } else if fenced || trimmed.starts_with('#') || trimmed.starts_with("<!--") {
            flush(&mut units, &mut current);
        } else if trimmed.is_empty() {
            flush(&mut units, &mut current);
        } else if let Some(item) = list_item(trimmed) {
            flush(&mut units, &mut current);
            current.push_str(item);
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(trimmed);
        }
    }
    flush(&mut units, &mut current);
    units
}

fn markdown_files(directory: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| Some(entry.ok()?.path()))
        .collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            markdown_files(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "md") {
            found.push(path);
        }
    }
}

fn hand_written_files(checkout: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = ["CLAUDE.md", "AGENTS.md"]
        .iter()
        .map(|name| checkout.join(name))
        .filter(|path| path.is_file())
        .collect();
    markdown_files(&checkout.join(".claude/rules"), &mut files);
    files.retain(|path| path != &checkout.join(STANDING_FILE));
    files
}

pub(super) fn hand_written_units(checkout: &Path, chunk_chars: usize) -> Vec<Unit> {
    let mut units = Vec::new();
    for file in hand_written_files(checkout) {
        let Ok(text) = fs::read_to_string(&file) else {
            continue;
        };
        if text.contains(MARKER) {
            continue;
        }
        let relative = file
            .strip_prefix(checkout)
            .unwrap_or(&file)
            .display()
            .to_string();
        for unit in split_units(&text) {
            units.push(Unit {
                path: relative.clone(),
                text: unit.chars().take(chunk_chars.max(MIN_UNIT_CHARS)).collect(),
            });
        }
    }
    units.truncate(MAX_UNITS);
    units
}

pub(super) fn cosine(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let (mut dot, mut norm_a, mut norm_b) = (0.0_f64, 0.0_f64, 0.0_f64);
    for (x, y) in a.iter().zip(b) {
        let (x, y) = (f64::from(*x), f64::from(*y));
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a.sqrt() * norm_b.sqrt())
    }
}

pub(super) struct Embedded<'a> {
    pub units: &'a [Unit],
    pub unit_vectors: &'a [Vec<f32>],
    pub lesson_vectors: &'a BTreeMap<i64, Vec<f32>>,
}

pub(super) fn covered_lessons(
    lessons: &[LessonRow],
    embedded: &Embedded<'_>,
    threshold: f64,
) -> BTreeMap<i64, String> {
    let mut covered = BTreeMap::new();
    for lesson in lessons {
        let Some(vector) = embedded.lesson_vectors.get(&lesson.id) else {
            continue;
        };
        let best = embedded
            .units
            .iter()
            .zip(embedded.unit_vectors)
            .map(|(unit, unit_vector)| (unit, cosine(vector, unit_vector)))
            .filter(|(_, score)| *score >= threshold)
            .max_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((unit, _)) = best {
            covered.insert(lesson.id, unit.path.clone());
        }
    }
    covered
}

#[cfg(test)]
#[path = "coverage_tests.rs"]
mod coverage_tests;
