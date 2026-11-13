use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct BuildStamp {
    pub sha: &'static str,
    pub sha_short: &'static str,
    pub dirty: bool,
    pub built_at: &'static str,
}

pub fn current_build() -> BuildStamp {
    BuildStamp {
        sha: env!("MIKE_BUILD_SHA"),
        sha_short: env!("MIKE_BUILD_SHA_SHORT"),
        dirty: matches!(env!("MIKE_BUILD_DIRTY"), "true"),
        built_at: env!("MIKE_BUILD_TIME"),
    }
}

impl BuildStamp {
    pub fn to_value(self, defaults: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "sha": self.sha,
            "sha_short": self.sha_short,
            "dirty": self.dirty,
            "built_at": self.built_at,
            "defaults": defaults,
        })
    }
}
