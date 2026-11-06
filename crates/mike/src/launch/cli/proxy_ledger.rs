use std::time::Duration;

use chrono::{DateTime, Utc};
use reqwest::{RequestBuilder, StatusCode};
use serde::Serialize;
use serde::de::DeserializeOwned;
use uuid::Uuid;

use super::fleet_settings::RecordedSettings;
use super::ledger_wire::{
    ChannelPresent, ClosedBody, HELLO_PATH, LAUNCHES_PATH, LaunchId, LedgerHello, LinkBody,
    LinkReply, RUNS_PATH, SESSIONS_PATH, TurnSeen,
};
use crate::store::persistence::launch::{
    LaunchListFilter, LaunchOutcome, LaunchRow, LaunchSurfaceRefs, LinkOutcome, NewLaunch, NewRun,
    RunRow,
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone)]
pub(crate) struct ProxyLedger {
    addr: String,
    client: reqwest::Client,
}

fn keep(row: &LaunchRow, filter: &LaunchListFilter) -> bool {
    (!filter.open_only || row.closed_at.is_none())
        && filter
            .backend
            .as_deref()
            .is_none_or(|backend| row.backend == backend)
}

impl ProxyLedger {
    pub(crate) async fn connect(addr: &str) -> Result<(Self, LedgerHello), String> {
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .no_proxy()
            .build()
            .map_err(|e| format!("could not build an HTTP client: {e}"))?;
        let ledger = Self {
            addr: addr.to_string(),
            client,
        };
        let hello: LedgerHello = ledger
            .read(ledger.get(HELLO_PATH))
            .await?
            .ok_or_else(|| format!("the proxy at {addr} has no launch ledger route"))?;
        if !hello.database {
            return Err(format!(
                "the proxy at {addr} answers but has no database either"
            ));
        }
        Ok((ledger, hello))
    }

    pub(crate) fn addr(&self) -> &str {
        &self.addr
    }

    fn get(&self, path: &str) -> RequestBuilder {
        self.client.get(format!("http://{}{path}", self.addr))
    }

    fn post(&self, path: &str, body: &impl Serialize) -> RequestBuilder {
        self.client
            .post(format!("http://{}{path}", self.addr))
            .json(body)
    }

    async fn send(&self, request: RequestBuilder) -> Result<Option<reqwest::Response>, String> {
        let response = request
            .send()
            .await
            .map_err(|e| format!("the proxy at {} did not answer: {e}", self.addr))?;
        match response.status() {
            StatusCode::NOT_FOUND => Ok(None),
            status if status.is_success() => Ok(Some(response)),
            status => {
                let detail = response.text().await.unwrap_or_default();
                Err(format!(
                    "the proxy at {} answered {status}: {detail}",
                    self.addr
                ))
            }
        }
    }

    async fn read<T: DeserializeOwned>(
        &self,
        request: RequestBuilder,
    ) -> Result<Option<T>, String> {
        let Some(response) = self.send(request).await? else {
            return Ok(None);
        };
        response
            .json()
            .await
            .map(Some)
            .map_err(|e| format!("the proxy at {} sent an unreadable answer: {e}", self.addr))
    }

    async fn must_read<T: DeserializeOwned>(&self, request: RequestBuilder) -> Result<T, String> {
        self.read(request)
            .await?
            .ok_or_else(|| format!("the proxy at {} has no such launch ledger route", self.addr))
    }

    async fn write(&self, request: RequestBuilder) -> Result<(), String> {
        match self.send(request).await? {
            Some(_) => Ok(()),
            None => Err(format!(
                "the proxy at {} has no such launch ledger route",
                self.addr
            )),
        }
    }

    pub(crate) async fn upsert_run(&self, run: &NewRun) -> Result<(), String> {
        self.write(self.post(RUNS_PATH, run)).await
    }

    pub(crate) async fn find_run(&self, run_id: &str) -> Result<Option<RunRow>, String> {
        self.read(self.get(RUNS_PATH).query(&[("run", run_id)]))
            .await
    }

    pub(super) async fn fleet_settings(
        &self,
        key: &str,
    ) -> Result<Option<RecordedSettings>, String> {
        self.read(self.get(&format!("/api/fleets/{key}"))).await
    }

    pub(crate) async fn insert_launch(&self, new: &NewLaunch) -> Result<i64, String> {
        let id: LaunchId = self.must_read(self.post(LAUNCHES_PATH, new)).await?;
        Ok(id.id)
    }

    pub(crate) async fn record_opened(
        &self,
        id: i64,
        refs: &LaunchSurfaceRefs,
    ) -> Result<(), String> {
        self.write(self.post(&format!("{LAUNCHES_PATH}/{id}/opened"), refs))
            .await
    }

    pub(crate) async fn record_outcome(
        &self,
        id: i64,
        outcome: &LaunchOutcome,
    ) -> Result<(), String> {
        self.write(self.post(&format!("{LAUNCHES_PATH}/{id}/outcome"), outcome))
            .await
    }

    pub(crate) async fn record_closed(&self, id: i64, body: &ClosedBody) -> Result<(), String> {
        self.write(self.post(&format!("{LAUNCHES_PATH}/{id}/closed"), body))
            .await
    }

    pub(crate) async fn find_launch_by_session(
        &self,
        session: Uuid,
    ) -> Result<Option<LaunchRow>, String> {
        self.read(self.get(&format!("/api/launches/{session}")))
            .await
    }

    pub(crate) async fn list_launches(
        &self,
        filter: &LaunchListFilter,
    ) -> Result<Vec<LaunchRow>, String> {
        let mut request = self.get("/api/launches");
        if let Some(run) = &filter.run_id {
            request = request.query(&[("run", run.as_str())]);
        }
        let rows: Vec<LaunchRow> = self.must_read(request).await?;
        let limit = usize::try_from(filter.limit).unwrap_or(0);
        Ok(rows
            .into_iter()
            .filter(|row| keep(row, filter))
            .take(limit)
            .collect())
    }

    pub(crate) async fn channel_present(&self, session: Uuid) -> Result<bool, String> {
        let reply: ChannelPresent = self
            .must_read(self.get(&format!("{SESSIONS_PATH}/{session}/channel")))
            .await?;
        Ok(reply.present)
    }

    pub(crate) async fn link_channel(
        &self,
        cse_id: &str,
        session: Uuid,
        cwd: &str,
    ) -> Result<LinkOutcome, String> {
        let body = LinkBody {
            cse_id: cse_id.to_string(),
            cwd: cwd.to_string(),
        };
        let reply: LinkReply = self
            .must_read(self.post(&format!("{SESSIONS_PATH}/{session}/channel"), &body))
            .await?;
        Ok(reply.outcome)
    }

    pub(crate) async fn prompt_turn_seen(
        &self,
        session: Uuid,
        since: DateTime<Utc>,
        prefix: &str,
    ) -> Result<bool, String> {
        let request = self
            .get(&format!("{SESSIONS_PATH}/{session}/turn"))
            .query(&[
                ("since", since.to_rfc3339()),
                ("prefix", prefix.to_string()),
            ]);
        let reply: TurnSeen = self.must_read(request).await?;
        Ok(reply.seen)
    }
}
