#[derive(Debug, thiserror::Error)]
pub enum CodexError {
    #[error("could not start `{program} app-server`: {error}")]
    Spawn { program: String, error: String },
    #[error("the Codex app-server closed its {0}")]
    Closed(&'static str),
    #[error("the Codex app-server did not answer {0} within the call deadline")]
    Timeout(String),
    #[error("the Codex app-server refused {method} with params {params}: {error}")]
    Rpc {
        method: String,
        params: String,
        error: String,
    },
    #[error("the Codex app-server answered {method} without {field}: {answer}")]
    Shape {
        method: String,
        field: &'static str,
        answer: String,
    },
    #[error("could not stop the Codex app-server: {0}")]
    Stop(String),
    #[error("turn {turn_id} of thread {thread_id} did not complete before the deadline")]
    TurnDeadline { thread_id: String, turn_id: String },
}
