use std::collections::{HashMap, HashSet};
use std::ops::Range;

use ast_grep_config::RuleConfig;
use ast_grep_core::tree_sitter::{LanguageExt, StrDoc};
use ast_grep_core::{AstGrep, NodeMatch};
use ast_grep_language::SupportLang;

use super::classify::{in_ranges, is_test_path, test_ranges};
use super::names;
use super::spec::{Matcher, Normalize, Spec, WiringRules};
use super::{MAX_WIRING_BYTES, to_u32};
use crate::SourceFile;
use crate::language::language_for;

pub(super) struct Declaration {
    pub(super) spec: usize,
    pub(super) name: String,
    pub(super) aliases: Vec<String>,
    pub(super) file: usize,
    pub(super) range: Range<usize>,
    pub(super) line: u32,
}

impl Declaration {
    pub(super) fn contains(&self, reference: &Reference) -> bool {
        reference.file == self.file && self.range.contains(&reference.offset)
    }
}

#[derive(Clone, Copy)]
pub(super) struct Reference {
    pub(super) file: usize,
    pub(super) offset: usize,
}

#[derive(Default)]
pub(super) struct References {
    pub(super) by_name: HashMap<String, Vec<Reference>>,
    pub(super) prefixes: HashSet<String>,
}

impl References {
    fn add(&mut self, name: String, reference: Reference, normalize: Normalize) {
        if normalize == Normalize::Route && name.ends_with('/') {
            self.prefixes.insert(name.clone());
        }
        self.by_name.entry(name).or_default().push(reference);
    }
}

pub(super) struct Index {
    pub(super) declarations: Vec<Declaration>,
    pub(super) references: Vec<References>,
}

struct Tree {
    language: SupportLang,
    root: AstGrep<StrDoc<SupportLang>>,
    tests: Vec<Range<usize>>,
}

impl Tree {
    fn parse(file: &SourceFile) -> Option<Tree> {
        let language = language_for(&file.path, file.content.as_bytes())?;
        let root = language.ast_grep(&file.content);
        let tests = if language == SupportLang::Rust {
            test_ranges(&root.root())
        } else {
            Vec::new()
        };
        Some(Tree {
            language,
            root,
            tests,
        })
    }
}

struct LazyTree<'a> {
    file: &'a SourceFile,
    tree: Option<Option<Tree>>,
}

impl LazyTree<'_> {
    fn get(&mut self) -> Option<&Tree> {
        let file = self.file;
        self.tree.get_or_insert_with(|| Tree::parse(file)).as_ref()
    }
}

pub(super) fn build(rules: &WiringRules, files: &[SourceFile]) -> Index {
    let mut index = Index {
        declarations: Vec::new(),
        references: rules.specs.iter().map(|_| References::default()).collect(),
    };
    for (file_index, file) in files.iter().enumerate() {
        if is_test_path(&file.path) || file.content.len() > MAX_WIRING_BYTES {
            continue;
        }
        index_file(rules, file_index, file, &mut index);
    }
    index
}

fn index_file(rules: &WiringRules, file_index: usize, file: &SourceFile, index: &mut Index) {
    let mut lazy = LazyTree { file, tree: None };
    for (spec_index, spec) in rules.specs.iter().enumerate() {
        declarations_in(
            spec_index,
            spec,
            file_index,
            &mut lazy,
            &mut index.declarations,
        );
        if let Some(references) = index.references.get_mut(spec_index) {
            references_in(spec, file_index, &mut lazy, references);
        }
    }
}

fn declarations_in(
    spec_index: usize,
    spec: &Spec,
    file_index: usize,
    lazy: &mut LazyTree<'_>,
    out: &mut Vec<Declaration>,
) {
    let declare = &spec.declare;
    let file = lazy.file;
    if !declare.paths.admits(&file.path) {
        return;
    }
    let Some(tree) = lazy.get() else { return };
    if tree.language != declare.rule.language {
        return;
    }
    for node in tree.root.root().find_all(&declare.rule.matcher) {
        let range = node.range();
        if in_ranges(&tree.tests, range.start) {
            continue;
        }
        let Some(raw) = captured(&node, declare.capture.as_deref()) else {
            continue;
        };
        let Some(found) = names::declared(&raw, spec.normalize, &spec.aliases, &file.content)
        else {
            continue;
        };
        if declare.except_names.contains(&found.name) {
            continue;
        }
        out.push(Declaration {
            spec: spec_index,
            name: found.name,
            aliases: found.aliases,
            file: file_index,
            range,
            line: to_u32(node.start_pos().line() + 1),
        });
    }
}

fn references_in(
    spec: &Spec,
    file_index: usize,
    lazy: &mut LazyTree<'_>,
    references: &mut References,
) {
    for usage in &spec.uses {
        if !usage.paths.admits(&lazy.file.path) {
            continue;
        }
        match &usage.matcher {
            Matcher::Ast { rule, capture } => {
                let sink = Sink {
                    normalize: spec.normalize,
                    file_index,
                };
                ast_references(rule, capture.as_deref(), &sink, lazy, references);
            }
            Matcher::Text(pattern) => {
                text_references(pattern, spec.normalize, file_index, lazy.file, references);
            }
        }
    }
}

struct Sink {
    normalize: Normalize,
    file_index: usize,
}

fn ast_references(
    rule: &RuleConfig<SupportLang>,
    capture: Option<&str>,
    sink: &Sink,
    lazy: &mut LazyTree<'_>,
    references: &mut References,
) {
    let Some(tree) = lazy.get() else { return };
    if tree.language != rule.language {
        return;
    }
    for node in tree.root.root().find_all(&rule.matcher) {
        let offset = node.range().start;
        if in_ranges(&tree.tests, offset) {
            continue;
        }
        if let Some(raw) = captured(&node, capture) {
            let reference = Reference {
                file: sink.file_index,
                offset,
            };
            references.add(
                names::clean(&raw, sink.normalize),
                reference,
                sink.normalize,
            );
        }
    }
}

fn text_references(
    pattern: &regex::Regex,
    normalize: Normalize,
    file_index: usize,
    file: &SourceFile,
    references: &mut References,
) {
    let mut line_start = 0;
    for line in file.content.split_inclusive('\n') {
        if !names::is_comment_line(line) {
            let prepared = names::prepare_line(line, normalize);
            for found in pattern.captures_iter(&prepared) {
                let Some(value) = found.get(1) else { continue };
                let reference = Reference {
                    file: file_index,
                    offset: line_start + value.start(),
                };
                references.add(
                    names::clean(value.as_str(), normalize),
                    reference,
                    normalize,
                );
            }
        }
        line_start += line.len();
    }
}

fn captured(node: &NodeMatch<'_, StrDoc<SupportLang>>, capture: Option<&str>) -> Option<String> {
    match capture {
        Some(name) => node
            .get_env()
            .get_match(name)
            .map(|found| found.text().into_owned()),
        None => Some(node.text().into_owned()),
    }
}
