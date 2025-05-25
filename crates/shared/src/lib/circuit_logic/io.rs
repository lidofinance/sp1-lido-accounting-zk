use crate::{circuit_logic::report::ReportData, eth_consensus_layer::Hash256};

use crate::io::eth_io::{
    self, conversions, BeaconChainSlot, LidoValidatorStateSolidity, LidoWithdrawalVaultDataRust, PublicValuesSolidity,
    ReportMetadataSolidity, ReportSolidity,
};

pub fn create_public_values(
    report: &ReportData,
    bc_slot: BeaconChainSlot,
    beacon_block_hash: &Hash256,
    lido_withdrawal_vault_data: LidoWithdrawalVaultDataRust,
    old_state_slot: BeaconChainSlot,
    old_state_hash: &Hash256,
    new_state_slot: BeaconChainSlot,
    new_state_hash: &Hash256,
) -> Result<PublicValuesSolidity, eth_io::Error> {
    let result = PublicValuesSolidity {
        report: ReportSolidity {
            reference_slot: report.slot.try_into()?,
            deposited_lido_validators: conversions::u64_to_uint256(report.deposited_lido_validators)?,
            exited_lido_validators: conversions::u64_to_uint256(report.exited_lido_validators)?,
            lido_cl_valance: conversions::u64_to_uint256(report.lido_cl_balance)?,
            lido_withdrawal_vault_balance: lido_withdrawal_vault_data.balance,
        },
        metadata: ReportMetadataSolidity {
            bc_slot: bc_slot.try_into()?,
            epoch: conversions::u64_to_uint256(report.epoch)?,
            lido_withdrawal_credentials: report.lido_withdrawal_credentials,
            beacon_block_hash: *beacon_block_hash,
            state_for_previous_report: LidoValidatorStateSolidity {
                slot: old_state_slot.try_into()?,
                merkle_root: *old_state_hash,
            },
            new_state: LidoValidatorStateSolidity {
                slot: new_state_slot.try_into()?,
                merkle_root: *new_state_hash,
            },
            withdrawal_vault_data: lido_withdrawal_vault_data.into(),
        },
    };
    Ok(result)
}

#[cfg(test)]
mod tests {
    use crate::{
        circuit_logic::report::ReportData,
        io::eth_io::{
            BeaconChainSlot, HaveEpoch, LidoValidatorStateRust, LidoWithdrawalVaultDataRust, PublicValuesRust,
            PublicValuesSolidity, ReferenceSlot, ReportMetadataRust, ReportRust,
        },
    };
    use alloy_sol_types::SolType;
    use hex_literal::hex;

    // Helper function to reduce the number of search hits for `assert` in the production files
    fn check_eq<T: PartialEq + std::fmt::Debug>(left: T, right: T) {
        assert_eq!(left, right);
    }

    fn get_data() -> (ReportData, PublicValuesRust) {
        let (ref_slot, bc_slot) = (ReferenceSlot(125), BeaconChainSlot(123));
        let credentials = hex!("010000000000000000000000b9d7934878b5fb9610b3fe8a5e441e8fad7e293f");
        let address: [u8; 20] = hex!("1111222233334444555566667777888899990000");
        let withdrawal_vault_balance = alloy_primitives::U256::from(1234567890);
        let report_data = ReportData {
            slot: ref_slot,
            epoch: ref_slot.epoch(),
            deposited_lido_validators: 1,
            exited_lido_validators: 2,
            lido_cl_balance: 3,
            lido_withdrawal_credentials: credentials.into(),
        };
        let (bh_hash, old_state_hash, new_state_hash) = ([1u8; 32], [2u8; 32], [3u8; 32]);

        let public_values_rust = PublicValuesRust {
            report: ReportRust {
                reference_slot: report_data.slot,
                deposited_lido_validators: report_data.deposited_lido_validators,
                exited_lido_validators: report_data.exited_lido_validators,
                lido_cl_balance: report_data.lido_cl_balance,
                lido_withdrawal_vault_balance: withdrawal_vault_balance,
            },
            metadata: ReportMetadataRust {
                bc_slot,
                epoch: bc_slot.epoch(),
                lido_withdrawal_credentials: credentials,
                beacon_block_hash: bh_hash,
                state_for_previous_report: LidoValidatorStateRust {
                    slot: bc_slot - 10,
                    merkle_root: old_state_hash,
                },
                new_state: LidoValidatorStateRust {
                    slot: bc_slot,
                    merkle_root: new_state_hash,
                },
                withdrawal_vault_data: LidoWithdrawalVaultDataRust {
                    vault_address: address.into(),
                    balance: withdrawal_vault_balance,
                },
            },
        };

        (report_data, public_values_rust)
    }

    #[test]
    pub fn round_trip() {
        let (report_data, public_values) = get_data();
        let withdrawal_vault_data = public_values.metadata.withdrawal_vault_data.clone();
        let public_values_solidity = super::create_public_values(
            &report_data,
            public_values.metadata.bc_slot,
            &public_values.metadata.beacon_block_hash.into(),
            withdrawal_vault_data,
            public_values.metadata.state_for_previous_report.slot,
            &public_values.metadata.state_for_previous_report.merkle_root.into(),
            public_values.metadata.new_state.slot,
            &public_values.metadata.new_state.merkle_root.into(),
        )
        .expect("Test: Failed to create public values");

        let public_values_rust: PublicValuesRust = public_values_solidity
            .try_into()
            .expect("Test: Failed to convert PublicValuesSolidity to PublicValuesRust");

        check_eq(public_values, public_values_rust)
    }

    #[test]
    pub fn round_trip_abi_encode() {
        let (report_data, public_values) = get_data();
        let withdrawal_vault_data = public_values.metadata.withdrawal_vault_data.clone();
        let public_values_solidity = super::create_public_values(
            &report_data,
            public_values.metadata.bc_slot,
            &public_values.metadata.beacon_block_hash.into(),
            withdrawal_vault_data,
            public_values.metadata.state_for_previous_report.slot,
            &public_values.metadata.state_for_previous_report.merkle_root.into(),
            public_values.metadata.new_state.slot,
            &public_values.metadata.new_state.merkle_root.into(),
        )
        .expect("Test: Failed to create public values");

        let abi_encoded = PublicValuesSolidity::abi_encode(&public_values_solidity);
        let decoded =
            PublicValuesSolidity::abi_decode(&abi_encoded, true).expect("Failed to decode PublicValuesSolidity");
        let public_values_rust: PublicValuesRust = decoded
            .try_into()
            .expect("Test: Failed to convert PublicValuesSolidity to PublicValuesRust");

        check_eq(public_values, public_values_rust)
    }
}
