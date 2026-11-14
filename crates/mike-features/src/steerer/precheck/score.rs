use serde::Serialize;

pub fn auroc(scored: &[(f64, bool)]) -> Option<f64> {
    let positives: Vec<f64> = scored.iter().filter(|(_, y)| *y).map(|(s, _)| *s).collect();
    let negatives: Vec<f64> = scored
        .iter()
        .filter(|(_, y)| !*y)
        .map(|(s, _)| *s)
        .collect();
    if positives.is_empty() || negatives.is_empty() {
        return None;
    }
    let mut wins = 0.0;
    for p in &positives {
        for n in &negatives {
            wins += if p > n {
                1.0
            } else if (p - n).abs() < f64::EPSILON {
                0.5
            } else {
                0.0
            };
        }
    }
    Some(wins / (positives.len() * negatives.len()) as f64)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Cut {
    pub threshold: f64,
    pub right_kept: usize,
    pub right_total: usize,
    pub wrong_dropped: usize,
    pub wrong_total: usize,
}

pub fn cut(scored: &[(f64, bool)], threshold: f64) -> Cut {
    let right_total = scored.iter().filter(|(_, y)| *y).count();
    let wrong_total = scored.len() - right_total;
    Cut {
        threshold,
        right_kept: scored.iter().filter(|(s, y)| *y && *s >= threshold).count(),
        right_total,
        wrong_dropped: scored.iter().filter(|(s, y)| !*y && *s < threshold).count(),
        wrong_total,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auroc_of_a_perfect_ranking_is_one_and_of_a_reversed_one_is_zero() {
        let perfect = [(0.9, true), (0.8, true), (0.2, false), (0.1, false)];
        assert_eq!(auroc(&perfect), Some(1.0));
        let reversed = [(0.1, true), (0.9, false)];
        assert_eq!(auroc(&reversed), Some(0.0));
        let tied = [(0.5, true), (0.5, false)];
        assert_eq!(auroc(&tied), Some(0.5));
        assert_eq!(auroc(&[(0.5, true)]), None);
    }

    #[test]
    fn cut_counts_right_kept_and_wrong_dropped() {
        let scored = [(0.9, true), (0.3, true), (0.6, false), (0.1, false)];
        let c = cut(&scored, 0.5);
        assert_eq!((c.right_kept, c.right_total), (1, 2));
        assert_eq!((c.wrong_dropped, c.wrong_total), (1, 2));
    }
}
