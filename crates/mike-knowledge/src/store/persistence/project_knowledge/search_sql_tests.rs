const SEARCH_SQL: &str = include_str!("search.sql");

fn occurrences(needle: &str) -> usize {
    SEARCH_SQL.matches(needle).count()
}

#[test]
fn search_sql_candidate_row_vectors_computed_once_per_row() {
    for (expression, branch) in [
        ("to_tsvector('english', l.text)", "lesson"),
        ("to_tsvector('english', ss.overview)", "session"),
        ("memory.session_learnings_text(k.learnings)", "learning"),
    ] {
        assert_eq!(
            occurrences(expression),
            1,
            "the {branch} branch must compute `{expression}` once, in its LATERAL, and reuse it for the match, rank and identifier test"
        );
    }
}

#[test]
fn search_sql_candidate_row_vectors_reused_by_match_rank_and_ident() {
    for alias in ["lt", "st", "kt"] {
        for usage in [
            format!("{alias}.tsv @@ q.ident_tsq"),
            format!("ts_rank_cd({alias}.tsv, q.tsq)"),
            format!("{alias}.tsv @@ q.match_tsq"),
        ] {
            assert_eq!(
                occurrences(&usage),
                1,
                "expected `{usage}` once in search.sql"
            );
        }
    }
}
