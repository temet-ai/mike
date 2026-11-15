use std::collections::BTreeMap;

use super::facts::NudgeSource;
use crate::llm::decisions::Question;
use crate::llm::decisions::point::noul;

pub const POINT: &str = "verifier_precheck";
pub const QUESTION_SET: &str = "verifier_precheck_v4";

pub const UNFINISHED: &str = "ask_left_unfinished";
pub const DISCLOSED: &str = "turn_already_says_it";

pub const STILL_RUNNING: &str = "work_still_running";
pub const NOT_OPERATOR_ASK: &str = "ask_not_in_operator_words";
pub const CRITERION_GOAL: &str = "criterion_matches_operator_goal";
pub const DONE_EARLIER: &str = "done_earlier";

pub const VETOES: [&str; 4] = [STILL_RUNNING, NOT_OPERATOR_ASK, DONE_EARLIER, DISCLOSED];

fn still_running() -> Question {
    noul(
        "Does `turn_end` say the work is still under way rather than finished: a build, push, gate, agent, check or deploy is still running, or it names the next step it is about to take?",
        "turn_end reports work in progress or a next step it is about to take, not a finished result.",
        "turn_end presents the work as finished.",
    )
}

fn not_operator_ask() -> Question {
    noul(
        "Is the thing `nudge` says was asked for missing from the words in `ask`, so that it comes from machine text such as a checklist, a heading, hook output or a template?",
        "The operator never asked for what the nudge names; it comes from generated or machine text.",
        "The operator's own words ask for what the nudge names.",
    )
}

fn criterion_goal() -> Question {
    noul(
        "`nudge` quotes an acceptance criterion. Does that criterion match a goal stated in `ask`?",
        "The operator stated this goal in their own words.",
        "No goal the operator stated matches the criterion.",
    )
}

fn done_earlier() -> Question {
    noul(
        "Does turn_end say that what the nudge asks about was already done or answered before this turn, naming a commit, a time, an earlier reply or something the operator already has, or does recent_commits list a commit that did it?",
        "It was done or answered earlier, by turn_end's account or by a commit in recent_commits.",
        "Neither turn_end nor recent_commits shows it done or answered earlier.",
    )
}

fn unfinished() -> Question {
    noul(
        "Does `nudge` point at something `ask` asked for that `turn_end` does not show as done or checked?",
        "The nudge names a real part of the ask that turn_end leaves undone or unchecked.",
        "The nudge names nothing the ask needed, or turn_end shows it done and checked.",
    )
}

fn disclosed() -> Question {
    noul(
        "Does turn_end itself already tell the operator plainly what the nudge asks about: what was not done, not checked, or left for them?",
        "turn_end names the gap in words the operator cannot miss.",
        "turn_end does not name the gap the nudge points at.",
    )
}

pub fn questions_for(source: NudgeSource) -> BTreeMap<String, Question> {
    let (ask_key, ask_question) = match source {
        NudgeSource::Criteria => (CRITERION_GOAL, criterion_goal()),
        _ => (NOT_OPERATOR_ASK, not_operator_ask()),
    };
    BTreeMap::from([
        (STILL_RUNNING.to_owned(), still_running()),
        (ask_key.to_owned(), ask_question),
        (DONE_EARLIER.to_owned(), done_earlier()),
        (UNFINISHED.to_owned(), unfinished()),
        (DISCLOSED.to_owned(), disclosed()),
    ])
}

pub fn questions() -> BTreeMap<String, Question> {
    questions_for(NudgeSource::Verifier)
}
