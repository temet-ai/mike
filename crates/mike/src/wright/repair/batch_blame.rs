fn module_path(file: &str) -> Option<String> {
    let rest = file.split_once("/src/")?.1.strip_suffix(".rs")?;
    let mut segments: Vec<&str> = rest.split('/').collect();
    if segments.last() == Some(&"mod") {
        segments.pop();
    }
    (segments.len() >= 2).then(|| segments.join("::"))
}

fn crate_dir(file: &str) -> Option<&str> {
    file.strip_prefix("crates")?
        .strip_prefix('/')?
        .split('/')
        .next()
}

fn names_file(output: &str, file: &str) -> bool {
    output.contains(file) || module_path(file).is_some_and(|module| output.contains(&module))
}

fn strong(output: &str, files: &[String]) -> bool {
    files.iter().any(|file| names_file(output, file))
}

fn weak(output: &str, files: &[String], all: &[Vec<String>]) -> bool {
    files.iter().filter_map(|file| crate_dir(file)).any(|name| {
        let alone = all
            .iter()
            .filter(|other| other.iter().any(|f| crate_dir(f) == Some(name)))
            .count()
            == 1;
        alone && output.contains(&format!("crates/{name}/"))
    })
}

pub(super) fn attribute(output: &str, fixes: &[Vec<String>]) -> Vec<usize> {
    let strong_hits: Vec<usize> = fixes
        .iter()
        .enumerate()
        .filter(|(_, files)| strong(output, files))
        .map(|(index, _)| index)
        .collect();
    if !strong_hits.is_empty() {
        return strong_hits;
    }
    fixes
        .iter()
        .enumerate()
        .filter(|(_, files)| weak(output, files, fixes))
        .map(|(index, _)| index)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::attribute;

    fn files(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|p| (*p).to_owned()).collect()
    }

    #[test]
    fn a_file_path_in_the_output_names_its_fix() {
        let fixes = vec![
            files(&["crates/mike/src/wright/repair/attempt.rs"]),
            files(&["crates/mike/src/api/wright/checks.rs"]),
        ];
        let output = "error[E0308]\n --> crates/mike/src/api/wright/checks.rs:12:5";
        assert_eq!(attribute(output, &fixes), vec![1]);
    }

    #[test]
    fn a_test_module_path_names_the_fix_that_changed_its_file() {
        let fixes = vec![
            files(&["crates/mike/src/wright/repair/attempt.rs"]),
            files(&["crates/mike/src/api/wright/checks.rs"]),
        ];
        let output = "FAIL [ 0.1s] mike wright::repair::attempt::tests::batches_one";
        assert_eq!(attribute(output, &fixes), vec![0]);
    }

    #[test]
    fn a_crate_path_alone_names_a_fix_only_when_one_fix_touches_that_crate() {
        let one = vec![
            files(&["crates/mike-base/src/a/b.rs"]),
            files(&["crates/mike/src/c/d.rs"]),
        ];
        let output = "error in crates/mike-base/src/elsewhere.rs";
        assert_eq!(attribute(output, &one), vec![0]);
        let two = vec![
            files(&["crates/mike/src/a/b.rs"]),
            files(&["crates/mike/src/c/d.rs"]),
        ];
        let output = "error in crates/mike/src/elsewhere.rs";
        assert!(attribute(output, &two).is_empty());
    }

    #[test]
    fn output_that_names_nothing_blames_nobody() {
        let fixes = vec![files(&["crates/mike/src/wright/repair/attempt.rs"])];
        assert!(attribute("gitleaks found a secret", &fixes).is_empty());
    }
}
