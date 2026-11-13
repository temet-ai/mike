#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prefixes {
    pub document: &'static str,
    pub query: &'static str,
    pub version: &'static str,
    pub previous_version: Option<&'static str>,
    title_slot: Option<TitleSlot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TitleSlot {
    before: &'static str,
    between: &'static str,
}

impl Prefixes {
    pub fn titled_document(&self, header: &str, chunk: &str) -> String {
        let header = header.trim();
        match self.title_slot {
            Some(_) if header.is_empty() => format!("{}{}", self.document, chunk),
            Some(slot) => format!("{}{header}{}{chunk}", slot.before, slot.between),
            None if header.is_empty() => format!("{}{chunk}", self.document),
            None => format!("{}{header}\n{chunk}", self.document),
        }
    }
}

const NOMIC: Prefixes = Prefixes {
    document: "search_document: ",
    query: "search_query: ",
    version: "nomic-v1",
    previous_version: None,
    title_slot: None,
};

const EMBEDDING_GEMMA: Prefixes = Prefixes {
    document: "title: none | text: ",
    query: "task: search result | query: ",
    version: "embeddinggemma-v2",
    previous_version: Some("embeddinggemma-v1"),
    title_slot: Some(TitleSlot {
        before: "title: ",
        between: " | text: ",
    }),
};

const UNPREFIXED: Prefixes = Prefixes {
    document: "",
    query: "",
    version: "unprefixed-v1",
    previous_version: None,
    title_slot: None,
};

fn base_name(model: &str) -> String {
    let name = model.rsplit('/').next().unwrap_or(model);
    name.trim().to_ascii_lowercase()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScoreFloor {
    pub floor: f64,
    pub window: f64,
}

impl ScoreFloor {
    pub const NO_CUT: ScoreFloor = ScoreFloor {
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

pub fn score_floor_for(model: &str) -> ScoreFloor {
    let name = base_name(model);
    if name.starts_with("nomic-embed-text") {
        NOMIC_FLOOR
    } else if name.starts_with("embeddinggemma") {
        EMBEDDING_GEMMA_FLOOR
    } else {
        ScoreFloor::NO_CUT
    }
}

pub fn prefixes_for(model: &str) -> Prefixes {
    let name = base_name(model);
    if name.starts_with("nomic-embed-text") {
        NOMIC
    } else if name.starts_with("embeddinggemma") {
        EMBEDDING_GEMMA
    } else {
        UNPREFIXED
    }
}
