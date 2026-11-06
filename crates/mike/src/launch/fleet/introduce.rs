use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use super::definition::FleetDefinition;
use super::work_of::work_through;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::{ChangedPathRow, FleetRow, SlowCommandRow};

const SLOW_SECONDS: f64 = 5.0;
const LOOK_BACK_HOURS: i64 = 24;

#[path = "introduce_receipt.rs"]
mod receipt;

#[derive(Debug, Clone, PartialEq)]
pub struct Repeat {
    pub work: String,
    pub first: SlowCommandRow,
    pub again: SlowCommandRow,
    pub first_seconds: f64,
    pub again_seconds: f64,
}

pub async fn introduce(persist: &PersistHandle, row: &FleetRow) {
    let (Ok(def), Some(run)) = (FleetDefinition::from_value(&row.definition), &row.run_id) else {
        return;
    };
    let since = row
        .started_at
        .unwrap_or_else(|| Utc::now() - Duration::hours(LOOK_BACK_HOURS));
    let fresh = match fresh_repeats(persist, run, since, &def).await {
        Ok(fresh) => fresh,
        Err(err) => return note_unread(run, &err),
    };
    let Some(earliest) = fresh.iter().map(|r| r.first.started_at).min() else {
        return;
    };
    let changed = match persist
        .tree_changed_paths_since(run, earliest - Duration::microseconds(1))
        .await
    {
        Ok(changed) => changed,
        Err(err) => return note_unread(run, &err.to_string()),
    };
    let held: Vec<Repeat> = fresh
        .into_iter()
        .filter(|r| no_observed_invalidation(&changed, r))
        .collect();
    mail_the_claimed(persist, run, held, &changed).await;
}

async fn mail_the_claimed(
    persist: &PersistHandle,
    run: &str,
    held: Vec<Repeat>,
    changed: &[ChangedPathRow],
) {
    let pairs: Vec<(i64, i64)> = held
        .iter()
        .map(|r| (r.again.use_id, r.first.use_id))
        .collect();
    let claimed = match persist.record_introductions(run, &pairs).await {
        Ok(claimed) => claimed,
        Err(err) => return note_unread(run, &err.to_string()),
    };
    for repeat in held.iter().filter(|r| claimed.contains(&r.again.use_id)) {
        let sent = persist
            .insert_message(
                Uuid::new_v4(),
                repeat.again.session,
                None,
                &mail(repeat, changed),
            )
            .await;
        release_unqueued(run, sent, persist.release_introduction(repeat.again.use_id)).await;
    }
}

async fn release_unqueued<T, E: std::fmt::Display>(
    run: &str,
    sent: Result<Option<T>, E>,
    release: impl std::future::Future<Output = Result<(), E>>,
) {
    match sent {
        Ok(Some(_)) => return,
        Ok(None) => note_unsent(run, "recipient unavailable; message was not queued"),
        Err(err) => note_unsent(run, &err.to_string()),
    }
    if let Err(err) = release.await {
        note_unsent(run, &err.to_string());
    }
}

async fn fresh_repeats(
    persist: &PersistHandle,
    run: &str,
    since: DateTime<Utc>,
    def: &FleetDefinition,
) -> Result<Vec<Repeat>, String> {
    let slow = persist
        .slow_commands(run, since, SLOW_SECONDS)
        .await
        .map_err(|e| e.to_string())?;
    let recorded = super::build_timing::read_run(&super::seat_settings::timings_dir(def), run);
    let (rows, timings) = super::build_timing::matched(&slow, &recorded, &def.wrappers);
    let found = repeats_with_timings(&rows, &def.wrappers, &timings);
    let again: Vec<i64> = found.iter().map(|r| r.again.use_id).collect();
    let recorded = persist
        .recorded_introductions(&again)
        .await
        .map_err(|e| e.to_string())?;
    Ok(found
        .into_iter()
        .filter(|r| !recorded.contains(&r.again.use_id))
        .collect())
}

fn note_unread(run: &str, err: &str) {
    tracing::warn!(%run, %err, "fleet: could not read which seats repeated each other's commands");
}

fn note_unsent(run: &str, err: &str) {
    tracing::warn!(%run, %err, "fleet: could not tell the second seat that another seat ran the same command");
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommandTiming {
    pub use_id: i64,
    pub work: String,
    pub seconds: f64,
}

pub fn repeats(slow: &[SlowCommandRow], wrappers: &[String]) -> Vec<Repeat> {
    repeats_with_timings(slow, wrappers, &[])
}

fn timed_work<'a>(
    row: &SlowCommandRow,
    wrappers: &[String],
    timings: &'a [CommandTiming],
) -> Vec<&'a CommandTiming> {
    if row.in_background {
        return Vec::new();
    }
    let work = work_through(&row.command, wrappers);
    timings
        .iter()
        .filter(|timing| timing.use_id == row.use_id && work.contains(&timing.work))
        .filter(|timing| timing.seconds.is_finite() && timing.seconds >= SLOW_SECONDS)
        .collect()
}

pub fn repeats_with_timings(
    slow: &[SlowCommandRow],
    wrappers: &[String],
    timings: &[CommandTiming],
) -> Vec<Repeat> {
    let worked: Vec<_> = slow
        .iter()
        .map(|row| (row, timed_work(row, wrappers, timings)))
        .collect();
    let mut out = Vec::new();
    for (again, again_work) in &worked {
        let earlier = worked
            .iter()
            .filter(|(first, _)| first.seat != again.seat && first.finished_at <= again.started_at)
            .filter_map(|(first, first_work)| {
                again_work.iter().find_map(|again_timing| {
                    first_work
                        .iter()
                        .find(|first_timing| first_timing.work == again_timing.work)
                        .map(|first_timing| (*first, *first_timing, *again_timing))
                })
            })
            .max_by_key(|(first, _, _)| (first.finished_at, first.use_id));
        if let Some((first, first_timing, again_timing)) = earlier {
            out.push(Repeat {
                work: again_timing.work.clone(),
                first: first.clone(),
                again: (*again).clone(),
                first_seconds: first_timing.seconds,
                again_seconds: again_timing.seconds,
            });
        }
    }
    out
}

pub fn changed_between(
    changed: &[ChangedPathRow],
    after: DateTime<Utc>,
    until: DateTime<Utc>,
) -> usize {
    changed
        .iter()
        .filter(|c| c.seen_at > after && c.seen_at <= until)
        .map(|c| c.path.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len()
}

pub fn no_observed_invalidation(changed: &[ChangedPathRow], repeat: &Repeat) -> bool {
    !repeat.first.in_background
        && !changed
            .iter()
            .any(|change| change.seen_at >= repeat.first.started_at)
}

pub fn mail(repeat: &Repeat, changed: &[ChangedPathRow]) -> String {
    format!(
        "[mike introduce] {again} (session {again_session}) spent {seconds} s on a command previously run by {first} (session {first_session}).\nUse the captured receipt below, including when its seat is closed or its task is accepted. No contact with the first seat is required. Tool success is not a test or lint pass. Commit binding and dependency validity are unknown unless recorded explicitly; no observed changes is not proof of complete change coverage.\n{receipt}",
        again = repeat.again.seat,
        again_session = repeat.again.session,
        first = repeat.first.seat,
        first_session = repeat.first.session,
        seconds = repeat.again_seconds,
        receipt = receipt::handoff(repeat, changed),
    )
}

#[cfg(test)]
#[path = "introduce_delivery_tests.rs"]
mod delivery_tests;

#[cfg(test)]
#[path = "introduce_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "introduce_timing_tests.rs"]
mod timing_tests;
