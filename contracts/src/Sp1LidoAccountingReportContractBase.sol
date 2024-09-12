// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ISP1Verifier} from "@sp1-contracts/ISP1Verifier.sol";

struct Report {
    uint256 slot;
    uint256 deposited_lido_validators;
    uint256 exited_lido_validators;
    uint256 lido_cl_valance;
}
struct ReportMetadata {
    uint256 slot;
    uint256 epoch;
    bytes32 lido_withdrawal_credentials;
    bytes32 beacon_block_hash;
    LidoValidatorState old_state;
    LidoValidatorState new_state;
}

struct LidoValidatorState {
    uint256 slot;
    bytes32 merkle_root;
}

struct PublicValues {
    Report report;
    ReportMetadata metadata;
}

abstract contract Sp1LidoAccountingReportContractBase {
    event ReportAccepted(Report report);
    // This should later become an error
    event ReportRejected(string reason);
    event LidoValidatorStateHashRecorded(uint256 slot, bytes32 merkle_root);

    mapping(uint256 => Report) private _reports;
    mapping(uint256 => bytes32) private _states;
    address public immutable verifier;
    bytes32 public immutable vkey;

    bytes32 public immutable widthrawal_credentials;

    uint256 _latestValidatorStateSlot;

    constructor(
        address _verifier,
        bytes32 _vkey,
        bytes32 _widthrawal_credentials,
        LidoValidatorState memory _initial_state
    ) {
        verifier = _verifier;
        vkey = _vkey;
        widthrawal_credentials = _widthrawal_credentials;
        _recordLidoValidatorStateHash(
            _initial_state.slot,
            _initial_state.merkle_root
        );
    }

    function submitReportData(
        uint256 slot,
        Report calldata report,
        ReportMetadata calldata metadata,
        bytes calldata proof,
        bytes calldata publicValues
    ) public {
        _verify(slot, report, metadata, proof, publicValues);

        // If all checks pass - update report and record the state
        _updateReport(slot, report);
        _recordLidoValidatorStateHash(
            metadata.new_state.slot,
            metadata.new_state.merkle_root
        );
    }

    function _verify(
        uint256 slot,
        Report calldata report,
        ReportMetadata calldata metadata,
        bytes calldata proof,
        bytes calldata publicValues
    ) public view {
        // Check the report was not previously set
        Report storage report_at_slot = _reports[slot];
        _require(
            report_at_slot.slot == 0,
            "Report was already accepted for a given slot"
        );

        // Check the report is for the target slot
        _require(report.slot == slot, "Slot mismatch: report");
        _require(metadata.new_state.slot == slot, "Slot mismatch: new state");

        // Check that passed beacon_block_hash matches the one observed on the blockchain for
        // the target slot
        _require(
            metadata.beacon_block_hash == _getBeaconBlockHash(slot),
            "BeaconBlockHash mismatch"
        );

        // Check that correct withdrawal credentials were used
        _require(
            metadata.lido_withdrawal_credentials ==
                _getExpectedWithdrawalCredentials(),
            "Withdrawal credentials mismatch"
        );

        // Check that the old report hash matches the one recorded in contract
        bytes32 old_state_hash = getLidoValidatorStateHash(
            metadata.old_state.slot
        );
        _require(old_state_hash != 0, "Old state merkle_root not found");
        _require(
            metadata.old_state.merkle_root == old_state_hash,
            "Old state merkle_root mismatch"
        );

        // Check that report and metadata match public values committed in the ZK-program
        PublicValues memory public_values = abi.decode(
            publicValues,
            (PublicValues)
        );
        _verify_public_values(report, metadata, public_values);

        // Verify ZK-program and public values
        ISP1Verifier(verifier).verifyProof(vkey, publicValues, proof);
    }

    function _verify_public_values(
        Report memory report,
        ReportMetadata memory metadata,
        PublicValues memory publicValues
    ) internal pure {
        _require(
            report.slot == publicValues.report.slot,
            "Report.slot doesn't match public values"
        );
        _require(
            report.deposited_lido_validators ==
                publicValues.report.deposited_lido_validators,
            "Report.deposited_lido_validators doesn't match public values"
        );
        _require(
            report.exited_lido_validators ==
                publicValues.report.exited_lido_validators,
            "Report.exited_lido_validators doesn't match public values"
        );
        _require(
            report.lido_cl_valance == publicValues.report.lido_cl_valance,
            "Report.lido_cl_valance doesn't match public values"
        );

        _require(
            metadata.slot == publicValues.metadata.slot,
            "Metadata.slot doesn't match public values"
        );
        _require(
            metadata.epoch == publicValues.metadata.epoch,
            "Metadata.epoch doesn't match public values"
        );
        _require(
            metadata.lido_withdrawal_credentials ==
                publicValues.metadata.lido_withdrawal_credentials,
            "Metadata.lido_withdrawal_credentials doesn't match public values"
        );
        _require(
            metadata.beacon_block_hash ==
                publicValues.metadata.beacon_block_hash,
            "Metadata.beacon_block_hash doesn't match public values"
        );

        _require(
            metadata.old_state.slot == publicValues.metadata.old_state.slot,
            "Metadata.old_state.slot doesn't match public values"
        );
        _require(
            metadata.old_state.merkle_root ==
                publicValues.metadata.old_state.merkle_root,
            "Metadata.old_state.merkle_root doesn't match public values"
        );

        _require(
            metadata.new_state.slot == publicValues.metadata.new_state.slot,
            "Metadata.new_state.slot doesn't match public values"
        );
        _require(
            metadata.new_state.merkle_root ==
                publicValues.metadata.new_state.merkle_root,
            "Metadata.new_state.merkle_root doesn't match public values"
        );
    }

    function getReport(
        uint256 slot
    ) public view returns (Report memory result) {
        return (_reports[slot]);
    }

    function _updateReport(uint256 slot, Report memory report) internal {
        _reports[slot] = report;
        emit ReportAccepted(report);
    }

    function getLidoValidatorStateHash(
        uint256 slot
    ) public view returns (bytes32 result) {
        return (_states[slot]);
    }

    function getLatestLidoValidatorStateSlot() public view returns (uint256) {
        return (_latestValidatorStateSlot);
    }

    function _recordLidoValidatorStateHash(
        uint256 slot,
        bytes32 state_merkle_root
    ) internal {
        _states[slot] = state_merkle_root;
        if (slot > _latestValidatorStateSlot) {
            _latestValidatorStateSlot = slot;
        }
        emit LidoValidatorStateHashRecorded(slot, state_merkle_root);
    }

    function _require(bool condition, string memory reason) internal pure {
        if (!condition) {
            revert(reason);
        }
    }

    function _getExpectedWithdrawalCredentials()
        internal
        view
        virtual
        returns (bytes32)
    {
        return (widthrawal_credentials);
    }

    function _getBeaconBlockHash(
        uint256 slot
    ) internal view virtual returns (bytes32);
}
