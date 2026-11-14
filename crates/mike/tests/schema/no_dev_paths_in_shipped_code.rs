use std::path::{Path, PathBuf};

use crate::split_crates::src_roots;

const DEV_PATH_STARTS: [&str; 3] = ["\"/mnt/data", "\"/home/", "\"/Users/"];
const PACKAGE_PREFIXES: [&str; 1] = ["\"/home/linuxbrew/"];
const TEST_MODULE_SPAN: usize = 12;

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            rust_files(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

fn is_test_file(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    name.ends_with("_tests.rs")
        || name == "tests.rs"
        || path.components().any(|c| c.as_os_str() == "tests")
}

fn opens_inline_module(line: &str) -> bool {
    let line = line.trim();
    let line = ["pub(crate) ", "pub(super) ", "pub "]
        .iter()
        .find_map(|visibility| line.strip_prefix(visibility))
        .unwrap_or(line);
    line.starts_with("mod ") && line.ends_with('{')
}

fn shipped_lines(text: &str) -> Vec<&str> {
    let lines: Vec<&str> = text.lines().collect();
    let mut cfg_test_at: Option<usize> = None;
    for (index, line) in lines.iter().enumerate() {
        if line.trim() == "#[cfg(test)]" {
            cfg_test_at = Some(index);
        } else if opens_inline_module(line)
            && let Some(start) = cfg_test_at.filter(|start| index - start <= TEST_MODULE_SPAN)
        {
            return lines[..start].to_vec();
        }
    }
    lines
}

fn dev_path_literal(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") {
        return false;
    }
    let dev = DEV_PATH_STARTS.iter().any(|start| line.contains(start));
    let package = PACKAGE_PREFIXES.iter().any(|prefix| line.contains(prefix));
    dev && !package
}

fn offenders_in(file: &Path) -> anyhow::Result<Vec<String>> {
    let text = std::fs::read_to_string(file)?;
    Ok(shipped_lines(&text)
        .into_iter()
        .enumerate()
        .filter(|(_, line)| dev_path_literal(line))
        .map(|(index, line)| format!("{}:{}: {}", file.display(), index + 1, line.trim()))
        .collect())
}

#[test]
fn shipped_rust_names_no_developer_machine_path() -> anyhow::Result<()> {
    let mut files = Vec::new();
    for root in src_roots()? {
        rust_files(&root, &mut files)?;
    }
    anyhow::ensure!(!files.is_empty(), "no Rust sources were found to scan");
    let mut offenders = Vec::new();
    for file in files.iter().filter(|file| !is_test_file(file)) {
        offenders.extend(offenders_in(file)?);
    }
    assert!(
        offenders.is_empty(),
        "non-test code holds a string literal that starts with /mnt/data, /home/ or /Users/; derive the path from the project's stored location or the environment:\n{}",
        offenders.join("\n")
    );
    Ok(())
}

#[test]
fn the_scan_skips_test_modules_and_comments() {
    let text = "fn a() {}\n// \"/home/x\"\n#[cfg(test)]\n#[allow(\n    x,\n)]\nmod tests {\n    const P: &str = \"/home/u\";\n}\n";
    assert!(shipped_lines(text).iter().all(|l| !dev_path_literal(l)));
    assert!(dev_path_literal("let p = \"/mnt/data/projects/x\";"));
    assert!(!dev_path_literal("\"/home/linuxbrew/.linuxbrew/bin\","));
}
