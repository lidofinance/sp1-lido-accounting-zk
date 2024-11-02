// SPDX-License-Identifier: MIT
pragma solidity 0.8.27;

import {Test, console} from "forge-std/Test.sol";
import {stdJson} from "forge-std/StdJson.sol";
import {Sp1LidoAccountingReportContract} from "../src/Sp1LidoAccountingReportContract.sol";
import {SP1VerifierGateway} from "@sp1-contracts/SP1VerifierGateway.sol";

contract Sp1LidoAccountingReportContractTest is Test {
    using stdJson for string;

    address private verifier;
    Sp1LidoAccountingReportContract private _contract;

    uint256 private immutable GENESIS_BLOCK_TIMESTAMP = 1606824023;
    uint256 private immutable SECONDS_PER_SLOT = 12;

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
                    json.readUint(".report.slot"),
                    json.readUint(".report.deposited_lido_validators"),
                    json.readUint(".report.exited_lido_validators"),
                    json.readUint(".report.lido_cl_balance")
                ),
                Sp1LidoAccountingReportContract.ReportMetadata(
                    json.readUint(".metadata.slot"),
                    json.readUint(".metadata.epoch"),
                    json.readBytes32(".metadata.lido_withdrawal_credentials"),
                    json.readBytes32(".metadata.beacon_block_hash"),
                    Sp1LidoAccountingReportContract.LidoValidatorState(
                        json.readUint(
                            ".metadata.state_for_previous_report.slot"
                        ),
                        json.readBytes32(
                            ".metadata.state_for_previous_report.merkle_root"
                        )
                    ),
                    Sp1LidoAccountingReportContract.LidoValidatorState(
                        json.readUint(".metadata.new_state.slot"),
                        json.readBytes32(".metadata.new_state.merkle_root")
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
        _contract = new Sp1LidoAccountingReportContract(
            verifier,
            fixture.vkey,
            fixture.metadata.lido_withdrawal_credentials,
            GENESIS_BLOCK_TIMESTAMP,
            fixture.metadata.old_state
        );
    }

    function getSlotTimestamp(uint256 slot) internal view returns (uint256) {
        uint256 timestamp = _contract.GENESIS_BLOCK_TIMESTAMP() +
            ((slot + 1) * _contract.SECONDS_PER_SLOT());
        return (timestamp);
    }

    function setBeaconBlockHash(uint256 slot, bytes32 expected_hash) private {
        uint256 reportBlockTimestamp = getSlotTimestamp(slot);
        vm.mockCall(
            _contract.BEACON_ROOTS(),
            abi.encode(reportBlockTimestamp),
            abi.encode(expected_hash)
        );

        vm.warp(reportBlockTimestamp + 15 * _contract.SECONDS_PER_SLOT());
    }

    function verification_error(
        string memory message
    ) internal pure returns (bytes memory) {
        return
            abi.encodeWithSelector(
                Sp1LidoAccountingReportContract.VerificationError.selector,
                message
            );
    }

    function verifierPasses() internal {
        vm.mockCall(
            verifier,
            abi.encodeWithSelector(SP1VerifierGateway.verifyProof.selector),
            abi.encode()
        );
    }

    function verifierRejects(bytes memory err) internal {
        vm.mockCallRevert(
            verifier,
            abi.encodeWithSelector(SP1VerifierGateway.verifyProof.selector),
            err
        );
    }

    function assertReportAccepted(
        uint256 slot,
        Sp1LidoAccountingReportContract.Report memory expected_report
    ) internal view {
        (
            bool success,
            uint256 clBalanceGwei,
            uint256 withdrawalVaultBalanceWei,
            uint256 totalDepositedValidators,
            uint256 totalExitedValidators
        ) = _contract.getReport(slot);

        assertEq(success, true);
        assertEq(clBalanceGwei, expected_report.lido_cl_balance);
        assertEq(
            totalDepositedValidators,
            expected_report.deposited_lido_validators
        );
        assertEq(totalExitedValidators, expected_report.exited_lido_validators);
        assertEq(withdrawalVaultBalanceWei, 0); // TODO: not done yet
    }

    function test_validProof() public {
        SP1ProofFixtureJson memory fixture = loadFixture();

        setBeaconBlockHash(
            fixture.metadata.slot,
            fixture.metadata.beacon_block_hash
        );
        verifierPasses();

        Sp1LidoAccountingReportContract.PublicValues memory public_values = abi.decode(
            fixture.publicValues,
            (Sp1LidoAccountingReportContract.PublicValues)
        );
        Sp1LidoAccountingReportContract.Report memory expected_report = public_values.report;

        _contract.submitReportData(fixture.proof, fixture.publicValues);
        assertReportAccepted(public_values.report.slot, expected_report);
    }

    function test_validProofWrongExpectedSlot_reverts() public {
        SP1ProofFixtureJson memory fixture = loadFixture();

        setBeaconBlockHash(
            fixture.metadata.slot,
            fixture.metadata.beacon_block_hash
        );
        verifierPasses();

        Sp1LidoAccountingReportContract.PublicValues memory public_values = abi.decode(
            fixture.publicValues,
            (Sp1LidoAccountingReportContract.PublicValues)
        );
        public_values.metadata.slot = 1111111;
        bytes memory public_values_encoded = abi.encode(public_values);

        vm.expectRevert(
            verification_error("Report and metadata slot do not match")
        );
        _contract.submitReportData(fixture.proof, public_values_encoded);
    }

    function test_validProofWrongExpectedBeaconBlockHash_reverts() public {
        SP1ProofFixtureJson memory fixture = loadFixture();
        bytes32 expectedHash = 0x1111111100000000000000000000000000000000000000000000000022222222;

        setBeaconBlockHash(fixture.metadata.slot, expectedHash);
        verifierPasses();

        vm.expectRevert(verification_error("BeaconBlockHash mismatch"));
        _contract.submitReportData(fixture.proof, fixture.publicValues);
    }

    function test_validProofWrongLidoWithdrawalCredentials_reverts() public {
        SP1ProofFixtureJson memory fixture = loadFixture();

        setBeaconBlockHash(
            fixture.metadata.slot,
            fixture.metadata.beacon_block_hash
        );
        verifierPasses();
        Sp1LidoAccountingReportContract.PublicValues memory public_values = abi.decode(
            fixture.publicValues,
            (Sp1LidoAccountingReportContract.PublicValues)
        );
        public_values
            .metadata
            .lido_withdrawal_credentials = 0xABCDEF0000000000000000000000000000000000000000000000000000FEDCBA;
        bytes memory public_values_encoded = abi.encode(public_values);

        vm.expectRevert(verification_error("Withdrawal credentials mismatch"));
        _contract.submitReportData(fixture.proof, public_values_encoded);
    }

    function test_noStateRecordedForOldStateSlot_reverts() public {
        SP1ProofFixtureJson memory fixture = loadFixture();

        setBeaconBlockHash(
            fixture.metadata.slot,
            fixture.metadata.beacon_block_hash
        );
        verifierPasses();
        Sp1LidoAccountingReportContract.PublicValues memory public_values = abi.decode(
            fixture.publicValues,
            (Sp1LidoAccountingReportContract.PublicValues)
        );
        public_values.metadata.old_state.slot = 987654321;
        bytes memory public_values_encoded = abi.encode(public_values);

        vm.expectRevert(verification_error("Old state merkle_root not found"));
        _contract.submitReportData(fixture.proof, public_values_encoded);
    }

    function test_oldStateWrongMerkleRoot_reverts() public {
        SP1ProofFixtureJson memory fixture = loadFixture();

        setBeaconBlockHash(
            fixture.metadata.slot,
            fixture.metadata.beacon_block_hash
        );
        verifierPasses();
        Sp1LidoAccountingReportContract.PublicValues memory public_values = abi.decode(
            fixture.publicValues,
            (Sp1LidoAccountingReportContract.PublicValues)
        );
        public_values
            .metadata
            .old_state
            .merkle_root = 0x0102030405060708090000000000000000000000000000000000000000000000;
        bytes memory public_values_encoded = abi.encode(public_values);

        vm.expectRevert(verification_error("Old state merkle_root mismatch"));
        _contract.submitReportData(fixture.proof, public_values_encoded);
    }

    function test_validatorRejects_reverts() public {
        SP1ProofFixtureJson memory fixture = loadFixture();

        setBeaconBlockHash(
            fixture.metadata.slot,
            fixture.metadata.beacon_block_hash
        );
        verifierRejects("Some Error");
        vm.expectRevert();
        _contract.submitReportData(fixture.proof, fixture.publicValues);
    }
}
