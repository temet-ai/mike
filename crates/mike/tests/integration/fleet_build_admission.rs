use mike::launch::fleet::test_seams::admission::{
    Error as AdmissionError, LOCK_NAME, acquire, io_error,
};

#[path = "../../../mike-fleet/src/launch/fleet/build_admission_tests.rs"]
mod tests;
