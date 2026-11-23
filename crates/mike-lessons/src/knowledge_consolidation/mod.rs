pub mod answer;
pub mod apply;
pub mod carry;
pub mod context;
pub mod decide;
pub mod execute;
pub mod files;
pub mod group;
pub mod guard;
pub mod health;
pub mod items;
pub mod llm;
pub mod measure;
pub mod merge;
pub mod passage;
pub mod prompt;
pub mod ranges;
pub mod reach;
pub mod record;
pub mod rewrite;
pub mod settings;
pub mod settle;
pub mod subjects;
pub mod tally;
pub mod undo;
pub mod worker;

pub use worker::spawn_knowledge_consolidation;

#[cfg(test)]
pub mod harness;
#[cfg(test)]
pub mod test_support;
#[cfg(test)]
mod answer_tests;
#[cfg(test)]
mod api_tests;
#[cfg(test)]
mod settle_tests;
#[cfg(test)]
mod decide_tests;
#[cfg(test)]
mod files_tests;
#[cfg(test)]
mod health_tests;
#[cfg(test)]
mod items_tests;
#[cfg(test)]
mod measure_tests;
#[cfg(test)]
mod passage_tests;
#[cfg(test)]
mod carry_tests;
#[cfg(test)]
mod ranges_tests;
#[cfg(test)]
mod rewrite_tests;
#[cfg(test)]
mod reach_tests;
#[cfg(test)]
mod settings_tests;
#[cfg(test)]
mod subjects_tests;
#[cfg(test)]
mod undo_tests;
#[cfg(test)]
mod worker_tests;
