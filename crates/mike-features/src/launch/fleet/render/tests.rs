use chrono::{DateTime, Duration, Utc};

use super::clock::ago;
use super::ink::{RED, Rgb, fit, paint, strip, visible_len};
use super::seats::context_bar;
use super::tests_fixture::fixture;
use super::{FleetListing, render, render_list};
use crate::launch::fleet::checks::Missing;
use crate::launch::fleet::status::{FleetStatus, SeatStatus, TaskStatus, Verdict};

pub fn at(text: &str) -> anyhow::Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(text)?.with_timezone(&Utc))
}

pub fn verdict(when: &str, word: &str, title: &str, task: &str) -> anyhow::Result<Verdict> {
    Ok(Verdict {
        at: at(when)?,
        verdict: word.to_string(),
        title: title.to_string(),
        tasks: vec![task.to_string()],
        criteria: vec![],
    })
}

pub fn task(id: &str, name: &str, discipline: &str, state: &str) -> TaskStatus {
    TaskStatus {
        id: id.to_string(),
        name: name.to_string(),
        discipline: discipline.to_string(),
        criteria: vec!["C1".to_string()],
        seat: None,
        last_seat: None,
        state: state.to_string(),
        passes: 0,
        fails: 0,
        rounds: 0,
        last: None,
        passed_criteria: vec![],
        ready: false,
    }
}

pub fn seat(name: &str, role: &str, state: &str, tokens: Option<i64>, closed: bool) -> SeatStatus {
    SeatStatus {
        seat: name.to_string(),
        role: role.to_string(),
        model: "sonnet".to_string(),
        state: state.to_string(),
        session: None,
        goal: None,
        context_tokens: tokens,
        requests: if tokens.is_some() { 400 } else { 0 },
        last_request_at: None,
        opened_at: None,
        closed,
        first_request_cache: None,
    }
}

pub fn empty(now: DateTime<Utc>) -> FleetStatus {
    FleetStatus {
        fleet: "3f0a1c52-6d2e-4a55-9b1e-0c8d7e6f5a41".to_string(),
        slug: "fleet-workspace".to_string(),
        state: "draft".to_string(),
        run_id: None,
        goal: "ship it".to_string(),
        started_at: None,
        finished_at: None,
        now,
        missing: Vec::new(),
        criteria: Vec::new(),
        tasks: Vec::new(),
        seats: Vec::new(),
        verdicts: Vec::new(),
        blocked: Vec::new(),
        needs_operator: Vec::new(),
        accepted: false,
        obligations: vec![],
        every_seat_idle_seconds: None,
        tree_broken: None,
        problems: Vec::new(),
    }
}

fn plain_lines(status: &FleetStatus, width: usize) -> Vec<String> {
    strip(&render(status, width))
        .lines()
        .map(str::to_string)
        .collect()
}

fn position(lines: &[String], needle: &str) -> anyhow::Result<usize> {
    lines
        .iter()
        .position(|l| l.contains(needle))
        .ok_or_else(|| anyhow::anyhow!("no line contains {needle:?} in:\n{}", lines.join("\n")))
}

fn seat_row<'a>(lines: &'a [String], needle: &str) -> anyhow::Result<&'a String> {
    let seats = position(lines, "SEATS")?;
    lines[seats..]
        .iter()
        .find(|l| l.contains(needle))
        .ok_or_else(|| anyhow::anyhow!("no seat row contains {needle:?}"))
}

#[test]
fn render_wide_frame_draws_every_section_in_order() -> anyhow::Result<()> {
    let lines = plain_lines(&fixture()?, 170);
    let header = position(&lines, "FLEET  fleet-workspace  running")?;
    let tasks = position(&lines, "TASKS")?;
    let seats = position(&lines, "SEATS")?;
    let latest = position(&lines, "LATEST FROM ACCEPTANCE")?;
    let blocked = position(&lines, "BLOCKED")?;
    assert_eq!(header, 0);
    assert!(header < tasks && tasks < seats && seats < latest && latest < blocked);
    assert!(lines[header].ends_with("29 Sep 01:30 UTC"));
    assert!(lines.iter().all(|l| !l.contains("MISSING")));
    Ok(())
}

#[test]
fn render_wide_frame_summarises_the_fleet_in_the_header() -> anyhow::Result<()> {
    let lines = plain_lines(&fixture()?, 170);
    assert!(lines[1].trim() == "ship it", "{}", lines[1]);
    assert!(lines[2].contains("running for 6h03m"), "{}", lines[2]);
    assert!(lines[2].contains("1 of 4 tasks accepted"), "{}", lines[2]);
    assert!(lines[2].contains("1 pass 3 fail"), "{}", lines[2]);
    assert!(
        lines[2].contains("4 seats open, 5 launched"),
        "{}",
        lines[2]
    );
    assert!(lines[3].contains("criteria"));
    assert!(
        lines[3].contains("C1 1/2 accepted, 0 ready"),
        "{}",
        lines[3]
    );
    assert!(
        lines[3].contains("C2 0/1 accepted, 0 ready"),
        "{}",
        lines[3]
    );
    Ok(())
}

#[test]
fn render_header_without_a_goal_skips_the_goal_line() -> anyhow::Result<()> {
    let mut status = fixture()?;
    status.goal = "  ".to_string();
    let lines = plain_lines(&status, 170);
    assert!(lines[1].contains("running for"), "{}", lines[1]);
    Ok(())
}

#[test]
fn render_narrow_header_cuts_a_long_goal_to_the_width() -> anyhow::Result<()> {
    let mut status = fixture()?;
    status.goal = "word ".repeat(40);
    let lines = plain_lines(&status, 40);
    assert!(visible_len(&lines[1]) <= 40, "{}", lines[1]);
    Ok(())
}

#[test]
fn render_wide_frame_draws_a_row_per_task_with_its_newest_verdict() -> anyhow::Result<()> {
    let lines = plain_lines(&fixture()?, 170);
    let p1 = &lines[position(&lines, "✔ P1")?];
    assert!(p1.contains("passed") && p1.contains("db-dev-P1") && p1.contains("1✔ 1✘"));
    assert!(p1.contains("PASS C1 P1 schema landed"));
    let p2 = &lines[position(&lines, "↻ P2")?];
    assert!(p2.contains("rework") && p2.contains("C2,C4") && p2.contains("FAIL C2 P2"));
    let p3 = &lines[position(&lines, "● P3")?];
    assert!(p3.contains("working") && p3.contains("no verdict yet"));
    let p4 = &lines[position(&lines, "○ P4")?];
    assert!(p4.contains("queued") && p4.contains("no seat yet"));
    Ok(())
}

#[test]
fn render_wide_frame_draws_open_seats_and_counts_the_closed_ones() -> anyhow::Result<()> {
    let lines = plain_lines(&fixture()?, 170);
    let seats = position(&lines, "SEATS")?;
    assert!(lines[seats + 1].contains("seat") && lines[seats + 1].contains("last request"));
    let rust = seat_row(&lines, "rust-developer-P2  ")?;
    assert!(
        rust.contains("946k of 1000k") && rust.contains("replace"),
        "{rust}"
    );
    let observer = seat_row(&lines, "observer-9")?;
    assert!(
        observer.contains("no requests") && observer.contains("never"),
        "{observer}"
    );
    let acceptance = seat_row(&lines, "acceptance  ")?;
    assert!(acceptance.contains("525k of 1000k") && acceptance.contains("2m ago"));
    assert!(!acceptance.contains("replace"));
    assert!(
        lines
            .iter()
            .all(|l| !l.contains("db-dev-P1  ") || l.contains("passed"))
    );
    assert!(lines.iter().any(|l| l.trim() == "1 closed seat not shown"));
    Ok(())
}

#[test]
fn render_wide_frame_lists_verdicts_and_blocked_seats() -> anyhow::Result<()> {
    let lines = plain_lines(&fixture()?, 170);
    let latest = position(&lines, "LATEST FROM ACCEPTANCE")?;
    assert!(lines[latest + 1].contains("01:12  FAIL  C2 P2 b39d9351"));
    assert!(lines[latest + 2].contains("00:05  PASS  C1 P1 schema landed"));
    assert!(lines[latest + 3].contains("23:09  FAIL  C1 P3 4 of 7 tests fail"));
    let blocked = position(&lines, "BLOCKED")?;
    assert!(lines[blocked + 1].contains("frontend-developer-P3  P3 progress"));
    Ok(())
}

#[test]
fn render_narrow_frame_cuts_rows_to_the_width_and_ends_them_with_an_ellipsis() -> anyhow::Result<()>
{
    let status = fixture()?;
    let lines = plain_lines(&status, 100);
    for line in &lines {
        assert!(
            line.chars().count() <= 100,
            "{} chars: {line}",
            line.chars().count()
        );
    }
    let p2 = &lines[position(&lines, "↻ P2")?];
    assert!(p2.ends_with('…'), "{p2}");
    let rust = seat_row(&lines, "rust-developer-P2  ")?;
    assert!(rust.ends_with('…'), "{rust}");
    assert!(lines[0].ends_with("29 Sep 01:30 UTC"));
    assert_eq!(lines.len(), plain_lines(&status, 170).len());
    Ok(())
}

#[test]
fn render_narrow_frame_never_splits_or_leaves_open_a_colour_code() -> anyhow::Result<()> {
    let coloured = render(&fixture()?, 100);
    for line in coloured.lines() {
        assert!(
            !strip(line).contains('\x1b'),
            "a code was split in {line:?}"
        );
        let last = line.rfind("\x1b[").map(|i| &line[i..]);
        if let Some(code) = last {
            assert!(
                code.starts_with("\x1b[0m"),
                "line ends inside a colour: {line:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn render_with_missing_items_puts_them_above_everything_else() -> anyhow::Result<()> {
    let mut status = fixture()?;
    status.missing = vec![
        Missing::new("goal", "is empty"),
        Missing::new("acceptance.contract", "names a file that does not exist"),
    ];
    let lines = plain_lines(&status, 170);
    assert!(lines[0].trim_start().starts_with("MISSING"), "{}", lines[0]);
    assert!(lines[1].contains("goal") && lines[1].contains("is empty"));
    assert!(lines[2].contains("acceptance.contract") && lines[2].contains("does not exist"));
    assert!(position(&lines, "FLEET  fleet-workspace")? > 2);
    Ok(())
}

#[test]
fn render_with_nothing_recorded_says_so_and_does_not_panic() -> anyhow::Result<()> {
    let lines = plain_lines(&empty(at("2026-09-29T01:30:00Z")?), 170);
    let all = lines.join("\n");
    assert!(all.contains("not started"), "{all}");
    assert!(all.contains("0 of 0 tasks accepted"), "{all}");
    assert!(all.contains("0 seats open, 0 launched"), "{all}");
    assert!(all.contains("nothing judged yet"), "{all}");
    assert!(!all.contains("BLOCKED") && !all.contains("MISSING"));
    Ok(())
}

#[test]
fn render_when_every_seat_is_closed_shows_the_newest_eight() -> anyhow::Result<()> {
    let mut status = empty(at("2026-09-29T01:30:00Z")?);
    status.seats = (1..=10)
        .map(|n| seat(&format!("worker-{n}"), "unit", "closed", Some(50_000), true))
        .collect();
    let lines = plain_lines(&status, 170);
    assert!(
        lines
            .iter()
            .all(|l| !l.contains("worker-1 ") && !l.contains("worker-2 "))
    );
    assert!(position(&lines, "worker-3")? < position(&lines, "worker-10")?);
    assert!(lines.iter().any(|l| l.trim() == "2 closed seats not shown"));
    Ok(())
}

#[test]
fn render_task_in_a_recorded_word_keeps_that_word_in_plain_ink() -> anyhow::Result<()> {
    let mut status = empty(at("2026-09-29T01:30:00Z")?);
    status.tasks = vec![task("P7", "spike", "unit", "on hold")];
    let coloured = render(&status, 170);
    let row = coloured
        .lines()
        .find(|l| l.contains("P7"))
        .ok_or_else(|| anyhow::anyhow!("no P7 row"))?;
    assert!(row.contains("\x1b[38;2;192;202;245mon hold"), "{row:?}");
    assert!(strip(row).contains("• P7"));
    Ok(())
}

#[test]
fn context_bar_is_twenty_cells_with_a_mark_at_six_hundred_thousand() {
    let bar: Vec<char> = strip(&context_bar(Some(100_000))).chars().collect();
    assert_eq!(bar.iter().take_while(|c| "█░▐".contains(**c)).count(), 20);
    assert_eq!(&bar[..2], &['█', '█']);
    assert_eq!(bar[12], '▐');
}

#[test]
fn context_bar_changes_colour_at_four_hundred_and_six_hundred_thousand() {
    let green = "\x1b[38;2;158;206;106m";
    let yellow = "\x1b[38;2;224;175;104m";
    let red = "\x1b[38;2;247;118;142m";
    assert!(context_bar(Some(399_999)).contains(green));
    assert!(context_bar(Some(400_000)).contains(yellow));
    assert!(context_bar(Some(599_999)).contains(yellow));
    assert!(!strip(&context_bar(Some(599_999))).contains("replace"));
    assert!(context_bar(Some(600_000)).contains(red));
    assert!(strip(&context_bar(Some(600_000))).ends_with("600k of 1000k  replace"));
}

#[test]
fn context_bar_without_a_request_is_empty_and_says_no_requests() {
    let bar = strip(&context_bar(None));
    assert!(bar.starts_with(&"·".repeat(20)));
    assert!(bar.ends_with("no requests"));
}

#[test]
fn fit_cuts_between_colour_codes_and_resets_before_the_ellipsis() {
    let green = Rgb(0, 255, 0);
    let row = format!(
        "{}{}",
        paint("abcdefghij", RED, true),
        paint("klm", green, false)
    );
    let cut = fit(&row, 5);
    assert_eq!(strip(&cut), "abcd…");
    assert_eq!(visible_len(&cut), 5);
    assert!(cut.ends_with("\x1b[0m…"), "{cut:?}");
    assert_eq!(fit(&row, 13), row);
}

#[test]
fn ago_reads_seconds_then_minutes_then_hours() -> anyhow::Result<()> {
    let now = at("2026-09-29T01:30:00Z")?;
    let back = |seconds| ago(Some(now - Duration::seconds(seconds)), now);
    assert_eq!(back(45), "45s ago");
    assert_eq!(back(300), "5m ago");
    assert_eq!(back(7500), "2h05m ago");
    assert_eq!(ago(None, now), "never");
    Ok(())
}

#[test]
fn render_list_draws_a_line_per_fleet_and_says_when_there_are_none() -> anyhow::Result<()> {
    let fleet = |slug: &str, state: &str, missing: usize, run: Option<&str>| {
        Ok::<_, anyhow::Error>(FleetListing {
            fleet: "3f0a1c52-6d2e-4a55-9b1e-0c8d7e6f5a41".to_string(),
            slug: slug.to_string(),
            project_path: "/p".to_string(),
            state: state.to_string(),
            run_id: run.map(str::to_string),
            missing: (0..missing)
                .map(|n| Missing::new(format!("item {n}"), "empty"))
                .collect(),
            created_at: at("2026-09-28T19:30:00Z")?,
        })
    };
    let fleets = vec![
        fleet(
            "fleet-workspace",
            "running",
            0,
            Some("fleet-fleet-workspace-3f0a1c52"),
        )?,
        fleet("second", "draft", 2, None)?,
    ];
    let text = strip(&render_list(&fleets, 120));
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 3);
    assert!(lines[1].starts_with("fleet-workspace"));
    assert!(lines[1].contains("running") && lines[1].contains("none missing"));
    assert!(
        lines[1].contains("28 Sep 19:30 UTC")
            && lines[1].ends_with("fleet-fleet-workspace-3f0a1c52")
    );
    assert!(
        lines[2].contains("draft")
            && lines[2].contains("2 missing")
            && lines[2].ends_with("no run yet")
    );
    assert_eq!(strip(&render_list(&[], 120)), "no fleets yet");
    Ok(())
}

#[test]
fn pad_keeps_a_space_after_a_column_it_had_to_cut() {
    use super::ink::pad;
    assert_eq!(strip(&pad("abcdefghij", 5)), "abc… ");
    assert_eq!(strip(&pad("ab", 5)), "ab   ");
}
