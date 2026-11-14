use std::sync::OnceLock;

use ast_grep_config::{GlobalRules, RuleCollection, RuleConfig, from_yaml_string};
use ast_grep_language::SupportLang;
use sha2::{Digest, Sha256};

const BUILTIN: [(&str, &str); 6] = [
    ("rust.yml", include_str!("../rules/rust.yml")),
    ("rust-sql.yml", include_str!("../rules/rust-sql.yml")),
    ("rust-tests.yml", include_str!("../rules/rust-tests.yml")),
    ("shell.yml", include_str!("../rules/shell.yml")),
    ("python.yml", include_str!("../rules/python.yml")),
    ("typescript.yml", include_str!("../rules/typescript.yml")),
];

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
}

impl RuleSet {
    pub fn builtin() -> Result<&'static RuleSet, &'static RuleError> {
        static BUILT: OnceLock<Result<RuleSet, RuleError>> = OnceLock::new();
        BUILT.get_or_init(|| RuleSet::from_yaml(&BUILTIN)).as_ref()
    }

    pub fn from_yaml(files: &[(&str, &str)]) -> Result<RuleSet, RuleError> {
        let mut configs: Vec<RuleConfig<SupportLang>> = Vec::new();
        let mut digest = Sha256::new();
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
        let ids = configs.iter().map(|rule| rule.id.clone()).collect();
        let collection =
            RuleCollection::try_new(configs).map_err(|err| RuleError::Globs(err.to_string()))?;
        let version = hex::encode(&digest.finalize()[..6]);
        Ok(RuleSet {
            collection,
            version,
            ids,
        })
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn ids(&self) -> &[String] {
        &self.ids
    }

    pub fn for_file(&self, path: &str, language: SupportLang) -> Vec<&RuleConfig<SupportLang>> {
        self.collection
            .get_rule_from_lang(std::path::Path::new(path), language)
    }
}
