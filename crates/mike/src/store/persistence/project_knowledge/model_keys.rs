#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ModelKeys {
    current: String,
    previous: Option<String>,
}

impl ModelKeys {
    pub(crate) fn new(current: String, previous: Option<String>) -> Self {
        Self { current, previous }
    }

    #[cfg(test)]
    pub(crate) fn only(current: impl Into<String>) -> Self {
        Self::new(current.into(), None)
    }

    pub(crate) fn current(&self) -> &str {
        &self.current
    }

    pub(crate) fn accepted(&self) -> Vec<String> {
        std::iter::once(self.current.clone())
            .chain(self.previous.clone())
            .collect()
    }
}

pub(crate) type QueryEmbedding = (String, ModelKeys, String);
