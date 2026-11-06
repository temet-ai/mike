#![allow(dead_code)]

mod certs;
mod client;
mod mock_upstream;

pub use certs::{install_crypto, mint_cert};
pub use client::{
    connect_and_request, connect_and_request_h2, connect_and_request_owned, connect_and_stream,
    connect_and_upgrade,
};
pub use mock_upstream::{
    StubHandle, accept_then_reset_stub, plain_http_stub, plain_sse_stub, sse_frame,
    stoppable_accept_stub, tcp_echo_stub, tls_http_stub, tls_upgrade_stub, wait_until_unbound,
};
