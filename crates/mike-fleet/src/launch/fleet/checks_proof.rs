use super::checks::Missing;
use super::definition::FleetDefinition;

const COUNT_WORDS: [&str; 10] = [
    "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "twelve",
];
const REPEAT_UNITS: [&str; 7] = [
    "run", "runs", "time", "times", "attempt", "attempts", "passes",
];
const PASS_WORDS: [&str; 5] = ["pass", "green", "succe", "clean", "work"];
const RERUN_WORDS: [&str; 6] = [
    "rerun",
    "re-run",
    "run again",
    "run it again",
    "retake",
    "retaken",
];
const LAST_STATE_WORDS: [&str; 5] = [
    "last commit",
    "final commit",
    "latest commit",
    "newest commit",
    "end of the run",
];
const EXPIRY_WORDS: [&str; 7] = [
    "stops counting",
    "stop counting",
    "expires",
    "expire",
    "goes stale",
    "go stale",
    "stale",
];
const LATER_COMMIT_WORDS: [&str; 6] = [
    "later commit",
    "any commit",
    "every commit",
    "last commit",
    "newer commit",
    "next commit",
];

pub fn repeated_proof(def: &FleetDefinition) -> Vec<Missing> {
    let mut found = Vec::new();
    for (id, criterion) in &def.criteria {
        scan(&format!("criteria.{id}.text"), &criterion.text, &mut found);
        scan(
            &format!("criteria.{id}.check"),
            &criterion.check,
            &mut found,
        );
    }
    for (id, task) in &def.tasks {
        scan(&format!("tasks.{id}.brief"), &task.brief, &mut found);
    }
    found
}

const REPORTING_WORDS: [&str; 8] = [
    "none of",
    "asks for",
    "not ask",
    "never ask",
    "lists as missing",
    "listed as missing",
    "does not count",
    "refuses",
];

fn without_quotes(text: &str) -> String {
    let mut kept = String::with_capacity(text.len());
    let mut inside = false;
    for c in text.chars() {
        if c == '"' {
            inside = !inside;
        } else if !inside {
            kept.push(c);
        }
    }
    kept
}

fn scan(item: &str, text: &str, found: &mut Vec<Missing>) {
    for sentence in sentences(&without_quotes(text)) {
        if let Some(asks) = asks_for(&sentence) {
            found.push(Missing::new(
                item,
                format!(
                    "asks for {asks}: \"{sentence}\". A test or a run is run once per state of the tree and a flaky one is fixed where it waits: ask for the cause to be found and for one run that shows it"
                ),
            ));
        }
    }
}

fn sentences(text: &str) -> Vec<String> {
    text.replace(['\n', ';'], ". ")
        .split(". ")
        .map(|s| s.trim().trim_end_matches('.').trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn any_in(text: &str, words: &[&str]) -> bool {
    words.iter().any(|w| text.contains(w))
}

fn asks_for(sentence: &str) -> Option<&'static str> {
    let lower = format!(" {} ", sentence.to_lowercase());
    if any_in(&lower, &REPORTING_WORDS) {
        return None;
    }
    if repeated_pass(&lower) {
        Some("a test or a run to pass more than once")
    } else if any_in(&lower, &RERUN_WORDS) && any_in(&lower, &LAST_STATE_WORDS) {
        Some("everything to be run again at the last commit")
    } else if expires_at_later_commit(&lower) {
        Some("evidence that stops counting at a later commit")
    } else {
        None
    }
}

fn repeated_pass(lower: &str) -> bool {
    let passes = any_in(lower, &PASS_WORDS);
    if lower.contains("in a row") || lower.contains("consecutive") {
        return passes || lower.contains("run");
    }
    passes && counts_a_repeat(lower)
}

fn counts_a_repeat(lower: &str) -> bool {
    let tokens: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric() && c != '-')
        .filter(|t| !t.is_empty())
        .collect();
    tokens.iter().enumerate().any(|(index, token)| {
        let counts = COUNT_WORDS.contains(token)
            || token.parse::<u32>().is_ok_and(|n| n >= 2)
            || *token == "twice";
        counts
            && (*token == "twice"
                || tokens
                    .iter()
                    .skip(index + 1)
                    .take(2)
                    .any(|next| REPEAT_UNITS.contains(next)))
    })
}

fn expires_at_later_commit(lower: &str) -> bool {
    if lower.contains("at or after") && lower.contains("commit") {
        return true;
    }
    any_in(lower, &EXPIRY_WORDS) && any_in(lower, &LATER_COMMIT_WORDS)
}
