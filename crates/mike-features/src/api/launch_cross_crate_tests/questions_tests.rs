use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::api::questions::answer::{AnswerBody, DeclineBody};
use crate::api::questions::ask::AskBody;
use crate::api::questions::{post_question, post_question_answer, post_question_decline};
use crate::config::ProxyConfig;
use crate::launch::fleet::brief_notes_sync::latest_part;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::session_notes::NewSessionNote;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};

struct Env {
    db: TestDbHandle,
    state: AppState,
    persist: PersistHandle,
    asker: Uuid,
    recipient: Uuid,
}

impl Env {
    async fn boot() -> anyhow::Result<Option<Self>> {
        let Some(db) = acquire_ephemeral_db().await? else {
            eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
            return Ok(None);
        };
        let config = ProxyConfig::try_parse_from([
            "mike",
            "--upstream-url",
            "https://api.anthropic.com",
            "--proxy-listen-addr",
            "127.0.0.1:0",
            "--inspect-listen-addr",
            "127.0.0.1:0",
            "--persist-enabled",
            "1",
            "--database-url",
            &db.database_url,
        ])?;
        let state = AppState::new(config).await?;
        let persist = state
            .persist()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("persist_enabled config must yield a PersistHandle"))?;
        let (asker, recipient) = (Uuid::new_v4(), Uuid::new_v4());
        for session in [asker, recipient] {
            persist
                .launch()
                .ensure_session_exists(&session.to_string())
                .await?;
        }
        Ok(Some(Self {
            db,
            state,
            persist,
            asker,
            recipient,
        }))
    }

    async fn cleanup(self) -> anyhow::Result<()> {
        self.state.shutdown().cancel();
        self.db.cleanup().await
    }

    async fn ask(&self, to: &str, text: &str, about: Option<Uuid>) -> (StatusCode, Value) {
        let body = AskBody {
            from: self.asker.to_string(),
            to: to.to_owned(),
            question: text.to_owned(),
            blocked: true,
            about: about.map(|a| a.to_string()),
        };
        read(post_question(State(self.state.base.clone()), Json(body)).await).await
    }

    async fn answer(&self, id: Uuid, text: &str, amend: bool) -> (StatusCode, Value) {
        let body = AnswerBody {
            from: self.recipient.to_string(),
            answer: text.to_owned(),
            amend,
        };
        let response =
            post_question_answer(State(self.state.base.clone()), Path(id), Json(body)).await;
        read(response).await
    }

    async fn inbox(&self, session: Uuid) -> anyhow::Result<Vec<String>> {
        Ok(self
            .persist
            .launch()
            .fetch_for_session_with_activity(session)
            .await?
            .inbox
            .into_iter()
            .map(|m| m.row.body)
            .collect())
    }
}

async fn read(response: Response) -> (StatusCode, Value) {
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap_or_default();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn id_of(body: &Value) -> anyhow::Result<Uuid> {
    let text = body["question_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("no question_id in {body}"))?;
    Ok(text.parse()?)
}

#[tokio::test]
async fn asking_writes_the_question_and_mails_the_recipient_once() -> anyhow::Result<()> {
    let Some(env) = Env::boot().await? else {
        return Ok(());
    };
    let (status, body) = env
        .ask(&env.recipient.to_string(), "which file holds it?", None)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["state"], json!("open"));
    assert_eq!(body["blocked"], json!(true));
    assert_eq!(body["mailed"], json!(true));
    let id = id_of(&body)?;

    let open = env
        .persist
        .launch()
        .open_questions_for(env.recipient)
        .await?;
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].question_uuid, id);
    let mail = env.inbox(env.recipient).await?;
    assert_eq!(mail.len(), 1, "{mail:?}");
    assert!(mail[0].contains(&format!("[question {id}]")));
    assert!(mail[0].contains("which file holds it?"));
    assert!(mail[0].contains("blocked until you settle it"));
    env.cleanup().await
}

#[tokio::test]
async fn asking_is_refused_for_what_cannot_be_resolved_and_nothing_is_written() -> anyhow::Result<()>
{
    let Some(env) = Env::boot().await? else {
        return Ok(());
    };
    let to = env.recipient.to_string();
    let (blank, _) = env.ask(&to, "   ", None).await;
    assert_eq!(blank, StatusCode::BAD_REQUEST);
    let (no_fleet, body) = env.ask("task:T3", "who?", None).await;
    assert_eq!(no_fleet, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().is_some_and(|e| e.contains("fleet")));
    let (reference, _) = env.ask("reference", "where?", None).await;
    assert_eq!(reference, StatusCode::BAD_REQUEST);
    let (fleet, _) = env.ask("fleet", "all?", None).await;
    assert_eq!(fleet, StatusCode::BAD_REQUEST);
    let (unknown, _) = env.ask("ffffffff", "hello?", None).await;
    assert_eq!(unknown, StatusCode::NOT_FOUND);
    let (short, _) = env.ask("abc", "hello?", None).await;
    assert_eq!(short, StatusCode::BAD_REQUEST);
    let (bad_note, _) = env.ask(&to, "about?", Some(Uuid::new_v4())).await;
    assert_eq!(bad_note, StatusCode::NOT_FOUND);
    assert!(
        env.persist
            .launch()
            .open_questions_for(env.recipient)
            .await?
            .is_empty()
    );
    assert!(env.inbox(env.recipient).await?.is_empty());
    env.cleanup().await
}

#[tokio::test]
async fn an_answer_settles_the_question_and_mails_the_asker_once() -> anyhow::Result<()> {
    let Some(env) = Env::boot().await? else {
        return Ok(());
    };
    let (_, asked) = env.ask(&env.recipient.to_string(), "which?", None).await;
    let id = id_of(&asked)?;

    let (status, body) = env.answer(id, "fleet_target.rs", false).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["state"], json!("answered"));
    assert_eq!(body["mailed_asker"], json!(true));
    let mail = env.inbox(env.asker).await?;
    assert_eq!(mail.len(), 1, "{mail:?}");
    assert!(mail[0].contains("fleet_target.rs"));
    assert!(
        env.persist
            .launch()
            .open_questions_for(env.recipient)
            .await?
            .is_empty()
    );

    let (again, refused) = env.answer(id, "something else", false).await;
    assert_eq!(again, StatusCode::CONFLICT, "{refused}");
    assert_eq!(env.inbox(env.asker).await?.len(), 1);
    let (empty, _) = env.answer(Uuid::new_v4(), "x", false).await;
    assert_eq!(empty, StatusCode::NOT_FOUND);
    env.cleanup().await
}

#[tokio::test]
async fn a_decline_mails_the_asker_its_reason_and_an_empty_answer_is_refused() -> anyhow::Result<()>
{
    let Some(env) = Env::boot().await? else {
        return Ok(());
    };
    let (_, asked) = env.ask(&env.recipient.to_string(), "can you?", None).await;
    let id = id_of(&asked)?;
    let (blank, _) = env.answer(id, "  ", false).await;
    assert_eq!(blank, StatusCode::BAD_REQUEST);

    let body = DeclineBody {
        from: env.recipient.to_string(),
        reason: Some("not mine".to_owned()),
    };
    let response = post_question_decline(State(env.state.base.clone()), Path(id), Json(body)).await;
    let (status, declined) = read(response).await;
    assert_eq!(status, StatusCode::OK, "{declined}");
    assert_eq!(declined["state"], json!("declined"));
    let mail = env.inbox(env.asker).await?;
    assert_eq!(mail.len(), 1, "{mail:?}");
    assert!(mail[0].contains("declined") && mail[0].contains("Reason: not mine"));
    env.cleanup().await
}

#[tokio::test]
async fn an_answer_with_amend_adds_itself_to_the_brief_a_question_is_about() -> anyhow::Result<()> {
    let Some(env) = Env::boot().await? else {
        return Ok(());
    };
    let tags = vec![
        "fleet:f-1".to_owned(),
        "task:T5".to_owned(),
        "part:task".to_owned(),
    ];
    let note = env
        .persist
        .fleet()
        .create_session_note(
            env.recipient,
            NewSessionNote {
                kind: Some("brief"),
                title: "Brief, task T5 (version 1)",
                body: Some("Your task is T5."),
                status: None,
                tags: Some(tags.as_slice()),
                data: Some(&json!({"part": "task", "task": "T5", "version": 1})),
            },
        )
        .await?;
    let plain = env
        .persist
        .fleet()
        .create_session_note(
            env.recipient,
            NewSessionNote {
                kind: Some("finding"),
                title: "A finding",
                body: Some("text"),
                status: None,
                tags: None,
                data: None,
            },
        )
        .await?;

    let (_, about_plain) = env
        .ask(
            &env.recipient.to_string(),
            "about a finding",
            Some(plain.note_uuid),
        )
        .await;
    let plain_id = id_of(&about_plain)?;
    let (refused, _) = env.answer(plain_id, "x", true).await;
    assert_eq!(refused, StatusCode::BAD_REQUEST);
    let (_, none_about) = env
        .ask(&env.recipient.to_string(), "about nothing", None)
        .await;
    let (refused_again, _) = env.answer(id_of(&none_about)?, "x", true).await;
    assert_eq!(refused_again, StatusCode::BAD_REQUEST);
    assert_eq!(
        env.persist
            .launch()
            .open_questions_for(env.recipient)
            .await?
            .len(),
        2,
        "a refused amend leaves the question open"
    );

    let (_, asked) = env
        .ask(
            &env.recipient.to_string(),
            "is the index list right?",
            Some(note.note_uuid),
        )
        .await;
    let id = id_of(&asked)?;
    let (status, body) = env.answer(id, "add the index on to_session_id", true).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["amended_version"], json!(2));
    assert_eq!(body["amend_error"], Value::Null);

    let latest = latest_part(&env.persist, env.recipient, &tags).await;
    let Some(latest) = latest else {
        anyhow::bail!("the brief has no stored version");
    };
    assert_eq!(latest.version, 2);
    assert!(latest.body.starts_with("Your task is T5."));
    assert!(latest.body.contains("is the index list right?"));
    assert!(latest.body.ends_with("add the index on to_session_id"));
    let mail = env.inbox(env.asker).await?;
    assert!(mail.iter().any(|m| m.contains("version 2")), "{mail:?}");
    env.cleanup().await
}
