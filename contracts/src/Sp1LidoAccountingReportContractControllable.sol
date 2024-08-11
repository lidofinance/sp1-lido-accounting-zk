// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "./Sp1LidoAccountingReportContractBase.sol";

/// @title Fibonacci.
/// @author Succinct Labs
/// @notice This contract implements a simple example of verifying the proof of a computing a
///         fibonacci number.
contract Sp1LidoAccountingReportContractControllable is Sp1LidoAccountingReportContractBase {
    mapping (uint64 => bytes32) private _beaconBlockHashes;
    bytes32 _withdrawalCredentials;

    constructor(address _verifier, bytes32 _vkey) ValidatorsMerkleVerifierBase(_verifier, _vkey) {}

    function setBeaconBlockHash(uint64 slot, bytes32 beaconBlockHash) public {
        _beaconBlockHashes[slot] = beaconBlockHash;
    }


    function _getBeaconBlockHash(uint64 slot) internal override view returns (bytes32) {
        bytes32 result = _beaconBlockHashes[slot];
        require(result != 0, "Block hash is not set for target slot");
        return result;
    }

    function setWithdrawalCredentials(bytes32 calldata withdrawalCredentials) public {
        _withdrawalCredentials = withdrawalCredentials;
    }

    function _getExpectedWithdrawalCredentials() internal view returns (bytes32) {
        return _withdrawalCredentials;
    }

    function verify(
        uint64 slot,
        Report calldata report,
        ReportMetadata calldata metadata,
        bytes calldata proof,
        bytes calldata publicValues
    ) public {
        _verify(slot, report, metadata, proof, publicValues);
    }
}
