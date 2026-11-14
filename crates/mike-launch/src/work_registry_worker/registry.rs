use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};

use super::delivery::{self, Decision};
use super::{ACTIVE_WINDOW_HOURS, SWEEP_EVERY, WAKE_MODE_TTL};
use crate::change_feed::Drained;
use crate::launch_settings::meta_work_registry::WAKE_MODE_KEY;
use crate::store::persistence::work_item_attention::{AttentionKey, AttentionRecord};
use crate::store::persistence::{PersistError, PersistHandle};
use crate::work_items::attention::Attention;
use crate::work_items::ledger_cache::LedgerCache;

#[derive(Default)]
pub struct Registry {
    pub cache: LedgerCache,
    pub known: HashMap<AttentionKey, AttentionRecord>,
    held: bool,
    last_sweep: Option<Instant>,
    wake_mode: Option<(Instant, bool)>,
}

pub fn key_of(session_db_id: i64, item: &Attention) -> AttentionKey {
    AttentionKey {
        session_db_id,
        kind: item.kind.to_owned(),
        subject: item.subject.clone(),
    }
}

impl Registry {
    pub fn release(&mut self) {
        *self = Self::default();
    }

    pub async fn step(
        &mut self,
        persist: &PersistHandle,
        drained: Drained,
        now: DateTime<Utc>,
    ) -> Result<(), PersistError> {
        let full = drained.everything || !self.held;
        self.held = true;
        let sweep = self.last_sweep.is_none_or(|at| at.elapsed() >= SWEEP_EVERY);
        if full || sweep {
            self.load_all(persist, full).await?;
            self.last_sweep = Some(Instant::now());
        } else {
            self.load_named(persist, &drained.sessions).await?;
        }
        self.act(persist, now).await
    }

    async fn load_all(&mut self, persist: &PersistHandle, reset: bool) -> Result<(), PersistError> {
        let rows = persist
            .launch()
            .work_item_scope(ACTIVE_WINDOW_HOURS, None)
            .await?;
        self.cache.apply_scope(rows, None);
        let ids = self.cache.ids();
        if reset {
            self.known.clear();
            for id in &ids {
                self.cache.forget_reads(*id);
            }
        }
        self.read(persist, &ids).await
    }

    async fn load_named(
        &mut self,
        persist: &PersistHandle,
        sessions: &[String],
    ) -> Result<(), PersistError> {
        let named: Vec<uuid::Uuid> = sessions
            .iter()
            .filter_map(|s| uuid::Uuid::parse_str(s).ok())
            .collect();
        if named.is_empty() {
            return Ok(());
        }
        let rows = persist
            .launch()
            .work_item_scope(ACTIVE_WINDOW_HOURS, Some(&named))
            .await?;
        self.cache.apply_scope(rows, Some(sessions));
        let ids = self.cache.ids_of(sessions);
        self.read(persist, &ids).await
    }

    pub async fn reload_session(
        &mut self,
        persist: &PersistHandle,
        session_db_id: i64,
        now: DateTime<Utc>,
    ) -> Result<Vec<Attention>, PersistError> {
        let Some(uuid) = self.cache.session_uuid(session_db_id).map(str::to_owned) else {
            return Ok(Vec::new());
        };
        self.cache.forget_reads(session_db_id);
        self.load_named(persist, std::slice::from_ref(&uuid))
            .await?;
        Ok(self
            .cache
            .ledger(session_db_id, now)
            .map(|ledger| ledger.attention)
            .unwrap_or_default())
    }

    async fn read(&mut self, persist: &PersistHandle, ids: &[i64]) -> Result<(), PersistError> {
        let after = self.cache.reads_after(ids);
        let reads = persist.launch().work_item_reads(&after).await?;
        self.cache.apply_reads(&reads);
        let live = self.cache.ids();
        self.known
            .retain(|key, _| live.binary_search(&key.session_db_id).is_ok());
        Ok(())
    }

    fn assessed(&self, now: DateTime<Utc>) -> Vec<(i64, Vec<Attention>)> {
        self.cache
            .ids()
            .into_iter()
            .filter_map(|id| {
                let attention = self.cache.ledger(id, now)?.attention;
                (!attention.is_empty()).then_some((id, attention))
            })
            .collect()
    }

    async fn record_unknown(
        &mut self,
        persist: &PersistHandle,
        assessed: &[(i64, Vec<Attention>)],
    ) -> Result<(), PersistError> {
        let unknown: Vec<(i64, &Attention)> = assessed
            .iter()
            .flat_map(|(id, items)| items.iter().map(move |item| (*id, item)))
            .filter(|(id, item)| !self.known.contains_key(&key_of(*id, item)))
            .collect();
        for (key, record) in persist
            .launch()
            .record_work_item_attentions(&unknown)
            .await?
        {
            if record.inserted {
                self.log_new(&key, &unknown);
            }
            self.known.insert(key, record);
        }
        Ok(())
    }

    fn log_new(&self, key: &AttentionKey, items: &[(i64, &Attention)]) {
        let Some((_, item)) = items.iter().find(|(id, item)| key_of(*id, item) == *key) else {
            return;
        };
        tracing::info!(
            session_id = %self.cache.session_uuid(key.session_db_id).unwrap_or_default(),
            kind = item.kind,
            subject = %item.subject,
            message = %item.message,
            "work_registry: attention"
        );
    }

    async fn deliver_mode(&mut self, persist: &PersistHandle) -> Result<bool, PersistError> {
        if let Some((at, deliver)) = self.wake_mode
            && at.elapsed() < WAKE_MODE_TTL
        {
            return Ok(deliver);
        }
        let deliver = persist
            .launch()
            .setting_text(WAKE_MODE_KEY)
            .await?
            .as_deref()
            == Some("deliver");
        self.wake_mode = Some((Instant::now(), deliver));
        Ok(deliver)
    }

    async fn act(
        &mut self,
        persist: &PersistHandle,
        now: DateTime<Utc>,
    ) -> Result<(), PersistError> {
        let assessed = self.assessed(now);
        if assessed.is_empty() {
            return Ok(());
        }
        self.record_unknown(persist, &assessed).await?;
        if !assessed
            .iter()
            .flat_map(|(id, items)| items.iter().map(move |item| key_of(*id, item)))
            .any(|key| self.known.get(&key).is_some_and(delivery::may_act))
        {
            return Ok(());
        }
        let deliver = self.deliver_mode(persist).await?;
        for (id, items) in &assessed {
            self.act_on_session(persist, *id, items, deliver, now)
                .await?;
        }
        Ok(())
    }

    async fn act_on_session(
        &mut self,
        persist: &PersistHandle,
        session_db_id: i64,
        items: &[Attention],
        deliver: bool,
        now: DateTime<Utc>,
    ) -> Result<(), PersistError> {
        let mut still_true: Option<Vec<Attention>> = None;
        for item in items {
            let key = key_of(session_db_id, item);
            let Some(recorded) = self.known.get(&key).cloned() else {
                continue;
            };
            match delivery::decide(deliver, item, &recorded, Utc::now()) {
                Decision::Nothing => {}
                Decision::SettleStale => {
                    delivery::settle(persist, &recorded, "stale", delivery::STALE_REASON).await?;
                    self.settled(&key, "stale");
                }
                Decision::Mail => {
                    if still_true.is_none() {
                        delivery::rescan(persist, session_db_id).await?;
                        still_true = Some(self.reload_session(persist, session_db_id, now).await?);
                    }
                    self.mail_if_still_true(persist, &key, item, &recorded, still_true.as_deref())
                        .await?;
                }
            }
        }
        Ok(())
    }

    async fn mail_if_still_true(
        &mut self,
        persist: &PersistHandle,
        key: &AttentionKey,
        item: &Attention,
        recorded: &AttentionRecord,
        still_true: Option<&[Attention]>,
    ) -> Result<(), PersistError> {
        let Some(current) = delivery::current(still_true, item) else {
            delivery::settle(persist, recorded, "resolved", delivery::RESOLVED_REASON).await?;
            self.settled(key, "resolved");
            return Ok(());
        };
        let session_uuid = self
            .cache
            .session_uuid(key.session_db_id)
            .unwrap_or_default()
            .to_owned();
        let after = delivery::mail(persist, &session_uuid, current, recorded).await;
        self.known.insert(key.clone(), after);
        Ok(())
    }

    fn settled(&mut self, key: &AttentionKey, disposition: &str) {
        if let Some(record) = self.known.get_mut(key)
            && record.disposition == "pending"
        {
            disposition.clone_into(&mut record.disposition);
        }
    }

    pub fn sleep_for(&self, now: DateTime<Utc>, cap: Duration) -> Duration {
        let due = [
            self.cache.next_change(now),
            delivery::next_retry(self.known.values()),
        ]
        .into_iter()
        .flatten()
        .min();
        due.and_then(|at| (at - now + chrono::Duration::seconds(1)).to_std().ok())
            .map_or(cap, |wait| wait.min(cap))
    }
}
