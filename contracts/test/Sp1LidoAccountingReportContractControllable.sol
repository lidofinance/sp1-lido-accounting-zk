// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Sp1LidoAccountingReportContractBase, LidoValidatorState, Report, ReportMetadata} from "../src/Sp1LidoAccountingReportContractBase.sol";

contract Sp1LidoAccountingReportContractControllable is
    Sp1LidoAccountingReportContractBase
{
    mapping(uint256 => bytes32) private _beaconBlockHashes;

    constructor(
        address _verifier,
        bytes32 _vkey,
        bytes32 _widthrawal_credentials,
        LidoValidatorState memory _initial_state
    )
        Sp1LidoAccountingReportContractBase(
            _verifier,
            _vkey,
            _widthrawal_credentials,
            _initial_state
        )
    {}

    function setBeaconBlockHash(uint256 slot, bytes32 beaconBlockHash) public {
        _beaconBlockHashes[slot] = beaconBlockHash;
    }

    function _getBeaconBlockHash(
        uint256 slot
    ) internal view override returns (bytes32) {
        bytes32 result = _beaconBlockHashes[slot];
        require(result != 0, "Block hash is not set for target slot");
        return result;
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
