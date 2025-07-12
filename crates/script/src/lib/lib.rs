pub mod beacon_state_reader;
pub mod consts;
pub mod deploy;
pub mod env;
pub mod eth_client;
pub mod prometheus_metrics;
pub mod proof_storage;
pub mod scripts;
pub mod sp1_client_wrapper;
pub mod tracing;
pub mod utils;
pub mod validator_delta;

use std::sync::atomic::{AtomicBool, Ordering};

/// Global flag to indicate if the runtime is in testing environment.
/// Set this to true in your test setup, and false in production - this disables some checks that get in a way
/// of some data tampering scenarios.
/// This should be set to false in production
pub static RELAX_INPUT_PREPARATION_CHECKS: AtomicBool = AtomicBool::new(false);

pub struct InputChecks {}
impl InputChecks {
    pub fn set_relaxed() {
        RELAX_INPUT_PREPARATION_CHECKS.store(true, Ordering::SeqCst);
    }

    pub fn set_strict() {
        RELAX_INPUT_PREPARATION_CHECKS.store(false, Ordering::SeqCst);
    }

    pub fn is_relaxed() -> bool {
        RELAX_INPUT_PREPARATION_CHECKS.load(Ordering::SeqCst)
    }
}
