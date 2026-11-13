use std::sync::OnceLock;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct BuildStamp {
    pub sha: &'static str,
    pub sha_short: &'static str,
    pub dirty: bool,
    pub built_at: &'static str,
}

const STAMP_SLOT_LEN: usize = 256;
const STAMP_SLOT_MARKER: &[u8] = b"MIKE-BUILD-STAMP-SLOT-V1:";

static STAMP_SLOT: [u8; STAMP_SLOT_LEN] = stamp_slot_template();

const fn stamp_slot_template() -> [u8; STAMP_SLOT_LEN] {
    let mut slot = [0u8; STAMP_SLOT_LEN];
    let mut index = 0;
    while index < STAMP_SLOT_MARKER.len() {
        slot[index] = STAMP_SLOT_MARKER[index];
        index += 1;
    }
    slot
}

pub fn current_build() -> BuildStamp {
    static CURRENT: OnceLock<BuildStamp> = OnceLock::new();
    *CURRENT.get_or_init(|| {
        let slot = unsafe { std::ptr::read_volatile(&raw const STAMP_SLOT) };
        stamped_build(&slot[STAMP_SLOT_MARKER.len()..]).unwrap_or_else(compiled_build)
    })
}

fn compiled_build() -> BuildStamp {
    BuildStamp {
        sha: env!("MIKE_BUILD_SHA"),
        sha_short: env!("MIKE_BUILD_SHA_SHORT"),
        dirty: matches!(env!("MIKE_BUILD_DIRTY"), "true"),
        built_at: env!("MIKE_BUILD_TIME"),
    }
}

fn stamped_build(payload: &[u8]) -> Option<BuildStamp> {
    let end = payload.iter().position(|byte| *byte == 0)?;
    let text = std::str::from_utf8(&payload[..end]).ok()?;
    let mut fields = text.split('\t');
    let (sha, sha_short, dirty, built_at) = (
        fields.next()?,
        fields.next()?,
        fields.next()?,
        fields.next()?,
    );
    let dirty = match dirty {
        "true" => true,
        "false" => false,
        _ => return None,
    };
    if fields.next().is_some() || [sha, sha_short, built_at].iter().any(|f| f.is_empty()) {
        return None;
    }
    let leak = |value: &str| -> &'static str { Box::leak(value.to_owned().into_boxed_str()) };
    Some(BuildStamp {
        sha: leak(sha),
        sha_short: leak(sha_short),
        dirty,
        built_at: leak(built_at),
    })
}

impl BuildStamp {
    pub fn to_value(self, defaults: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "sha": self.sha,
            "sha_short": self.sha_short,
            "dirty": self.dirty,
            "built_at": self.built_at,
            "defaults": defaults,
        })
    }
}

#[cfg(test)]
mod stamp_tests {
    use super::*;

    #[test]
    fn current_build_unstamped_slot_keeps_compiled_identity() {
        assert_eq!(stamped_build(&[0u8; 8]), None);
        assert_eq!(current_build(), compiled_build());
    }

    #[test]
    fn stamped_build_four_fields_gives_them() {
        let stamp = stamped_build(b"abc123\tabc\tfalse\t2026-11-13T15:00:00Z\0\0\0");
        assert_eq!(
            stamp,
            Some(BuildStamp {
                sha: "abc123",
                sha_short: "abc",
                dirty: false,
                built_at: "2026-11-13T15:00:00Z",
            })
        );
    }

    #[test]
    fn stamped_build_malformed_payload_is_ignored() {
        for payload in [
            &b"abc\tabc\tmaybe\tt\0"[..],
            b"abc\tabc\tfalse\0",
            b"abc\tabc\tfalse\tt\textra\0",
            b"\tabc\tfalse\tt\0",
            b"abc\tabc\tfalse\tt",
        ] {
            assert_eq!(stamped_build(payload), None, "{payload:?}");
        }
    }

    #[test]
    fn stamp_slot_template_opens_with_marker_then_zeros() {
        assert!(STAMP_SLOT.starts_with(STAMP_SLOT_MARKER));
        assert!(
            STAMP_SLOT[STAMP_SLOT_MARKER.len()..]
                .iter()
                .all(|b| *b == 0)
        );
    }
}
