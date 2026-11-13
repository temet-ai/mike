//! Facade: `api::handlers` as mike_features sees it (crate split, ADR 0002).

use std::collections::BTreeMap;
use std::sync::atomic::Ordering;
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
use crate::store::persistence::meta::pg_interval_to_humantime;
use crate::store::{DiagnosticStatsView, FindingView, MemoryBreakdownView, MemoryEntryRow};
pub use crate::types::SortOrder;
use crate::types::{RequestParams, TokenUsage, ToolCall};
pub use crate::web_base::{persist_error_response, persist_unavailable_response};
#[cfg(any(test, feature = "test-seams"))]
use chrono::{DateTime, TimeZone, Utc};
#[cfg(any(test, feature = "test-seams"))]
use serde_json::Value;
#[cfg(any(test, feature = "test-seams"))]
use std::collections::HashMap;
pub use agent_composition_stats::*;
pub use agents::*;
pub use filter_rule_dto::*;
pub use filters::*;
pub use health::*;
#[cfg(any(test, feature = "test-seams"))]
pub use health::{capture_outage_notices, refresh_capture_outage_now, refresh_health_now};
pub use lesson_create::*;
pub use lesson_types::*;
pub use mailbox_dto::*;
pub use mailbox_reads::*;
pub use mailbox_thread_create::*;
pub use mailbox_thread_detail::*;
pub use mailbox_thread_dto::*;
pub use mailbox_thread_list::*;
pub use projects::*;
pub use prompt_anomalies::*;
pub use prompt_class_versions::*;
pub use prompt_classes::*;
pub use prompts_01::*;
pub use prompts_02::*;
pub use prompts_03::*;
pub use prompts_04::*;
pub use session_admin::*;
pub use session_notes::*;
pub use session_quality::*;
pub use stats::*;
pub use steerer_client::*;
pub use steerer_dry_run::*;
pub use tags::*;
pub use team_comms::*;
pub use tools_02::*;
pub use verifier_01::*;
pub use work_items::*;

pub mod agent_composition_stats;
pub mod agents;
pub mod filter_rule_dto;
pub mod filters;
pub mod health;
pub mod lesson_create;
pub mod lesson_project_guard;
pub mod lesson_types;
pub mod mailbox_dto;
pub mod mailbox_reads;
pub mod mailbox_thread_create;
pub mod mailbox_thread_detail;
pub mod mailbox_thread_dto;
pub mod mailbox_thread_list;
pub mod projects;
pub mod prompt_anomalies;
pub mod prompt_class_versions;
pub mod prompt_classes;
pub mod prompts_01;
pub mod prompts_02;
pub mod prompts_03;
pub mod prompts_04;
pub mod recall_turns_rerank;
pub mod session_admin;
pub mod session_notes;
pub mod session_quality;
pub mod stats;
pub mod steerer_client;
pub mod steerer_dry_run;
pub mod tags;
pub mod team_comms;
pub mod tools_02;
pub mod verifier_01;
pub mod work_items;
pub mod turn_dto;
pub use turn_dto::*;
