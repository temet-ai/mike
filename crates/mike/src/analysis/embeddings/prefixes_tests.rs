use super::*;

#[test]
fn prefixes_for_nomic_names_returns_search_prefixes() {
    for model in [
        "nomic-embed-text-v1.5",
        "nomic-embed-text",
        "nomic-embed-text:latest",
        "nomic-ai/nomic-embed-text-v1.5",
    ] {
        let prefixes = prefixes_for(model);
        assert_eq!(prefixes.document, "search_document: ", "{model}");
        assert_eq!(prefixes.query, "search_query: ", "{model}");
        assert_eq!(prefixes.version, "nomic-v1", "{model}");
    }
}

#[test]
fn prefixes_for_embeddinggemma_names_returns_model_card_prefixes() {
    for model in [
        "embeddinggemma",
        "embeddinggemma:latest",
        "embeddinggemma:300m",
        "google/embeddinggemma-300m",
        "EmbeddingGemma",
    ] {
        let prefixes = prefixes_for(model);
        assert_eq!(prefixes.document, "title: none | text: ", "{model}");
        assert_eq!(prefixes.query, "task: search result | query: ", "{model}");
        assert_eq!(prefixes.version, "embeddinggemma-v1", "{model}");
    }
}

#[test]
fn prefixes_for_unknown_model_returns_no_prefix() {
    let prefixes = prefixes_for("all-minilm");
    assert_eq!(prefixes.document, "");
    assert_eq!(prefixes.query, "");
    assert_eq!(prefixes.version, "unprefixed-v1");
}

#[test]
fn score_floor_for_nomic_names_returns_floor_060_window_008() {
    for model in [
        "nomic-embed-text-v1.5",
        "nomic-embed-text:latest",
        "nomic-ai/nomic-embed-text-v1.5",
    ] {
        let cut = score_floor_for(model);
        assert_eq!((cut.floor, cut.window), (0.60, 0.08), "{model}");
    }
}

#[test]
fn score_floor_for_embeddinggemma_names_returns_floor_045_window_012() {
    for model in [
        "embeddinggemma",
        "embeddinggemma:300m",
        "google/embeddinggemma-300m",
        "EmbeddingGemma",
    ] {
        let cut = score_floor_for(model);
        assert_eq!((cut.floor, cut.window), (0.45, 0.12), "{model}");
    }
}

#[test]
fn score_floor_for_unknown_model_returns_no_cut() {
    assert_eq!(score_floor_for("all-minilm"), ScoreFloor::NO_CUT);
    assert_eq!(score_floor_for(""), ScoreFloor::NO_CUT);
}

#[test]
fn prefixes_for_different_models_return_different_versions() {
    let versions = [
        prefixes_for("nomic-embed-text-v1.5").version,
        prefixes_for("embeddinggemma").version,
        prefixes_for("other").version,
    ];
    assert_ne!(versions[0], versions[1]);
    assert_ne!(versions[1], versions[2]);
    assert_ne!(versions[0], versions[2]);
}
