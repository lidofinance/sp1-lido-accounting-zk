// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "./ValidatorsMerkleVerifierBase.sol";

/// @title Fibonacci.
/// @author Succinct Labs
/// @notice This contract implements a simple example of verifying the proof of a computing a
///         fibonacci number.
contract ValidatorsMerkleVerifierControllable is ValidatorsMerkleVerifierBase {
    mapping (uint64 => bytes32) private _beaconBlockHashes;

    constructor(address _verifier, bytes32 _vkey) ValidatorsMerkleVerifierBase(_verifier, _vkey) {}

    function setBeaconBlockHash(uint64 slot, bytes32 beaconBlockHash) public {
        _beaconBlockHashes[slot] = beaconBlockHash;
    }


    function getBeaconBlockHash(uint64 slot) internal override view returns (bytes32) {
        bytes32 result = _beaconBlockHashes[slot];
        require(result != 0, "Block hash is not set for target slot");
        return result;
    }
}
