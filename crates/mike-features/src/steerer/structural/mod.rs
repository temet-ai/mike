pub mod current_primitives;
pub mod import_graph;
pub mod never_fired;
pub mod reachability;
pub mod source_tree;
pub mod wiring;
mod wiring_checks;
pub mod wiring_store;
pub mod wiring_worker;

#[cfg(test)]
mod wiring_tests;

pub use current_primitives::{PrimitiveFinding, check_current_primitives};
pub use import_graph::{ImportGraph, build_import_graph};
pub use never_fired::{
    CONTEXT_ITEM_KIND, FLEET_EVENT_KIND, NeverFiredFinding, NeverFiredRule,
    check_never_fired_context_kind, check_never_written_fleet_event_kind,
};
pub use reachability::{UnreachableFinding, check_reachability};
pub use source_tree::{SourceFile, read_source_tree};
pub use wiring_worker::spawn_wiring_liveness;
