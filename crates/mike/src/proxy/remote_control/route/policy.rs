use super::RemoteControlRoute;
use super::payload::{self, classify_payloads};
use super::signals::{self, ChannelSignals};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BodyCapture {
    Full,
    DigestOnly,
}

impl BodyCapture {
    pub(crate) fn keeps_bytes(self) -> bool {
        matches!(self, Self::Full)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SignalsFrom {
    Neither,
    Request,
    Response,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RequestPolicy {
    Whole,
    ByPayloadType,
}

pub(super) struct KnownEndpoint {
    pub(super) suffix: &'static str,
    request: RequestPolicy,
    response: BodyCapture,
    signals_from: SignalsFrom,
}

pub(super) const KNOWN_ENDPOINTS: &[KnownEndpoint] = &[
    KnownEndpoint {
        suffix: "/worker/heartbeat",
        request: RequestPolicy::Whole,
        response: BodyCapture::Full,
        signals_from: SignalsFrom::Request,
    },
    KnownEndpoint {
        suffix: "/worker",
        request: RequestPolicy::Whole,
        response: BodyCapture::Full,
        signals_from: SignalsFrom::Request,
    },
    KnownEndpoint {
        suffix: "/bridge",
        request: RequestPolicy::Whole,
        response: BodyCapture::DigestOnly,
        signals_from: SignalsFrom::Response,
    },
    KnownEndpoint {
        suffix: "/worker/register",
        request: RequestPolicy::Whole,
        response: BodyCapture::DigestOnly,
        signals_from: SignalsFrom::Response,
    },
    KnownEndpoint {
        suffix: "/worker/events",
        request: RequestPolicy::ByPayloadType,
        response: BodyCapture::Full,
        signals_from: SignalsFrom::Neither,
    },
    KnownEndpoint {
        suffix: "/worker/internal-events",
        request: RequestPolicy::ByPayloadType,
        response: BodyCapture::Full,
        signals_from: SignalsFrom::Request,
    },
    KnownEndpoint {
        suffix: "/worker/events/delivery",
        request: RequestPolicy::Whole,
        response: BodyCapture::Full,
        signals_from: SignalsFrom::Neither,
    },
    KnownEndpoint {
        suffix: "/client/presence",
        request: RequestPolicy::Whole,
        response: BodyCapture::Full,
        signals_from: SignalsFrom::Neither,
    },
    KnownEndpoint {
        suffix: "/worker/diagnostics",
        request: RequestPolicy::Whole,
        response: BodyCapture::Full,
        signals_from: SignalsFrom::Neither,
    },
    KnownEndpoint {
        suffix: "/worker/events/stream",
        request: RequestPolicy::Whole,
        response: BodyCapture::DigestOnly,
        signals_from: SignalsFrom::Neither,
    },
];

fn known(suffix: &str) -> Option<&'static KnownEndpoint> {
    KNOWN_ENDPOINTS.iter().find(|e| e.suffix == suffix)
}

impl RemoteControlRoute<'_> {
    pub(crate) fn plan_request<'b>(&self, body: Option<&'b [u8]>) -> payload::RequestPlan<'b> {
        let policy = known(self.suffix).map_or(RequestPolicy::Whole, |e| e.request);
        let payloads = match (policy, body) {
            (RequestPolicy::ByPayloadType, Some(body)) => classify_payloads(body),
            _ => None,
        };
        match payloads {
            Some(payloads) => {
                let payload_link = payloads.iter().find_map(|p| {
                    p.body
                        .and_then(|b| payload::payload_session_link(p.payload_type, b))
                });
                payload::RequestPlan {
                    capture: BodyCapture::DigestOnly,
                    payloads,
                    payload_link,
                }
            }
            None => payload::RequestPlan {
                capture: BodyCapture::Full,
                payloads: Vec::new(),
                payload_link: None,
            },
        }
    }

    pub(crate) fn response_capture(&self) -> BodyCapture {
        known(self.suffix).map_or(BodyCapture::Full, |e| e.response)
    }

    pub(crate) fn channel_signals(
        &self,
        request_body: Option<&[u8]>,
        response_body: Option<&[u8]>,
    ) -> ChannelSignals {
        let body = match known(self.suffix).map(|e| e.signals_from) {
            Some(SignalsFrom::Request) => request_body,
            Some(SignalsFrom::Response) => response_body,
            Some(SignalsFrom::Neither) | None => None,
        };
        body.map(signals::parse_channel_signals).unwrap_or_default()
    }

    pub(crate) fn endpoint_label(&self) -> &'static str {
        known(self.suffix).map_or("other", |e| e.suffix)
    }
}

pub(crate) fn endpoint_labels() -> impl Iterator<Item = &'static str> {
    KNOWN_ENDPOINTS.iter().map(|e| e.suffix).chain(["other"])
}
