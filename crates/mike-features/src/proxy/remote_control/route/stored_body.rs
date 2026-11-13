use super::policy::BodyCapture;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct StoredBody<'a> {
    pub bytes: Option<&'a [u8]>,
    pub sha256: Option<[u8; 32]>,
    pub size: Option<i32>,
}

pub fn stored_body(body: Option<&[u8]>, capture: BodyCapture) -> StoredBody<'_> {
    let Some(body) = body else {
        return StoredBody::default();
    };
    let sha: [u8; 32] = <sha2::Sha256 as sha2::Digest>::digest(body).into();
    StoredBody {
        bytes: capture.keeps_bytes().then_some(body),
        sha256: Some(sha),
        size: Some(i32::try_from(body.len()).unwrap_or(i32::MAX)),
    }
}
