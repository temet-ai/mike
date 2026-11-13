#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModelKeys {
    current: String,
    previous: Option<String>,
}

impl ModelKeys {
    pub fn new(current: String, previous: Option<String>) -> Self {
        Self { current, previous }
    }

    #[cfg(test)]
    pub fn only(current: impl Into<String>) -> Self {
        Self::new(current.into(), None)
    }

    pub fn current(&self) -> &str {
        &self.current
    }

    pub fn accepted(&self) -> Vec<String> {
        std::iter::once(self.current.clone())
            .chain(self.previous.clone())
            .collect()
    }
}

pub type QueryEmbedding = (String, ModelKeys, String);
