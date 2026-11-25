use chrono::Duration;
use mike::dossier::render::render_inputs;
use mike::dossier::view::types::{AsOf, Audience, Budget, Needs};
use serde_json::json;

use super::after;
use crate::seed::database_now;
use crate::turn_facts::resolution::{build, message};
use crate::turn_facts::{
    Fixture, Req, attach, fixture, insert_request, insert_response, insert_text_block,
};

const PEER: &str = "<teammate-message teammate_id=\"peer\">status please</teammate-message>";
const RECAP_PROMPT: &str = "The user stepped away and is coming back. Recap in under 40 words, lead with the overall goal and current task, then the one next action.";

async fn lane(
    f: &Fixture,
    id: i64,
    at_secs: i64,
    texts: &[&str],
    stop_reason: &str,
) -> anyhow::Result<()> {
    insert_request(f, &Req::lane(id, "primary", at_secs)).await?;
    for (seq, text) in texts.iter().enumerate() {
        insert_text_block(f, id, i32::try_from(seq)?, text).await?;
    }
    insert_response(f, id, stop_reason).await?;
    Ok(())
}

async fn tool_use(
    f: &Fixture,
    request_id: i64,
    ord: i32,
    name: &str,
    input: serde_json::Value,
    at_secs: i64,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO tools.uses
             (request_id, session_id, ord, wire_tool_use_id, tool_name, input, created_at)
         VALUES ($1, $2, $3, 'toolu_' || $1::text || '_' || $3::text, $4, $5, $6)
         RETURNING id",
    )
    .bind(request_id)
    .bind(f.session_id)
    .bind(ord)
    .bind(name)
    .bind(input)
    .bind(f.base + Duration::seconds(at_secs))
    .fetch_one(f.handle.pool())
    .await?;
    Ok(id)
}

async fn bash(
    f: &Fixture,
    request_id: i64,
    ord: i32,
    command: &str,
    at_secs: i64,
) -> anyhow::Result<i64> {
    tool_use(
        f,
        request_id,
        ord,
        "Bash",
        json!({ "command": command }),
        at_secs,
    )
    .await
}

async fn error_result(
    f: &Fixture,
    request_id: i64,
    tool_use_id: i64,
    at_secs: i64,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO tools.results
             (request_id, session_id, tool_use_id, ord, is_error, content, content_sha256, content_bytes, created_at)
         VALUES ($1, $2, $3, 0, true, to_jsonb('Exit code 101'::text), 'sha', 1, $4)",
    )
    .bind(request_id)
    .bind(f.session_id)
    .bind(tool_use_id)
    .bind(f.base + Duration::seconds(at_secs))
    .execute(f.handle.pool())
    .await?;
    Ok(())
}

async fn recap(
    f: &Fixture,
    id: i64,
    at_secs: i64,
    text: &str,
    agent: Option<&str>,
) -> anyhow::Result<()> {
    insert_request(f, &Req::primary(id, at_secs, RECAP_PROMPT)).await?;
    insert_text_block(f, id, 0, text).await?;
    insert_response(f, id, "end_turn").await?;
    sqlx::query(
        "INSERT INTO prompt_kinds.classifications (request_id, session_id, kind_id)
         SELECT $1, $2, k.id FROM prompt_kinds.kinds k WHERE k.code = 'session-recap'",
    )
    .bind(id)
    .bind(f.session_id)
    .execute(f.handle.pool())
    .await?;
    sqlx::query("UPDATE requests.requests SET claude_code_agent_id = $2 WHERE id = $1")
        .bind(id)
        .bind(agent)
        .execute(f.handle.pool())
        .await?;
    Ok(())
}

async fn seed_first_turn(f: &Fixture) -> anyhow::Result<i64> {
    let turn = message(f, 9100, 0, "Refactor the store.", Some(100)).await?;
    lane(f, 9101, 10, &["Working on it."], "tool_use").await?;
    lane(f, 9102, 20, &["First half.", "Second half."], "end_turn").await?;
    attach(f, turn, &[9101, 9102]).await?;
    sqlx::query("UPDATE memory.operator_turns SET files_touched = ARRAY['/repo/a.rs', '/repo/b.rs'] WHERE id = $1")
        .bind(turn)
        .execute(f.handle.pool())
        .await?;
    let failing = bash(f, 9101, 0, "cargo test -p store", 11).await?;
    error_result(f, 9102, failing, 21).await?;
    bash(f, 9101, 1, "git commit -m \"one\"", 12).await?;
    tool_use(f, 9101, 2, "Edit", json!({ "file_path": "/repo/a.rs" }), 13).await?;
    bash(f, 9102, 0, "cd /repo && git push origin main", 22).await?;
    let long = format!("git -C /repo commit -m \"{}\"", "m".repeat(200));
    bash(f, 9102, 1, &long, 23).await?;
    bash(f, 9102, 2, "git commit --amend --no-edit", 24).await?;
    bash(f, 9102, 3, "git commit -am wip", 25).await?;
    bash(f, 9102, 4, "git commit -m late", 260).await?;
    Ok(turn)
}

async fn seed_later_turns(f: &Fixture) -> anyhow::Result<()> {
    let peer = message(f, 9200, 200, PEER, Some(300)).await?;
    lane(f, 9201, 210, &["Looking into it."], "end_turn").await?;
    lane(f, 9202, 220, &[""], "end_turn").await?;
    attach(f, peer, &[9201, 9202]).await?;
    let third = message(f, 9400, 400, "thanks, now add docs", Some(500)).await?;
    lane(f, 9401, 410, &["Docs written."], "end_turn").await?;
    attach(f, third, &[9401]).await?;
    recap(f, 9500, 900, "Added a retry; docs next.", None).await?;
    recap(f, 9501, 910, "An agent's recap.", Some("agent-x")).await?;
    message(f, 9600, 1_200, "still going", None).await?;
    Ok(())
}

async fn seed_lessons(f: &Fixture) -> anyhow::Result<()> {
    sqlx::query(
        "WITH l AS (
             INSERT INTO memory.lessons (project_id, kind_id, text)
             SELECT $1, 1, 'lesson number ' || g FROM generate_series(1, 24) g
             RETURNING id, text
         )
         INSERT INTO memory.supplied_items (session_id, request_id, item_kind, lesson_id, route, item_text, supplied_at)
         SELECT $2, 9101, 'lesson', l.id, 'proxy', l.text,
                $3 + make_interval(secs => (substring(l.text from '[0-9]+$'))::int)
           FROM l",
    )
    .bind(f.project_id)
    .bind(f.session_id)
    .bind(f.base)
    .execute(f.handle.pool())
    .await?;
    sqlx::query(
        "INSERT INTO memory.supplied_items (session_id, request_id, item_kind, lesson_id, route, item_text, supplied_at)
         SELECT $1, 9102, 'lesson', l.id, 'mod', l.text, $2 + make_interval(secs => 100)
           FROM memory.lessons l WHERE l.project_id = $3 AND l.text = 'lesson number 3'",
    )
    .bind(f.session_id)
    .bind(f.base)
    .bind(f.project_id)
    .execute(f.handle.pool())
    .await?;
    Ok(())
}

async fn seed(f: &Fixture) -> anyhow::Result<i64> {
    let first = seed_first_turn(f).await?;
    seed_later_turns(f).await?;
    seed_lessons(f).await?;
    build(f).await?;
    Ok(first)
}

#[tokio::test]
async fn fetch_session_record_loads_every_turn_with_its_stop_text_and_work() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let first = seed(&f).await?;
    let now = database_now(f.handle.pool()).await?;
    let inputs = mike::dossier::view::load(
        &f.handle,
        f.session_id,
        AsOf::knowledge(now),
        Needs {
            session_record: true,
            ..Needs::default()
        },
    )
    .await?;
    let turns = &inputs.record.turns;
    assert_eq!(turns.len(), 4);
    let one = &turns[0];
    assert_eq!(one.turn_id, first);
    assert_eq!(one.prompt_kind.as_deref(), Some("genuine"));
    assert_eq!(one.opened_by.as_deref(), Some("operator"));
    assert_eq!(one.prompt, "Refactor the store.");
    assert_eq!(
        one.stop_text.as_deref(),
        Some("First half.\n\nSecond half.")
    );
    assert_eq!(one.last_text, None);
    assert_eq!(one.stop_reason.as_deref(), Some("end_turn"));
    assert_eq!(one.files_touched, 2);
    assert_eq!(
        one.tools().collect::<Vec<_>>(),
        vec![("Bash", 7), ("Edit", 1)]
    );
    assert_eq!(one.tool_errors, 1);
    assert_eq!(one.commit_count, 5);
    assert_eq!(one.commit_commands.len(), 3);
    assert_eq!(one.commit_commands[0], "git commit -m \"one\"");
    assert_eq!(one.commit_commands[1].chars().count(), 160);
    assert_eq!(one.push_count, 1);
    assert_eq!(one.push_commands, vec!["cd /repo && git push origin main"]);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn fetch_session_record_empty_stop_request_falls_back_to_the_last_text() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    seed(&f).await?;
    let now = database_now(f.handle.pool()).await?;
    let inputs = mike::dossier::view::load(
        &f.handle,
        f.session_id,
        AsOf::knowledge(now),
        Needs {
            session_record: true,
            ..Needs::default()
        },
    )
    .await?;
    let peer = &inputs.record.turns[1];
    assert_eq!(peer.prompt_kind.as_deref(), Some("peer-message"));
    assert_eq!(peer.opened_by.as_deref(), Some("machine"));
    assert_eq!(peer.stop_text, None);
    assert_eq!(peer.last_text.as_deref(), Some("Looking into it."));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn fetch_session_record_recap_that_stopped_a_turn_is_not_its_stop_text() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    seed(&f).await?;
    let now = database_now(f.handle.pool()).await?;
    let inputs = mike::dossier::view::load(
        &f.handle,
        f.session_id,
        AsOf::knowledge(now),
        Needs {
            session_record: true,
            ..Needs::default()
        },
    )
    .await?;
    let third = &inputs.record.turns[2];
    assert_eq!(third.stop_text.as_deref(), Some("Docs written."));
    assert_eq!(third.last_text, None);
    let open = &inputs.record.turns[3];
    assert_eq!(open.prompt_kind, None);
    assert_eq!(open.stop_text, None);
    assert_eq!(open.checks_count, None);
    let recaps: Vec<&str> = inputs
        .record
        .recaps
        .iter()
        .map(|r| r.text.as_str())
        .collect();
    assert_eq!(recaps, vec!["Added a retry; docs next."]);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn fetch_session_record_lessons_are_distinct_newest_twenty_oldest_first() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    seed(&f).await?;
    let now = database_now(f.handle.pool()).await?;
    let record = mike::dossier::view::load(
        &f.handle,
        f.session_id,
        AsOf::knowledge(now),
        Needs {
            session_record: true,
            ..Needs::default()
        },
    )
    .await?
    .record;
    assert_eq!(record.lessons.len(), 20);
    let texts: Vec<&str> = record.lessons.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(texts.last().copied(), Some("lesson number 3"));
    assert_eq!(texts.first().copied(), Some("lesson number 6"));
    let mut distinct = record
        .lessons
        .iter()
        .map(|l| l.lesson_id)
        .collect::<Vec<_>>();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(distinct.len(), 20);
    assert_eq!(record.lessons.last().map(|l| l.route.as_str()), Some("mod"));
    assert_eq!(record.lessons.last().map(|l| l.request_id), Some(9102));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn fetch_session_record_as_of_hides_later_turns_facts_work_and_recaps() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    seed(&f).await?;
    let inputs = mike::dossier::view::load(
        &f.handle,
        f.session_id,
        AsOf::knowledge(after(&f, 250)),
        Needs {
            session_record: true,
            ..Needs::default()
        },
    )
    .await?;
    let record = &inputs.record;
    assert_eq!(record.turns.len(), 2);
    assert_eq!(record.turns[0].commit_count, 4);
    assert_eq!(record.turns[1].prompt_kind, None);
    assert_eq!(record.turns[1].stop_text, None);
    assert!(record.recaps.is_empty());
    assert!(
        record
            .lessons
            .iter()
            .all(|l| l.supplied_at <= after(&f, 250))
    );
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn session_judge_render_reads_the_loaded_record() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    seed(&f).await?;
    let now = database_now(f.handle.pool()).await?;
    let audience = Audience::SessionJudge;
    let inputs = mike::dossier::view::load(
        &f.handle,
        f.session_id,
        AsOf::knowledge(now),
        Needs::for_audience(&audience),
    )
    .await?;
    let rendered = render_inputs(&inputs, &audience, Budget::Chars(60_000))?;
    assert!(rendered.text.contains("#2 "));
    assert!(rendered.text.contains(" peer message\nPROMPT:\n"));
    assert!(rendered.text.contains(
        "STOPPED WITH:\n(the last request wrote no text; last text in the turn:)\nLooking into it."
    ));
    assert!(rendered.text.contains("- 2026-"));
    assert!(rendered.text.contains("Added a retry; docs next."));
    assert!(rendered.text.contains("LESSONS SUPPLIED"));
    assert!(!rendered.text.contains("An agent's recap."));
    f.db.cleanup().await?;
    Ok(())
}
