use std::collections::BTreeMap;

use std::time::{Duration, Instant};

use super::cache::NudgeCache;
use super::compose::compose;
use super::questions::{ONLY_NUDGE, questions};
use super::{reply_tail, state};
use crate::llm::decisions::point::Asked;
use crate::llm::decisions::{Answer, Question};
use crate::steerer::guard::label_rules::is_prod;

fn answered(p: f64) -> Asked {
    Asked::answered(BTreeMap::from([(
        ONLY_NUDGE.to_owned(),
        Answer::Noul {
            noul: p,
            confidence: None,
        },
    )]))
}

#[test]
fn questions_ask_one_noul() {
    assert_eq!(questions().len(), 1);
}

#[test]
fn state_carries_the_message_and_its_length() {
    let value = state("  no, use the other one  ", None);
    assert_eq!(value["message"], "no, use the other one");
    assert_eq!(value["message_chars"], 21);
}

#[test]
fn question_counts_a_bare_go_ahead_as_a_nudge_and_a_chosen_option_as_content() {
    let questions = questions();
    let Some(Question::Noul {
        instructions,
        criteria,
    }) = questions.get(ONLY_NUDGE)
    else {
        panic!("only_nudge must be a noul question");
    };
    assert!(instructions.contains("bare approval or go-ahead"));
    assert!(instructions.contains("\"yes\", \"do it\", \"go\""));
    assert!(instructions.contains("picks between options the assistant offered"));
    assert!(instructions.contains("state.previous_reply_tail"));
    assert!(criteria.yes.contains("bare approval or go-ahead"));
    assert!(criteria.no.contains("picks between options"));
}

#[test]
fn state_fact_is_null_without_a_previous_reply() {
    assert!(state("yes", None)["previous_reply_tail"].is_null());
    assert!(state("yes", Some("  \n "))["previous_reply_tail"].is_null());
    assert_eq!(reply_tail(None), None);
}

#[test]
fn state_fact_is_the_last_300_characters_of_the_previous_reply() {
    let reply = format!("{}{}", "a".repeat(500), "b".repeat(300));
    let value = state("yes", Some(&reply));
    assert_eq!(value["previous_reply_tail"], "b".repeat(300));
    let short = state("yes", Some("Shall I use A or B?\n"));
    assert_eq!(short["previous_reply_tail"], "Shall I use A or B?");
}

#[test]
fn cache_key_differs_by_reply_tail_and_ignores_message_padding() {
    let a = NudgeCache::key("yes", Some("A or B?"));
    assert_ne!(a, NudgeCache::key("yes", Some("Shall I continue?")));
    assert_ne!(a, NudgeCache::key("yes", None));
    assert_eq!(a, NudgeCache::key("  yes\n", Some("A or B?")));
}

#[test]
fn cache_gives_the_second_asker_the_first_answer_until_it_expires() {
    let cache = NudgeCache::new(Duration::from_secs(60), 10);
    let key = NudgeCache::key("yes", Some("Continue?"));
    let start = Instant::now();
    assert_eq!(cache.get(key, start), None);
    cache.put(key, true, start);
    assert_eq!(cache.get(key, start + Duration::from_secs(59)), Some(true));
    assert_eq!(cache.get(key, start + Duration::from_secs(60)), None);
    assert!(cache.is_empty());
}

#[test]
fn cache_stays_within_its_bound() {
    let cache = NudgeCache::new(Duration::from_secs(60), 3);
    let start = Instant::now();
    for key in 0..3 {
        cache.put(key, true, start);
    }
    assert_eq!(cache.len(), 3);
    cache.put(10, false, start + Duration::from_secs(61));
    assert_eq!(cache.len(), 1);
    for key in 11..20 {
        cache.put(key, true, start + Duration::from_secs(61));
        assert!(cache.len() <= 3);
    }
}

#[test]
fn compose_short_message_with_content_is_not_a_prod_even_though_the_regex_says_so() {
    assert!(is_prod("no, use the other"));
    let Some(verdict) = compose(&answered(0.1)) else {
        panic!("an answered question must compose");
    };
    assert!(!verdict.is_prod);
    assert!((verdict.score() - 0.9).abs() < 1e-9);
}

#[test]
fn compose_long_nudge_is_a_prod_even_though_the_regex_says_it_is_not() {
    let message = "please keep going with whatever you were doing before";
    assert!(!is_prod(message));
    let Some(verdict) = compose(&answered(0.95)) else {
        panic!("an answered question must compose");
    };
    assert!(verdict.is_prod);
}

#[test]
fn compose_failed_call_gives_the_off_path_result() {
    assert_eq!(compose(&Asked::failed_with("timeout")), None);
}
