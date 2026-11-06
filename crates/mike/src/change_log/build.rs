use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) struct BuildStamp {
    pub(crate) sha: &'static str,
    pub(crate) sha_short: &'static str,
    pub(crate) dirty: bool,
    pub(crate) built_at: &'static str,
}

pub(crate) fn current_build() -> BuildStamp {
    BuildStamp {
        sha: env!("MIKE_BUILD_SHA"),
        sha_short: env!("MIKE_BUILD_SHA_SHORT"),
        dirty: matches!(env!("MIKE_BUILD_DIRTY"), "true"),
        built_at: env!("MIKE_BUILD_TIME"),
    }
}

impl BuildStamp {
    pub(crate) fn to_value(self, defaults: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "sha": self.sha,
            "sha_short": self.sha_short,
            "dirty": self.dirty,
            "built_at": self.built_at,
            "defaults": defaults,
        })
    }
}
