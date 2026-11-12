//! The wire layer of the crate split (docs/adr/0002-split-crate-mike.md): pure
//! classification of request and response text that the capture path and the
//! features share. Nothing here reaches into the database or a feature.

pub mod composition;
pub mod genuine;
pub(crate) mod machine_text;
pub(crate) mod mail_text;
pub(crate) mod manifest;
pub(crate) mod prompt_blocks;
pub(crate) mod prompt_classify;
pub(crate) mod read_dedup;
pub(crate) mod steering_block;
pub(crate) mod volatile;
