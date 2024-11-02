use crate::{circuit_logic::report::ReportData, eth_consensus_layer::Hash256};

use crate::io::eth_io::{
    conversions, LidoValidatorStateSolidity, PublicValuesSolidity, ReportMetadataSolidity, ReportSolidity,
};

pub fn create_public_values(
    report: &ReportData,
    beacon_block_hash: &Hash256,
    old_state_slot: u64,
    old_state_hash: &Hash256,
    new_state_slot: u64,
    new_state_hash: &Hash256,
) -> PublicValuesSolidity {
    PublicValuesSolidity {
        report: ReportSolidity {
            slot: conversions::u64_to_uint256(report.slot),
            deposited_lido_validators: conversions::u64_to_uint256(report.deposited_lido_validators),
            exited_lido_validators: conversions::u64_to_uint256(report.exited_lido_validators),
            lido_cl_valance: conversions::u64_to_uint256(report.lido_cl_balance),
        },
        metadata: ReportMetadataSolidity {
            slot: conversions::u64_to_uint256(report.slot),
            epoch: conversions::u64_to_uint256(report.epoch),
            lido_withdrawal_credentials: report.lido_withdrawal_credentials.to_fixed_bytes().into(),
            beacon_block_hash: beacon_block_hash.to_fixed_bytes().into(),
            state_for_previous_report: LidoValidatorStateSolidity {
                slot: conversions::u64_to_uint256(old_state_slot),
                merkle_root: old_state_hash.to_fixed_bytes().into(),
            },
            new_state: LidoValidatorStateSolidity {
                slot: conversions::u64_to_uint256(new_state_slot),
                merkle_root: new_state_hash.to_fixed_bytes().into(),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        circuit_logic::report::ReportData,
        io::eth_io::{LidoValidatorStateRust, PublicValuesRust, ReportMetadataRust, ReportRust},
    };
    use hex_literal::hex;

    #[test]
    pub fn round_trip() {
        let (slot, epoch) = (123, 10);
        let credentials = hex!("010000000000000000000000b9d7934878b5fb9610b3fe8a5e441e8fad7e293f");
        let report_data = ReportData {
            slot,
            epoch,
            deposited_lido_validators: 1,
            exited_lido_validators: 2,
            lido_cl_balance: 3,
            lido_withdrawal_credentials: credentials.into(),
        };
        let (bh_hash, old_state_hash, new_state_hash) = ([1u8; 32], [2u8; 32], [3u8; 32]);

        let expected_public_values = PublicValuesRust {
            report: ReportRust {
                slot: report_data.slot,
                deposited_lido_validators: report_data.deposited_lido_validators,
                exited_lido_validators: report_data.exited_lido_validators,
                lido_cl_balance: report_data.lido_cl_balance,
            },
            metadata: ReportMetadataRust {
                slot,
                epoch,
                lido_withdrawal_credentials: credentials,
                beacon_block_hash: bh_hash,
                state_for_previous_report: LidoValidatorStateRust {
                    slot: slot - 10,
                    merkle_root: old_state_hash,
                },
                new_state: LidoValidatorStateRust {
                    slot,
                    merkle_root: new_state_hash,
                },
            },
        };

        let public_values_solidity = super::create_public_values(
            &report_data,
            &bh_hash.into(),
            slot - 10,
            &old_state_hash.into(),
            slot,
            &new_state_hash.into(),
        );
        let public_values_rust: PublicValuesRust = public_values_solidity.into();

        assert_eq!(public_values_rust, expected_public_values)
    }
}
