use std::collections::HashSet;
use std::sync::OnceLock;

use ast_grep_config::{GlobalRules, RuleCollection, RuleConfig, from_yaml_string};
use ast_grep_language::SupportLang;
use serde::Deserialize;
use sha2::{Digest, Sha256};

const BUILTIN: [(&str, &str); 7] = [
    ("rust.yml", include_str!("../rules/rust.yml")),
    ("rust-sql.yml", include_str!("../rules/rust-sql.yml")),
    ("rust-tests.yml", include_str!("../rules/rust-tests.yml")),
    ("shell.yml", include_str!("../rules/shell.yml")),
    ("python.yml", include_str!("../rules/python.yml")),
    ("typescript.yml", include_str!("../rules/typescript.yml")),
    ("systems.yml", include_str!("../rules/systems.yml")),
];

pub const SOURCE_BUILTIN: &str = "builtin";
pub const SOURCE_PROJECT: &str = "project";

#[derive(Debug, thiserror::Error)]
pub enum RuleError {
    #[error("rule file {file}: {reason}")]
    Parse { file: String, reason: String },
    #[error("rule globs: {0}")]
    Globs(String),
}

pub struct RuleSet {
    collection: RuleCollection<SupportLang>,
    version: String,
    ids: Vec<String>,
    project_ids: HashSet<String>,
    catalogue: Vec<RuleInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RuleInfo {
    pub id: String,
    pub language: String,
    pub severity: &'static str,
    pub source: &'static str,
    pub replaces: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SgConfig {
    #[serde(default)]
    rule_dirs: Vec<String>,
}

pub fn sgconfig_rule_dirs(text: &str) -> Result<Vec<String>, RuleError> {
    let config: SgConfig = serde_saphyr::from_str(text).map_err(|err| RuleError::Parse {
        file: "sgconfig.yml".to_string(),
        reason: err.to_string(),
    })?;
    Ok(config.rule_dirs)
}

impl RuleSet {
    pub fn builtin() -> Result<&'static RuleSet, &'static RuleError> {
        static BUILT: OnceLock<Result<RuleSet, RuleError>> = OnceLock::new();
        BUILT.get_or_init(|| RuleSet::from_yaml(&BUILTIN)).as_ref()
    }

    pub fn from_yaml(files: &[(&str, &str)]) -> Result<RuleSet, RuleError> {
        Self::build(files, &[])
    }

    pub fn with_project(project: &[(String, String)]) -> Result<RuleSet, RuleError> {
        let project: Vec<(&str, &str)> = project
            .iter()
            .map(|(file, text)| (file.as_str(), text.as_str()))
            .collect();
        Self::build(&BUILTIN, &project)
    }

    fn build(builtin: &[(&str, &str)], project: &[(&str, &str)]) -> Result<RuleSet, RuleError> {
        let mut digest = Sha256::new();
        let mut configs = parse_all(builtin, &mut digest)?;
        let local = parse_all(project, &mut digest)?;
        let project_ids: HashSet<String> = local.iter().map(|rule| rule.id.clone()).collect();
        configs.retain(|rule| !project_ids.contains(&rule.id));
        configs.extend(local);
        let ids = configs.iter().map(|rule| rule.id.clone()).collect();
        let catalogue = configs
            .iter()
            .map(|rule| RuleInfo {
                id: rule.id.clone(),
                language: crate::language::language_name(rule.language),
                severity: crate::scan::severity_name(&rule.severity),
                source: if project_ids.contains(&rule.id) {
                    SOURCE_PROJECT
                } else {
                    SOURCE_BUILTIN
                },
                replaces: rule.url.clone(),
            })
            .collect();
        let collection =
            RuleCollection::try_new(configs).map_err(|err| RuleError::Globs(err.to_string()))?;
        let version = hex::encode(&digest.finalize()[..6]);
        Ok(RuleSet {
            collection,
            version,
            ids,
            project_ids,
            catalogue,
        })
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn ids(&self) -> &[String] {
        &self.ids
    }

    pub fn catalogue(&self) -> &[RuleInfo] {
        &self.catalogue
    }

    pub fn project_ids(&self) -> impl Iterator<Item = &str> {
        self.project_ids.iter().map(String::as_str)
    }

    pub fn source_of(&self, rule_id: &str) -> &'static str {
        if self.project_ids.contains(rule_id) {
            SOURCE_PROJECT
        } else {
            SOURCE_BUILTIN
        }
    }

    pub fn for_file(&self, path: &str, language: SupportLang) -> Vec<&RuleConfig<SupportLang>> {
        self.collection
            .get_rule_from_lang(std::path::Path::new(path), language)
    }
}

fn parse_all(
    files: &[(&str, &str)],
    digest: &mut Sha256,
) -> Result<Vec<RuleConfig<SupportLang>>, RuleError> {
    let mut configs = Vec::new();
    for (file, text) in files {
        digest.update(file.as_bytes());
        digest.update(text.as_bytes());
        let parsed =
            from_yaml_string::<SupportLang>(text, &GlobalRules::default()).map_err(|err| {
                RuleError::Parse {
                    file: (*file).to_string(),
                    reason: err.to_string(),
                }
            })?;
        configs.extend(parsed);
    }
    Ok(configs)
}
