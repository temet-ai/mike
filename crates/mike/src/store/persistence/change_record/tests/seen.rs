use chrono::Duration;

use super::fixture;
use super::seed::{
    ToolUse, micros_now, project, request, response, session, tool_result, tool_use,
    tool_use_with_input,
};

#[tokio::test]
async fn paths_seen_by_lists_each_read_or_edited_path_with_its_latest_time() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let now = micros_now()?;
    let project_id = project(pool).await?;
    let me = session(pool, project_id, None).await?;
    let other = session(pool, project_id, None).await?;
    let req = request(pool, me, project_id, now).await?;
    let other_req = request(pool, other, project_id, now).await?;
    let calls = [
        (
            me,
            req,
            "Read",
            Some("/work/alpha/a.rs"),
            Duration::minutes(30),
        ),
        (
            me,
            req,
            "Edit",
            Some("/work/alpha/a.rs"),
            Duration::minutes(10),
        ),
        (
            me,
            req,
            "Write",
            Some("/work/alpha/b.rs"),
            Duration::minutes(20),
        ),
        (
            me,
            req,
            "Read",
            Some("/work/alpha/old.rs"),
            Duration::hours(5),
        ),
        (me, req, "Bash", None, Duration::minutes(5)),
        (
            me,
            req,
            "Grep",
            Some("/work/alpha/grep.rs"),
            Duration::minutes(5),
        ),
        (
            other,
            other_req,
            "Read",
            Some("/work/alpha/theirs.rs"),
            Duration::minutes(5),
        ),
    ];
    for (who, rq, name, path, ago) in calls {
        tool_use(
            pool,
            &ToolUse {
                session_id: who,
                request_id: rq,
                name,
                file_path: path,
                at: now - ago,
            },
        )
        .await?;
    }

    let seen = f.handle.paths_seen_by(me, now - Duration::hours(1)).await?;
    let names: Vec<&str> = seen.iter().map(|s| s.path.as_str()).collect();
    assert_eq!(names, vec!["/work/alpha/a.rs", "/work/alpha/b.rs"]);
    assert_eq!(seen[0].last_seen, now - Duration::minutes(10));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn tool_calls_naming_path_returns_only_edit_calls_on_that_path_inside_the_window()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let now = micros_now()?;
    let project_id = project(pool).await?;
    let me = session(pool, project_id, None).await?;
    let req = request(pool, me, project_id, now).await?;
    let path = "/work/alpha/a.rs";
    let edit = tool_use(
        pool,
        &ToolUse {
            session_id: me,
            request_id: req,
            name: "Edit",
            file_path: Some(path),
            at: now - Duration::seconds(20),
        },
    )
    .await?;
    tool_use(
        pool,
        &ToolUse {
            session_id: me,
            request_id: req,
            name: "Read",
            file_path: Some(path),
            at: now - Duration::seconds(15),
        },
    )
    .await?;
    tool_use(
        pool,
        &ToolUse {
            session_id: me,
            request_id: req,
            name: "Write",
            file_path: Some(path),
            at: now - Duration::minutes(10),
        },
    )
    .await?;
    tool_use(
        pool,
        &ToolUse {
            session_id: me,
            request_id: req,
            name: "Edit",
            file_path: Some("/work/alpha/b.rs"),
            at: now - Duration::seconds(10),
        },
    )
    .await?;

    let calls = f
        .handle
        .tool_calls_naming_path(path, now - Duration::seconds(30), now)
        .await?;
    let [call] = calls.as_slice() else {
        anyhow::bail!("expected one call, got {calls:?}");
    };
    assert_eq!((call.session_id, call.tool_use_id), (me, edit));
    assert_eq!(call.tool_name, "Edit");
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn shell_commands_running_finds_overlapping_bash_calls_in_sessions_inside_the_checkout()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let now = micros_now()?;
    let project_id = project(pool).await?;
    let inside = session(pool, project_id, Some("/work/alpha/crates")).await?;
    let outside = session(pool, project_id, Some("/work/alpha-two")).await?;
    let req_in = request(pool, inside, project_id, now - Duration::minutes(2)).await?;
    let req_out = request(pool, outside, project_id, now - Duration::minutes(2)).await?;
    for rq in [req_in, req_out] {
        response(pool, rq, now - Duration::minutes(2)).await?;
    }
    let spanning = tool_use(
        pool,
        &ToolUse {
            session_id: inside,
            request_id: req_in,
            name: "Bash",
            file_path: None,
            at: now - Duration::minutes(2),
        },
    )
    .await?;
    tool_result(pool, spanning, req_in, inside, now - Duration::seconds(10)).await?;
    tool_use(
        pool,
        &ToolUse {
            session_id: inside,
            request_id: req_in,
            name: "Bash",
            file_path: None,
            at: now - Duration::minutes(1),
        },
    )
    .await?;
    let finished_before = tool_use(
        pool,
        &ToolUse {
            session_id: inside,
            request_id: req_in,
            name: "Bash",
            file_path: None,
            at: now - Duration::minutes(40),
        },
    )
    .await?;
    tool_result(
        pool,
        finished_before,
        req_in,
        inside,
        now - Duration::minutes(39),
    )
    .await?;
    let elsewhere = tool_use(
        pool,
        &ToolUse {
            session_id: outside,
            request_id: req_out,
            name: "Bash",
            file_path: None,
            at: now - Duration::minutes(2),
        },
    )
    .await?;
    tool_result(
        pool,
        elsewhere,
        req_out,
        outside,
        now - Duration::seconds(10),
    )
    .await?;

    let runs = f
        .handle
        .shell_commands_running("/work/alpha", now - Duration::seconds(30), now)
        .await?;
    assert_eq!(runs.len(), 2);
    assert!(runs.iter().all(|r| r.session_id == inside));
    assert_eq!(runs.iter().filter(|r| r.finished.is_none()).count(), 1);
    f.db.cleanup().await?;
    Ok(())
}

async fn unfinished_bash(
    f: &super::seed::Fixture,
    project_id: i64,
    session_id: i64,
    started_ago: Duration,
    input: serde_json::Value,
) -> anyhow::Result<()> {
    let pool = f.handle.pool();
    let at = micros_now()? - started_ago;
    let rq = request(pool, session_id, project_id, at).await?;
    response(pool, rq, at).await?;
    tool_use_with_input(
        pool,
        &ToolUse {
            session_id,
            request_id: rq,
            name: "Bash",
            file_path: None,
            at,
        },
        &input,
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn shell_commands_running_drops_an_unfinished_call_of_a_session_that_went_quiet()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let now = micros_now()?;
    let project_id = project(pool).await?;
    let dead = session(pool, project_id, Some("/work/alpha")).await?;
    let slow = session(pool, project_id, Some("/work/alpha")).await?;
    let fresh = session(pool, project_id, Some("/work/alpha")).await?;
    let revived = session(pool, project_id, Some("/work/alpha")).await?;
    let plain = serde_json::json!({"command": "sleep 1"});
    let long = serde_json::json!({"command": "make", "timeout": 600_000});
    unfinished_bash(&f, project_id, dead, Duration::minutes(30), plain.clone()).await?;
    unfinished_bash(&f, project_id, slow, Duration::minutes(8), long).await?;
    unfinished_bash(&f, project_id, fresh, Duration::seconds(30), plain.clone()).await?;
    unfinished_bash(&f, project_id, revived, Duration::minutes(10), plain).await?;
    let later = request(pool, revived, project_id, now - Duration::minutes(2)).await?;
    response(pool, later, now - Duration::minutes(2)).await?;

    let runs = f
        .handle
        .shell_commands_running("/work/alpha", now - Duration::seconds(30), now)
        .await?;
    let mut who: Vec<i64> = runs.iter().map(|r| r.session_id).collect();
    who.sort_unstable();
    let mut expected = vec![slow, fresh, revived];
    expected.sort_unstable();
    assert_eq!(who, expected, "{runs:?}; the quiet session is {dead}");
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn shell_commands_running_ignores_a_timeout_input_that_is_not_a_number() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let now = micros_now()?;
    let project_id = project(pool).await?;
    let odd = session(pool, project_id, Some("/work/alpha")).await?;
    let input = serde_json::json!({"command": "x", "timeout": "soon"});
    unfinished_bash(&f, project_id, odd, Duration::seconds(60), input).await?;
    let runs = f
        .handle
        .shell_commands_running("/work/alpha", now - Duration::seconds(30), now)
        .await?;
    assert_eq!(runs.len(), 1, "{runs:?}");
    f.db.cleanup().await?;
    Ok(())
}
