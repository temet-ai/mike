use super::source_tree::SourceFile;

struct RawElementRule {
    needle: &'static str,
    label: &'static str,
    canonical_import: &'static str,
}

const RAW_ELEMENT_RULES: &[RawElementRule] = &[RawElementRule {
    needle: "<button",
    label: "raw <button>",
    canonical_import: "$lib/components/ui/button",
}];

pub struct PrimitiveFinding {
    pub file: String,
    pub raw_element: &'static str,
    pub canonical_equivalent: &'static str,
}

pub fn check_current_primitives(
    files: &[SourceFile],
    surface_prefix: &str,
) -> Vec<PrimitiveFinding> {
    let surface_files: Vec<&SourceFile> = files
        .iter()
        .filter(|f| f.path.starts_with(surface_prefix))
        .collect();
    let surface_has_any_canonical_import = surface_files
        .iter()
        .any(|f| f.content.contains("$lib/components/ui/"));
    if surface_has_any_canonical_import {
        return Vec::new();
    }

    let mut findings = Vec::new();
    for f in surface_files {
        if !f.path.ends_with(".svelte") {
            continue;
        }
        for rule in RAW_ELEMENT_RULES {
            if f.content.contains(rule.needle) {
                findings.push(PrimitiveFinding {
                    file: f.path.clone(),
                    raw_element: rule.label,
                    canonical_equivalent: rule.canonical_import,
                });
            }
        }
    }
    findings
}

#[cfg(test)]
pub mod tests {
    use std::path::{Path, PathBuf};

    use super::super::source_tree::read_source_tree;
    use super::*;

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn file(path: &str, content: &str) -> SourceFile {
        SourceFile {
            path: path.to_owned(),
            content: content.to_owned(),
        }
    }

    #[test]
    fn primitives_check_flags_a_raw_button_with_no_canonical_import() {
        let files = vec![file(
            "frontend/src/lib/comms/CommsThing.svelte",
            "<script>let x = 1;</script>\n<button onclick={x}>go</button>",
        )];
        let findings = check_current_primitives(&files, "frontend/src/lib/comms/");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].file, "frontend/src/lib/comms/CommsThing.svelte");
        assert_eq!(
            findings[0].canonical_equivalent,
            "$lib/components/ui/button"
        );
    }

    #[test]
    fn primitives_check_passes_a_file_that_imports_the_canonical_wrapper() {
        let files = vec![file(
            "frontend/src/lib/shell/TopBar.svelte",
            "<script>import Button from '$lib/components/ui/button';</script>\n<Button>go</Button>",
        )];
        assert!(check_current_primitives(&files, "frontend/src/lib/shell/").is_empty());
    }

    #[test]
    fn live_primitives_check_passes_the_current_comms_surface() {
        let files = read_source_tree(&repo_root());
        assert!(
            !files.is_empty(),
            "expected to read real files under frontend/src"
        );
        let findings = check_current_primitives(&files, "frontend/src/lib/comms/");
        let files_at_fault: Vec<&str> = findings.iter().map(|f| f.file.as_str()).collect();
        assert!(
            files_at_fault.is_empty(),
            "the comms surface hand-rolls a raw element the canonical set covers: {files_at_fault:?}"
        );
    }

    #[test]
    fn live_primitives_check_passes_the_shell_surface() {
        let files = read_source_tree(&repo_root());
        let findings = check_current_primitives(&files, "frontend/src/lib/components/shell/");
        assert!(
            findings.is_empty(),
            "expected the shell surface to pass (found: {:?})",
            findings.iter().map(|f| &f.file).collect::<Vec<_>>()
        );
    }
}
