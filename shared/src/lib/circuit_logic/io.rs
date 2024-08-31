use crate::lido::LidoValidatorState;
use crate::{circuit_logic::report::ReportData, eth_consensus_layer::Hash256};
use tree_hash::TreeHash;

use crate::io::eth_io::{LidoValidatorStateSolidity, PublicValuesSolidity, ReportMetadataSolidity, ReportSolidity};

pub fn create_public_values(
    report: &ReportData,
    beacon_block_hash: &Hash256,
    old_state: &LidoValidatorState,
    new_state: &LidoValidatorState,
) -> PublicValuesSolidity {
    PublicValuesSolidity {
        report: ReportSolidity {
            slot: report.slot,
            deposited_lido_validators: report.deposited_lido_validators,
            exited_lido_validators: report.exited_lido_validators,
            lido_cl_valance: report.lido_cl_balance,
        },
        metadata: ReportMetadataSolidity {
            slot: report.slot,
            epoch: report.epoch,
            lido_withdrawal_credentials: report.lido_withdrawal_credentials.to_fixed_bytes().into(),
            beacon_block_hash: beacon_block_hash.to_fixed_bytes().into(),
            state_for_previous_report: LidoValidatorStateSolidity {
                slot: old_state.slot,
                merkle_root: old_state.tree_hash_root().to_fixed_bytes().into(),
            },
            new_state: LidoValidatorStateSolidity {
                slot: new_state.slot,
                merkle_root: new_state.tree_hash_root().to_fixed_bytes().into(),
            },
        },
    }
}
