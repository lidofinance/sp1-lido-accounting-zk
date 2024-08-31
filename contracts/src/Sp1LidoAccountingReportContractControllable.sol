// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "./Sp1LidoAccountingReportContractBase.sol";

/// @title Fibonacci.
/// @author Succinct Labs
/// @notice This contract implements a simple example of verifying the proof of a computing a
///         fibonacci number.
contract Sp1LidoAccountingReportContractControllable is Sp1LidoAccountingReportContractBase {
    mapping (uint256 => bytes32) private _beaconBlockHashes;
    bytes32 _withdrawalCredentials;

    constructor(address _verifier, bytes32 _vkey, LidoValidatorState memory _initial_state) Sp1LidoAccountingReportContractBase(_verifier, _vkey, _initial_state) {}

    function setBeaconBlockHash(uint256 slot, bytes32 beaconBlockHash) public {
        _beaconBlockHashes[slot] = beaconBlockHash;
    }


    function _getBeaconBlockHash(uint256 slot) internal override view returns (bytes32) {
        bytes32 result = _beaconBlockHashes[slot];
        require(result != 0, "Block hash is not set for target slot");
        return result;
    }

    function setWithdrawalCredentials(bytes32 withdrawalCredentials) public {
        _withdrawalCredentials = withdrawalCredentials;
    }

    function _getExpectedWithdrawalCredentials() internal override view returns (bytes32) {
        return _withdrawalCredentials;
    }

    function verify(
        uint256 slot,
        Report calldata report,
        ReportMetadata calldata metadata,
        bytes calldata proof,
        bytes calldata publicValues
    ) public view {
        _verify(slot, report, metadata, proof, publicValues);
    }
}
