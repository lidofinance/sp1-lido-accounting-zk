// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ISP1Verifier} from "@sp1-contracts/ISP1Verifier.sol";

/// @title Fibonacci.
/// @author Succinct Labs
/// @notice This contract implements a simple example of verifying the proof of a computing a
///         fibonacci number.
abstract contract ValidatorsMerkleVerifierBase {
    /// @notice The address of the SP1 verifier contract.
    /// @dev This can either be a specific SP1Verifier for a specific version, or the
    ///      SP1VerifierGateway which can be used to verify proofs for any version of SP1.
    ///      For the list of supported verifiers on each chain, see:
    ///      https://github.com/succinctlabs/sp1-contracts/tree/main/contracts/deployments
    address public verifier;

    /// @notice The verification key.
    bytes32 public vkey;

    constructor(address _verifier, bytes32 _vkey) {
        verifier = _verifier;
        vkey = _vkey;
    }

    /// @notice The entrypoint for verifying the proof of a fibonacci number.
    /// @param proof The encoded proof.
    /// @param publicValues The encoded public values.
    function verify(
        uint64 slot,
        bytes memory proof,
        bytes memory publicValues
    ) public view returns (bytes32) {
        ISP1Verifier(verifier).verifyProof(vkey, publicValues, proof);
        (uint64 proof_slot, bytes32 beaconBlockHash) = abi.decode(
            publicValues,
            (uint64, bytes32)
        );
        require(proof_slot == slot, "Slot mismatch");
        require(
            beaconBlockHash == getBeaconBlockHash(slot), 
            "Beacon block hash mismatch"
        );
        return (beaconBlockHash);
    }

    function getBeaconBlockHash(uint64 slot) internal virtual view returns (bytes32);
}
