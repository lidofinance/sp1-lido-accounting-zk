// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Test, console} from "forge-std/Test.sol";
import {stdJson} from "forge-std/StdJson.sol";
import {ValidatorsMerkleVerifierControllable} from "../src/ValidatorsMerkleVerifierControllable.sol";
import {SP1VerifierGateway} from "@sp1-contracts/SP1VerifierGateway.sol";

struct SP1ProofFixtureJson {
    uint64 slot;
    bytes32 beaconBlockHash;
    bytes proof;
    bytes publicValues;
    bytes32 vkey;
}


contract ValidatorsMerkleVerifierTest is Test {
    using stdJson for string;

    address verifier;
    ValidatorsMerkleVerifierControllable public _contract;

    function loadFixture() public view returns (SP1ProofFixtureJson memory) {
        string memory root = vm.projectRoot();
        string memory path = string.concat(root, "/src/fixtures/fixture.json");
        string memory json = vm.readFile(path);
        // bytes memory jsonBytes = json.parseRaw(".");
        // return abi.decode(jsonBytes, (SP1ProofFixtureJson));
        return (SP1ProofFixtureJson(
            uint64(json.readUint(".slot")),
            json.readBytes32(".beaconBlockHash"),
            json.readBytes(".proof"),
            json.readBytes(".publicValues"),
            json.readBytes32(".vkey")
        ));
    }

    function setUp() public {
        SP1ProofFixtureJson memory fixture = loadFixture();

        verifier = address(new SP1VerifierGateway(address(1)));
        _contract = new ValidatorsMerkleVerifierControllable(verifier, fixture.vkey);
    }

    function test_validProof() public {
        SP1ProofFixtureJson memory fixture = loadFixture();

        _contract.setBeaconBlockHash(fixture.slot, fixture.beaconBlockHash);
        vm.mockCall(verifier, abi.encodeWithSelector(SP1VerifierGateway.verifyProof.selector), abi.encode(true));

        bytes32 beaconBlockHash = _contract.verify(fixture.slot, fixture.proof, fixture.publicValues);
        assert(beaconBlockHash == fixture.beaconBlockHash);
    }

    function testFail_validProofWrongExpectedSlot() public {
        SP1ProofFixtureJson memory fixture = loadFixture();

        _contract.setBeaconBlockHash(fixture.slot, fixture.beaconBlockHash);
        vm.mockCall(verifier, abi.encodeWithSelector(SP1VerifierGateway.verifyProof.selector), abi.encode(true));

        vm.expectRevert("Beacon block hash mismatch");
        _contract.verify(123456, fixture.proof, fixture.publicValues);
    }

    function testFail_validProofWrongExpectedBeaconBlockHash() public {
        SP1ProofFixtureJson memory fixture = loadFixture();
        bytes32 expectedHash = 0x0000000000000000000000000000000000000000000000000000000000000000;

        _contract.setBeaconBlockHash(fixture.slot, expectedHash);
        vm.mockCall(verifier, abi.encodeWithSelector(SP1VerifierGateway.verifyProof.selector), abi.encode(true));

        vm.expectRevert("Beacon block hash mismatch");
        _contract.verify(fixture.slot, fixture.proof, fixture.publicValues);
    }

    function testFail_invalidProof() public view {
        SP1ProofFixtureJson memory fixture = loadFixture();

        // Create a fake proof.
        bytes memory fakeProof = new bytes(fixture.proof.length);

        _contract.verify(fixture.slot, fakeProof, fixture.publicValues);
    }
}
