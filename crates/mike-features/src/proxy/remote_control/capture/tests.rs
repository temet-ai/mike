use super::*;
use axum::http::HeaderValue;

#[test]
fn a_request_without_framing_headers_carried_no_body() {
    assert!(!request_carried_a_body(&HeaderMap::new()));

    let mut with_length = HeaderMap::new();
    with_length.insert(
        axum::http::header::CONTENT_LENGTH,
        HeaderValue::from_static("0"),
    );
    assert!(
        request_carried_a_body(&with_length),
        "content-length: 0 is an empty body, not an absent one"
    );

    let mut chunked = HeaderMap::new();
    chunked.insert(
        axum::http::header::TRANSFER_ENCODING,
        HeaderValue::from_static("chunked"),
    );
    assert!(request_carried_a_body(&chunked));
}

#[test]
fn only_the_statuses_defined_to_have_no_body_are_read_as_having_none() {
    let post = Method::POST;
    for (code, carries) in [
        (200, true),
        (201, true),
        (202, true),
        (400, true),
        (429, true),
        (500, true),
        (529, true),
        (100, false),
        (204, false),
        (304, false),
    ] {
        let Ok(status) = reqwest::StatusCode::from_u16(code) else {
            panic!("{code} is a real status code");
        };
        assert_eq!(
            response_carries_a_body(&post, status),
            carries,
            "{code} was read the wrong way round"
        );
    }
    assert!(
        !response_carries_a_body(&Method::HEAD, reqwest::StatusCode::OK),
        "a HEAD response carries no body whatever its status says"
    );
}
