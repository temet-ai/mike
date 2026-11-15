use crate::store::persistence::web_repo::WebPersist as _;
use std::path::Path;

use serde_json::Value;

use super::RunMode;
use super::context::EngineerContext;
use super::finding;
use super::record;
use super::supervise::RunEnd;

pub(crate) struct Settled {
    pub(crate) output: Result<Value, String>,
    pub(crate) finding_id: Option<i64>,
}

fn read(out: &Path, mode: &RunMode, code: Option<i32>) -> Result<Vec<u8>, String> {
    let path = out.join(mode.output_file());
    std::fs::read(&path).map_err(|e| {
        let code = code.map_or("no exit code".to_owned(), |c| format!("exit code {c}"));
        format!(
            "the run ended with {code} and left no readable out/{}: {e}",
            mode.output_file()
        )
    })
}

fn json(bytes: &[u8], mode: &RunMode) -> Result<Value, String> {
    serde_json::from_slice(bytes).map_err(|e| {
        format!(
            "out/{} is not JSON at line {}, column {}: {e}",
            mode.output_file(),
            e.line(),
            e.column()
        )
    })
}

async fn end(ctx: &EngineerContext, id: i64, state: &str, code: Option<i32>, reason: Option<&str>) {
    if let Err(e) = ctx
        .persist
        .web()
        .end_wright_run(id, state, code, reason)
        .await
    {
        tracing::warn!(error = %e, investigation = id, "wright: the run's end could not be recorded; the next start reconciles it");
    }
}

async fn investigated(
    ctx: &EngineerContext,
    run: &Ended<'_>,
    code: Option<i32>,
    bytes: &[u8],
) -> Settled {
    let id = run.id;
    let validated = match finding::validate(bytes, run.work) {
        Ok(validated) => validated,
        Err(reason) => {
            end(ctx, id, "failed", code, Some(&reason)).await;
            return Settled {
                output: Err(reason),
                finding_id: None,
            };
        }
    };
    match ctx
        .persist
        .web()
        .finish_wright_run_with_finding(id, code, &validated.finding)
        .await
    {
        Ok(Some(finding_id)) => {
            let origin = record::Origin {
                finding_id,
                problem_id: run.problem_id,
                followup_id: run.followup_id,
            };
            record::record(ctx, &origin, &validated).await;
            Settled {
                output: Ok(finding::as_json(&validated)),
                finding_id: Some(finding_id),
            }
        }
        Ok(None) => Settled {
            output: Err(
                "the run's row was no longer live, so its finding was not stored".to_owned(),
            ),
            finding_id: None,
        },
        Err(e) => {
            let reason = format!("the finding could not be stored: {e}");
            end(ctx, id, "failed", code, Some(&reason)).await;
            Settled {
                output: Err(reason),
                finding_id: None,
            }
        }
    }
}

pub(crate) struct Ended<'a> {
    pub(crate) id: i64,
    pub(crate) mode: &'a RunMode,
    pub(crate) out: &'a Path,
    pub(crate) work: &'a Path,
    pub(crate) problem_id: Option<i64>,
    pub(crate) followup_id: Option<i64>,
}

pub(crate) async fn settle(ctx: &EngineerContext, run: &Ended<'_>, outcome: &RunEnd) -> Settled {
    let code = match outcome {
        RunEnd::Killed { reason } => {
            end(ctx, run.id, "killed", None, Some(reason)).await;
            return Settled {
                output: Err(reason.clone()),
                finding_id: None,
            };
        }
        RunEnd::Exited { code } => *code,
    };
    let bytes = match read(run.out, run.mode, code) {
        Ok(bytes) => bytes,
        Err(reason) => {
            end(ctx, run.id, "failed", code, Some(&reason)).await;
            return Settled {
                output: Err(reason),
                finding_id: None,
            };
        }
    };
    if matches!(run.mode, RunMode::Investigate) {
        return investigated(ctx, run, code, &bytes).await;
    }
    match json(&bytes, run.mode) {
        Ok(value) => {
            end(ctx, run.id, "finished", code, None).await;
            Settled {
                output: Ok(value),
                finding_id: None,
            }
        }
        Err(reason) => {
            end(ctx, run.id, "failed", code, Some(&reason)).await;
            Settled {
                output: Err(reason),
                finding_id: None,
            }
        }
    }
}
