use chrono::{Duration, Utc};
use mike::analysis::{EmbeddingConfig, EmbeddingService};
use mike::semantic::SemanticConfig;
use mike::semantic::builder::Builder;
use mike::store::persistence::dossier_repo::DossierPersist as _;
use mike::store::persistence::semantic::{EmbedderRow, SemanticWindow};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::turn_facts::{Fixture, NewTurn, Req, insert_request, insert_turn, sample_row};

pub(crate) const DIM: usize = 768;

pub(crate) fn vector_for(text: &str) -> Vec<f32> {
    let body = text.trim_start_matches("search_document: ");
    let mut v = vec![0.0_f32; DIM];
    for (i, c) in body.chars().enumerate() {
        let slot = (u32::from(c) as usize * 31 + i % 7) % DIM;
        v[slot] += 1.0;
    }
    v[DIM - 1] += 0.01;
    v
}

fn responder() -> impl wiremock::Respond {
    |req: &wiremock::Request| {
        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or_default();
        let data: Vec<serde_json::Value> = body["input"]
            .as_array()
            .map(|inputs| {
                inputs
                    .iter()
                    .map(|t| serde_json::json!({ "embedding": vector_for(t.as_str().unwrap_or_default()) }))
                    .collect()
            })
            .unwrap_or_default();
        ResponseTemplate::new(200).set_body_json(serde_json::json!({ "data": data }))
    }
}

pub(crate) async fn embedding_stub() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(responder())
        .mount(&server)
        .await;
    server
}

pub(crate) fn service(url: &str) -> EmbeddingService {
    EmbeddingService::new(&EmbeddingConfig {
        enabled: true,
        url: url.to_owned(),
        model: "nomic-embed-text-v1.5".to_owned(),
        chunk_max_chars: 1024,
    })
}

pub(crate) async fn closed_turn(
    f: &Fixture,
    id: i64,
    prompt: &str,
    secs: i64,
) -> anyhow::Result<i64> {
    insert_request(f, &Req::primary(id, secs, prompt)).await?;
    let turn = insert_turn(f, &NewTurn::closed(id, id, secs, secs + 20)).await?;
    sqlx::query("UPDATE memory.operator_turns SET user_prompt = $2, final_assistant_text = $3 WHERE id = $1")
        .bind(turn)
        .bind(prompt)
        .bind(format!("I worked on: {prompt}"))
        .execute(f.handle.pool())
        .await?;
    Ok(turn)
}

pub(crate) async fn facts(f: &Fixture, turn: i64, next_turn: Option<i64>) -> anyhow::Result<()> {
    let mut row = sample_row(turn, f.session_id, Utc::now());
    crate::turn_facts::resolve(&mut row);
    row.next_genuine_turn_id = next_turn;
    row.next_genuine_at = Some(f.base + Duration::seconds(30));
    f.handle.dossier().upsert_turn_facts(&[row]).await?;
    Ok(())
}

pub(crate) async fn embedder(f: &Fixture, svc: &EmbeddingService) -> anyhow::Result<EmbedderRow> {
    Ok(mike::semantic::check::ensure_embedder(&f.handle, svc, false).await?)
}

pub(crate) async fn build_all(
    f: &Fixture,
    svc: &EmbeddingService,
    embedder: &EmbedderRow,
) -> anyhow::Result<usize> {
    let config = SemanticConfig::default();
    let builder = Builder {
        persist: &f.handle,
        embedding: svc,
        config: &config,
        embedder,
    };
    let window = SemanticWindow {
        from: None,
        until: None,
        after_turn: 0,
        limit: 64,
    };
    Ok(builder.build_batch(&window).await?.turns)
}

pub(crate) async fn floors_in_force(f: &Fixture, key: &str, floor: f64) -> anyhow::Result<()> {
    for pair in [
        "prompt:corr",
        "ending:corr",
        "prompt:prompt",
        "ending:ending",
        "corr:corr",
        "prompt:directive",
    ] {
        sqlx::query(
            "INSERT INTO memory.semantic_floors
                (embedder_key, pair, quantile, floor, n_pairs, p50, p95, p99, p999, sample, builder_version, status)
             VALUES ($1, $2, 0.999, $3, 20000, 0.1, 0.2, $3, $3, '{}'::jsonb, 'test', 'in_force')",
        )
        .bind(key)
        .bind(pair)
        .bind(floor)
        .execute(f.handle.pool())
        .await?;
    }
    Ok(())
}

pub(crate) async fn label(f: &Fixture, turn: i64, tier: &str) -> anyhow::Result<()> {
    let method = if tier == "seed" {
        "single_model"
    } else {
        "operator_click"
    };
    crate::turn_facts::dev_split(f, turn).await?;
    let batch = if tier == "seed" {
        Some(crate::turn_facts::test_import_batch(f).await?)
    } else {
        None
    };
    sqlx::query(
        "INSERT INTO eval.labels (subject_kind, subject_id, operator_turn_id, session_id, session_uuid,
                                  axis, label, tier, method, split, source, import_batch_id)
         SELECT 'turn', t.id, t.id, t.session_id, s.session_uuid, 'reaction', 'corrected', $2, $3, 'dev', 'test', $4
           FROM memory.operator_turns t JOIN sessions.sessions s ON s.id = t.session_id
          WHERE t.id = $1",
    )
    .bind(turn)
    .bind(tier)
    .bind(method)
    .bind(batch)
    .execute(f.handle.pool())
    .await?;
    Ok(())
}
