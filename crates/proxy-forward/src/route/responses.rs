use bytes::Bytes;
use http::{Response, StatusCode};
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};

pub type ResponseBody = BoxBody<Bytes, hyper::Error>;

pub(super) fn empty_body() -> ResponseBody {
    Full::new(Bytes::new())
        .map_err(|never: std::convert::Infallible| match never {})
        .boxed()
}

pub(super) fn payload_too_large(max_body_bytes: usize) -> Response<ResponseBody> {
    let body = Full::new(Bytes::from(format!(
        "request body exceeds the maximum buffered size of {max_body_bytes} bytes"
    )))
    .map_err(|never: std::convert::Infallible| match never {})
    .boxed();
    Response::builder()
        .status(StatusCode::PAYLOAD_TOO_LARGE)
        .header(http::header::CONTENT_TYPE, "text/plain")
        .body(body)
        .unwrap_or_else(|_| Response::new(empty_body()))
}

pub(super) fn bad_gateway(msg: &str) -> Response<ResponseBody> {
    let body = Full::new(Bytes::from(msg.to_string()))
        .map_err(|never: std::convert::Infallible| match never {})
        .boxed();
    Response::builder()
        .status(StatusCode::BAD_GATEWAY)
        .header(http::header::CONTENT_TYPE, "text/plain")
        .body(body)
        .unwrap_or_else(|_| Response::new(empty_body()))
}
