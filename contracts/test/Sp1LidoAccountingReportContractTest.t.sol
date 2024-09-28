// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Test, console} from "forge-std/Test.sol";
import {stdJson} from "forge-std/StdJson.sol";
import {Sp1LidoAccountingReportContractControllable} from "./Sp1LidoAccountingReportContractControllable.sol";
import {LidoValidatorState, Report, ReportMetadata} from "../src/Sp1LidoAccountingReportContract.sol";
import {SP1VerifierGateway} from "@sp1-contracts/SP1VerifierGateway.sol";

contract Sp1LidoAccountingReportContractTest is Test {
    using stdJson for string;

    address verifier;
    Sp1LidoAccountingReportContractControllable public _contract;

    uint256 public immutable GENESIS_BLOCK_TIMESTAMP = 1606824023;

    struct SP1ProofFixtureJson {
        bytes32 vkey;
        Report report;
        ReportMetadata metadata;
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
                Report(
                    json.readUint(".report.slot"),
                    json.readUint(".report.deposited_lido_validators"),
                    json.readUint(".report.exited_lido_validators"),
                    json.readUint(".report.lido_cl_balance")
                ),
                ReportMetadata(
                    json.readUint(".metadata.slot"),
                    json.readUint(".metadata.epoch"),
                    json.readBytes32(".metadata.lido_withdrawal_credentials"),
                    json.readBytes32(".metadata.beacon_block_hash"),
                    LidoValidatorState(
                        json.readUint(
                            ".metadata.state_for_previous_report.slot"
                        ),
                        json.readBytes32(
                            ".metadata.state_for_previous_report.merkle_root"
                        )
                    ),
                    LidoValidatorState(
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
        _contract = new Sp1LidoAccountingReportContractControllable(
            verifier,
            fixture.vkey,
            fixture.metadata.lido_withdrawal_credentials,
            GENESIS_BLOCK_TIMESTAMP,
            fixture.metadata.old_state
        );
    }

    function test_validProof() public {
        SP1ProofFixtureJson memory fixture = loadFixture();
        _contract.setBeaconBlockHash(
            fixture.metadata.slot,
            fixture.metadata.beacon_block_hash
        );
        vm.mockCall(
            verifier,
            abi.encodeWithSelector(SP1VerifierGateway.verifyProof.selector),
            abi.encode()
        );

        _contract.verify(
            fixture.metadata.slot,
            fixture.report,
            fixture.metadata,
            fixture.proof,
            fixture.publicValues
        );
    }

    function test_validProofWrongExpectedSlot_reverts() public {
        SP1ProofFixtureJson memory fixture = loadFixture();

        _contract.setBeaconBlockHash(
            fixture.metadata.slot,
            fixture.metadata.beacon_block_hash
        );
        vm.mockCall(
            verifier,
            abi.encodeWithSelector(SP1VerifierGateway.verifyProof.selector),
            abi.encode()
        );

        vm.expectRevert("Slot mismatch: report");
        _contract.verify(
            111111,
            fixture.report,
            fixture.metadata,
            fixture.proof,
            fixture.publicValues
        );
    }

    function test_validProofWrongExpectedBeaconBlockHash_reverts() public {
        SP1ProofFixtureJson memory fixture = loadFixture();
        bytes32 expectedHash = 0x1111111100000000000000000000000000000000000000000000000022222222;

        _contract.setBeaconBlockHash(fixture.metadata.slot, expectedHash);
        vm.mockCall(
            verifier,
            abi.encodeWithSelector(SP1VerifierGateway.verifyProof.selector),
            abi.encode()
        );
        vm.expectRevert("BeaconBlockHash mismatch");

        _contract.verify(
            fixture.metadata.slot,
            fixture.report,
            fixture.metadata,
            fixture.proof,
            fixture.publicValues
        );
    }

    function test_validProofWrongLidoWithdrawalCredentials_reverts() public {
        SP1ProofFixtureJson memory fixture = loadFixture();

        _contract.setBeaconBlockHash(
            fixture.metadata.slot,
            fixture.metadata.beacon_block_hash
        );
        vm.mockCall(
            verifier,
            abi.encodeWithSelector(SP1VerifierGateway.verifyProof.selector),
            abi.encode()
        );

        fixture.metadata.lido_withdrawal_credentials = 0xABCDEF0000000000000000000000000000000000000000000000000000FEDCBA;
        vm.expectRevert("Withdrawal credentials mismatch");
        _contract.verify(
            fixture.metadata.slot,
            fixture.report,
            fixture.metadata,
            fixture.proof,
            fixture.publicValues
        );
    }

    function test_invalidProof_reverts() public {
        SP1ProofFixtureJson memory fixture = loadFixture();
        _contract.setBeaconBlockHash(
            fixture.metadata.slot,
            fixture.metadata.beacon_block_hash
        );

        // Create a fake proof.
        bytes memory fakeProof = new bytes(fixture.proof.length);
        vm.mockCallRevert(
            verifier,
            abi.encodeWithSelector(SP1VerifierGateway.verifyProof.selector),
            "MOCKED_REVERT"
        );

        vm.expectRevert();
        _contract.verify(
            fixture.metadata.slot,
            fixture.report,
            fixture.metadata,
            fakeProof,
            fixture.publicValues
        );
    }

    function test_invalidPublicValues_reverts() public {
        SP1ProofFixtureJson memory fixture = loadFixture();
        _contract.setBeaconBlockHash(
            fixture.metadata.slot,
            fixture.metadata.beacon_block_hash
        );

        // Create a fake public values.
        bytes memory fakePublicValues = new bytes(fixture.proof.length);
        vm.mockCallRevert(
            verifier,
            abi.encodeWithSelector(SP1VerifierGateway.verifyProof.selector),
            "MOCKED_REVERT"
        );

        vm.expectRevert();
        _contract.verify(
            fixture.metadata.slot,
            fixture.report,
            fixture.metadata,
            fixture.proof,
            fakePublicValues
        );
    }

    function test_noStateRecordedForOldStateSlot_reverts() public {
        SP1ProofFixtureJson memory fixture = loadFixture();
        fixture.metadata.old_state.slot = 1111111;

        _contract.setBeaconBlockHash(
            fixture.metadata.slot,
            fixture.metadata.beacon_block_hash
        );
        vm.mockCall(
            verifier,
            abi.encodeWithSelector(SP1VerifierGateway.verifyProof.selector),
            abi.encode()
        );

        vm.expectRevert("Old state merkle_root not found");
        _contract.verify(
            fixture.metadata.slot,
            fixture.report,
            fixture.metadata,
            fixture.proof,
            fixture.publicValues
        );
    }

    function test_oldStateWrongMerkleRoot_reverts() public {
        SP1ProofFixtureJson memory fixture = loadFixture();
        fixture
            .metadata
            .old_state
            .merkle_root = 0x0102030405060708090000000000000000000000000000000000000000000000;

        _contract.setBeaconBlockHash(
            fixture.metadata.slot,
            fixture.metadata.beacon_block_hash
        );
        vm.mockCall(
            verifier,
            abi.encodeWithSelector(SP1VerifierGateway.verifyProof.selector),
            abi.encode()
        );

        vm.expectRevert("Old state merkle_root mismatch");
        _contract.verify(
            fixture.metadata.slot,
            fixture.report,
            fixture.metadata,
            fixture.proof,
            fixture.publicValues
        );
    }
}
