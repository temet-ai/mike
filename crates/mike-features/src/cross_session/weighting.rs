use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::threshold::Bin;
use crate::settings::cross_session::CrossSessionSettings;
use crate::store::persistence::cross_session::CalibrationRateRow;

pub const UNSPLIT: &str = "unsplit";

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Tally {
    pub k: f64,
    pub n: f64,
}

pub type SourceCounts = BTreeMap<String, Tally>;

#[derive(Debug, Clone, PartialEq)]
pub struct BinInput {
    pub lower_edge: f64,
    pub sources: SourceCounts,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Weighted {
    pub bins: Vec<Bin>,
    pub totals: SourceCounts,
    pub n_raw: i64,
    pub n_weighted: f64,
}

pub fn bin_edge(value: Option<&str>) -> Option<f64> {
    match value? {
        "[0,0.5)" => Some(0.0),
        v => v.parse().ok(),
    }
}

fn tally_of(value: &Value) -> Option<Tally> {
    Some(Tally {
        k: value.get("k")?.as_f64()?,
        n: value.get("n")?.as_f64()?,
    })
}

pub fn bin_input(row: &CalibrationRateRow) -> Option<BinInput> {
    let lower_edge = bin_edge(row.dimension_value.as_deref())?;
    let sources = match row.detail.get("by_source").and_then(Value::as_object) {
        Some(map) => map
            .iter()
            .filter_map(|(source, v)| Some((source.clone(), tally_of(v)?)))
            .collect(),
        None => BTreeMap::from([(
            UNSPLIT.to_owned(),
            Tally {
                k: f64::from(row.k),
                n: f64::from(row.n),
            },
        )]),
    };
    Some(BinInput {
        lower_edge,
        sources,
    })
}

pub fn weight_of(settings: &CrossSessionSettings, source: &str) -> f64 {
    if source == UNSPLIT {
        1.0
    } else {
        settings.source_weight(source)
    }
}

pub fn weigh(inputs: &[BinInput], settings: &CrossSessionSettings) -> Weighted {
    let mut totals = SourceCounts::new();
    let mut bins = Vec::with_capacity(inputs.len());
    for input in inputs {
        let (mut k, mut n) = (0.0, 0.0);
        for (source, tally) in &input.sources {
            let weight = weight_of(settings, source);
            k += weight * tally.k;
            n += weight * tally.n;
            let total = totals.entry(source.clone()).or_default();
            total.k += tally.k;
            total.n += tally.n;
        }
        if n > 0.0 {
            bins.push(Bin {
                lower_edge: input.lower_edge,
                k,
                n,
            });
        }
    }
    let n_raw = totals.values().map(|t| t.n).sum::<f64>().round() as i64;
    let n_weighted = bins.iter().map(|b| b.n).sum();
    Weighted {
        bins,
        totals,
        n_raw,
        n_weighted,
    }
}

pub fn by_source_json(totals: &SourceCounts, settings: &CrossSessionSettings) -> Value {
    Value::Object(
        totals
            .iter()
            .map(|(source, t)| {
                (
                    source.clone(),
                    json!({"k": t.k, "n": t.n, "weight": weight_of(settings, source)}),
                )
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(edge: f64, sources: &[(&str, f64, f64)]) -> BinInput {
        BinInput {
            lower_edge: edge,
            sources: sources
                .iter()
                .map(|(s, k, n)| ((*s).to_owned(), Tally { k: *k, n: *n }))
                .collect(),
        }
    }

    #[test]
    fn weigh_scales_each_source_by_its_weight_and_keeps_raw_totals() {
        let settings = CrossSessionSettings::default();
        let w = weigh(
            &[
                input(0.9, &[("operator", 2.0, 2.0), ("next_message", 4.0, 8.0)]),
                input(0.8, &[("session_reply", 3.0, 4.0)]),
            ],
            &settings,
        );
        assert_eq!(w.n_raw, 14);
        assert!((w.bins[0].n - 6.0).abs() < 1e-9);
        assert!((w.bins[0].k - 4.0).abs() < 1e-9);
        assert!((w.bins[1].n - 4.0).abs() < 1e-9);
        assert!((w.n_weighted - 10.0).abs() < 1e-9);
        assert_eq!(w.totals["next_message"].n, 8.0);
    }

    #[test]
    fn weigh_drops_a_bin_whose_every_source_has_weight_zero() {
        let settings = CrossSessionSettings {
            weight_next_message: 0.0,
            ..CrossSessionSettings::default()
        };
        let w = weigh(&[input(0.9, &[("next_message", 4.0, 8.0)])], &settings);
        assert!(w.bins.is_empty());
        assert_eq!(w.n_raw, 8);
    }

    #[test]
    fn bin_input_without_by_source_counts_the_row_unweighted() -> anyhow::Result<()> {
        let row = CalibrationRateRow {
            family: "steer_calibration".to_owned(),
            metric: "precision".to_owned(),
            population: "steerer_evaluations".to_owned(),
            scope_kind: "global".to_owned(),
            project_id: None,
            axis: "steer_candidate".to_owned(),
            class: "applies".to_owned(),
            dimension: Some("confidence_bin_05".to_owned()),
            dimension_value: Some("0.85".to_owned()),
            question: Some("relapse".to_owned()),
            mode: Some("shadow".to_owned()),
            k: 3,
            n: 5,
            value: None,
            ci_low: None,
            ci_high: None,
            detail: json!({"label_basis": {"operator": 1}}),
        };
        let Some(parsed) = bin_input(&row) else {
            anyhow::bail!("bin row did not parse");
        };
        assert_eq!(parsed.sources[UNSPLIT], Tally { k: 3.0, n: 5.0 });
        let settings = CrossSessionSettings::default();
        assert_eq!(weigh(&[parsed], &settings).bins[0].n, 5.0);
        Ok(())
    }

    #[test]
    fn bin_edge_reads_the_low_bucket_and_decimal_edges() {
        assert_eq!(bin_edge(Some("[0,0.5)")), Some(0.0));
        assert_eq!(bin_edge(Some("0.85")), Some(0.85));
        assert_eq!(bin_edge(Some("junk")), None);
        assert_eq!(bin_edge(None), None);
    }
}
