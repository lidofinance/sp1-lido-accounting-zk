// SPDX-License-Identifier: MIT
pragma solidity 0.8.27;

import {Test, console} from "forge-std/Test.sol";
import {stdJson} from "forge-std/StdJson.sol";
import {Sp1LidoAccountingReportContract} from "../src/Sp1LidoAccountingReportContract.sol";
import {Sp1LidoAccountingReportContractTestWrapper} from "test/Sp1LidoAccountingReportContractTestWrapper.sol";
import {SP1VerifierGateway} from "@sp1-contracts/SP1VerifierGateway.sol";

contract Sp1LidoAccountingReportContractTest is Test {
    using stdJson for string;

    address private verifier;
    Sp1LidoAccountingReportContractTestWrapper private _contract;

    uint256 private immutable GENESIS_BLOCK_TIMESTAMP = 1606824023;
    uint256 private immutable SECONDS_PER_SLOT = 12;

    bytes32 private SLOT_EXISTED_SENTIEL = 0x1111222233334444555566667777888899990000aaaabbbbccccddddeeeeffff;

    struct SP1ProofFixtureJson {
        bytes32 vkey;
        Sp1LidoAccountingReportContract.Report report;
        Sp1LidoAccountingReportContract.ReportMetadata metadata;
        bytes publicValues;
        bytes proof;
    }

    function loadFixture() public view returns (SP1ProofFixtureJson memory) {
        string memory root = vm.projectRoot();
        string memory path = string.concat(root, "/test/fixtures/fixture.json");
        string memory json = vm.readFile(path);
        // This should be
        // return abi.decode(json.parseRaw("."), (SP1ProofFixtureJson));
        // ... but it reverts with no explanation - so just doing it manually
        return (
            SP1ProofFixtureJson(
                json.readBytes32(".vkey"),
                Sp1LidoAccountingReportContract.Report(
                    json.readUint(".report.reference_slot"),
                    json.readUint(".report.deposited_lido_validators"),
                    json.readUint(".report.exited_lido_validators"),
                    json.readUint(".report.lido_cl_balance"),
                    json.readUint(".report.lido_withdrawal_vault_balance")
                ),
                Sp1LidoAccountingReportContract.ReportMetadata(
                    json.readUint(".metadata.bc_slot"),
                    json.readUint(".metadata.epoch"),
                    json.readBytes32(".metadata.lido_withdrawal_credentials"),
                    json.readBytes32(".metadata.beacon_block_hash"),
                    Sp1LidoAccountingReportContract.LidoValidatorState(
                        json.readUint(".metadata.state_for_previous_report.slot"),
                        json.readBytes32(".metadata.state_for_previous_report.merkle_root")
                    ),
                    Sp1LidoAccountingReportContract.LidoValidatorState(
                        json.readUint(".metadata.new_state.slot"), json.readBytes32(".metadata.new_state.merkle_root")
                    ),
                    Sp1LidoAccountingReportContract.WithdrawalVaultData(
                        json.readUint(".metadata.withdrawal_vault_data.balance"),
                        json.readAddress(".metadata.withdrawal_vault_data.vault_address")
                    )
                ),
                json.readBytes(".publicValues"),
                json.readBytes(".proof")
            )
        );
    }

    function setUp() public {
        SP1ProofFixtureJson memory fixture = loadFixture();

        verifier = address(new SP1VerifierGateway(address(1)));
        _contract = new Sp1LidoAccountingReportContractTestWrapper(
            verifier,
            fixture.vkey,
            fixture.metadata.lido_withdrawal_credentials,
            fixture.metadata.withdrawal_vault_data.vault_address,
            GENESIS_BLOCK_TIMESTAMP,
            fixture.metadata.old_state,
            address(12345678901234567890)
        );
    }

    function getSlotTimestamp(uint256 slot) internal view returns (uint256) {
        uint256 timestamp = _contract.GENESIS_BLOCK_TIMESTAMP() + (slot * _contract.SECONDS_PER_SLOT());
        return (timestamp);
    }

    function setBeaconHashSequence(uint256 start_slot, uint256 end_slot, bytes32[] memory hashes) private {
        console.log(start_slot, end_slot, hashes.length);
        assert(start_slot + hashes.length == end_slot + 2);
        for (uint256 idx; idx < hashes.length; idx++) {
            uint256 target_slot = start_slot + idx;
            _setHash(target_slot, hashes[idx]);
        }
        console.log("Warp to slot %d", end_slot + 2);
        vm.warp(getSlotTimestamp(end_slot + 2));
    }

    function _setHash(uint256 slot, bytes32 expected_hash) private {
        uint256 reportBlockTimestamp = getSlotTimestamp(slot);
        if (expected_hash != 0) {
            console.log("Setting block hash for %d at time %d", slot, reportBlockTimestamp);
            console.logBytes32(expected_hash);
            vm.mockCall(_contract.BEACON_ROOTS(), abi.encode(reportBlockTimestamp), abi.encode(expected_hash));
        } else {
            console.log("Setting block hash call to fail for %d at time %d", slot, reportBlockTimestamp);
            vm.mockCallRevert(_contract.BEACON_ROOTS(), abi.encode(reportBlockTimestamp), "No block");
        }
    }

    function setSingleBlockHash(uint256 slot, bytes32 expected_hash) private {
        bytes32[] memory hashes = _createDyn(SLOT_EXISTED_SENTIEL, expected_hash);
        setBeaconHashSequence(slot, slot, hashes);
    }

    function _createDyn(bytes32 val1) private returns (bytes32[] memory result) {
        result = new bytes32[](1);
        result[0] = val1;
    }

    function _createDyn(bytes32 val1, bytes32 val2) private returns (bytes32[] memory result) {
        result = new bytes32[](2);
        result[0] = val1;
        result[1] = val2;
    }

    function _createDyn(bytes32 val1, bytes32 val2, bytes32 val3) private returns (bytes32[] memory result) {
        result = new bytes32[](3);
        result[0] = val1;
        result[1] = val2;
        result[2] = val3;
    }

    function _createDyn(bytes32 val1, bytes32 val2, bytes32 val3, bytes32 val4)
        private
        returns (bytes32[] memory result)
    {
        result = new bytes32[](4);
        result[0] = val1;
        result[1] = val2;
        result[2] = val3;
        result[3] = val4;
    }

    function _createDyn(bytes32 val1, bytes32 val2, bytes32 val3, bytes32 val4, bytes32 val5)
        private
        returns (bytes32[] memory result)
    {
        result = new bytes32[](5);
        result[0] = val1;
        result[1] = val2;
        result[2] = val3;
        result[3] = val4;
        result[4] = val5;
    }

    function illegal_old_state_slot(uint256 bc_slot, uint256 old_state_slot) internal pure returns (bytes memory) {
        return abi.encodeWithSelector(Sp1LidoAccountingReportContract.IllegalOldStateSlotError.selector, bc_slot, old_state_slot);
    }

    function verification_error(string memory message) internal pure returns (bytes memory) {
        return abi.encodeWithSelector(Sp1LidoAccountingReportContract.VerificationError.selector, message);
    }

    function beacon_block_hash_mismatch_error(bytes32 expected_hash, bytes32 actual_hash)
        internal
        view
        returns (bytes memory)
    {
        return abi.encodeWithSelector(
            Sp1LidoAccountingReportContract.BeaconBlockHashMismatch.selector,
            expected_hash,
            actual_hash
        );
    }

    function illegal_ref_slot_error(uint256 bc_slot, uint256 ref_slot, string memory message)
        internal
        view
        returns (bytes memory)
    {
        return abi.encodeWithSelector(
            Sp1LidoAccountingReportContract.IllegalReferenceSlotError.selector,
            bc_slot,
            getSlotTimestamp(bc_slot),
            ref_slot,
            getSlotTimestamp(ref_slot),
            message
        );
    }

    function verifierPasses() internal {
        vm.mockCall(verifier, abi.encodeWithSelector(SP1VerifierGateway.verifyProof.selector), abi.encode());
    }

    function verifierRejects(bytes memory err) internal {
        vm.mockCallRevert(verifier, abi.encodeWithSelector(SP1VerifierGateway.verifyProof.selector), err);
    }

    function assertReportAccepted(uint256 slot, Sp1LidoAccountingReportContract.Report memory expected_report)
        internal
        view
    {
        (
            bool success,
            uint256 clBalanceGwei,
            uint256 withdrawalVaultBalanceWei,
            uint256 totalDepositedValidators,
            uint256 totalExitedValidators
        ) = _contract.getReport(slot);

        assertEq(success, true);
        assertEq(clBalanceGwei, expected_report.lido_cl_balance);
        assertEq(totalDepositedValidators, expected_report.deposited_lido_validators);
        assertEq(totalExitedValidators, expected_report.exited_lido_validators);
        assertEq(withdrawalVaultBalanceWei, expected_report.lido_withdrawal_vault_balance);
    }

    function test_validProof() public {
        SP1ProofFixtureJson memory fixture = loadFixture();

        setSingleBlockHash(fixture.metadata.bc_slot, fixture.metadata.beacon_block_hash);
        verifierPasses();

        Sp1LidoAccountingReportContract.PublicValues memory public_values =
            abi.decode(fixture.publicValues, (Sp1LidoAccountingReportContract.PublicValues));
        Sp1LidoAccountingReportContract.Report memory expected_report = public_values.report;

        _contract.submitReportData(fixture.proof, fixture.publicValues);
        assertReportAccepted(public_values.report.reference_slot, expected_report);
    }

    function test_validProofWrongExpectedBeaconBlockHash_reverts() public {
        SP1ProofFixtureJson memory fixture = loadFixture();
        bytes32 expectedHash = 0x1111111100000000000000000000000000000000000000000000000022222222;

        setSingleBlockHash(fixture.metadata.bc_slot, expectedHash);
        verifierPasses();

        vm.expectRevert(beacon_block_hash_mismatch_error(expectedHash, fixture.metadata.beacon_block_hash));
        _contract.submitReportData(fixture.proof, fixture.publicValues);
    }

    function test_validProofWrongLidoWithdrawalCredentials_reverts() public {
        SP1ProofFixtureJson memory fixture = loadFixture();

        setSingleBlockHash(fixture.metadata.bc_slot, fixture.metadata.beacon_block_hash);
        verifierPasses();
        Sp1LidoAccountingReportContract.PublicValues memory public_values =
            abi.decode(fixture.publicValues, (Sp1LidoAccountingReportContract.PublicValues));
        public_values.metadata.lido_withdrawal_credentials =
            0xABCDEF0000000000000000000000000000000000000000000000000000FEDCBA;
        bytes memory public_values_encoded = abi.encode(public_values);

        vm.expectRevert(verification_error("Withdrawal credentials mismatch"));
        _contract.submitReportData(fixture.proof, public_values_encoded);
    }

    function test_noStateRecordedForOldStateSlot_reverts() public {
        SP1ProofFixtureJson memory fixture = loadFixture();

        setSingleBlockHash(fixture.metadata.bc_slot, fixture.metadata.beacon_block_hash);
        verifierPasses();
        Sp1LidoAccountingReportContract.PublicValues memory public_values =
            abi.decode(fixture.publicValues, (Sp1LidoAccountingReportContract.PublicValues));
        public_values.metadata.old_state.slot = public_values.metadata.old_state.slot - 10;
        bytes memory public_values_encoded = abi.encode(public_values);

        vm.expectRevert(verification_error("Old state merkle_root not found"));
        _contract.submitReportData(fixture.proof, public_values_encoded);
    }

    function test_oldStateSlotAheadSameAsBcSlot_reverts() public {
        SP1ProofFixtureJson memory fixture = loadFixture();

        uint256 modified_old_state_slot = fixture.metadata.bc_slot;

        setSingleBlockHash(fixture.metadata.bc_slot, fixture.metadata.beacon_block_hash);
        verifierPasses();

        Sp1LidoAccountingReportContract.PublicValues memory public_values =
            abi.decode(fixture.publicValues, (Sp1LidoAccountingReportContract.PublicValues));

        public_values.metadata.old_state.slot = modified_old_state_slot;
        bytes memory public_values_encoded = abi.encode(public_values);
        Sp1LidoAccountingReportContract.Report memory expected_report = public_values.report;

        vm.expectRevert(illegal_old_state_slot(fixture.metadata.bc_slot, modified_old_state_slot));
        _contract.submitReportData(fixture.proof, public_values_encoded);
    }

    function test_oldStateSlotAheadOfBcSlot_reverts() public {
        SP1ProofFixtureJson memory fixture = loadFixture();

        uint256 modified_old_state_slot = fixture.metadata.bc_slot + 10;

        setSingleBlockHash(fixture.metadata.bc_slot, fixture.metadata.beacon_block_hash);
        setSingleBlockHash(modified_old_state_slot, fixture.metadata.beacon_block_hash);
        _contract.recordLidoValidatorStateHash(modified_old_state_slot, fixture.metadata.old_state.merkle_root);
        verifierPasses();

        Sp1LidoAccountingReportContract.PublicValues memory public_values =
            abi.decode(fixture.publicValues, (Sp1LidoAccountingReportContract.PublicValues));

        public_values.metadata.old_state.slot = modified_old_state_slot;
        bytes memory public_values_encoded = abi.encode(public_values);
        Sp1LidoAccountingReportContract.Report memory expected_report = public_values.report;

        vm.expectRevert(illegal_old_state_slot(fixture.metadata.bc_slot, modified_old_state_slot));
        _contract.submitReportData(fixture.proof, public_values_encoded);
    }

    function test_oldStateWrongMerkleRoot_reverts() public {
        SP1ProofFixtureJson memory fixture = loadFixture();

        setSingleBlockHash(fixture.metadata.bc_slot, fixture.metadata.beacon_block_hash);
        verifierPasses();
        Sp1LidoAccountingReportContract.PublicValues memory public_values =
            abi.decode(fixture.publicValues, (Sp1LidoAccountingReportContract.PublicValues));
        public_values.metadata.old_state.merkle_root =
            0x0102030405060708090000000000000000000000000000000000000000000000;
        bytes memory public_values_encoded = abi.encode(public_values);

        vm.expectRevert(verification_error("Old state merkle_root mismatch"));
        _contract.submitReportData(fixture.proof, public_values_encoded);
    }

    function test_newStateSlot_mismatchBeaconStateSlot_reverts() public {
        SP1ProofFixtureJson memory fixture = loadFixture();

        setSingleBlockHash(fixture.metadata.bc_slot, fixture.metadata.beacon_block_hash);
        verifierPasses();
        Sp1LidoAccountingReportContract.PublicValues memory public_values =
            abi.decode(fixture.publicValues, (Sp1LidoAccountingReportContract.PublicValues));
        public_values.metadata.new_state.slot = public_values.metadata.bc_slot + 1;
        bytes memory public_values_encoded = abi.encode(public_values);

        vm.expectRevert(verification_error("New state slot must match beacon state slot"));
        _contract.submitReportData(fixture.proof, public_values_encoded);
    }

    function test_withdrawalVault_ReportAndMetadataBalanceMismatch_reverts() public {
        SP1ProofFixtureJson memory fixture = loadFixture();

        setSingleBlockHash(fixture.metadata.bc_slot, fixture.metadata.beacon_block_hash);
        verifierPasses();
        Sp1LidoAccountingReportContract.PublicValues memory public_values =
            abi.decode(fixture.publicValues, (Sp1LidoAccountingReportContract.PublicValues));
        public_values.metadata.withdrawal_vault_data.balance += 10;
        bytes memory public_values_encoded = abi.encode(public_values);

        vm.expectRevert(verification_error("Withdrawal vault balance mismatch between report and metadata"));
        _contract.submitReportData(fixture.proof, public_values_encoded);
    }

    function test_withdrawalVault_WrongWithdrawalVaultAddress_reverts() public {
        SP1ProofFixtureJson memory fixture = loadFixture();

        setSingleBlockHash(fixture.metadata.bc_slot, fixture.metadata.beacon_block_hash);
        verifierPasses();
        Sp1LidoAccountingReportContract.PublicValues memory public_values =
            abi.decode(fixture.publicValues, (Sp1LidoAccountingReportContract.PublicValues));
        public_values.metadata.withdrawal_vault_data.vault_address = 0x1122334455667788990011223344556677889900;
        bytes memory public_values_encoded = abi.encode(public_values);

        vm.expectRevert(verification_error("Withdrawal vault address mismatch"));
        _contract.submitReportData(fixture.proof, public_values_encoded);
    }

    function test_validatorRejects_reverts() public {
        SP1ProofFixtureJson memory fixture = loadFixture();

        setSingleBlockHash(fixture.metadata.bc_slot, fixture.metadata.beacon_block_hash);
        verifierRejects("Some Error");
        vm.expectRevert();
        _contract.submitReportData(fixture.proof, fixture.publicValues);
    }

    function test_refSlotHasBlock_beaconStateSlotEqualToRefSlot_passes() public {
        SP1ProofFixtureJson memory fixture = loadFixture();

        setSingleBlockHash(fixture.metadata.bc_slot, fixture.metadata.beacon_block_hash);
        verifierPasses();
        Sp1LidoAccountingReportContract.PublicValues memory public_values =
            abi.decode(fixture.publicValues, (Sp1LidoAccountingReportContract.PublicValues));
        // fixture should have ref slot and beacon state slot match, this is a self-check
        assertEq(public_values.metadata.bc_slot, public_values.report.reference_slot);
        bytes memory public_values_encoded = abi.encode(public_values);
        Sp1LidoAccountingReportContract.Report memory expected_report = public_values.report;

        _contract.submitReportData(fixture.proof, public_values_encoded);
        assertReportAccepted(public_values.report.reference_slot, expected_report);
    }

    function test_refSlotEmpty_beaconStateSlotFirstPrecedingNonEmpty_passes() public {
        SP1ProofFixtureJson memory fixture = loadFixture();

        verifierPasses();
        Sp1LidoAccountingReportContract.PublicValues memory public_values =
            abi.decode(fixture.publicValues, (Sp1LidoAccountingReportContract.PublicValues));
        public_values.report.reference_slot = public_values.metadata.bc_slot + 2;
        bytes32[] memory hashes = _createDyn(
            /*bc*/
            SLOT_EXISTED_SENTIEL,
            0,
            /*ref*/
            0,
            fixture.metadata.beacon_block_hash
        );

        setBeaconHashSequence(public_values.metadata.bc_slot, public_values.report.reference_slot, hashes);

        bytes memory public_values_encoded = abi.encode(public_values);
        Sp1LidoAccountingReportContract.Report memory expected_report = public_values.report;

        _contract.submitReportData(fixture.proof, public_values_encoded);
        assertReportAccepted(public_values.report.reference_slot, expected_report);
    }

    function test_refSlotHasBlock_beaconStateSlotNotEqualToRefSlot_reverts() public {
        SP1ProofFixtureJson memory fixture = loadFixture();

        verifierPasses();
        Sp1LidoAccountingReportContract.PublicValues memory public_values =
            abi.decode(fixture.publicValues, (Sp1LidoAccountingReportContract.PublicValues));
        public_values.report.reference_slot = public_values.metadata.bc_slot + 1;
        bytes memory public_values_encoded = abi.encode(public_values);

        // Block exists at bc_slot and reference, but bc_slot != reference
        bytes32[] memory hashes = _createDyn(
            /*bc*/
            SLOT_EXISTED_SENTIEL,
            /*ref*/
            fixture.metadata.beacon_block_hash,
            SLOT_EXISTED_SENTIEL
        );

        setBeaconHashSequence(public_values.metadata.bc_slot, public_values.report.reference_slot, hashes);
        vm.expectRevert(
            illegal_ref_slot_error(
                public_values.metadata.bc_slot,
                public_values.report.reference_slot,
                "Reference slot has a block, but beacon state slot != reference slot"
            )
        );
        _contract.submitReportData(fixture.proof, public_values_encoded);
    }

    function test_beaconStateSlotEmpty_reverts() public {
        SP1ProofFixtureJson memory fixture = loadFixture();

        verifierPasses();
        Sp1LidoAccountingReportContract.PublicValues memory public_values =
            abi.decode(fixture.publicValues, (Sp1LidoAccountingReportContract.PublicValues));
        public_values.report.reference_slot = public_values.metadata.bc_slot + 1;
        bytes memory public_values_encoded = abi.encode(public_values);

        bytes32[] memory hashes = _createDyn(
            /*bc*/
            0,
            /* ref */
            0,
            fixture.metadata.beacon_block_hash
        );
        setBeaconHashSequence(public_values.metadata.bc_slot, public_values.report.reference_slot, hashes);
        vm.expectRevert(
            illegal_ref_slot_error(
                public_values.metadata.bc_slot, public_values.report.reference_slot, "Beacon state slot is empty"
            )
        );
        _contract.submitReportData(fixture.proof, public_values_encoded);
    }

    function test_refSlotEmpty_beaconStateSlotFirstNonEmptyPreceding_passes() public {
        SP1ProofFixtureJson memory fixture = loadFixture();

        verifierPasses();
        Sp1LidoAccountingReportContract.PublicValues memory public_values =
            abi.decode(fixture.publicValues, (Sp1LidoAccountingReportContract.PublicValues));
        public_values.report.reference_slot = public_values.metadata.bc_slot + 2;
        bytes memory public_values_encoded = abi.encode(public_values);

        // bc is filled, [beacon state + 1, ref_slot] is empty, ref+1 points to bc hash
        bytes32[] memory hashes = _createDyn(
            /*bc*/
            SLOT_EXISTED_SENTIEL,
            0,
            /*ref*/
            0,
            fixture.metadata.beacon_block_hash
        );

        setBeaconHashSequence(public_values.metadata.bc_slot, public_values.report.reference_slot, hashes);
        Sp1LidoAccountingReportContract.Report memory expected_report = public_values.report;
        _contract.submitReportData(fixture.proof, public_values_encoded);
        assertReportAccepted(public_values.report.reference_slot, expected_report);
    }

    function test_refSlotEmpty_beaconStateSlotNotFirstNonEmptyPreceding_reverts() public {
        SP1ProofFixtureJson memory fixture = loadFixture();

        verifierPasses();
        Sp1LidoAccountingReportContract.PublicValues memory public_values =
            abi.decode(fixture.publicValues, (Sp1LidoAccountingReportContract.PublicValues));
        public_values.report.reference_slot = public_values.metadata.bc_slot + 3;
        bytes memory public_values_encoded = abi.encode(public_values);

        // beacon state is filled, bc + 1 is filled, [beacon state + 2, ref_slot] is empty
        // bc is filled, bc+1 filled, [beacon state + 2, ref_slot] is empty, ref+1 points to bc+1 hash
        bytes32[] memory hashes = _createDyn(
            /*bc*/
            SLOT_EXISTED_SENTIEL,
            0xa1b2c3d4e5f6a7b8c9d000000000000000000000000000000000000000000000,
            0,
            /*ref*/
            0,
            fixture.metadata.beacon_block_hash
        );

        setBeaconHashSequence(public_values.metadata.bc_slot, public_values.report.reference_slot, hashes);
        vm.expectRevert(
            illegal_ref_slot_error(
                public_values.metadata.bc_slot,
                public_values.report.reference_slot,
                "Beacon state slot should be the first preceding non-empty slot before reference"
            )
        );
        _contract.submitReportData(fixture.proof, public_values_encoded);
    }

    function test_refSlotLessThanBeaconStateSlot_reverts() public {
        SP1ProofFixtureJson memory fixture = loadFixture();

        verifierPasses();
        Sp1LidoAccountingReportContract.PublicValues memory public_values =
            abi.decode(fixture.publicValues, (Sp1LidoAccountingReportContract.PublicValues));
        public_values.report.reference_slot = public_values.metadata.bc_slot - 2;
        bytes32[] memory hashes = _createDyn(
            /*ref*/
            0,
            0,
            /*bc*/
            SLOT_EXISTED_SENTIEL,
            fixture.metadata.beacon_block_hash
        );

        setBeaconHashSequence(public_values.report.reference_slot, public_values.metadata.bc_slot, hashes);

        bytes memory public_values_encoded = abi.encode(public_values);
        Sp1LidoAccountingReportContract.Report memory expected_report = public_values.report;

        vm.expectRevert(
            illegal_ref_slot_error(
                public_values.metadata.bc_slot,
                public_values.report.reference_slot,
                "Reference slot must be after beacon state slot"
            )
        );
        _contract.submitReportData(fixture.proof, public_values_encoded);
    }

    function test_refSlotInFuture_reverts() public {
        SP1ProofFixtureJson memory fixture = loadFixture();

        verifierPasses();
        Sp1LidoAccountingReportContract.PublicValues memory public_values =
            abi.decode(fixture.publicValues, (Sp1LidoAccountingReportContract.PublicValues));
        public_values.report.reference_slot = public_values.metadata.bc_slot + 2;
        bytes32[] memory hashes = _createDyn(
            /*bc*/
            SLOT_EXISTED_SENTIEL,
            0,
            /*ref*/
            0,
            fixture.metadata.beacon_block_hash
        );

        setBeaconHashSequence(public_values.metadata.bc_slot, public_values.report.reference_slot, hashes);
        vm.warp(getSlotTimestamp(public_values.report.reference_slot - 1));

        bytes memory public_values_encoded = abi.encode(public_values);
        Sp1LidoAccountingReportContract.Report memory expected_report = public_values.report;

        vm.expectRevert(
            illegal_ref_slot_error(
                public_values.metadata.bc_slot,
                public_values.report.reference_slot,
                "Reference slot must not be in the future"
            )
        );
        _contract.submitReportData(fixture.proof, public_values_encoded);
    }
}
