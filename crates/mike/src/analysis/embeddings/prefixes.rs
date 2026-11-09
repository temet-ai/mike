#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Prefixes {
    pub(crate) document: &'static str,
    pub(crate) query: &'static str,
    pub(crate) version: &'static str,
}

const NOMIC: Prefixes = Prefixes {
    document: "search_document: ",
    query: "search_query: ",
    version: "nomic-v1",
};

const EMBEDDING_GEMMA: Prefixes = Prefixes {
    document: "title: none | text: ",
    query: "task: search result | query: ",
    version: "embeddinggemma-v1",
};

const UNPREFIXED: Prefixes = Prefixes {
    document: "",
    query: "",
    version: "unprefixed-v1",
};

fn base_name(model: &str) -> String {
    let name = model.rsplit('/').next().unwrap_or(model);
    name.trim().to_ascii_lowercase()
}

pub(crate) fn prefixes_for(model: &str) -> Prefixes {
    let name = base_name(model);
    if name.starts_with("nomic-embed-text") {
        NOMIC
    } else if name.starts_with("embeddinggemma") {
        EMBEDDING_GEMMA
    } else {
        UNPREFIXED
    }
}
