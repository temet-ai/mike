mod capping;
mod http;

mod channel;
mod config_history;
mod cross_session;
mod dossier;
mod eval_bundles;
mod eval_labels;
mod eval_metrics;
mod eval_runs;
mod improvement;
mod questions;
mod semantic;
mod session_verdicts;
mod steer_questions;
mod supplied;
mod steering_evaluations;
mod turn_verdicts;
pub(crate) use config_history::ConfigChangesQuery;
pub(crate) use eval_bundles::BundlesQuery;
pub(crate) use steering_evaluations::SteererEvaluationsQuery;
pub(crate) use turn_verdicts::TurnVerdictsQuery;
mod changes;
mod diagnostics;
pub(crate) use channel::{ClaimRoutes, ClaimedChannelMessage};
mod filters;
mod fleets;
mod knowledge;
#[cfg(test)]
mod knowledge_tests;
mod launches;
mod mail;
mod memory;
mod prompts;
mod requests;
mod session_notes;
mod sessions;
mod turns;
mod work_threads;
mod workspace;

use serde_json::Value;

pub(crate) use session_notes::{ProjectNotesQuery, SessionNotesQuery, WriteSessionNote};
pub(crate) use work_threads::{ThreadGetQuery, ThreadListQuery};

pub(crate) const DEFAULT_MAX_RESPONSE_BYTES: u64 = 256 * 1024 * 1024;

pub(crate) struct RegisterWorkspaceSession<'a> {
    pub(crate) session_id: &'a str,
    pub(crate) cwd: &'a str,
    pub(crate) repo_root: Option<&'a str>,
    pub(crate) location_uuid: Option<&'a str>,
    pub(crate) registered_via: Option<&'a str>,
    pub(crate) goal: Option<&'a str>,
    pub(crate) next_steps: Option<&'a str>,
    pub(crate) role: Option<&'a str>,
    pub(crate) name: Option<&'a str>,
    pub(crate) instructions: Option<&'a str>,
    pub(crate) pid: Option<i32>,
}

pub(crate) struct SetSessionIdentity<'a> {
    pub(crate) session_id: &'a str,
    pub(crate) name: Option<&'a str>,
    pub(crate) role: Option<&'a str>,
    pub(crate) goal: Option<&'a str>,
    pub(crate) next_steps: Option<&'a str>,
    pub(crate) instructions: Option<&'a str>,
    pub(crate) set_by: Option<&'a str>,
}

#[derive(Debug, Default)]
pub(crate) struct FileWritesQuery<'a> {
    pub(crate) project: Option<&'a str>,
    pub(crate) from: Option<&'a str>,
    pub(crate) to: Option<&'a str>,
    pub(crate) path: Option<&'a str>,
    pub(crate) collisions_only: Option<bool>,
    pub(crate) offset: Option<i64>,
    pub(crate) limit: Option<i64>,
}

#[derive(Debug, Default)]
pub(crate) struct TimeSpentQuery<'a> {
    pub(crate) project: Option<&'a str>,
    pub(crate) project_path: Option<&'a str>,
    pub(crate) from: Option<&'a str>,
    pub(crate) to: Option<&'a str>,
    pub(crate) days: Option<i64>,
    pub(crate) include_subagents: Option<bool>,
    pub(crate) session_id: Option<&'a str>,
    pub(crate) kind: Option<&'a str>,
    pub(crate) limit: Option<i64>,
    pub(crate) offset: Option<i64>,
}

#[derive(Debug, Default)]
pub(crate) struct ListSessionToolUsesQuery<'a> {
    pub(crate) offset: Option<u64>,
    pub(crate) limit: Option<u64>,
    pub(crate) order: Option<&'a str>,
    pub(crate) include_results: Option<bool>,
    pub(crate) agent: Option<&'a str>,
}

#[derive(Debug, Default)]
pub(crate) struct SessionOverviewQuery<'a> {
    pub(crate) order: Option<&'a str>,
    pub(crate) prompt_kind: Option<&'a str>,
    pub(crate) limit: Option<u64>,
    pub(crate) offset: Option<u64>,
    pub(crate) fields: Option<&'a str>,
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct RecallShape<'a> {
    pub(crate) order: Option<&'a str>,
    pub(crate) detail: Option<&'a str>,
}

pub(crate) const DEFAULT_DETAIL: &str = "best";

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct CallerScope<'a> {
    pub(crate) session_id: Option<&'a str>,
    pub(crate) include_own_turn: Option<bool>,
}

#[derive(Debug, Default, serde::Deserialize)]
pub(crate) struct CallerArgs {
    #[serde(default)]
    pub(crate) session_id: Option<String>,
    #[serde(default)]
    pub(crate) include_own_turn: Option<bool>,
}

impl CallerArgs {
    pub(crate) fn scope(&self) -> CallerScope<'_> {
        CallerScope {
            session_id: self.session_id.as_deref(),
            include_own_turn: self.include_own_turn,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct RecordLessonRequest<'a> {
    pub(crate) topic: Option<&'a str>,
    pub(crate) source_turn_id: Option<i64>,
    pub(crate) agent_name: Option<&'a str>,
    pub(crate) session_id: Option<&'a str>,
    pub(crate) supersedes: Option<i64>,
}

#[derive(Debug, Default)]
pub(crate) struct SendMessageOptions<'a> {
    pub(crate) from: Option<&'a str>,
    pub(crate) thread_id: Option<&'a str>,
    pub(crate) in_reply_to: Option<&'a str>,
    pub(crate) subject: Option<&'a str>,
    pub(crate) tags: Option<&'a [String]>,
    pub(crate) context: Option<&'a [Value]>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SendTarget<'a> {
    Recipient(&'a str),
    ThreadPost(&'a str),
    OpenThread(&'a str),
}

#[derive(Clone)]
pub(crate) struct InspectClient {
    base_url: String,
    client: reqwest::Client,
    max_response_bytes: u64,
}

impl InspectClient {
    pub(crate) fn new(base_url: String) -> Self {
        Self::with_max_response_bytes(base_url, DEFAULT_MAX_RESPONSE_BYTES)
    }

    pub(crate) fn with_max_response_bytes(base_url: String, max_response_bytes: u64) -> Self {
        Self {
            base_url,
            client: reqwest::Client::new(),
            max_response_bytes,
        }
    }
}
