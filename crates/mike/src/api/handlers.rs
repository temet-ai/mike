pub use mike_features::api::handlers::*;
use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::analysis::EmbeddingService;
use crate::api::embedding_graph::EmbeddingStatusDto;
use crate::state::AppState;
use crate::store::models::{
    Diagnostic, DiagnosticCategory, InterceptedRequest, RequestId, RequestStatus, SessionView,
    SystemPromptSource,
};
use crate::store::persistence::SessionSummaryRow;
use crate::store::{DiagnosticStatsView, FindingView, MemoryBreakdownView, MemoryEntryRow};
use crate::types::{RequestParams, TokenUsage, ToolCall};

#[cfg(test)]
use chrono::{DateTime, TimeZone, Utc};
#[cfg(test)]
use serde_json::Value;
#[cfg(test)]
use std::collections::HashMap;

mod anthropic_integration_01;
mod anthropic_integration_02;
mod anthropic_integration_03;
mod connection_info;
pub(crate) use connection_info::connection_info;
mod diagnostic_dto;
mod diagnostic_memory;
mod spa_delta_fallback;
pub(crate) use spa_delta_fallback::spa_delta_fallback_route;
mod agent_dispatches;
mod embedding_change;
mod file_writes_01;
mod lesson_list_delete;
mod listing;
mod llm_credit;
mod llm_job_models;
mod llm_provider_health;
mod llm_providers;
mod llm_role_health;
mod llm_worker_lease;
mod models;
mod operator_turns;
mod operator_turns_ledger;
mod operator_turns_outcomes;
mod project_merge;
#[cfg(test)]
mod recall_own_turn_tests;
#[cfg(test)]
mod recall_recent_tests;
mod request_body;
mod request_deltas;
mod request_transform;
mod scheduled_jobs;
mod search_body;
mod search_own;
mod search_reminders;
mod session_detail;
mod session_summary;
mod settings_01;
mod settings_01_launch_values;
mod settings_02;
pub mod spend_01;
mod steerer_assist;
#[cfg(test)]
mod tests_01;
#[cfg(test)]
mod tests_02;
#[cfg(test)]
mod tests_03;
#[cfg(test)]
mod tests_04;
mod time_spent;
mod worker_toggles;

pub(crate) use agent_dispatches::*;
pub(crate) use anthropic_integration_01::*;
pub(crate) use anthropic_integration_02::*;
pub(crate) use anthropic_integration_03::*;
pub(crate) use capabilities::*;
pub(crate) use compression_01::*;
pub(crate) use diagnostic_dto::*;
pub(crate) use diagnostic_memory::*;
pub(crate) use diagnostics::*;
pub(crate) use file_writes_01::*;
pub(crate) use filter_dry_run::*;
#[cfg(any(test, feature = "test-seams"))]
pub use health::{capture_outage_notices, refresh_capture_outage_now, refresh_health_now};
pub(crate) use lesson_list_delete::*;
pub(crate) use listing::*;
pub(crate) use llm_credit::*;
pub(crate) use llm_job_models::*;
pub(crate) use llm_provider_health::*;
pub(crate) use llm_providers::*;
pub(crate) use llm_role_health::*;
pub(crate) use llm_worker_lease::*;
pub(crate) use mailbox_post::*;
pub(crate) use mailbox_post_fanout::*;
pub(crate) use mailbox_post_name::*;
pub(crate) use models::*;
pub(crate) use operator_turns::*;
pub(crate) use operator_turns_ledger::*;
pub(crate) use operator_turns_outcomes::*;
pub(crate) use project_merge::*;
pub(crate) use recall_context::*;
pub(crate) use recall_turns::*;
pub(crate) use receipts_01::*;
pub(crate) use remote_control_link::*;
pub(crate) use request_body::*;
pub(crate) use request_deltas::*;
pub(crate) use request_detail::*;
pub(crate) use request_summary::*;
pub(crate) use request_transform::*;
pub(crate) use scheduled_jobs::*;
pub(crate) use search_body::*;
use search_own::*;
use search_reminders::*;
pub(crate) use search_session::*;
use search_session_messages::*;
pub(crate) use search_session_scan::*;
pub(crate) use search_unified::*;
pub(crate) use session_detail::*;
pub(crate) use session_list::*;
pub(crate) use session_overview::*;
pub(crate) use session_summary::*;
pub(crate) use settings_01::*;
pub(crate) use settings_02::*;
pub(crate) use spend_01::*;
pub(crate) use steerer_assist::*;
pub(crate) use steerer_model_check::*;
pub(crate) use steerers_01::*;
pub(crate) use steerers_02::*;
pub(crate) use steering_views_01::*;
pub(crate) use steering_views_02::*;
pub(crate) use time_spent::*;
pub(crate) use tools_01::*;
pub(crate) use tools_03::*;
pub(crate) use usage_workflows_01::*;
pub(crate) use worker_toggles::*;

#[cfg(test)]
mod health_03;
mod capabilities;
mod diagnostics;
mod filter_dry_run;
mod recall_context;
mod recall_turns;
pub mod receipts_01;
mod request_detail;
mod request_summary;
pub(super) mod search_session;
mod search_session_messages;
mod search_session_scan;
mod search_unified;
mod session_list;
mod session_overview;
mod steerers_01;
mod steerers_02;
mod tools_01;
mod tools_03;
mod usage_workflows_01;
pub(crate) mod recall_turns_detail;
mod compression_01;
mod remote_control_link;
mod steering_views_01;
mod steering_views_02;
mod mailbox_post_fanout;
mod mailbox_post_name;
mod steerer_model_check;
mod mailbox_post;
#[cfg(test)]
mod mailbox_post_tests;
#[cfg(test)]
mod request_recovery_tests;
