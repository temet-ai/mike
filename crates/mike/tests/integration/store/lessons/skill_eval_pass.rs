use mike::lesson_proposer::ProposerPrompts;
use mike::lesson_proposer::skill_eval::{EVALUATOR_VERSION, run_pass};
use mike::llm::{OpenRouterClient, reset_global_for_tests};
use mike::store::persistence::lessons::skill_drafts::DraftTarget;
use mike::store::persistence::lessons_repo::LessonsPersist as _;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

use super::skill_drafts::{Fixture, draft, fixture};

pub(super) const FOLDER: &str = "database-migrations";
const MARKER: &str = "<!-- mike-learning-generated:v1 -->";

pub(super) struct Scripted {
    pub(super) loads_skill: bool,
    pub(super) loads_when_patched: bool,
    pub(super) decision: &'static str,
}

impl Scripted {
    pub(super) fn approving() -> Self {
        Self {
            loads_skill: true,
            loads_when_patched: false,
            decision: "approve",
        }
    }

    fn weak(decision: &'static str, loads_when_patched: bool) -> Self {
        Self {
            loads_skill: false,
            loads_when_patched,
            decision,
        }
    }
}

pub(super) fn message(body: &Value, index: usize) -> String {
    body["messages"][index]["content"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

fn numbered_messages(user: &str) -> Vec<String> {
    user.split("## USER MESSAGES\n")
        .nth(1)
        .unwrap_or_default()
        .split("\n\n")
        .filter_map(|block| block.split_once(". ").map(|(_, text)| text.to_owned()))
        .collect()
}

fn patched_skill_md() -> String {
    format!(
        "---\nname: {FOLDER}\ndescription: \"Patched description. Use when MSG-YES renumbering.\"\n---\n{MARKER}\n\n# Database migrations\n\n- [references/numbering.md](references/numbering.md): read when numbering one.\n"
    )
}

fn review_reply(decision: &str) -> Value {
    let (overlap, belongs_in, skill_md) = match decision {
        "nothing_to_save" => ("deploy-and-restart", "none", String::new()),
        "wrong_place" => ("ui-design", "existing_skill", String::new()),
        "patch" => ("", "none", patched_skill_md()),
        _ => ("", "none", String::new()),
    };
    json!({
        "decision": decision,
        "reason": format!("scripted {decision}"),
        "overlap": overlap,
        "belongs_in": belongs_in,
        "skill_md": skill_md
    })
}

impl Respond for Scripted {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let Ok(body) = serde_json::from_slice::<Value>(&request.body) else {
            return ResponseTemplate::new(400);
        };
        let (system, user) = (message(&body, 0), message(&body, 1));
        let patched_listed = user
            .lines()
            .any(|l| l.starts_with(&format!("- {FOLDER}: ")) && l.contains("Patched"));
        if !user.contains("belong to the project at /tmp/skill-drafts") {
            return ResponseTemplate::new(400);
        }
        let content = if system.contains("followed by numbered messages") {
            let loads = if patched_listed {
                self.loads_when_patched
            } else {
                self.loads_skill
            };
            answers_for(&user, loads)
        } else if system.contains("You review a draft Claude Code skill") {
            review_reply(self.decision)
        } else {
            return ResponseTemplate::new(400);
        };
        ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"content": content.to_string()}}]
        }))
    }
}

fn answers_for(user: &str, loads: bool) -> Value {
    let answers: Vec<Value> = numbered_messages(user)
        .iter()
        .zip(1..)
        .map(|(text, number)| {
            let load = if loads && text.contains("MSG-YES") {
                vec![FOLDER]
            } else {
                Vec::new()
            };
            json!({"message": number, "load": load})
        })
        .collect();
    json!({"answers": answers})
}

pub(super) async fn insert_evaluable(fx: &Fixture) -> anyhow::Result<i64> {
    let first = fx.daily_lesson("Run mike migrate-check first.").await?;
    let second = fx
        .daily_lesson("Commit migrations with their code.")
        .await?;
    let target = DraftTarget {
        project_id: fx.project_id,
        project_path: "/tmp/skill-drafts".to_owned(),
        folder: FOLDER.to_owned(),
        topic: FOLDER.to_owned(),
        agent_id: None,
        agent_name: None,
        lesson_watermark: 50,
    };
    let references = json!([{
        "name": "numbering",
        "when_to_read": "numbering one.",
        "content": "# Numbering\n\nRun mike migrate-check."
    }]);
    let queries: Vec<Value> = (1..=10)
        .flat_map(|n| {
            [
                json!({"query": format!("MSG-YES {n}"), "should_trigger": true}),
                json!({"query": format!("near miss {n}"), "should_trigger": false}),
            ]
        })
        .collect();
    let queries = Value::Array(queries);
    let ids = [first, second];
    let mut new = draft(&target, &ids, &references);
    new.description = "Ship a migration safely.";
    new.skill_md = "---\nname: database-migrations\ndescription: \"Ship a migration safely.\"\n---\n<!-- mike-learning-generated:v1 -->\n\n# Database migrations\n\n- [references/numbering.md](references/numbering.md): read when numbering one.\n";
    new.reference_files = &references;
    new.trigger_queries = &queries;
    Ok(fx.handle.lessons().insert_skill_draft(new).await?)
}

async fn sibling(fx: &Fixture) -> anyhow::Result<()> {
    let target = DraftTarget {
        project_id: fx.project_id,
        project_path: "/tmp/skill-drafts".to_owned(),
        folder: "ui-design".to_owned(),
        topic: "ui-design".to_owned(),
        agent_id: None,
        agent_name: None,
        lesson_watermark: 60,
    };
    let empty = json!([]);
    fx.handle
        .lessons()
        .insert_skill_draft(draft(&target, &[3, 4], &empty))
        .await?;
    Ok(())
}

pub(super) async fn serve(responder: Scripted) -> anyhow::Result<(MockServer, OpenRouterClient)> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(responder)
        .mount(&server)
        .await;
    let client = OpenRouterClient::new(
        "sk-test".to_owned(),
        "test/model".to_owned(),
        Some(server.uri()),
        None,
    )?;
    Ok((server, client))
}

pub(super) async fn passes_until_decided(
    fx: &Fixture,
    client: &OpenRouterClient,
) -> anyhow::Result<usize> {
    for pass in 1..=40 {
        reset_global_for_tests();
        let report = run_pass(&fx.handle, client, &ProposerPrompts::default()).await;
        if report.accepted.is_some() {
            return Ok(pass);
        }
        anyhow::ensure!(report.error.is_none(), "pass {pass} failed: {report:?}");
    }
    anyhow::bail!("the draft was not decided in 40 passes")
}

pub(super) async fn stored(
    fx: &Fixture,
    id: i64,
) -> anyhow::Result<(String, Option<Value>, Option<Value>)> {
    Ok(
        sqlx::query_as("SELECT status, eval, eval_progress FROM memory.skill_drafts WHERE id = $1")
            .bind(id)
            .fetch_one(fx.handle.pool())
            .await?,
    )
}

async fn texts(fx: &Fixture, id: i64) -> anyhow::Result<(String, String)> {
    Ok(
        sqlx::query_as("SELECT description, skill_md FROM memory.skill_drafts WHERE id = $1")
            .bind(id)
            .fetch_one(fx.handle.pool())
            .await?,
    )
}

async fn reviewer_input(server: &MockServer) -> Option<String> {
    let requests = server.received_requests().await?;
    requests
        .iter()
        .filter_map(|r| serde_json::from_slice::<Value>(&r.body).ok())
        .find(|body| message(body, 0).contains("You review a draft Claude Code skill"))
        .map(|body| message(&body, 1))
}

#[tokio::test]
async fn an_approved_draft_that_triggers_is_accepted_in_three_calls() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let id = insert_evaluable(&fx).await?;
    sibling(&fx).await?;
    let (server, client) = serve(Scripted::approving()).await?;

    passes_until_decided(&fx, &client).await?;

    let (status, eval, progress) = stored(&fx, id).await?;
    let eval = eval.ok_or_else(|| anyhow::anyhow!("an eval must be stored"))?;
    assert_eq!(status, "accepted", "{eval}");
    assert!(progress.is_none());
    assert_eq!(eval["trigger"]["correct"], 20);
    assert_eq!(eval["review"]["decision"], "approve");
    assert_eq!(eval["recheck"], Value::Null);
    assert_eq!(eval["calls"], 3);
    assert_eq!(eval["models"], json!(["test/model"]));
    assert_eq!(eval["siblings"][0]["folder"], "ui-design");
    assert_eq!(eval["workspace"]["path"], "/tmp/skill-drafts");
    assert_eq!(eval["evaluator_version"], EVALUATOR_VERSION);
    assert_eq!(eval["verdict"]["accepted"], true);
    assert!(eval.get("usefulness").is_none());
    assert_eq!(
        server.received_requests().await.map(|r| r.len()),
        Some(3),
        "two trigger batches of ten and one review, none repeated across passes"
    );
    let (description, _) = texts(&fx, id).await?;
    assert_eq!(description, "Ship a migration safely.");
    fx.finish().await
}

#[tokio::test]
async fn the_reviewer_is_shown_the_lessons_the_existing_skills_and_the_trigger_result()
-> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    insert_evaluable(&fx).await?;
    sibling(&fx).await?;
    let (server, client) = serve(Scripted::approving()).await?;

    passes_until_decided(&fx, &client).await?;

    let input = reviewer_input(&server)
        .await
        .ok_or_else(|| anyhow::anyhow!("the reviewer must have been asked"))?;
    assert!(input.contains("<<draft-skill-md>>"));
    assert!(input.contains("<<reference-file references/numbering.md>>"));
    assert!(input.contains("Run mike migrate-check first."));
    assert!(input.contains("Commit migrations with their code."));
    assert!(input.contains("<<existing-skills>>\n- ui-design: Ship a migration safely."));
    assert!(!input.contains("- database-migrations:"));
    assert!(input.contains("20 of 20 test messages were answered as labelled"));
    fx.finish().await
}

#[tokio::test]
async fn a_patched_draft_that_triggers_after_the_patch_is_accepted_with_the_patch_in_five_calls()
-> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let id = insert_evaluable(&fx).await?;
    let (server, client) = serve(Scripted::weak("patch", true)).await?;

    passes_until_decided(&fx, &client).await?;

    let (status, eval, _) = stored(&fx, id).await?;
    let eval = eval.ok_or_else(|| anyhow::anyhow!("an eval must be stored"))?;
    assert_eq!(status, "accepted", "{eval}");
    assert_eq!(eval["trigger"]["score"], 0.5);
    assert_eq!(eval["recheck"]["score"], 1.0);
    assert_eq!(eval["review"]["decision"], "patch");
    assert_eq!(eval["calls"], 5);
    assert_eq!(
        eval["review"]["original_description"],
        "Ship a migration safely."
    );
    assert_eq!(
        server.received_requests().await.map(|r| r.len()),
        Some(5),
        "two trigger batches, one review, two batches again on the patched draft"
    );
    let (description, skill_md) = texts(&fx, id).await?;
    assert!(description.starts_with("Patched description."));
    assert!(skill_md.contains("description: \"Patched description."));
    assert_eq!(
        skill_md,
        eval["review"]["patched_skill_md"]
            .as_str()
            .unwrap_or_default()
    );
    fx.finish().await
}

#[tokio::test]
async fn a_patch_that_still_does_not_trigger_is_rejected_and_leaves_the_draft_unchanged()
-> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let id = insert_evaluable(&fx).await?;
    let (server, client) = serve(Scripted::weak("patch", false)).await?;

    passes_until_decided(&fx, &client).await?;

    let (status, eval, _) = stored(&fx, id).await?;
    let eval = eval.ok_or_else(|| anyhow::anyhow!("an eval must be stored"))?;
    assert_eq!(status, "rejected");
    assert_eq!(eval["recheck"]["score"], 0.5);
    assert!(
        eval["verdict"]["reasons"][0]
            .as_str()
            .is_some_and(|r| r.contains("the patched draft's trigger score 0.50"))
    );
    let missed = eval["findings"]["missed_triggers"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("missed_triggers"))?;
    assert_eq!(missed.len(), 10);
    assert_eq!(server.received_requests().await.map(|r| r.len()), Some(5));
    let (description, _) = texts(&fx, id).await?;
    assert_eq!(description, "Ship a migration safely.");
    assert!(eval["review"]["patched_skill_md"].is_string());
    fx.finish().await
}

#[tokio::test]
async fn a_weak_trigger_the_reviewer_approved_without_a_patch_is_rejected() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let id = insert_evaluable(&fx).await?;
    let (server, client) = serve(Scripted::weak("approve", false)).await?;

    passes_until_decided(&fx, &client).await?;

    let (status, eval, _) = stored(&fx, id).await?;
    let eval = eval.ok_or_else(|| anyhow::anyhow!("an eval must be stored"))?;
    assert_eq!(status, "rejected");
    assert_eq!(eval["trigger"]["score"], 0.5);
    assert!(
        eval["verdict"]["reasons"][0]
            .as_str()
            .is_some_and(|r| r.contains("approved it without a patch"))
    );
    assert_eq!(server.received_requests().await.map(|r| r.len()), Some(3));
    fx.finish().await
}
