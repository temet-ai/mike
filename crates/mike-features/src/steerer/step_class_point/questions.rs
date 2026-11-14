use std::collections::BTreeMap;

use crate::llm::decisions::Question;
use crate::llm::decisions::point::choice;

pub const POINT: &str = "step_class";
pub const QUESTION_SET: &str = "step_class_v1";
pub const CLASS: &str = "class";

pub const CLASSES: &[(&str, &str)] = &[
    (
        "commit",
        "The session asks whether it may make a git commit.",
    ),
    (
        "push",
        "The session asks whether it may push commits to a remote. A phrase such as push back is not this.",
    ),
    (
        "merge",
        "The session asks whether it may merge a branch or pull request.",
    ),
    (
        "deploy",
        "The session asks whether it may deploy, release or roll something out.",
    ),
    (
        "pr",
        "The session asks whether it may open or raise a pull request.",
    ),
    (
        "proceed",
        "The session asks whether it may carry on with the work in general.",
    ),
    (
        "file_issue",
        "The session asks whether it may file an issue.",
    ),
    (
        "fix_found",
        "The session asks whether it may fix a problem it found.",
    ),
    (
        "pick_option",
        "The session asks the operator to choose between options or approaches.",
    ),
    (
        "run_command",
        "The session asks whether it may run a command or script.",
    ),
    (
        "credentials",
        "The session needs a credential, login, token or secret that only the operator holds.",
    ),
    (
        "physical",
        "The session needs the operator to do something on a device or in the physical world.",
    ),
    (
        "destructive",
        "The session asks whether it may delete, overwrite, force-push or otherwise destroy something.",
    ),
    (
        "external",
        "The session asks whether it may contact, post to or spend money with someone outside.",
    ),
    ("unknown", "None of the other classes fits the sentence."),
];

pub fn questions() -> BTreeMap<String, Question> {
    BTreeMap::from([(
        CLASS.to_owned(),
        choice(
            "Which action is the session asking permission for in state.sentence? Use state.turn_end for context.",
            CLASSES,
        ),
    )])
}
