use std::collections::BTreeMap;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rules = mike_code::RuleSet::builtin().map_err(|err| err.to_string())?;
    let mut per_rule: BTreeMap<String, usize> = BTreeMap::new();
    let mut files = 0usize;
    let mut slowest = (0u128, String::new());
    let started = Instant::now();
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path)?;
        let at = Instant::now();
        let scan = mike_code::scan(rules, &path, &bytes);
        let took = at.elapsed().as_micros();
        if took > slowest.0 {
            slowest = (took, path.clone());
        }
        if scan.skipped.is_none() {
            files += 1;
        }
        for finding in scan.findings {
            *per_rule.entry(finding.rule_id).or_default() += 1;
        }
    }
    println!("rules {} version {}", rules.ids().len(), rules.version());
    println!(
        "files scanned {files} in {} ms; slowest {} us {}",
        started.elapsed().as_millis(),
        slowest.0,
        slowest.1
    );
    for (rule, count) in per_rule {
        println!("{count:>7} {rule}");
    }
    Ok(())
}
