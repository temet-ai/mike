//! The wire layer of the crate split (docs/adr/0002-split-crate-mike.md): pure
//! classification of request and response text that the capture path and the
//! features share. Nothing here reaches into the database or a feature.

pub mod composition;
pub mod genuine;
pub mod machine_text;
pub mod mail_text;
pub mod manifest;
pub mod prompt_blocks;
pub mod prompt_classify;
pub mod read_dedup;
pub mod steering_block;
pub mod volatile;
