pub(crate) fn env_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
}

pub(crate) struct EnvVarGuard {
    key: &'static str,
    prior: Option<std::ffi::OsString>,
}

impl EnvVarGuard {
    pub(crate) fn set(key: &'static str, value: &str) -> Self {
        let prior = std::env::var_os(key);
        unsafe { std::env::set_var(key, value) };
        Self { key, prior }
    }

    pub(crate) fn unset(key: &'static str) -> Self {
        let prior = std::env::var_os(key);
        unsafe { std::env::remove_var(key) };
        Self { key, prior }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.prior {
            Some(v) => unsafe { std::env::set_var(self.key, v) },
            None => unsafe { std::env::remove_var(self.key) },
        }
    }
}

pub(crate) fn clear_proxy_env() -> Vec<EnvVarGuard> {
    [
        "MIKE_LISTEN_ADDR",
        "MIKE_INSPECT_ADDR",
        "MIKE_UPSTREAM_URL",
        "MIKE_MAX_REQUESTS",
        "MIKE_MAX_AGE",
        "MIKE_OTEL_ENDPOINT",
        "MIKE_LOG_FORMAT",
        "MIKE_PERSIST_ENABLED",
        "MIKE_DATABASE_URL",
        "MIKE_LOCAL_DATABASE_URL",
        "MIKE_DATA_DIR",
        "MIKE_PROJECT_PINNED",
        "MIKE_DEFAULT_TTL_SECS",
        "MIKE_SMOKE_RUN",
        "MIKE_FRONTEND_PORT",
        "MIKE_COMPOSITION_ENABLED",
        "MIKE_CONFIG_DIR",
        "XDG_CONFIG_HOME",
        "MIKE_CONFIG_HOME",
    ]
    .into_iter()
    .map(EnvVarGuard::unset)
    .collect()
}
