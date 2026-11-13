use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

use super::{IngestConnection, IngestOutcome, IngestSource};

pub const LIVE_TURN: &str = include_str!("fixtures/live_turn.jsonl");
pub const RECONNECT: &str = include_str!("fixtures/reconnect.jsonl");
pub const SYNTHETIC_ITEMS: &str = include_str!("fixtures/synthetic_items.jsonl");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Sent,
    Received,
}

#[derive(Debug, Clone)]
pub struct Transcript {
    pub header: Value,
    pub frames: Vec<(Direction, String)>,
}

impl Transcript {
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        let mut lines = text.lines();
        let header: Value = serde_json::from_str(
            lines
                .next()
                .ok_or_else(|| anyhow::anyhow!("empty transcript"))?,
        )?;
        let mut frames = Vec::new();
        for line in lines.filter(|line| !line.trim().is_empty()) {
            let record: Value = serde_json::from_str(line)?;
            let direction = match record.get("dir").and_then(Value::as_str) {
                Some("out") => Direction::Sent,
                Some("in") => Direction::Received,
                other => anyhow::bail!("unknown frame direction {other:?} in {line}"),
            };
            let frame = record
                .get("frame")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow::anyhow!("frame missing in {line}"))?;
            frames.push((direction, frame.to_owned()));
        }
        Ok(Self { header, frames })
    }

    pub fn header_text(&self, key: &str) -> anyhow::Result<String> {
        self.header
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| anyhow::anyhow!("transcript header has no {key}"))
    }

    pub fn source(&self) -> anyhow::Result<IngestSource> {
        Ok(IngestSource {
            codex_version: self.header_text("codex_version")?,
        })
    }

    pub fn received(&self) -> impl Iterator<Item = anyhow::Result<Value>> + '_ {
        self.frames
            .iter()
            .filter(|(direction, _)| *direction == Direction::Received)
            .map(|(_, frame)| Ok(serde_json::from_str(frame)?))
    }
}

pub fn replay(
    transcript: &Transcript,
    connection: &mut IngestConnection,
) -> anyhow::Result<Vec<IngestOutcome>> {
    let mut outcomes = Vec::new();
    for (direction, frame) in &transcript.frames {
        match direction {
            Direction::Sent => {
                connection.sent(frame.as_bytes());
            }
            Direction::Received => {
                outcomes.push(connection.received(frame.as_bytes().to_vec(), Utc::now())?);
            }
        }
    }
    Ok(outcomes)
}

pub fn connection(transcript: &Transcript) -> anyhow::Result<IngestConnection> {
    Ok(IngestConnection::new(
        Uuid::from_u128(7),
        transcript.source()?,
    ))
}
