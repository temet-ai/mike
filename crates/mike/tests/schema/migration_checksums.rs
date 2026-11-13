use std::collections::BTreeMap;

use serde::Deserialize;

static MIGRATOR: sqlx::migrate::Migrator =
    sqlx::migrate!("../mike-base/src/store/persistence/migrations");

#[derive(Deserialize)]
struct Baseline {
    source_sha: String,
    checksums: BTreeMap<i64, String>,
}

fn differences(baseline: &BTreeMap<i64, String>, current: &BTreeMap<i64, String>) -> Vec<String> {
    baseline
        .iter()
        .filter_map(|(version, expected)| match current.get(version) {
            Some(actual) if actual == expected => None,
            Some(_) => Some(format!(
                "migration {version} changed after the frozen baseline"
            )),
            None => Some(format!(
                "migration {version} was removed after the frozen baseline"
            )),
        })
        .collect()
}

#[test]
fn historical_migrations_match_the_frozen_source_manifest_without_a_database() -> anyhow::Result<()>
{
    let baseline: Baseline = serde_json::from_str(include_str!("migration-baseline.json"))?;
    assert_eq!(
        baseline.source_sha,
        "3cadea314f7a2daf5495da03cac1735247f23822"
    );
    assert!(
        !baseline.checksums.is_empty(),
        "an empty baseline cannot certify history"
    );
    let current = MIGRATOR
        .iter()
        .map(|migration| (migration.version, hex::encode(&migration.checksum)))
        .collect();
    let changed = differences(&baseline.checksums, &current);
    assert!(
        changed.is_empty(),
        "{}\nAdd a new migration instead of editing history. Do not regenerate the frozen manifest to silence this check.",
        changed.join("\n")
    );
    Ok(())
}

#[test]
fn comparison_rejects_changed_and_removed_history_but_accepts_new_migrations() {
    let baseline = BTreeMap::from([(1, "a".to_string()), (2, "b".to_string())]);
    let mut current = baseline.clone();
    current.insert(3, "new".to_string());
    assert!(differences(&baseline, &current).is_empty());
    current.insert(1, "changed".to_string());
    current.remove(&2);
    let changed = differences(&baseline, &current);
    assert_eq!(changed.len(), 2);
    assert!(changed[0].contains("1 changed"));
    assert!(changed[1].contains("2 was removed"));
}
