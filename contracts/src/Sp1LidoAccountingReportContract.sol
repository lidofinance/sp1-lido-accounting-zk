// SPDX-FileCopyrightText: 2025 Lido <info@lido.fi>
// SPDX-License-Identifier: GPL-3.0
pragma solidity 0.8.27;

import {ISP1Verifier} from "@sp1-contracts/ISP1Verifier.sol";
import {AccessControl} from "@openzeppelin/contracts/access/AccessControl.sol";

import {SecondOpinionOracle} from "./ISecondOpinionOracle.sol";
import {PausableUntil} from "./PausableUntil.sol";

contract Sp1LidoAccountingReportContract is SecondOpinionOracle, AccessControl, PausableUntil {
    /// @notice The address of the beacon roots precompile.
    /// @dev https://eips.ethereum.org/EIPS/eip-4788
    address public constant BEACON_ROOTS = 0x000F3df6D732807Ef1319fB7B8bB8522d0Beac02;

    /// @notice role that allows to pause the contract
    bytes32 public constant PAUSE_ROLE = keccak256("Sp1LidoAccountingReportContract.PauseRole");
    /// @notice role that allows to resume the contract
    bytes32 public constant RESUME_ROLE = keccak256("Sp1LidoAccountingReportContract.ResumeRole");

    /// @notice The length of the beacon roots ring buffer.
    uint256 internal constant BEACON_ROOTS_HISTORY_BUFFER_LENGTH = 8191;

    address public immutable VERIFIER;
    /// @notice The verification key for the SP1 program.
    /// See https://docs.succinct.xyz/onchain-verification/solidity-sdk.html
    /// Essentially, vkey pins the code of ZK program to a particular state
    /// and changes with any code modification
    bytes32 public immutable VKEY;
    bytes32 public immutable WITHDRAWAL_CREDENTIALS;
    address public immutable WITHDRAWAL_VAULT_ADDRESS;

    /// @notice Seconds per slot
    uint256 public immutable SECONDS_PER_SLOT = 12;

    /// @notice The genesis block timestamp.
    uint256 public immutable GENESIS_BLOCK_TIMESTAMP;

    mapping(uint256 refSlot => Report) private _reports;
    mapping(uint256 refSlot => bytes32 state) private _states;
    uint256 private _latestValidatorStateSlot;

    struct Report {
        uint256 reference_slot;
        uint256 deposited_lido_validators;
        uint256 exited_lido_validators;
        uint256 lido_cl_balance;
        uint256 lido_withdrawal_vault_balance;
    }

    struct WithdrawalVaultData {
        uint256 balance;
        address vault_address;
    }

    struct ReportMetadata {
        uint256 bc_slot;
        uint256 epoch;
        bytes32 lido_withdrawal_credentials;
        bytes32 beacon_block_hash;
        LidoValidatorState old_state;
        LidoValidatorState new_state;
        WithdrawalVaultData withdrawal_vault_data;
    }

    struct LidoValidatorState {
        uint256 slot;
        bytes32 merkle_root;
    }

    struct PublicValues {
        Report report;
        ReportMetadata metadata;
    }

    event ReportAccepted(Report report);
    event LidoValidatorStateHashRecorded(uint256 indexed slot, bytes32 merkle_root);
    event PauserSet(address pauserAddr);

    /// @dev Timestamp out of range for the the beacon roots precompile.
    error TimestampOutOfRange(uint256 target_slot, uint256 target_timestamp, uint256 earliest_available_timestamp);
    /// @dev No block root is found using the beacon roots precompile.
    error NoBlockRootFound(uint256 target_slot);

    /// @dev Verification failed
    error VerificationError(string error_message);

    /// @dev Caller have to be authorized to call pause
    error UnauthorizedPauseAccount();

    error IllegalReferenceSlotError(
        uint256 bc_slot,
        uint256 bc_slot_timestamp,
        uint256 reference_slot,
        uint256 reference_slot_timestamp,
        string error_message
    );

    constructor(
        address _verifier,
        bytes32 _vkey,
        bytes32 _lido_withdrawal_credentials,
        address _withdrawal_vault_address,
        uint256 _genesis_timestamp,
        LidoValidatorState memory _initial_state,
        address _owner
    ) {
        VERIFIER = _verifier;
        VKEY = _vkey;
        WITHDRAWAL_CREDENTIALS = _lido_withdrawal_credentials;
        WITHDRAWAL_VAULT_ADDRESS = _withdrawal_vault_address;
        GENESIS_BLOCK_TIMESTAMP = _genesis_timestamp;
        _recordLidoValidatorStateHash(_initial_state.slot, _initial_state.merkle_root);
        _grantRole(DEFAULT_ADMIN_ROLE, _owner);
    }

    function getReport(uint256 refSlot)
        external
        view
        override
        returns (
            bool success,
            uint256 clBalanceGwei,
            uint256 withdrawalVaultBalanceWei,
            uint256 totalDepositedValidators,
            uint256 totalExitedValidators
        )
    {
        if (isPaused()) {
            return (false, 0, 0, 0, 0);
        }
        Report storage report = _reports[refSlot];
        // This check handles two conditions:
        // 1. Report is not found for a given slot - report.slot will be 0
        // 2. Something messed up with the reporting storare, and report for a different
        //    slot is stored there. Technically this is not necessary since it is ensured by
        //    the write-side invariants (in _verify),
        //    but this adds read-side check at no additional cost, so why not.
        success = report.reference_slot == refSlot;

        clBalanceGwei = report.lido_cl_balance;
        withdrawalVaultBalanceWei = report.lido_withdrawal_vault_balance;
        totalDepositedValidators = report.deposited_lido_validators;
        totalExitedValidators = report.exited_lido_validators;
    }

    function getLatestLidoValidatorStateSlot() public view returns (uint256) {
        return (_latestValidatorStateSlot);
    }

    function getLidoValidatorStateHash(uint256 slot) public view returns (bytes32 result) {
        return (_states[slot]);
    }

    function getBeaconBlockHash(uint256 slot) public view returns (bytes32) {
        (bool _success, bytes32 result) = _getBeaconBlockHashForTimestamp(_slotToTimestamp(slot));
        return (result);
    }

    /// @notice Main entrypoint for the contract - accepts proof and public values, verifies them,
    ///         and stores the report if verification passes.
    /// @param proof proof from succinct, in binary format
    /// @param publicValues public values from prover, in binary format
    /// @dev `publicValues` is passed as bytes and deserialized - if using fuzzing/property-based testing,
    ///         directly using bytes generator will produce enormous amount of trivial rejections. Recommend
    ///         generating `PublicValues` struct and abi.encoding it.
    ///         This function is INTENTIONALLY public and have no access modifiers - ANYONE
    ///         should be allowed to call it, and bring the report+proof to the contract - it is the responsibility
    ///         of this contract and SP1 verifier to reject invalid reports.
    function submitReportData(bytes calldata proof, bytes calldata publicValues) public whenResumed {
        PublicValues memory public_values = abi.decode(publicValues, (PublicValues));
        Report memory report = public_values.report;
        ReportMetadata memory metadata = public_values.metadata;
        _verify_reference_and_bc_slot(report.reference_slot, metadata.bc_slot);

        require(
            report.lido_withdrawal_vault_balance == metadata.withdrawal_vault_data.balance,
            VerificationError("Withdrawal vault balance mismatch between report and metadata")
        );

        // Check that public values from ZK program match expected blockchain state
        _verify_public_values(public_values);

        // Verify ZK-program and public values
        ISP1Verifier(VERIFIER).verifyProof(VKEY, publicValues, proof);

        // If all checks pass - record report and state
        _recordReport(report);
        _recordLidoValidatorStateHash(metadata.new_state.slot, metadata.new_state.merkle_root);
    }

    /// @notice Pause submit report data
    /// @param _duration pause duration in seconds (use `PAUSE_INFINITELY` for unlimited)
    /// @dev Reverts if contract is already paused
    /// @dev Reverts reason if sender is not the owner
    /// @dev Reverts if zero duration is passed
    function pauseFor(uint256 _duration) external onlyRole(PAUSE_ROLE) {
        _pauseFor(_duration);
    }

    /// @notice Pause submit report data
    /// @param _pauseUntilInclusive the last second to pause until inclusive
    /// @dev Reverts if the timestamp is in the past
    /// @dev Reverts if sender is not the owner
    /// @dev Reverts if contract is already paused
    function pauseUntil(uint256 _pauseUntilInclusive) external onlyRole(PAUSE_ROLE) {
        _pauseUntil(_pauseUntilInclusive);
    }

    /// @notice Resume submit report data
    /// @dev Reverts if sender is not the owner
    /// @dev Reverts if contract is not paused
    function resume() external onlyRole(RESUME_ROLE) {
        _resume();
    }

    /// @notice Verifies that reference slot and beacon state slot are correct:
    /// * If reference slot had a block, beacon state slot must be equal to reference slot
    /// * If reference slot did not have a block, beacon state slot must be the first preceding slot that had a block
    function _verify_reference_and_bc_slot(uint256 reference_slot, uint256 bc_slot) internal view {
        _require_for_refslot(_blockExists(bc_slot), bc_slot, reference_slot, "Beacon state slot is empty");

        // If beacon state slot has block and ref_slot == beacon state slot - no need to check further
        if (reference_slot == bc_slot) {
            return;
        }

        _require_for_refslot(
            bc_slot < reference_slot, bc_slot, reference_slot, "Reference slot must be after beacon state slot"
        );

        _require_for_refslot(
            _slotToTimestamp(reference_slot) <= block.timestamp,
            bc_slot,
            reference_slot,
            "Reference slot must not be in the future"
        );

        _require_for_refslot(
            !_blockExists(reference_slot),
            bc_slot,
            reference_slot,
            "Reference slot has a block, but beacon state slot != reference slot"
        );

        for (uint256 slot_to_check = reference_slot - 1; slot_to_check > bc_slot; slot_to_check--) {
            _require_for_refslot(
                !_blockExists(slot_to_check),
                bc_slot,
                reference_slot,
                "Beacon state slot should be the first preceding non-empty slot before reference"
            );
        }
    }

    function _verify_public_values(PublicValues memory publicValues) internal view {
        ReportMetadata memory metadata = publicValues.metadata;
        // Check that passed beacon_block_hash matches the one observed on the blockchain for
        // the target slot
        bytes32 expected_block_hash = _findBeaconBlockHash(metadata.bc_slot);
        require(metadata.beacon_block_hash == expected_block_hash, VerificationError("BeaconBlockHash mismatch"));

        // Check that correct withdrawal credentials were used
        require(
            metadata.lido_withdrawal_credentials == _getExpectedWithdrawalCredentials(),
            VerificationError("Withdrawal credentials mismatch")
        );

        // Check that the old report hash matches the one recorded in contract
        bytes32 old_state_hash = getLidoValidatorStateHash(metadata.old_state.slot);
        require(old_state_hash != 0, VerificationError("Old state merkle_root not found"));
        require(metadata.old_state.merkle_root == old_state_hash, VerificationError("Old state merkle_root mismatch"));

        require(
            metadata.bc_slot == metadata.new_state.slot,
            VerificationError("New state slot must match beacon state slot")
        );

        require(
            metadata.withdrawal_vault_data.vault_address == WITHDRAWAL_VAULT_ADDRESS,
            VerificationError("Withdrawal vault address mismatch")
        );
    }

    function _getExpectedWithdrawalCredentials() internal view virtual returns (bytes32) {
        return (WITHDRAWAL_CREDENTIALS);
    }

    /// @notice Attempts to find the block root for the given slot.
    /// @param slot The slot to get the block root for.
    /// @return blockRoot The beacon block root of the given slot.
    /// @dev BEACON_ROOTS returns a ParentRoot field for the block at the specified slot's timestamp.
    ///      To get the block root for slot N, pass to the BEACON_ROOTS the timestamp of a
    ///      first non-empty slot after N (i.e. N+1 if it has a block, N+2 if N+1 is empty, ...).
    function _findBeaconBlockHash(uint256 slot) internal view virtual returns (bytes32) {
        // See comment above re: why adding 1
        uint256 targetBlockTimestamp = _slotToTimestamp(slot + 1);

        uint256 earliestBlockTimestamp = block.timestamp - (BEACON_ROOTS_HISTORY_BUFFER_LENGTH * SECONDS_PER_SLOT);
        if (targetBlockTimestamp <= earliestBlockTimestamp) {
            revert TimestampOutOfRange(slot, earliestBlockTimestamp, targetBlockTimestamp);
        }

        uint256 timestampToCheck = targetBlockTimestamp;
        // This loop does the following:
        // * Tries getting a ParentRoot field for a given timestamp
        // * If not empty - returns
        // * If unsuccessful - slot at `timestampToCheck` was empty, so it moves to the next slot timestamp
        // * Stops if we reached current block timestamp - no further blocks are available
        while (timestampToCheck <= block.timestamp) {
            (bool success, bytes32 result) = _getBeaconBlockHashForTimestamp(timestampToCheck);

            if (success) {
                return result;
            }

            unchecked {
                timestampToCheck += SECONDS_PER_SLOT;
            }
        }
        revert NoBlockRootFound(slot);
    }

    function _blockExists(uint256 slot) internal view returns (bool) {
        uint256 slot_timestamp = _slotToTimestamp(slot);
        (bool read_success, bytes32 slot_hash) = _getBeaconBlockHashForTimestamp(slot_timestamp);
        return read_success && slot_hash != 0;
    }

    function _getBeaconBlockHashForTimestamp(uint256 timestamp)
        internal
        view
        virtual
        returns (bool success, bytes32 result)
    {
        (bool read_success, bytes memory raw_result) = BEACON_ROOTS.staticcall(abi.encode(timestamp));
        success = read_success;
        if (success && raw_result.length > 0) {
            result = abi.decode(raw_result, (bytes32));
        } else {
            result = 0;
        }
    }

    function _require_for_refslot(bool condition, uint256 bc_slot, uint256 refslot, string memory error_message)
        private
        view
    {
        if (condition) {
            return;
        }
        uint256 bc_slot_timestamp = _slotToTimestamp(bc_slot);
        uint256 refslot_timestamp = _slotToTimestamp(refslot);
        revert IllegalReferenceSlotError(bc_slot, bc_slot_timestamp, refslot, refslot_timestamp, error_message);
    }

    function _slotToTimestamp(uint256 slot) internal view returns (uint256) {
        return GENESIS_BLOCK_TIMESTAMP + slot * SECONDS_PER_SLOT;
    }

    function _recordReport(Report memory report) internal {
        _reports[report.reference_slot] = report;
        emit ReportAccepted(report);
    }

    function _recordLidoValidatorStateHash(uint256 slot, bytes32 state_merkle_root) internal {
        _states[slot] = state_merkle_root;
        if (slot > _latestValidatorStateSlot) {
            _latestValidatorStateSlot = slot;
        }
        emit LidoValidatorStateHashRecorded(slot, state_merkle_root);
    }
}
