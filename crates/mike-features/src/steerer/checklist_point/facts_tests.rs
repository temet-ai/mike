use super::facts::ends_with_operator_question;

#[test]
fn ends_with_operator_question_true_for_a_final_question_mark() {
    assert!(ends_with_operator_question(
        "Merged #306. Do you want the infra deploy as well?"
    ));
    assert!(ends_with_operator_question(
        "Both options are open. Which one should I take?**\n"
    ));
}

#[test]
fn ends_with_operator_question_true_for_an_offer_without_a_question_mark() {
    assert!(ends_with_operator_question(
        "The branch is pushed. Want me to open the PR."
    ));
    assert!(ends_with_operator_question(
        "Tests pass.\nShould I merge it now, or wait for the review"
    ));
}

#[test]
fn ends_with_operator_question_false_for_a_report_or_an_earlier_question() {
    assert!(!ends_with_operator_question(
        "Should I have merged it earlier? Yes, so I merged #306 and deployed omit."
    ));
    assert!(!ends_with_operator_question(
        "Merged #306 and deployed omit."
    ));
    assert!(!ends_with_operator_question(""));
}
