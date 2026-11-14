use mike::analysis::{EmbeddingConfig, EmbeddingService};
use mike::lesson_proposer::{ProposerDeps, ProposerSettings};
use mike::llm::{OpenRouterClient, reset_global_for_tests};
use mike::semantic::SemanticConfig;
use mike::store::persistence::PersistHandle;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

pub(crate) const REPLACEMENT: &str =
    "Run the tests and read their exit status before claiming they pass.";
pub(crate) const REWRITTEN_A: &str =
    "Verify a claim against the running system before reporting it.";
pub(crate) const REWRITTEN_B: &str = "Report what was checked, not what was assumed.";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Relate {
    Update,
    Skip,
    Fail,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shape {
    TwoRules,
    NoRule,
    TooLong,
}

pub(crate) struct Scripted {
    pub(crate) relate: Relate,
    pub(crate) shape: Shape,
    pub(crate) home: &'static str,
}

pub(crate) const PLACEMENT_FAILS: &str = "fail";

fn placement_answer(home: &str) -> Value {
    json!({
        "home": home,
        "covered_by": "",
        "reason": "scripted placement",
        "persona": false,
    })
}

fn message(body: &Value, index: usize) -> String {
    body["messages"][index]["content"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

fn first_lesson_id(user: &str) -> i64 {
    user.split("lesson_id ")
        .nth(1)
        .map(|rest| {
            rest.chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
        })
        .and_then(|digits| digits.parse().ok())
        .unwrap_or_default()
}

fn relate_answer(mode: Relate, home: &str, user: &str) -> Value {
    let decision = if mode == Relate::Skip {
        "skip"
    } else {
        "update"
    };
    let replacement = if mode == Relate::Update {
        REPLACEMENT
    } else {
        ""
    };
    let decisions = if user.contains("lesson_id ") {
        json!([{
            "lesson_id": first_lesson_id(user),
            "decision": decision,
            "reason": "the same subject, worded differently",
        }])
    } else {
        json!([])
    };
    json!({
        "decisions": decisions,
        "replacement": replacement,
        "placement": placement_answer(home),
    })
}

fn shape_answer(mode: Shape) -> Value {
    match mode {
        Shape::TwoRules => json!({"rules": [REWRITTEN_A, REWRITTEN_B], "reason": "two rules"}),
        Shape::NoRule => json!({"rules": [], "reason": "it only reports an outcome"}),
        Shape::TooLong => json!({
            "rules": ["Verify every claim against the running system. ".repeat(30)],
            "reason": "kept everything",
        }),
    }
}

impl Respond for Scripted {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let Ok(body) = serde_json::from_slice::<Value>(&request.body) else {
            return ResponseTemplate::new(400);
        };
        let (system, user) = (message(&body, 0), message(&body, 1));
        let content = if system.contains("You decide how one new lesson relates") {
            if self.relate == Relate::Fail {
                return ResponseTemplate::new(500);
            }
            relate_answer(self.relate, self.home, &user)
        } else if system.contains("You decide where one stored project lesson belongs") {
            if self.home == PLACEMENT_FAILS {
                return ResponseTemplate::new(500);
            }
            placement_answer(self.home)
        } else if system.contains("You rewrite one stored lesson") {
            shape_answer(self.shape)
        } else {
            json!({
                "rule": "Always run the tests before claiming they pass.",
                "kind": "correction",
                "generalises": true,
                "reason": "the operator caught a false claim",
            })
        };
        ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"content": content.to_string()}}]
        }))
    }
}

struct UnitEmbeddings;

impl Respond for UnitEmbeddings {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let count = serde_json::from_slice::<Value>(&request.body)
            .ok()
            .and_then(|body| body["input"].as_array().map(Vec::len))
            .unwrap_or(1);
        let mut vector = vec![0.0_f32; 768];
        vector[0] = 1.0;
        let data: Vec<Value> = (0..count)
            .map(|index| json!({"embedding": vector, "index": index, "object": "embedding"}))
            .collect();
        ResponseTemplate::new(200).set_body_json(json!({"data": data}))
    }
}

pub(crate) struct Rig {
    pub(crate) llm_server: MockServer,
    _embedding_server: MockServer,
    pub(crate) llm: OpenRouterClient,
    pub(crate) embedding: EmbeddingService,
    pub(crate) semantic: SemanticConfig,
    pub(crate) settings: ProposerSettings,
}

impl Rig {
    pub(crate) async fn new(
        scripted: Scripted,
        settings: ProposerSettings,
    ) -> anyhow::Result<Self> {
        reset_global_for_tests();
        let llm_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(scripted)
            .mount(&llm_server)
            .await;
        let embedding_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/embeddings"))
            .respond_with(UnitEmbeddings)
            .mount(&embedding_server)
            .await;
        let llm = OpenRouterClient::new(
            "sk-test".to_owned(),
            "test/model".to_owned(),
            Some(llm_server.uri()),
            None,
        )?;
        let embedding = EmbeddingService::new(&EmbeddingConfig {
            enabled: true,
            url: embedding_server.uri(),
            model: "nomic-embed-text-v1.5".to_owned(),
            chunk_max_chars: 1500,
        });
        Ok(Self {
            llm_server,
            _embedding_server: embedding_server,
            llm,
            embedding,
            semantic: SemanticConfig::default(),
            settings,
        })
    }

    pub(crate) fn deps<'a>(&'a self, persist: &'a PersistHandle) -> ProposerDeps<'a> {
        ProposerDeps {
            persist,
            llm: &self.llm,
            embedding: &self.embedding,
            semantic: &self.semantic,
            settings: &self.settings,
            decisions: None,
        }
    }

    pub(crate) async fn llm_calls(&self, system_marker: &str) -> usize {
        let received = self
            .llm_server
            .received_requests()
            .await
            .unwrap_or_default();
        received
            .iter()
            .filter(|request| {
                serde_json::from_slice::<Value>(&request.body)
                    .map(|body| message(&body, 0).contains(system_marker))
                    .unwrap_or(false)
            })
            .count()
    }
}
