use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::sync::atomic::{AtomicBool, Ordering};

use uuid::Uuid;

use super::fleet_settings::RecordedSettings;
use super::ledger_wire::ClosedBody;
use super::proxy_ledger::ProxyLedger;
use super::queue::{LocalQueue, QueuedLaunch};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::launch::{
    BackendValidationRow, InsertedTemplate, LaunchListFilter, LaunchOutcome, LaunchRow,
    LaunchSurfaceRefs, LinkOutcome, NewBackendValidation, NewLaunch, NewRun, NewTemplate, RunRow,
    TemplateRow,
};

enum Store {
    Database(PersistHandle),
    Proxy(ProxyLedger),
    Absent,
}

pub struct Ledger {
    store: Store,
    mail_briefs: bool,
    queue: Option<LocalQueue>,
    lost: AtomicBool,
}

const NO_STORE: &str = "no database and no proxy to record through";

fn note(what: &str, err: impl std::fmt::Display) {
    eprintln!("mike spawn: ledger {what} failed: {err}");
}

fn noted<T>(what: &str, result: Result<T, String>) -> Option<T> {
    result.map_err(|e| note(what, e)).ok()
}

impl Ledger {
    pub fn new(handle: Option<PersistHandle>) -> Self {
        Self {
            store: handle.map_or(Store::Absent, Store::Database),
            mail_briefs: false,
            queue: None,
            lost: AtomicBool::new(false),
        }
    }

    pub fn none() -> Self {
        Self::new(None)
    }

    pub fn through_proxy(proxy: ProxyLedger) -> Self {
        Self {
            store: Store::Proxy(proxy),
            mail_briefs: false,
            queue: None,
            lost: AtomicBool::new(false),
        }
    }

    pub fn with_queue(mut self, queue: Option<LocalQueue>) -> Self {
        self.queue = queue;
        self
    }

    pub fn with_mail_briefs(mut self, enabled: bool) -> Self {
        self.mail_briefs = enabled && self.available();
        self
    }

    pub async fn read_mail_briefs(self) -> Self {
        let Store::Database(h) = &self.store else {
            return self;
        };
        let enabled = match h.fetch_feature_toggles().await {
            Ok(rows) => rows
                .iter()
                .find(|(key, _, _)| key == "launch_brief")
                .map_or(
                    crate::feature_toggles::FeatureToggles::default().launch_brief,
                    |r| r.1,
                ),
            Err(e) => {
                note("launch_brief toggle read", e);
                false
            }
        };
        self.with_mail_briefs(enabled)
    }

    pub fn available(&self) -> bool {
        !matches!(self.store, Store::Absent)
    }

    pub fn route(&self) -> String {
        match &self.store {
            Store::Database(_) => "the database".to_string(),
            Store::Proxy(p) => format!("the proxy at {}", p.addr()),
            Store::Absent => "nothing".to_string(),
        }
    }

    pub fn mail_briefs(&self) -> bool {
        self.mail_briefs
    }

    pub fn fleet_database(&self) -> Result<&PersistHandle, String> {
        self.handle()
    }

    fn handle(&self) -> Result<&PersistHandle, String> {
        match &self.store {
            Store::Database(h) => Ok(h),
            Store::Proxy(p) => Err(format!(
                "the database is unreachable from this process; the ledger goes through the \
                 proxy at {}, which records launches but does not serve this",
                p.addr()
            )),
            Store::Absent => Err("database unreachable; no ledger".to_string()),
        }
    }

    pub async fn try_upsert_run(&self, run: &NewRun) -> Result<(), String> {
        match &self.store {
            Store::Database(h) => h.launch().upsert_run(run).await.map_err(|e| e.to_string()),
            Store::Proxy(p) => p.upsert_run(run).await,
            Store::Absent => Err(NO_STORE.to_string()),
        }
    }

    pub async fn try_insert_launch(&self, new: &NewLaunch) -> Result<i64, String> {
        match &self.store {
            Store::Database(h) => h
                .launch()
                .insert_launch(new)
                .await
                .map_err(|e| e.to_string()),
            Store::Proxy(p) => p.insert_launch(new).await,
            Store::Absent => Err(NO_STORE.to_string()),
        }
    }

    pub async fn try_record_opened(&self, id: i64, refs: &LaunchSurfaceRefs) -> Result<(), String> {
        match &self.store {
            Store::Database(h) => h
                .launch()
                .record_launch_opened(id, refs)
                .await
                .map_err(|e| e.to_string()),
            Store::Proxy(p) => p.record_opened(id, refs).await,
            Store::Absent => Err(NO_STORE.to_string()),
        }
    }

    pub async fn try_record_outcome(&self, id: i64, outcome: &LaunchOutcome) -> Result<(), String> {
        match &self.store {
            Store::Database(h) => h
                .launch()
                .record_launch_outcome(id, outcome)
                .await
                .map_err(|e| e.to_string()),
            Store::Proxy(p) => p.record_outcome(id, outcome).await,
            Store::Absent => Err(NO_STORE.to_string()),
        }
    }

    pub async fn try_record_closed(&self, id: i64, body: &ClosedBody) -> Result<(), String> {
        match &self.store {
            Store::Database(h) => h
                .launch()
                .record_launch_closed(id, &body.reason, body.process_exited)
                .await
                .map_err(|e| e.to_string()),
            Store::Proxy(p) => p.record_closed(id, body).await,
            Store::Absent => Err(NO_STORE.to_string()),
        }
    }

    pub async fn try_find_launch_by_session(
        &self,
        session: Uuid,
    ) -> Result<Option<LaunchRow>, String> {
        match &self.store {
            Store::Database(h) => h
                .launch()
                .find_launch_by_session(session)
                .await
                .map_err(|e| e.to_string()),
            Store::Proxy(p) => p.find_launch_by_session(session).await,
            Store::Absent => Err(NO_STORE.to_string()),
        }
    }

    fn lose(&self, what: &str, why: &str) {
        self.lost.store(true, Ordering::Relaxed);
        note(what, format!("{why}; the record is lost"));
    }

    fn queue_or_lose<T>(&self, what: &str, queued: Option<Result<T, String>>) -> Option<T> {
        match queued {
            Some(Ok(value)) => {
                if let Some(q) = &self.queue {
                    eprintln!(
                        "mike spawn: ledger {what} queued in {}; it is replayed when the database \
                         or the proxy answers",
                        q.dir().display()
                    );
                }
                Some(value)
            }
            Some(Err(why)) => {
                self.lose(what, &why);
                None
            }
            None => {
                self.lose(what, "no ledger and no local queue");
                None
            }
        }
    }

    pub async fn upsert_run(&self, run: &NewRun) -> bool {
        match self.try_upsert_run(run).await {
            Ok(()) => true,
            Err(e) if self.available() => {
                note("run upsert", e);
                false
            }
            Err(_) => {
                let queued = self.queue.as_ref().map(|q| q.save_run(run));
                self.queue_or_lose("run", queued).is_some()
            }
        }
    }

    pub async fn find_run(&self, run_id: &str) -> Option<RunRow> {
        let result = match &self.store {
            Store::Database(h) => h.launch().find_run(run_id).await.map_err(|e| e.to_string()),
            Store::Proxy(p) => p.find_run(run_id).await,
            Store::Absent => return None,
        };
        noted("run lookup", result).flatten()
    }

    pub async fn fleet_settings(&self, key: &str) -> Result<Option<RecordedSettings>, String> {
        match &self.store {
            Store::Database(h) => h
                .fleet()
                .find_fleet(key)
                .await
                .map(|row| {
                    row.map(|row| RecordedSettings {
                        run_id: row.run_id,
                        definition: row.definition,
                    })
                })
                .map_err(|e| e.to_string()),
            Store::Proxy(p) => p.fleet_settings(key).await,
            Store::Absent => Err(NO_STORE.to_string()),
        }
    }

    pub async fn insert_launch(&self, new: &NewLaunch) -> Option<i64> {
        match self.try_insert_launch(new).await {
            Ok(id) => Some(id),
            Err(e) => {
                if self.available() {
                    note("insert", e);
                }
                let queued = self.queue.as_ref().map(|q| q.save_launch(new));
                self.queue_or_lose("launch", queued)
            }
        }
    }

    fn queued_update(&self, what: &str, id: i64, change: impl FnOnce(&mut QueuedLaunch)) {
        let queued = self.queue.as_ref().map(|q| q.update_launch(id, change));
        self.queue_or_lose(what, queued);
    }

    pub async fn record_opened(&self, id: Option<i64>, refs: &LaunchSurfaceRefs) {
        match id {
            Some(id) if id < 0 => {
                self.queued_update("opened update", id, |q| q.opened = Some(refs.clone()));
            }
            Some(id) if self.available() => {
                noted("opened update", self.try_record_opened(id, refs).await);
            }
            _ => {}
        }
    }

    pub async fn record_outcome(&self, id: Option<i64>, outcome: &LaunchOutcome) {
        match id {
            Some(id) if id < 0 => {
                self.queued_update("outcome update", id, |q| q.outcome = Some(outcome.clone()));
            }
            Some(id) if self.available() => {
                noted("outcome update", self.try_record_outcome(id, outcome).await);
            }
            _ => {}
        }
    }

    pub async fn record_kill(&self, launch: Option<i64>, session: Uuid, body: ClosedBody) {
        if let (Some(id), true) = (launch, self.available())
            && self.try_record_closed(id, &body).await.is_ok()
        {
            return;
        }
        if launch.is_none() && self.available() {
            return;
        }
        let queued = self.queue.as_ref().map(|q| q.save_close(session, body));
        self.queue_or_lose("close", queued);
    }

    #[cfg(any(test, feature = "test-seams"))]
    pub fn lost_records(&self) -> bool {
        self.lost.load(Ordering::Relaxed)
    }

    pub fn queue(&self) -> Option<&LocalQueue> {
        self.queue.as_ref()
    }

    pub async fn find_launch_by_session(&self, session: Uuid) -> Option<LaunchRow> {
        if !self.available() {
            return None;
        }
        noted("lookup", self.try_find_launch_by_session(session).await).flatten()
    }

    pub async fn list_launches(&self, filter: &LaunchListFilter) -> Option<Vec<LaunchRow>> {
        let result = match &self.store {
            Store::Database(h) => h
                .launch()
                .list_launches(filter)
                .await
                .map_err(|e| e.to_string()),
            Store::Proxy(p) => p.list_launches(filter).await,
            Store::Absent => return None,
        };
        noted("list", result)
    }

    pub async fn channel_present(&self, session: Uuid) -> Result<bool, String> {
        match &self.store {
            Store::Proxy(p) => p.channel_present(session).await,
            _ => self
                .handle()?
                .launch()
                .session_has_remote_control_channel(session)
                .await
                .map_err(|e| e.to_string()),
        }
    }

    pub async fn prompt_turn_seen(
        &self,
        session: Uuid,
        since: chrono::DateTime<chrono::Utc>,
        prefix: &str,
    ) -> bool {
        let result = match &self.store {
            Store::Database(h) => h
                .launch()
                .session_prompt_turn_seen(session, since, prefix)
                .await
                .map_err(|e| e.to_string()),
            Store::Proxy(p) => p.prompt_turn_seen(session, since, prefix).await,
            Store::Absent => return false,
        };
        noted("prompt turn lookup", result).unwrap_or(false)
    }

    pub async fn link_channel(
        &self,
        cse_id: &str,
        session: Uuid,
        cwd: &str,
    ) -> Result<LinkOutcome, String> {
        match &self.store {
            Store::Proxy(p) => p.link_channel(cse_id, session, cwd).await,
            _ => self
                .handle()?
                .launch()
                .link_channel_to_session(cse_id, session, cwd)
                .await
                .map_err(|e| e.to_string()),
        }
    }

    pub async fn latest_validation(
        &self,
        backend: &str,
    ) -> Result<Option<BackendValidationRow>, String> {
        self.handle()?
            .launch()
            .latest_backend_validation(backend)
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn insert_validation(&self, v: &NewBackendValidation) -> Result<i64, String> {
        self.handle()?
            .launch()
            .insert_backend_validation(v)
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn insert_template(&self, new: &NewTemplate) -> Result<InsertedTemplate, String> {
        self.handle()?
            .launch()
            .insert_template(new)
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn find_template_by_name(&self, name: &str) -> Result<Option<TemplateRow>, String> {
        self.handle()?
            .launch()
            .find_live_template_by_name(name)
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn find_template_by_session(
        &self,
        session: Uuid,
    ) -> Result<Option<TemplateRow>, String> {
        self.handle()?
            .launch()
            .find_live_template_by_session(session)
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn list_templates(&self) -> Result<Vec<TemplateRow>, String> {
        self.handle()?
            .launch()
            .list_live_templates()
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn close_template(&self, name: &str) -> Result<bool, String> {
        self.handle()?
            .launch()
            .close_template(name)
            .await
            .map_err(|e| e.to_string())
    }
}
