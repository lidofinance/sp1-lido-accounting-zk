// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Test, console} from "forge-std/Test.sol";
import {stdJson} from "forge-std/StdJson.sol";
import {Sp1LidoAccountingReportContractControllable} from "../src/Sp1LidoAccountingReportContractControllable.sol";
import {Report, ReportMetadata} from "../src/Sp1LidoAccountingReportContractBase.sol";
import {SP1VerifierGateway} from "@sp1-contracts/SP1VerifierGateway.sol";


contract Sp1LidoAccountingReportContractTest is Test {
    using stdJson for string;

    address verifier;
    Sp1LidoAccountingReportContractControllable public _contract;

    struct SP1ProofFixtureJson {
        bytes32 vkey;
        Report report;
        ReportMetadata metadata;
        bytes proof;
        bytes publicValues;
    }

    function loadFixture() public view returns (SP1ProofFixtureJson memory) {
        string memory root = vm.projectRoot();
        string memory path = string.concat(root, "/src/fixtures/fixture.json");
        string memory json = vm.readFile(path);
        bytes memory jsonBytes = json.parseRaw(".");
        // This should be
        // return abi.decode(jsonBytes, (SP1ProofFixtureJson));
        // ... but it reverts with no explanation - so just doing it manually
        return (
            SP1ProofFixtureJson(
                json.readBytes32(".vkey"),
                Report(
                    json.readUint(".report.slot"),
                    json.readUint(".report.all_lido_validators"),
                    json.readUint(".report.exited_lido_validators"),
                    json.readUint(".report.lido_cl_valance")
                ),
                ReportMetadata(
                    json.readUint(".metadata.slot"),
                    json.readUint(".metadata.epoch"),
                    json.readBytes32(".metadata.lido_withdrawal_credentials"),
                    json.readBytes32(".metadata.beacon_block_hash")
                ),
                json.readBytes(".proof"),
                json.readBytes(".publicValues")
            )
        );
    }

    function setUp() public {
        SP1ProofFixtureJson memory fixture = loadFixture();

        verifier = address(new SP1VerifierGateway(address(1)));
        _contract = new Sp1LidoAccountingReportContractControllable(verifier, fixture.vkey);
    }

    function test_validProof() public {
        SP1ProofFixtureJson memory fixture = loadFixture();

        _contract.setBeaconBlockHash(fixture.metadata.slot, fixture.metadata.beacon_block_hash);
        _contract.setWithdrawalCredentials(fixture.metadata.lido_withdrawal_credentials);
        vm.mockCall(verifier, abi.encodeWithSelector(SP1VerifierGateway.verifyProof.selector), abi.encode(true));

        _contract.verify(fixture.metadata.slot, fixture.report, fixture.metadata, fixture.proof, fixture.publicValues);
    }

    function testFail_validProofWrongExpectedSlot() public {
        SP1ProofFixtureJson memory fixture = loadFixture();

        _contract.setBeaconBlockHash(fixture.metadata.slot, fixture.metadata.beacon_block_hash);
        _contract.setWithdrawalCredentials(fixture.metadata.lido_withdrawal_credentials);
        vm.mockCall(verifier, abi.encodeWithSelector(SP1VerifierGateway.verifyProof.selector), abi.encode(true));

        vm.expectRevert("Beacon block hash mismatch");
        _contract.verify(fixture.metadata.slot, fixture.report, fixture.metadata, fixture.proof, fixture.publicValues);
    }

    function testFail_validProofWrongExpectedBeaconBlockHash() public {
        SP1ProofFixtureJson memory fixture = loadFixture();
        bytes32 expectedHash = 0x0000000000000000000000000000000000000000000000000000000000000000;

        _contract.setBeaconBlockHash(fixture.metadata.slot, expectedHash);
        _contract.setWithdrawalCredentials(fixture.metadata.lido_withdrawal_credentials);
        vm.mockCall(verifier, abi.encodeWithSelector(SP1VerifierGateway.verifyProof.selector), abi.encode(true));

        vm.expectRevert("Beacon block hash mismatch");
        _contract.verify(fixture.metadata.slot, fixture.report, fixture.metadata, fixture.proof, fixture.publicValues);
    }

    function testFail_validProofWrongLidoWithdrawalCredentials() public {
        SP1ProofFixtureJson memory fixture = loadFixture();
        bytes32 expectedCredentials = 0x0000000000000000000000000000000000000000000000000000000000000000;

        _contract.setBeaconBlockHash(fixture.metadata.slot, fixture.metadata.beacon_block_hash);
        _contract.setWithdrawalCredentials(expectedCredentials);
        vm.mockCall(verifier, abi.encodeWithSelector(SP1VerifierGateway.verifyProof.selector), abi.encode(true));

        vm.expectRevert("Withdrawal credentials hash mismatch");
        _contract.verify(fixture.metadata.slot, fixture.report, fixture.metadata, fixture.proof, fixture.publicValues);
    }

    function testFail_invalidProof() public  view {
        SP1ProofFixtureJson memory fixture = loadFixture();

        // Create a fake proof.
        bytes memory fakeProof = new bytes(fixture.proof.length);
        _contract.verify(fixture.metadata.slot, fixture.report, fixture.metadata, fakeProof, fixture.publicValues);
    }

    function testFail_invalidPublicValues() public view {
        SP1ProofFixtureJson memory fixture = loadFixture();

        // Create a fake proof.
        bytes memory fakePublicValues = new bytes(fixture.proof.length);
        _contract.verify(fixture.metadata.slot, fixture.report, fixture.metadata, fixture.proof, fakePublicValues);
    }
}
