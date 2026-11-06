use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TokenBasis {
    RealUsageRatio,
    HeuristicBytesDiv4,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub(crate) struct SavingsReport {
    pub(crate) original_bytes: usize,
    pub(crate) compressed_bytes: usize,
    pub(crate) saved_bytes: usize,
    pub(crate) saved_pct: f64,
    pub(crate) est_original_tokens: u64,
    pub(crate) est_compressed_tokens: u64,
    pub(crate) token_basis: TokenBasis,
}

impl SavingsReport {
    pub(crate) fn compute(
        original_bytes: usize,
        compressed_bytes: usize,
        real_input_tokens: Option<u32>,
    ) -> Self {
        let saved_bytes = original_bytes.saturating_sub(compressed_bytes);
        let saved_pct = if original_bytes == 0 {
            0.0
        } else {
            let raw = (saved_bytes as f64 / original_bytes as f64) * 100.0;
            (raw * 100.0).round() / 100.0
        };

        let (est_original_tokens, est_compressed_tokens, token_basis) =
            match (real_input_tokens, original_bytes) {
                (Some(tokens), orig) if orig > 0 => {
                    let scaled = (tokens as f64 * compressed_bytes as f64 / orig as f64).round();
                    let est_compressed = scaled.max(0.0) as u64;
                    (
                        u64::from(tokens),
                        est_compressed,
                        TokenBasis::RealUsageRatio,
                    )
                }
                _ => (
                    (original_bytes / 4) as u64,
                    (compressed_bytes / 4) as u64,
                    TokenBasis::HeuristicBytesDiv4,
                ),
            };

        Self {
            original_bytes,
            compressed_bytes,
            saved_bytes,
            saved_pct,
            est_original_tokens,
            est_compressed_tokens,
            token_basis,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_usage_scales_compressed_tokens_by_byte_ratio() {
        let r = SavingsReport::compute(1000, 500, Some(800));
        assert_eq!(r.token_basis, TokenBasis::RealUsageRatio);
        assert_eq!(r.est_original_tokens, 800);
        assert_eq!(r.est_compressed_tokens, 400);
        assert_eq!(r.saved_bytes, 500);
        assert!((r.saved_pct - 50.0).abs() < 1e-9);
    }

    #[test]
    fn real_usage_rounds_compressed_tokens() {
        let r = SavingsReport::compute(1000, 333, Some(900));
        assert_eq!(r.est_compressed_tokens, 300);
    }

    #[test]
    fn heuristic_used_when_no_real_tokens() {
        let r = SavingsReport::compute(1000, 400, None);
        assert_eq!(r.token_basis, TokenBasis::HeuristicBytesDiv4);
        assert_eq!(r.est_original_tokens, 250);
        assert_eq!(r.est_compressed_tokens, 100);
    }

    #[test]
    fn zero_original_bytes_is_safe() {
        let r = SavingsReport::compute(0, 0, Some(123));
        assert_eq!(r.saved_pct, 0.0);
        assert_eq!(r.saved_bytes, 0);
        assert_eq!(r.token_basis, TokenBasis::HeuristicBytesDiv4);
        assert_eq!(r.est_original_tokens, 0);
        assert_eq!(r.est_compressed_tokens, 0);
    }

    #[test]
    fn saved_pct_rounds_to_two_dp() {
        let r = SavingsReport::compute(3, 1, None);
        assert_eq!(r.saved_bytes, 2);
        assert!((r.saved_pct - 66.67).abs() < 1e-9, "got {}", r.saved_pct);
    }

    #[test]
    fn no_savings_is_zero_pct() {
        let r = SavingsReport::compute(500, 500, Some(100));
        assert_eq!(r.saved_bytes, 0);
        assert_eq!(r.saved_pct, 0.0);
        assert_eq!(r.est_compressed_tokens, 100, "ratio 1.0 keeps tokens");
    }
}
