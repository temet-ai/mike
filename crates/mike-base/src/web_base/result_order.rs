use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultOrder {
    Relevance,
    Recent,
}

impl ResultOrder {
    pub fn parse(order: Option<&str>) -> Result<Self, Box<Response>> {
        match order {
            None | Some("relevance") => Ok(Self::Relevance),
            Some("recent") => Ok(Self::Recent),
            Some(other) => Err(Box::new(
                (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "error": format!("order must be `relevance` or `recent` (got `{other}`)")
                    })),
                )
                    .into_response(),
            )),
        }
    }

    pub fn is_recent(self) -> bool {
        self == Self::Recent
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn parse_without_a_value_means_relevance() {
        assert_eq!(ResultOrder::parse(None).ok(), Some(ResultOrder::Relevance));
    }

    #[test]
    fn parse_names_both_values_in_the_refusal() {
        let refused = ResultOrder::parse(Some("newest")).err();

        assert_eq!(
            refused.map(|response| response.status()),
            Some(StatusCode::BAD_REQUEST)
        );
        assert_eq!(
            ResultOrder::parse(Some("recent")).ok(),
            Some(ResultOrder::Recent)
        );
    }
}
