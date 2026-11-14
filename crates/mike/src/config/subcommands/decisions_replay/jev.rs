use std::collections::BTreeMap;
use std::future::Future;
use std::time::Duration;

use anyhow::Result;

use crate::config::CliOverrides;
use crate::llm::decisions::record::{DecisionContext, DecisionMode};
use crate::llm::decisions::{Answer, DecisionRequest};
use crate::llm::{LlmError, OpenRouterClient};
use crate::store::persistence::PersistHandle;

const RATE_WAIT: Duration = Duration::from_millis(500);
const RATE_WAITS: usize = 600;

pub(super) enum Reply {
    Answered {
        model: String,
        answers: BTreeMap<String, Answer>,
        cost_usd: Option<f64>,
        latency_ms: Option<i32>,
    },
    Failed(String),
    Stop(String),
}

pub(super) trait Jev {
    fn model(&self) -> &str;
    fn ask(&self, request: &DecisionRequest) -> impl Future<Output = Reply>;
}

pub(super) struct LiveJev {
    client: OpenRouterClient,
    model: String,
    context: DecisionContext,
}

impl LiveJev {
    pub(super) async fn connect(cli: CliOverrides, persist: &PersistHandle) -> Result<Self> {
        let (client, overlay) = super::super::decisions::read_only_client(cli, persist).await?;
        Ok(Self {
            client,
            model: overlay.decisions_model,
            context: DecisionContext::new("decisions_replay", DecisionMode::Replay, "replay"),
        })
    }
}

fn stops_the_run(error: &LlmError) -> bool {
    error.is_credential_failure()
        || (error.is_refusal() && !matches!(error, LlmError::RateLimited { .. }))
}

impl Jev for LiveJev {
    fn model(&self) -> &str {
        &self.model
    }

    async fn ask(&self, request: &DecisionRequest) -> Reply {
        for _ in 0..RATE_WAITS {
            match self.client.decide(request, &self.context).await.result {
                Ok(response) => {
                    return Reply::Answered {
                        model: response.model,
                        answers: response.answers,
                        cost_usd: response.usage.cost_usd,
                        latency_ms: response.usage.latency_ms,
                    };
                }
                Err(LlmError::RateLimited { .. }) => tokio::time::sleep(RATE_WAIT).await,
                Err(error) if stops_the_run(&error) => return Reply::Stop(error.to_string()),
                Err(error) => return Reply::Failed(error.to_string()),
            }
        }
        Reply::Stop("the decisions rate limit did not clear in five minutes".to_owned())
    }
}
