use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::Mutex;

use tracing::warn;

use crate::llm::usage::{LlmCallOutcome, LlmCallRecord, LlmUsageSink, RoleSpend};

pub(crate) struct FileSink {
    state: Mutex<SinkState>,
}

struct SinkState {
    writer: BufWriter<File>,
    total_usd: f64,
    calls: u64,
    cost_unknown: bool,
}

impl FileSink {
    pub(crate) fn create(path: &Path) -> std::io::Result<Self> {
        Ok(Self {
            state: Mutex::new(SinkState {
                writer: BufWriter::new(File::create(path)?),
                total_usd: 0.0,
                calls: 0,
                cost_unknown: false,
            }),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, SinkState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn total_usd(&self) -> f64 {
        self.lock().total_usd
    }

    pub(crate) fn calls(&self) -> u64 {
        self.lock().calls
    }

    pub(crate) fn cost_unknown(&self) -> bool {
        self.lock().cost_unknown
    }
}

fn call_json(call: &LlmCallRecord) -> serde_json::Value {
    serde_json::json!({
        "role": format!("{:?}", call.role),
        "model": call.model,
        "generation_id": call.generation_id,
        "prompt_tokens": call.prompt_tokens,
        "completion_tokens": call.completion_tokens,
        "reasoning_tokens": call.reasoning_tokens,
        "cached_tokens": call.cached_tokens,
        "cost_usd": call.cost_usd,
        "latency_ms": call.latency_ms,
        "outcome": format!("{:?}", call.outcome),
        "http_status": call.http_status,
        "request_body": call.request_body,
    })
}

fn returned_a_body(outcome: LlmCallOutcome) -> bool {
    matches!(outcome, LlmCallOutcome::Ok | LlmCallOutcome::Truncated)
}

impl LlmUsageSink for FileSink {
    fn record(&self, call: LlmCallRecord) {
        let line = call_json(&call).to_string();
        let mut state = self.lock();
        state.calls += 1;
        match call.cost_usd {
            Some(cost) if cost.is_finite() => state.total_usd += cost,
            _ if returned_a_body(call.outcome) => state.cost_unknown = true,
            _ => {}
        }
        let written = writeln!(state.writer, "{line}").and_then(|()| state.writer.flush());
        if let Err(e) = written {
            warn!(error = %e, "verifier replay: could not write a call record");
        }
    }

    fn spent_today(&self) -> RoleSpend {
        RoleSpend::default()
    }
}
