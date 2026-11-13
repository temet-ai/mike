use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detail {
    Full,
    Best,
}

impl Detail {
    pub fn parse(detail: Option<&str>) -> Result<Self, Box<Response>> {
        match detail {
            None | Some("full") => Ok(Self::Full),
            Some("best") => Ok(Self::Best),
            Some(other) => Err(Box::new(
                (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "error": format!("detail must be `full` or `best` (got `{other}`)")
                    })),
                )
                    .into_response(),
            )),
        }
    }

    pub fn is_best(self) -> bool {
        self == Self::Best
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_without_a_value_means_full() {
        assert_eq!(Detail::parse(None).ok(), Some(Detail::Full));
        assert_eq!(Detail::parse(Some("full")).ok(), Some(Detail::Full));
    }

    #[test]
    fn parse_best_means_best() {
        assert_eq!(Detail::parse(Some("best")).ok(), Some(Detail::Best));
    }

    #[test]
    fn parse_another_value_is_refused_with_400() {
        let refused = Detail::parse(Some("brief")).err();

        assert_eq!(
            refused.map(|response| response.status()),
            Some(StatusCode::BAD_REQUEST)
        );
    }
}
