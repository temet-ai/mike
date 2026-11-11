use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;

pub(super) const MIN_RUN_WORDS: usize = 8;
pub(super) const MIN_RUN_CHARS: usize = 40;
pub(super) const SHOWN_CHARS: usize = 1_500;
pub(super) const SHOWN_PER_EVENT: usize = 10;

pub(super) struct ShownText {
    pub(super) reference: String,
    pub(super) text: String,
}

pub(super) struct WindowTurn {
    pub(super) answer: String,
    pub(super) prompt: String,
}

pub(super) fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn run_hash(run: &[String]) -> u64 {
    let mut hasher = DefaultHasher::new();
    run.hash(&mut hasher);
    hasher.finish()
}

fn run_hashes(words: &[String]) -> HashSet<u64> {
    words.windows(MIN_RUN_WORDS).map(run_hash).collect()
}

struct TurnRuns {
    answer: HashSet<u64>,
    prompt: HashSet<u64>,
}

impl TurnRuns {
    fn of(turn: &WindowTurn) -> Self {
        Self {
            answer: run_hashes(&words(&turn.answer)),
            prompt: run_hashes(&words(&turn.prompt)),
        }
    }

    fn quotes(&self, shown: &[String]) -> bool {
        if self.answer.is_empty() {
            return false;
        }
        let hashes: Vec<u64> = shown.windows(MIN_RUN_WORDS).map(run_hash).collect();
        let in_answer: Vec<bool> = hashes
            .iter()
            .map(|hash| self.answer.contains(hash))
            .collect();
        let in_prompt: Vec<bool> = hashes
            .iter()
            .map(|hash| self.prompt.contains(hash))
            .collect();
        let lengths: Vec<usize> = shown.iter().map(|word| word.chars().count()).collect();
        common_runs(&in_answer)
            .into_iter()
            .filter(|run| !in_prompt[run.clone()].contains(&true))
            .any(|run| run_chars(&lengths, &run) >= MIN_RUN_CHARS)
    }
}

fn common_runs(in_answer: &[bool]) -> Vec<Range<usize>> {
    let mut runs = Vec::new();
    let mut position = 0;
    while position < in_answer.len() {
        if !in_answer[position] {
            position += 1;
            continue;
        }
        let start = position;
        while position < in_answer.len() && in_answer[position] {
            position += 1;
        }
        runs.push(start..position);
    }
    runs
}

fn run_chars(lengths: &[usize], run: &Range<usize>) -> usize {
    let words = &lengths[run.start..run.end + MIN_RUN_WORDS - 1];
    words.iter().sum::<usize>() + words.len() - 1
}

pub(super) fn quoted_references(shown: &[ShownText], turns: &[WindowTurn]) -> Vec<String> {
    let runs: Vec<TurnRuns> = turns.iter().map(TurnRuns::of).collect();
    shown
        .iter()
        .take(SHOWN_PER_EVENT)
        .filter(|shown| {
            let text: String = shown.text.chars().take(SHOWN_CHARS).collect();
            let shown_words = words(&text);
            runs.iter().any(|turn| turn.quotes(&shown_words))
        })
        .map(|shown| shown.reference.clone())
        .collect()
}
