use serde::Serialize;

pub const Z: f64 = 1.96;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Bounds {
    pub precision: Option<f64>,
    pub low: Option<f64>,
    pub high: Option<f64>,
    pub n: i64,
}

pub fn wilson_bounds(k: i64, n: i64) -> Bounds {
    if n <= 0 {
        return Bounds {
            precision: None,
            low: None,
            high: None,
            n,
        };
    }
    let nf = n as f64;
    let p = k as f64 / nf;
    let z2 = Z * Z;
    let centre = p + z2 / (2.0 * nf);
    let half = Z * (p * (1.0 - p) / nf + z2 / (4.0 * nf * nf)).sqrt();
    let denom = 1.0 + z2 / nf;
    Bounds {
        precision: Some(p),
        low: Some(((centre - half) / denom).max(0.0)),
        high: Some(((centre + half) / denom).min(1.0)),
        n,
    }
}

#[cfg(test)]
mod tests {
    use super::wilson_bounds;

    #[test]
    fn wilson_bounds_match_the_gate_arithmetic() {
        let b = wilson_bounds(34, 40);
        assert!((b.low.unwrap_or_default() - 0.709).abs() < 0.002, "{b:?}");
        let b = wilson_bounds(36, 40);
        assert!((b.low.unwrap_or_default() - 0.770).abs() < 0.002, "{b:?}");
        let b = wilson_bounds(68, 80);
        assert!((b.low.unwrap_or_default() - 0.756).abs() < 0.003, "{b:?}");
        assert_eq!(wilson_bounds(0, 0).precision, None);
    }
}
