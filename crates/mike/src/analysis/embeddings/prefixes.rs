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

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ScoreFloor {
    pub(crate) floor: f64,
    pub(crate) window: f64,
}

impl ScoreFloor {
    pub(crate) const NO_CUT: ScoreFloor = ScoreFloor {
        floor: 0.0,
        window: 1.0,
    };
}

const NOMIC_FLOOR: ScoreFloor = ScoreFloor {
    floor: 0.60,
    window: 0.08,
};

const EMBEDDING_GEMMA_FLOOR: ScoreFloor = ScoreFloor {
    floor: 0.45,
    window: 0.12,
};

pub(crate) fn score_floor_for(model: &str) -> ScoreFloor {
    let name = base_name(model);
    if name.starts_with("nomic-embed-text") {
        NOMIC_FLOOR
    } else if name.starts_with("embeddinggemma") {
        EMBEDDING_GEMMA_FLOOR
    } else {
        ScoreFloor::NO_CUT
    }
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
