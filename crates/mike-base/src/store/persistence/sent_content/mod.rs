mod assemble;
mod backfill;
mod backfill_derive;
mod compaction;
mod derive;
mod live;
mod parts;
mod read;
#[cfg(any(test, feature = "test-seams"))]
mod seam;
mod store;

pub use backfill::{SentBackfillBatch, SentBackfillFailure};
pub(crate) use compaction::{ClearCandidate, clearable_ids};
pub(crate) use derive::sent_envelope;
pub use live::{CapturedBodies, OriginalMessages, write_captured_sent_messages};
pub(crate) use read::fetch_sent_body;
#[cfg(any(test, feature = "test-seams"))]
pub use seam::sent_round_trip_in_memory;

#[cfg(test)]
mod tests;
