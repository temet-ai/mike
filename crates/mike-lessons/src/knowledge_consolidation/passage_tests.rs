use super::passage::{covered_share, locate};

#[test]
fn locates_a_verbatim_passage_with_its_byte_offsets() {
    let body = "# Rules\n\nAlways run clippy.\nNever force-push.\n";
    let Some(found) = locate(body, "Never force-push.") else {
        panic!("passage not located");
    };
    assert_eq!(&body[found.start..found.end], "Never force-push.");
}

#[test]
fn matches_across_changed_whitespace_and_offsets_stay_in_the_original() {
    let body = "Intro\n\n- Run   the tests\n  before   every commit.\n- Other.\n";
    let Some(found) = locate(body, "Run the tests before every commit.") else {
        panic!("passage not located");
    };
    assert_eq!(&body[found.start..found.end], "Run   the tests\n  before   every commit.");
}

#[test]
fn offsets_are_byte_positions_after_multibyte_characters() {
    let body = "Héllo wörld — keep this.\nSecond line: use jj.\n";
    let Some(found) = locate(body, "Second line: use jj.") else {
        panic!("passage not located");
    };
    assert_eq!(&body.as_bytes()[found.start..found.end], b"Second line: use jj.");
    assert!(body.is_char_boundary(found.start) && body.is_char_boundary(found.end));
}

#[test]
fn a_passage_that_is_not_in_the_body_or_is_blank_is_not_found() {
    assert_eq!(locate("Use jj.", "Use git."), None);
    assert_eq!(locate("Use jj.", "   \n "), None);
    assert_eq!(locate("", "Use jj."), None);
}

#[test]
fn passage_with_leading_and_trailing_whitespace_is_trimmed() {
    let body = "A. Use jj. B.";
    let Some(found) = locate(body, "  Use jj.\n") else {
        panic!("passage not located");
    };
    assert_eq!(&body[found.start..found.end], "Use jj.");
}

#[test]
fn covered_share_is_one_for_an_empty_answer_and_a_fraction_otherwise() {
    assert!((covered_share(0, 0) - 1.0).abs() < f64::EPSILON);
    assert!((covered_share(3, 5) - 0.6).abs() < 1e-9);
    assert!(covered_share(2, 5) < 0.6);
}

#[test]
fn a_long_body_is_cut_on_heading_boundaries_and_every_byte_is_covered_once() {
    let mut body = String::new();
    for n in 0..30 {
        body.push_str(&format!("# Section {n}\n"));
        body.push_str(&"line of text that fills the section.\n".repeat(10));
    }
    let parts = super::passage::sections(&body, 1000);
    assert!(parts.len() > 3);
    let joined: String = parts.iter().map(|(_, text)| *text).collect();
    assert_eq!(joined, body);
    let mut at = 0;
    for (offset, text) in &parts {
        assert_eq!(*offset, at);
        assert!(text.chars().count() <= 1100, "{}", text.chars().count());
        assert!(text.starts_with('#'), "section does not start on a heading");
        at += text.len();
    }
    assert_eq!(super::passage::sections("short", 1000), vec![(0, "short")]);
}

#[test]
fn a_long_body_without_headings_is_cut_on_line_boundaries() {
    let body = "one line here.\n".repeat(200);
    let parts = super::passage::sections(&body, 500);
    assert!(parts.len() > 4);
    assert_eq!(parts.iter().map(|(_, t)| *t).collect::<String>(), body);
    assert!(parts.iter().all(|(_, t)| t.ends_with('\n')));
}
