use std::ops::Range;
use std::sync::LazyLock;

use ast_grep_core::Node;
use ast_grep_core::tree_sitter::StrDoc;
use ast_grep_language::SupportLang;
use regex::Regex;

const TEST_PATH_MARKERS: [&str; 26] = [
    "/tests/",
    "/test/",
    "/e2e/",
    "/fixtures/",
    "/benches/",
    "/examples/",
    "/node_modules/",
    "/tests.rs",
    "/scripts/test-",
    "test_support",
    "test_seams",
    "test_kit",
    "tests_support",
    "test_fixture",
    "test_seed",
    "tests_db",
    "_tests/",
    "_cases.",
    "/fixtures.",
    "__harness__",
    ".stories.",
    ".test.",
    ".spec.",
    "_tests.",
    "_tests_",
    "_test.",
];

static TEST_ATTRIBUTE: LazyLock<Regex> =
    LazyLock::new(
        || match Regex::new(r"^#\[(cfg\([^\]]*\btest\b|(\w+::)?test\b|rstest\b)") {
            Ok(re) => re,
            Err(e) => panic!("invalid static test-attribute pattern: {e}"),
        },
    );

pub(super) fn is_test_path(path: &str) -> bool {
    let rooted = format!("/{path}");
    TEST_PATH_MARKERS
        .iter()
        .any(|marker| rooted.contains(marker))
}

pub(super) fn test_ranges(root: &Node<'_, StrDoc<SupportLang>>) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    for attribute in root.dfs().filter(|node| node.kind() == "attribute_item") {
        if !is_test_attribute(&attribute.text()) {
            continue;
        }
        if let Some(item) = attribute.next_all().find(|node| !is_trivia(node)) {
            ranges.push(item.range());
        }
    }
    ranges
}

pub(super) fn in_ranges(ranges: &[Range<usize>], offset: usize) -> bool {
    ranges.iter().any(|range| range.contains(&offset))
}

fn is_test_attribute(text: &str) -> bool {
    TEST_ATTRIBUTE.is_match(text) && !text.contains("not(test")
}

fn is_trivia(node: &Node<'_, StrDoc<SupportLang>>) -> bool {
    matches!(
        node.kind().as_ref(),
        "attribute_item" | "line_comment" | "block_comment"
    )
}
