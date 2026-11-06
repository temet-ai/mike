use super::step_class::{
    Authorisation, DirectiveInForce, JC03_CLASSES, PENDING_JC03_CLASSES, StepClass, authorise,
    classify_ending,
};

fn directive(id: i64, class: &str, polarity: &str) -> DirectiveInForce {
    DirectiveInForce {
        id,
        class: class.to_owned(),
        polarity: polarity.to_owned(),
        text: format!("{polarity} {class}"),
        operator_turn_id: Some(1),
    }
}

#[test]
fn shall_i_commit_is_commit() {
    assert_eq!(
        classify_ending("Tests pass. Shall I commit this?"),
        Some(StepClass::Commit)
    );
}

#[test]
fn want_me_to_push_is_push() {
    assert_eq!(
        classify_ending("Want me to push the branch?"),
        Some(StepClass::Push)
    );
}

#[test]
fn shall_i_open_a_pr_is_pr() {
    assert_eq!(
        classify_ending("Shall I open a PR for it?"),
        Some(StepClass::Pr)
    );
}

#[test]
fn force_push_is_boundary() {
    let class = classify_ending("Should I force-push to fix the history?");
    assert_eq!(class, Some(StepClass::Destructive));
    assert!(class.is_some_and(StepClass::is_boundary));
}

#[test]
fn needs_your_token_is_credentials() {
    assert_eq!(
        classify_ending("Want me to retry once you paste the API key token?"),
        Some(StepClass::Credentials)
    );
}

#[test]
fn post_to_github_is_external() {
    assert_eq!(
        classify_ending("Should I post a comment on the issue?"),
        Some(StepClass::External)
    );
}

#[test]
fn unknown_class_closes_gate() {
    assert_eq!(
        classify_ending("Shall I think about it more?"),
        Some(StepClass::Unknown)
    );
    assert!(StepClass::Unknown.is_boundary());
    assert_eq!(classify_ending("The build is green."), None);
}

#[test]
fn step_classes_use_jc03_vocabulary() {
    let all = [
        StepClass::Commit,
        StepClass::Push,
        StepClass::Merge,
        StepClass::Deploy,
        StepClass::Pr,
        StepClass::Proceed,
        StepClass::FileIssue,
        StepClass::FixFound,
        StepClass::PickOption,
        StepClass::RunCommand,
    ];
    for class in all {
        let code = class.directive_class().unwrap_or_default();
        assert!(
            JC03_CLASSES.contains(&code) || PENDING_JC03_CLASSES.contains(&code),
            "{code}"
        );
    }
}

#[test]
fn forbid_directive_closes_g3() {
    let d = [directive(1, "commit", "forbid")];
    assert_eq!(
        authorise(StepClass::Commit, &d),
        Authorisation::Forbidden(vec![1])
    );
}

#[test]
fn ask_first_closes_g3() {
    let d = [
        directive(2, "ask_first", "forbid"),
        directive(3, "commit", "allow"),
    ];
    assert_eq!(
        authorise(StepClass::Commit, &d),
        Authorisation::Forbidden(vec![2])
    );
}

#[test]
fn proceed_allow_opens_g3_for_non_boundary_class() {
    let d = [directive(4, "proceed", "allow")];
    assert_eq!(
        authorise(StepClass::Merge, &d),
        Authorisation::Authorised(vec![4])
    );
    assert_eq!(
        authorise(StepClass::Destructive, &d),
        Authorisation::NotAuthorised
    );
}

#[test]
fn pr_class_matches_jc03_pr() {
    assert_eq!(StepClass::Pr.directive_class(), Some("pr"));
    let d = [directive(5, "pr", "allow")];
    assert_eq!(
        authorise(StepClass::Pr, &d),
        Authorisation::Authorised(vec![5])
    );
}

#[test]
fn allow_of_other_class_does_not_open_g3() {
    let d = [directive(6, "push", "allow")];
    assert_eq!(
        authorise(StepClass::Commit, &d),
        Authorisation::NotAuthorised
    );
}
