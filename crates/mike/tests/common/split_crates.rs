use std::path::{Path, PathBuf};

pub fn src_roots() -> anyhow::Result<Vec<PathBuf>> {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let map_path = crates.join("../docs/adr/0002-crate-map.json");
    let map: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&map_path)?)?;
    let packages = map["packages"]
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("{} has no packages object", map_path.display()))?;
    let mut names: Vec<&String> = packages.keys().collect();
    names.sort();
    Ok(names
        .into_iter()
        .map(|name| crates.join(name).join("src"))
        .collect())
}
