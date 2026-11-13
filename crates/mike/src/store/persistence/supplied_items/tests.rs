use crate::store::persistence::lessons_repo::LessonsPersist as _;
use chrono::{Duration, Utc};
use uuid::Uuid;

use super::{CarriedCard, NewVerdict, SuppliedLesson};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{
    TestDbHandle, acquire_ephemeral_db, seed_project, seed_session_turns,
};

pub(crate) struct Fixture {
    pub(crate) _db: TestDbHandle,
    pub(crate) handle: PersistHandle,
    pub(crate) session: Uuid,
    pub(crate) requests: Vec<i64>,
    pub(crate) lessons: Vec<i64>,
}

impl Fixture {
    pub(crate) async fn boot() -> anyhow::Result<Self> {
        let db = acquire_ephemeral_db()
            .await?
            .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
        let handle = PersistHandle::connect(&db.database_url).await?;
        let project = seed_project(handle.pool(), "/supplied-items/project").await?;
        let session = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
             VALUES ($1, $2, now(), now())",
        )
        .bind(session)
        .bind(project)
        .execute(handle.pool())
        .await?;
        seed_session_turns(handle.pool(), &session.to_string(), project, 3).await?;
        sqlx::query(
            "UPDATE requests.requests r SET operator_turn_id = ot.id
               FROM memory.operator_turns ot WHERE ot.start_request_id = r.id",
        )
        .execute(handle.pool())
        .await?;
        let requests: Vec<i64> = sqlx::query_scalar(
            "SELECT r.id FROM requests.requests r JOIN sessions.sessions s ON s.id = r.session_id
              WHERE s.session_uuid = $1 ORDER BY r.id",
        )
        .bind(session)
        .fetch_all(handle.pool())
        .await?;
        let lessons: Vec<i64> = sqlx::query_scalar(
            "INSERT INTO memory.lessons (project_id, kind_id, text, stage_id)
             SELECT $1, k.id, t.text, 2
               FROM memory.lesson_kinds k,
                    (VALUES ('Run the narrow test first'), ('Never stash in a shared tree')) t(text)
              WHERE k.code = 'correction'
             RETURNING id",
        )
        .bind(project)
        .fetch_all(handle.pool())
        .await?;
        Ok(Self {
            _db: db,
            handle,
            session,
            requests,
            lessons,
        })
    }

    pub(crate) async fn keeper_supply(&self, hash: &str, carrier: &str) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO memory.keeper_supplies
                 (session_uuid, carrier, arm, resolution, block_hash, card_text, score, chance)
             VALUES ($1, $2, 'keeper', 'one', $3, $4, 2.5, 0.8)",
        )
        .bind(self.session)
        .bind(carrier)
        .bind(hash)
        .bind(format!(
            "<system-reminder source=\"memory-keeper\">\n[keeper:{hash}]\ncard\n</system-reminder>"
        ))
        .execute(self.handle.pool())
        .await?;
        Ok(())
    }
}

fn lesson(id: i64, route: &'static str, score: Option<f64>) -> SuppliedLesson {
    SuppliedLesson {
        lesson_id: id,
        route,
        score,
    }
}

fn card(hash: &str, proxy: bool) -> CarriedCard {
    CarriedCard {
        hash: hash.to_owned(),
        added_by_proxy: proxy,
    }
}

#[tokio::test]
async fn supplied_items_for_request_lists_lessons_and_cards_with_route_score_and_chance()
-> anyhow::Result<()> {
    let f = Fixture::boot().await?;
    let (first, second) = (f.requests[0], f.requests[1]);
    f.keeper_supply("aaaaaaaaaaaa", "tether").await?;
    f.keeper_supply("bbbbbbbbbbbb", "proxy").await?;
    let lessons = [
        lesson(f.lessons[0], "proxy", Some(0.75)),
        lesson(f.lessons[1], "mod", Some(0.4)),
    ];
    assert_eq!(
        f.handle
            .lessons()
            .record_supplied_lessons(first, f.session, &lessons)
            .await?,
        2
    );
    let cards = [card("aaaaaaaaaaaa", false), card("bbbbbbbbbbbb", true)];
    assert_eq!(
        f.handle
            .lessons()
            .record_supplied_cards(first, f.session, &cards)
            .await?,
        2
    );

    let again = f
        .handle
        .lessons()
        .record_supplied_lessons(second, f.session, &lessons)
        .await?;
    assert_eq!(
        again, 1,
        "a mod lesson carried again in history is not a new supply"
    );
    let again = f
        .handle
        .lessons()
        .record_supplied_cards(second, f.session, &cards)
        .await?;
    assert_eq!(
        again, 1,
        "a tether card in history is recorded once, a proxy card per request"
    );
    assert_eq!(
        f.handle
            .lessons()
            .record_supplied_cards(second, f.session, &cards)
            .await?,
        0,
        "a retry for the same request writes nothing twice"
    );

    let items = f
        .handle
        .lessons()
        .supplied_items_for_request(first, 10, 0)
        .await?;
    assert_eq!(items.len(), 4);
    let tether = items
        .iter()
        .find(|i| i.card_hash.as_deref() == Some("aaaaaaaaaaaa"));
    let tether = tether.ok_or_else(|| anyhow::anyhow!("tether card missing"))?;
    assert_eq!(tether.route, "tether");
    assert_eq!(tether.score, Some(2.5));
    assert_eq!(tether.chance, Some(0.8));
    assert!(tether.item_text.contains("[keeper:aaaaaaaaaaaa]"));
    let proxy_lesson = items.iter().find(|i| i.lesson_id == Some(f.lessons[0]));
    let proxy_lesson = proxy_lesson.ok_or_else(|| anyhow::anyhow!("proxy lesson missing"))?;
    assert_eq!(proxy_lesson.route, "proxy");
    assert_eq!(proxy_lesson.score, Some(0.75));
    assert_eq!(proxy_lesson.chance, Some(1.0));
    assert_eq!(proxy_lesson.item_text, "Run the narrow test first");

    let by_session = f
        .handle
        .lessons()
        .supplied_items_for_session(f.session, 10, 0)
        .await?;
    assert_eq!(by_session.len(), 6);
    Ok(())
}

#[tokio::test]
async fn verdicts_figures_and_hand_sample_count_ignored_items_as_cost() -> anyhow::Result<()> {
    let f = Fixture::boot().await?;
    let lessons = [
        lesson(f.lessons[0], "proxy", Some(0.75)),
        lesson(f.lessons[1], "proxy", None),
    ];
    f.handle
        .lessons()
        .record_supplied_lessons(f.requests[0], f.session, &lessons)
        .await?;
    let now = Utc::now() + Duration::hours(1);
    let pending = f
        .handle
        .lessons()
        .supplied_items_pending_verdict(10, now - Duration::days(14), now)
        .await?;
    assert_eq!(pending.len(), 2);
    for (item, verdict) in pending.iter().zip(["used", "ignored"]) {
        assert!(
            f.handle
                .lessons()
                .insert_supply_verdict(&NewVerdict {
                    item_id: item.item_id,
                    verdict,
                    rationale: "test",
                    evidence: "",
                    model: "fake",
                    input_text: "input",
                })
                .await?
        );
    }
    let left = f
        .handle
        .lessons()
        .supplied_items_pending_verdict(10, now - Duration::days(14), now)
        .await?;
    assert!(left.is_empty());

    let figures = f
        .handle
        .lessons()
        .supply_figures(Utc::now() - Duration::days(1))
        .await?;
    let row = figures
        .iter()
        .find(|r| r.item_kind == "lesson" && r.route == "proxy")
        .ok_or_else(|| anyhow::anyhow!("no lesson figure"))?;
    assert!(row.judged >= 2 && row.used >= 1 && row.ignored >= 1);
    assert!(row.bytes_ignored >= "Never stash in a shared tree".len() as i64);

    let week = Utc::now() - Duration::days(1);
    let sample = f.handle.lessons().supply_hand_sample(week, 200).await?;
    let mine = sample
        .iter()
        .find(|r| r.supplied_item_id == pending[0].item_id)
        .ok_or_else(|| anyhow::anyhow!("judged item not in this week's sample"))?;
    assert_eq!(mine.hand_verdict, None);
    assert!(
        f.handle
            .lessons()
            .set_supply_hand_verdict(pending[0].item_id, "used", "dawid")
            .await?
    );
    let sample = f.handle.lessons().supply_hand_sample(week, 200).await?;
    let checked = sample
        .iter()
        .find(|r| r.supplied_item_id == pending[0].item_id)
        .ok_or_else(|| anyhow::anyhow!("checked item left the sample"))?;
    assert_eq!(checked.hand_verdict.as_deref(), Some("used"));
    assert_eq!(
        checked.hand_verdict.as_deref(),
        Some(checked.verdict.as_str())
    );
    assert_eq!(checked.hand_checked_by.as_deref(), Some("dawid"));
    Ok(())
}
