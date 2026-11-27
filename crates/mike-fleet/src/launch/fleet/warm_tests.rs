use super::{Launch, Progress, Step, watched_backend};

#[test]
fn a_template_is_watched_in_the_backend_that_launched_it() {
    assert_eq!(watched_backend(Some("herdr"), "herdr-core"), "herdr");
    assert_eq!(watched_backend(Some("herdr-core"), "herdr"), "herdr-core");
}

#[test]
fn the_configured_backend_is_watched_until_the_launch_has_a_row() {
    assert_eq!(watched_backend(None, "herdr-core"), "herdr-core");
    assert_eq!(watched_backend(Some(""), "herdr"), "herdr");
}

#[test]
fn a_template_whose_backend_never_listed_it_times_out_naming_that_backend() {
    let mut progress = Progress::default();
    for _ in 0..3 {
        assert_eq!(
            progress.step(launch("success", true), None, false),
            Step::Wait
        );
    }
    let problem = progress.timed_out("herdr-core", 480);
    assert!(
        problem.contains("herdr-core terminal backend never reported"),
        "{problem}"
    );
}

#[test]
fn a_template_seen_but_never_at_rest_times_out_as_not_loaded() {
    let mut progress = Progress::default();
    assert_eq!(
        progress.step(launch("success", true), Some("working"), false),
        Step::Wait
    );
    assert_eq!(
        progress.timed_out("herdr", 480),
        "the template had not finished loading after 480 seconds; its workers start cold"
    );
}

fn launch(result: &'static str, confirmed: bool) -> Option<Launch<'static>> {
    Some(Launch {
        result,
        confirmed,
        reason: None,
    })
}

#[test]
fn a_template_is_loaded_once_its_prompt_ran_and_its_pane_stays_at_rest() {
    let mut progress = Progress::default();
    assert_eq!(progress.step(None, None, false), Step::Wait);
    assert_eq!(
        progress.step(launch("pending", false), Some("idle"), false),
        Step::Wait
    );
    assert_eq!(
        progress.step(launch("success", true), Some("working"), false),
        Step::Wait
    );
    assert_eq!(
        progress.step(launch("success", true), Some("idle"), false),
        Step::Wait
    );
    assert_eq!(
        progress.step(launch("success", true), Some("idle"), false),
        Step::Loaded
    );
}

#[test]
fn a_pane_that_goes_back_to_work_is_not_loaded_yet() {
    let mut progress = Progress::default();
    assert_eq!(
        progress.step(launch("success", true), Some("idle"), false),
        Step::Wait
    );
    assert_eq!(
        progress.step(launch("success", true), Some("working"), false),
        Step::Wait
    );
    assert_eq!(
        progress.step(launch("success", true), Some("done"), false),
        Step::Wait
    );
    assert_eq!(
        progress.step(launch("success", true), Some("done"), false),
        Step::Loaded
    );
}

#[test]
fn an_unconfirmed_prompt_counts_only_after_the_pane_was_seen_working() {
    let mut progress = Progress::default();
    assert_eq!(
        progress.step(launch("unconfirmed", false), Some("idle"), false),
        Step::Wait
    );
    assert_eq!(
        progress.step(launch("unconfirmed", false), Some("idle"), false),
        Step::Wait
    );
    assert_eq!(
        progress.step(launch("unconfirmed", false), Some("working"), false),
        Step::Wait
    );
    assert_eq!(
        progress.step(launch("unconfirmed", false), Some("idle"), false),
        Step::Wait
    );
    assert_eq!(
        progress.step(launch("unconfirmed", false), Some("idle"), false),
        Step::Loaded
    );
}

#[test]
fn a_failed_launch_ends_the_wait_with_its_reason() {
    let mut progress = Progress::default();
    let failed = Some(Launch {
        result: "failed",
        confirmed: false,
        reason: Some("surface_cap"),
    });
    assert_eq!(
        progress.step(failed, None, false),
        Step::Failed("surface_cap".into())
    );
}

#[test]
fn a_template_with_a_request_in_flight_is_not_loaded() {
    let mut progress = Progress::default();
    assert_eq!(
        progress.step(launch("success", true), Some("idle"), false),
        Step::Wait
    );
    assert_eq!(
        progress.step(launch("success", true), Some("idle"), true),
        Step::Wait
    );
    assert_eq!(
        progress.step(launch("success", true), Some("idle"), true),
        Step::Wait
    );
    assert_eq!(
        progress.step(launch("success", true), Some("idle"), false),
        Step::Wait
    );
    assert_eq!(
        progress.step(launch("success", true), Some("idle"), false),
        Step::Loaded
    );
}
