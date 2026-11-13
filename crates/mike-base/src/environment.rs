pub trait Environment: Sync {
    fn var(&self, name: &str) -> Option<String>;
    fn vars(&self) -> Vec<(String, String)>;
}

pub fn lookup(environment: &dyn Environment) -> impl Fn(&str) -> Option<String> + '_ {
    move |name| environment.var(name)
}

pub struct ProcessEnvironment;

impl Environment for ProcessEnvironment {
    fn var(&self, name: &str) -> Option<String> {
        std::env::var_os(name).map(|value| value.to_string_lossy().into_owned())
    }

    fn vars(&self) -> Vec<(String, String)> {
        std::env::vars_os()
            .map(|(name, value)| {
                (
                    name.to_string_lossy().into_owned(),
                    value.to_string_lossy().into_owned(),
                )
            })
            .collect()
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Default)]
pub struct MapEnvironment {
    entries: std::collections::BTreeMap<String, String>,
}

#[cfg(test)]
impl MapEnvironment {
    pub fn new(pairs: &[(&str, &str)]) -> Self {
        pairs
            .iter()
            .fold(Self::default(), |env, (name, value)| env.with(name, value))
    }

    pub fn isolated() -> Self {
        Self::default().with("HOME", "/nonexistent/mike-isolated-environment-home")
    }

    pub fn with(mut self, name: &str, value: &str) -> Self {
        self.entries.insert(name.to_owned(), value.to_owned());
        self
    }
}

#[cfg(test)]
impl Environment for MapEnvironment {
    fn var(&self, name: &str) -> Option<String> {
        self.entries.get(name).cloned()
    }

    fn vars(&self) -> Vec<(String, String)> {
        self.entries
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect()
    }
}

#[cfg(test)]
#[path = "environment_tests.rs"]
pub mod environment_tests;
