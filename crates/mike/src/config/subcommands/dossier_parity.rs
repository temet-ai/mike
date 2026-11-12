use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

use super::DossierParityArgs;
use super::dossier_parity_asof;
use super::dossier_parity_compare::{
    CaptureEffect, DispositionChange, FieldDiff, J2Row, MikeRow, diff_row, disposition_change,
    mike_row, summarise,
};
use crate::config::{CliOverrides, load};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::dossier_parity_reads::DispositionRequestRow as DispositionRow;
use crate::store::persistence::prompt_stream::{StreamRow, StreamSource, StreamWindow};
use crate::wire::genuine::disposition::{LaneFacts, TurnDisposition, turn_disposition};
use crate::wire::genuine::repeat::repeat_key;
use crate::wire::genuine::stream::{StreamMessage, resolve_turns};
use crate::wire::genuine::{CaptureRule, PromptClass, PromptInput, PromptKind, classify_prompt};

const SESSION_CHUNK: usize = 200;
const SAMPLE_SIZE: usize = 50;

pub(super) async fn dossier_parity(args: DossierParityArgs, cli: CliOverrides) -> Result<()> {
    let config = load::load(cli).context("failed to resolve config for dossier-parity")?;
    let Some(database_url) = config.database_url.as_deref() else {
        anyhow::bail!(
            "dossier-parity requires a database_url — persistence is not configured \
             (set it in /settings or via MIKE_DATABASE_URL)"
        );
    };
    let ph = PersistHandle::connect_read_only(database_url)
        .await
        .context("dossier-parity: could not connect to the capture database")?;
    std::fs::create_dir_all(&args.out)
        .with_context(|| format!("could not create {}", args.out.display()))?;
    if args.dispositions {
        run_dispositions(&ph, &args).await
    } else if args.compare_as_of {
        dossier_parity_asof::run_as_of_comparison(&ph, &args).await
    } else {
        run_parity(&ph, &args).await
    }
}

fn read_j2_rows(path: &Path, args: &DossierParityArgs) -> Result<Vec<J2Row>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("could not read {}", path.display()))?;
    let mut rows = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let row: J2Row = serde_json::from_str(line)
            .with_context(|| format!("{}:{}: not a J2 turn row", path.display(), index + 1))?;
        if row.closed_at < args.cutoff {
            rows.push(row);
        }
    }
    Ok(rows)
}

pub(super) fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let text = serde_json::to_string_pretty(value)?;
    std::fs::write(path, text).with_context(|| format!("could not write {}", path.display()))
}

pub(super) fn write_jsonl<T: Serialize>(path: &Path, records: &[T]) -> Result<()> {
    let mut file = std::fs::File::create(path)
        .with_context(|| format!("could not create {}", path.display()))?;
    for record in records {
        writeln!(file, "{}", serde_json::to_string(record)?)?;
    }
    Ok(())
}

async fn session_db_ids(ph: &PersistHandle, uuids: &[String]) -> Result<HashMap<String, i64>> {
    let parsed = uuids
        .iter()
        .map(|uuid| {
            uuid::Uuid::parse_str(uuid)
                .with_context(|| format!("could not resolve session ids: {uuid} is not a UUID"))
        })
        .collect::<Result<Vec<_>>>()?;
    let ids = ph
        .session_db_ids_by_uuid(&parsed)
        .await
        .context("could not resolve session ids")?;
    Ok(ids
        .into_iter()
        .map(|(uuid, id)| (uuid.to_string(), id))
        .collect())
}

fn capture_rule(code: &str) -> Option<CaptureRule> {
    match code {
        "a" => Some(CaptureRule::A),
        "c" => Some(CaptureRule::C),
        "envelope" => Some(CaptureRule::Envelope),
        _ => None,
    }
}

fn to_message(row: &StreamRow, source: StreamSource) -> Option<StreamMessage> {
    let (capture, key) = match source {
        StreamSource::J2Text => {
            let text = row.text.as_deref()?;
            let class = classify_prompt(&PromptInput {
                text,
                last_user_message: None,
                author: row.author.as_deref().unwrap_or("operator"),
                has_agent_id: row.has_agent_id,
            });
            (class, repeat_key(text))
        }
        StreamSource::Stored => {
            let kind = PromptKind::from_stored_code(row.kind_code.as_deref()?)?;
            let rule = row.rule.as_deref().and_then(capture_rule);
            (PromptClass { kind, rule }, row.repeat_key.clone())
        }
    };
    Some(StreamMessage {
        request_id: row.request_id,
        at: row.created_at,
        capture,
        repeat_key: key,
        turn_id: row.turn_id,
    })
}

pub(super) fn horizon(args: &DossierParityArgs) -> DateTime<Utc> {
    args.horizon.unwrap_or_else(Utc::now)
}

pub(super) struct SessionData {
    pub(super) messages: Vec<StreamMessage>,
    pub(super) turns: Vec<J2Row>,
}

pub(super) async fn load_sessions(
    ph: &PersistHandle,
    args: &DossierParityArgs,
    population: &[J2Row],
) -> Result<Vec<SessionData>> {
    let mut by_session: BTreeMap<String, Vec<J2Row>> = BTreeMap::new();
    for row in population {
        by_session
            .entry(row.session_uuid.clone())
            .or_default()
            .push(row.clone());
    }
    let source = stream_source(args);
    let window = StreamWindow {
        from: args.from,
        to: horizon(args),
    };
    let uuids: Vec<String> = by_session.keys().cloned().collect();
    let mut sessions = Vec::new();
    for chunk in uuids.chunks(SESSION_CHUNK) {
        let ids = session_db_ids(ph, chunk).await?;
        let db_ids: Vec<i64> = ids.values().copied().collect();
        let rows = ph.load_prompt_streams(&db_ids, window, source).await?;
        let mut streams: HashMap<i64, Vec<StreamMessage>> = HashMap::new();
        for row in &rows {
            if let Some(message) = to_message(row, source) {
                streams.entry(row.session_id).or_default().push(message);
            }
        }
        for uuid in chunk {
            let Some(messages) = ids.get(uuid).and_then(|id| streams.remove(id)) else {
                continue;
            };
            let turns = by_session.remove(uuid).unwrap_or_default();
            sessions.push(SessionData { messages, turns });
        }
    }
    Ok(sessions)
}

fn stream_source(args: &DossierParityArgs) -> StreamSource {
    if args.source == "stored" {
        StreamSource::Stored
    } else {
        StreamSource::J2Text
    }
}

pub(super) fn population_of(args: &DossierParityArgs) -> Result<Vec<J2Row>> {
    let against = args
        .against
        .as_deref()
        .context("dossier-parity needs --against <turns.jsonl>")?;
    read_j2_rows(against, args)
}

fn resolve_session(
    session: &SessionData,
    args: &DossierParityArgs,
    out: &mut BTreeMap<i64, MikeRow>,
) {
    let at_horizon = resolve_turns(&session.messages, horizon(args));
    for turn in &session.turns {
        let resolutions = if args.as_of == "close" {
            resolve_turns(&session.messages, turn.closed_at)
        } else {
            at_horizon.clone()
        };
        if let Some(found) = resolutions.iter().find(|r| r.turn_id == turn.turn_id) {
            out.insert(turn.turn_id, mike_row(found));
        }
    }
}

async fn run_parity(ph: &PersistHandle, args: &DossierParityArgs) -> Result<()> {
    let population = population_of(args)?;
    let sessions = load_sessions(ph, args, &population).await?;
    let mut mikes = BTreeMap::new();
    for session in &sessions {
        resolve_session(session, args, &mut mikes);
    }
    let mut diffs: Vec<FieldDiff> = Vec::new();
    for row in &population {
        if let Some(mike) = mikes.get(&row.turn_id) {
            diffs.extend(diff_row(row, mike));
        }
    }
    let mut summary = summarise(&population, &mikes, &diffs);
    summary.horizon = args.horizon;
    summary.as_of = args.as_of.clone();
    summary.source = args.source.clone();
    write_json(&args.out.join("parity-summary.json"), &summary)?;
    write_jsonl(&args.out.join("parity-diff.jsonl"), &diffs)?;
    println!(
        "dossier-parity: {} turns, {} compared, {} differing turns ({} differences), {} unmapped, sample {}/{} differing, controls {}/{} differing",
        summary.population,
        summary.compared,
        summary.differing_turns,
        diffs.len(),
        summary.unmapped_turns.len(),
        summary.sample_differing_turns,
        summary.sample_turns,
        summary.control_differing_turns,
        summary.control_turns
    );
    if args.max_diff.is_some_and(|max| diffs.len() > max) {
        std::process::exit(2);
    }
    Ok(())
}

#[derive(Debug, Serialize)]
struct DispositionRecord {
    request_id: i64,
    session_id: i64,
    created_at: DateTime<Utc>,
    origin: String,
    turn_type: String,
    has_agent_id: bool,
    kind: &'static str,
    at_capture: &'static str,
    disposition: &'static str,
    change: DispositionChange,
    human_prompt: String,
}

fn at_capture(row: &DispositionRow) -> &'static str {
    match (row.opened, row.closed_turn, row.attached) {
        (true, true, _) => "closed_and_opened",
        (true, false, _) => "opened",
        (false, true, _) => "closed",
        (false, false, true) => "attached",
        _ => "nothing",
    }
}

fn record_of(row: DispositionRow) -> (DispositionRecord, DispositionChange) {
    let class = classify_prompt(&PromptInput {
        text: &row.human_prompt,
        last_user_message: Some(&row.last_user_message),
        author: &row.author,
        has_agent_id: row.has_agent_id,
    });
    let disposition = turn_disposition(
        &LaneFacts {
            turn_origin: &row.origin,
            is_user_prompt_type: row.turn_type == "user_prompt",
            has_agent_id: row.has_agent_id,
        },
        &class,
    );
    let effect = CaptureEffect {
        opened: row.opened,
        closed: row.closed_turn,
    };
    let change = disposition_change(effect, disposition);
    let capture = at_capture(&row);
    let record = DispositionRecord {
        request_id: row.request_id,
        session_id: row.session_id,
        created_at: row.created_at,
        origin: row.origin,
        turn_type: row.turn_type,
        has_agent_id: row.has_agent_id,
        kind: class.kind.stored_code(),
        at_capture: capture,
        disposition: match disposition {
            TurnDisposition::OperatorTurn => "operator_turn",
            TurnDisposition::MachineSegment => "machine_segment",
            TurnDisposition::Attach => "attach",
            TurnDisposition::Ignore => "ignore",
        },
        change,
        human_prompt: row.human_prompt,
    };
    (record, change)
}

fn sample_candidates(records: &[&DispositionRecord]) -> Vec<serde_json::Value> {
    let step = records.len().div_ceil(SAMPLE_SIZE).max(1);
    records
        .iter()
        .step_by(step)
        .take(SAMPLE_SIZE)
        .map(|r| {
            serde_json::json!({
                "request_id": r.request_id,
                "session_id": r.session_id,
                "created_at": r.created_at,
                "kind": r.kind,
                "has_agent_id": r.has_agent_id,
                "human_prompt": r.human_prompt,
                "reviewer_call": null,
            })
        })
        .collect()
}

async fn run_dispositions(ph: &PersistHandle, args: &DossierParityArgs) -> Result<()> {
    let mut summary: BTreeMap<String, u64> = BTreeMap::new();
    let mut changed: Vec<DispositionRecord> = Vec::new();
    let mut day = args.from;
    while day < args.cutoff {
        let next = (day + Duration::days(1)).min(args.cutoff);
        let rows = ph
            .list_disposition_requests(day, next)
            .await
            .with_context(|| format!("dispositions: query failed for {day}"))?;
        for row in rows {
            let (record, change) = record_of(row);
            let lane = if record.has_agent_id {
                "agent_id"
            } else {
                "own_lane"
            };
            let key = format!("{change:?}|{}|{lane}", record.kind);
            *summary.entry(key).or_default() += 1;
            if change != DispositionChange::Unchanged {
                changed.push(record);
            }
        }
        day = next;
    }
    let losing: Vec<&DispositionRecord> = changed
        .iter()
        .filter(|r| r.change == DispositionChange::LosesOperatorTurn)
        .collect();
    write_json(&args.out.join("dispositions-summary.json"), &summary)?;
    write_jsonl(&args.out.join("dispositions-diff.jsonl"), &changed)?;
    write_jsonl(
        &args.out.join("dispositions-sample-candidates.jsonl"),
        &sample_candidates(&losing),
    )?;
    println!(
        "dossier-parity --dispositions: {} requests change effect, {} lose an operator turn",
        changed.len(),
        losing.len()
    );
    Ok(())
}
