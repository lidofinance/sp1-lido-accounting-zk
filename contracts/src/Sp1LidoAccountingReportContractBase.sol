// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ISP1Verifier} from "@sp1-contracts/ISP1Verifier.sol";

struct Report {
    uint256 slot;
    uint256 all_lido_validators;
    uint256 exited_lido_validators;
    uint256 lido_cl_valance;
}
struct ReportMetadata {
    uint256 slot;
    uint256 epoch;
    bytes32 lido_withdrawal_credentials;
    bytes32 beacon_block_hash;
}

struct PublicValues {
    Report report;
    ReportMetadata metadata;
}

abstract contract Sp1LidoAccountingReportContractBase {
    event ReportAccepted(Report report);
    // This should later become an error
    event ReportRejected(string reason);

    mapping (uint256 => Report) private _reports;
    address public verifier;
    bytes32 public vkey;

    constructor(address _verifier, bytes32 _vkey) {
        verifier = _verifier;
        vkey = _vkey;
    }

    function submitReportData(
        uint256 slot,
        Report calldata report,
        ReportMetadata calldata metadata,
        bytes calldata proof,
        bytes calldata publicValues
    ) public {
        _verify(slot, report, metadata, proof, publicValues);

        // If all checks pass - update report
        _updateReport(slot, report);
    }

    function _verify(
        uint256 slot,
        Report calldata report,
        ReportMetadata calldata metadata,
        bytes calldata proof,
        bytes calldata publicValues
    ) public view {
        // Check the report was not previously set
        Report storage report_at_slot = _reports[slot];
        _require(report_at_slot.slot == 0, "Report was already accepted for a given slot");

        // Check the report is for the target slot
        _require(report.slot == slot, "Slot mismatch");

        // Check that passed beacon_block_hash matches the one observed on the blockchain for 
        // the target slot
        _require(
            metadata.beacon_block_hash == _getBeaconBlockHash(slot), 
            "BeaconBlockHash mismatch"
        );

        // Check that correct withdrawal credentials were used
        _require(
            metadata.lido_withdrawal_credentials == _getExpectedWithdrawalCredentials(),
            "Withdrawal credentials mismatch"
        );

        // Check that report and metadata match public values committed in the ZK-program
        PublicValues memory public_values = abi.decode(publicValues, (PublicValues));
        _verify_public_values(report, metadata, public_values);

        // Verify ZK-program and public values
        ISP1Verifier(verifier).verifyProof(vkey, publicValues, proof);
    }


    function _verify_public_values(
        Report memory report,
        ReportMetadata memory metadata,
        PublicValues memory publicValues
    ) internal pure {
        _require(
            report.slot == publicValues.report.slot, 
            "Report.slot doesn't match public values"
        );
        _require(
            report.all_lido_validators == publicValues.report.all_lido_validators, 
            "Report.all_lido_validators doesn't match public values"
        );
        _require(
            report.exited_lido_validators == publicValues.report.exited_lido_validators, 
            "Report.exited_lido_validators doesn't match public values"
        );
        _require(
            report.lido_cl_valance == publicValues.report.lido_cl_valance, 
            "Report.lido_cl_valance doesn't match public values"
        );

        _require(
            metadata.slot == publicValues.metadata.slot, 
            "Metadata.slot doesn't match public values"
        );
        _require(
            metadata.epoch == publicValues.metadata.epoch, 
            "Metadata.epoch doesn't match public values"
        );
        _require(
            metadata.lido_withdrawal_credentials == publicValues.metadata.lido_withdrawal_credentials, 
            "Metadata.lido_withdrawal_credentials doesn't match public values"
        );
        _require(
            metadata.beacon_block_hash == publicValues.metadata.beacon_block_hash, 
            "Metadata.beacon_block_hash doesn't match public values"
        );
    }

    function getReport(uint256 slot) public view returns (Report memory result) {
        return (_reports[slot]);
    }

    function _updateReport(uint256 slot, Report memory report) internal {
        _reports[slot] = report;
        emit ReportAccepted(report);
    }

    function _require(bool condition, string memory reason) internal pure {
        if (!condition) {
            revert(reason);
        }
    }

    function _getBeaconBlockHash(uint256 slot) internal virtual view returns (bytes32);
    function _getExpectedWithdrawalCredentials() internal virtual view returns (bytes32);
}
