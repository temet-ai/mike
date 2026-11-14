use ast_grep_config::{GlobalRules, RuleConfig, SerializableRuleConfig, from_str};
use ast_grep_language::SupportLang;
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    PubFnNoCaller,
    StageUnregistered,
    WorkerUnspawned,
    SettingUnread,
    RouteUnconsumed,
}

impl Kind {
    pub const ALL: [Kind; 5] = [
        Kind::PubFnNoCaller,
        Kind::StageUnregistered,
        Kind::WorkerUnspawned,
        Kind::SettingUnread,
        Kind::RouteUnconsumed,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::PubFnNoCaller => "pub_fn_no_caller",
            Kind::StageUnregistered => "stage_unregistered",
            Kind::WorkerUnspawned => "worker_unspawned",
            Kind::SettingUnread => "setting_unread",
            Kind::RouteUnconsumed => "route_unconsumed",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Normalize {
    #[default]
    Plain,
    Route,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Alias {
    Const,
    Snake,
}

#[derive(Debug, thiserror::Error)]
pub enum WiringError {
    #[error("wiring rules: {0}")]
    Yaml(String),
    #[error("wiring rule {id}: {reason}")]
    Rule { id: String, reason: String },
}

#[derive(Deserialize)]
struct RulesDoc {
    rules: Vec<RuleDoc>,
}

#[derive(Deserialize)]
struct RuleDoc {
    id: String,
    kind: Kind,
    message: String,
    #[serde(default)]
    normalize: Normalize,
    #[serde(default)]
    aliases: Vec<Alias>,
    declare: DeclareDoc,
    uses: Vec<UseDoc>,
}

#[derive(Deserialize)]
struct DeclareDoc {
    ast: SerializableRuleConfig<SupportLang>,
    capture: Option<String>,
    #[serde(default)]
    scope: Vec<String>,
    #[serde(default)]
    except: Vec<String>,
    #[serde(default)]
    except_names: Vec<String>,
}

#[derive(Deserialize)]
struct UseDoc {
    ast: Option<SerializableRuleConfig<SupportLang>>,
    capture: Option<String>,
    text: Option<String>,
    #[serde(default)]
    scope: Vec<String>,
    #[serde(default)]
    except: Vec<String>,
}

pub struct WiringRules {
    pub(super) specs: Vec<Spec>,
}

pub(super) struct Spec {
    pub(super) id: String,
    pub(super) kind: Kind,
    pub(super) message: String,
    pub(super) normalize: Normalize,
    pub(super) aliases: Vec<Alias>,
    pub(super) declare: Declare,
    pub(super) uses: Vec<Use>,
}

pub(super) struct Declare {
    pub(super) rule: RuleConfig<SupportLang>,
    pub(super) capture: Option<String>,
    pub(super) paths: Paths,
    pub(super) except_names: Vec<String>,
}

pub(super) struct Use {
    pub(super) matcher: Matcher,
    pub(super) paths: Paths,
}

pub(super) enum Matcher {
    Ast {
        rule: Box<RuleConfig<SupportLang>>,
        capture: Option<String>,
    },
    Text(Regex),
}

pub(super) struct Paths {
    scope: Vec<String>,
    except: Vec<String>,
}

impl Paths {
    pub(super) fn admits(&self, path: &str) -> bool {
        let inside = self.scope.is_empty() || self.scope.iter().any(|part| path.contains(part));
        inside && !self.except.iter().any(|part| path.contains(part))
    }
}

impl WiringRules {
    pub fn from_yaml(text: &str) -> Result<WiringRules, WiringError> {
        let doc: RulesDoc = from_str(text).map_err(|err| WiringError::Yaml(err.to_string()))?;
        let specs = doc
            .rules
            .into_iter()
            .map(compile_spec)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(WiringRules { specs })
    }

    pub fn ids(&self) -> Vec<&str> {
        self.specs.iter().map(|spec| spec.id.as_str()).collect()
    }
}

fn compile_spec(doc: RuleDoc) -> Result<Spec, WiringError> {
    let declare = Declare {
        rule: compile_ast(&doc.id, doc.declare.ast)?,
        capture: doc.declare.capture,
        paths: Paths {
            scope: doc.declare.scope,
            except: doc.declare.except,
        },
        except_names: doc.declare.except_names,
    };
    let uses = doc
        .uses
        .into_iter()
        .map(|usage| compile_use(&doc.id, usage))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Spec {
        id: doc.id,
        kind: doc.kind,
        message: doc.message,
        normalize: doc.normalize,
        aliases: doc.aliases,
        declare,
        uses,
    })
}

fn compile_use(id: &str, doc: UseDoc) -> Result<Use, WiringError> {
    let matcher = match (doc.ast, doc.text) {
        (Some(ast), None) => Matcher::Ast {
            rule: Box::new(compile_ast(id, ast)?),
            capture: doc.capture,
        },
        (None, Some(text)) => {
            Matcher::Text(Regex::new(&text).map_err(|err| rule_error(id, &text, err))?)
        }
        _ => {
            return Err(WiringError::Rule {
                id: id.to_owned(),
                reason: "a use needs exactly one of `ast` or `text`".to_owned(),
            });
        }
    };
    Ok(Use {
        matcher,
        paths: Paths {
            scope: doc.scope,
            except: doc.except,
        },
    })
}

fn compile_ast(
    id: &str,
    ast: SerializableRuleConfig<SupportLang>,
) -> Result<RuleConfig<SupportLang>, WiringError> {
    RuleConfig::try_from(ast, &GlobalRules::default()).map_err(|err| WiringError::Rule {
        id: id.to_owned(),
        reason: err.to_string(),
    })
}

fn rule_error(id: &str, input: &str, err: regex::Error) -> WiringError {
    WiringError::Rule {
        id: id.to_owned(),
        reason: format!("text pattern {input:?}: {err}"),
    }
}
