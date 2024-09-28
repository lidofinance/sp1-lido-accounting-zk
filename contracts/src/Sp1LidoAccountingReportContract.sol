// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {SecondOpinionOracle} from "./ISecondOpinionOracle.sol";
import {ISP1Verifier} from "@sp1-contracts/ISP1Verifier.sol";

struct Report {
    uint256 slot;
    uint256 deposited_lido_validators;
    uint256 exited_lido_validators;
    uint256 lido_cl_balance;
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

contract Sp1LidoAccountingReportContract is SecondOpinionOracle {
    event ReportAccepted(Report report);
    // This should later become an error
    event ReportRejected(string reason);
    event LidoValidatorStateHashRecorded(uint256 slot, bytes32 merkle_root);

    mapping(uint256 => Report) private _reports;
    mapping(uint256 => bytes32) private _states;
    address public immutable VERIFIER;
    bytes32 public immutable VKEY;

    bytes32 public immutable WITHDRAWAL_CREDENTIALS;

    uint256 _latestValidatorStateSlot;

    /// @notice Seconds per slot
    uint256 public immutable SECONDS_PER_SLOT = 12;

    /// @notice The genesis block timestamp.
    uint256 public immutable GENESIS_BLOCK_TIMESTAMP;

    /// @notice The address of the beacon roots precompile.
    /// @dev https://eips.ethereum.org/EIPS/eip-4788
    address internal constant BEACON_ROOTS =
        0x000F3df6D732807Ef1319fB7B8bB8522d0Beac02;

    /// @notice The length of the beacon roots ring buffer.
    uint256 internal constant BEACON_ROOTS_HISTORY_BUFFER_LENGTH = 8191;

    /// @dev Timestamp out of range for the the beacon roots precompile.
    error TimestampOutOfRange(TimestampOutOfRangeData);
    struct TimestampOutOfRangeData {
        uint256 target_slot;
        uint256 target_timestamp;
        uint256 earliest_available_timestamp;
    }

    /// @dev No block root is found using the beacon roots precompile.
    error NoBlockRootFound(NoBlockRootFoundData);
    struct NoBlockRootFoundData {
        uint256 target_slot;
    }


    constructor(
        address _verifier,
        bytes32 _vkey,
        bytes32 _lido_withdrawal_credentials,
        uint256 _genesis_timestamp,
        LidoValidatorState memory _initial_state
    ) {
        VERIFIER = _verifier;
        VKEY = _vkey;
        WITHDRAWAL_CREDENTIALS = _lido_withdrawal_credentials;
        GENESIS_BLOCK_TIMESTAMP = _genesis_timestamp;
        _recordLidoValidatorStateHash(
            _initial_state.slot,
            _initial_state.merkle_root
        );
        
    }

    /// @notice Main entrypoint for the contract - accepts proof and public values, verifies them,
    ///         and stores the report if verification passes
    /// @param slot slot for report
    /// @param report Report struct
    /// @param metadata Metadata struct
    /// @param proof proof from succinct, in binary format
    /// @param publicValues public values from prover, in binary format
    function submitReportData(
        uint256 slot,
        Report calldata report,
        ReportMetadata calldata metadata,
        bytes calldata proof,
        bytes calldata publicValues
    ) public {
        _verify(slot, report, metadata, proof, publicValues);

        // If all checks pass - record report and state
        _recordReport(slot, report);
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
    ) internal view {
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
        ISP1Verifier(VERIFIER).verifyProof(VKEY, publicValues, proof);
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
            report.lido_cl_balance == publicValues.report.lido_cl_balance,
            "Report.lido_cl_balance doesn't match public values"
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

    function getReport(uint256 refSlot) external view returns ( 
		bool success, 
		uint256 clBalanceGwei, 
		uint256 withdrawalVaultBalanceWei, 
		uint256 totalDepositedValidators, 
		uint256 totalExitedValidators 
	) {
        Report storage report = _reports[refSlot];
        // This check handles two conditions:
        // 1. Report is not found for a given slot - report.slot will be 0
        // 2. Something messed up with the reporting storare, and report for a different
        //    slot is stored there. Technically this is not necessary since it is ensured by
        //    the write-side invariants (in _verify), 
        //    but this adds read-side check at no additional cost, so why not.
        success = report.slot == refSlot;

        clBalanceGwei = report.lido_cl_balance;
        withdrawalVaultBalanceWei = 0; // withdrawal vault is not reported yet
        totalDepositedValidators = report.deposited_lido_validators;
        totalExitedValidators = report.exited_lido_validators;
    }

    function _recordReport(uint256 slot, Report memory report) internal {
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

    function _getExpectedWithdrawalCredentials() internal view virtual returns (bytes32)
    {
        return (WITHDRAWAL_CREDENTIALS);
    }

    /// @notice Attempts to find the block root for the given slot.
    /// @param slot The slot to get the block root for.
    /// @return blockRoot The beacon block root of the given slot.
    /// @dev BEACON_ROOTS returns a block root for a given parent block's timestamp. To get the block root for slot
    ///      N, you use the timestamp of slot N+1. If N+1 is not avaliable, you use the timestamp of slot N+2, and
    //       so on.
    function _getBeaconBlockHash(
        uint256 slot
    ) internal view virtual returns (bytes32) {
        uint256 currBlockTimestamp = GENESIS_BLOCK_TIMESTAMP +
            ((slot + 1) * SECONDS_PER_SLOT);

        uint256 earliestBlockTimestamp = block.timestamp -
            (BEACON_ROOTS_HISTORY_BUFFER_LENGTH * SECONDS_PER_SLOT);
        if (currBlockTimestamp <= earliestBlockTimestamp) {
            revert TimestampOutOfRange(TimestampOutOfRangeData({
                target_slot: slot,
                earliest_available_timestamp: earliestBlockTimestamp, 
                target_timestamp: currBlockTimestamp
            }));
        }

        while (currBlockTimestamp <= block.timestamp) {
            (bool success, bytes memory result) = BEACON_ROOTS.staticcall(abi.encode(currBlockTimestamp));
            
            if (success && result.length > 0) {
                return abi.decode(result, (bytes32));
            }

            unchecked {
                currBlockTimestamp += 12;
            }
        }
        revert NoBlockRootFound(NoBlockRootFoundData({target_slot: slot}));
    }
}
