// SPDX-FileCopyrightText: 2025 Lido <info@lido.fi>
// SPDX-License-Identifier: GPL-3.0
pragma solidity 0.8.27;

import {Sp1LidoAccountingReportContract} from "../src/Sp1LidoAccountingReportContract.sol";

contract Sp1LidoAccountingReportContractTestWrapper is Sp1LidoAccountingReportContract {
    constructor(
        address _verifier,
        bytes32 _vkey,
        bytes32 _lido_withdrawal_credentials,
        address _withdrawal_vault_address,
        uint256 _genesis_timestamp,
        LidoValidatorState memory _initial_state,
        address _admin
    )
        Sp1LidoAccountingReportContract(
            _verifier,
            _vkey,
            _lido_withdrawal_credentials,
            _withdrawal_vault_address,
            _genesis_timestamp,
            _initial_state,
            _admin
        )
    {}

    function recordLidoValidatorStateHash(uint256 slot, bytes32 state_merkle_root) public {
        _recordLidoValidatorStateHash(slot, state_merkle_root);
    }

    function getVerifierParameters(uint256 stateSlot) public view returns (Sp1VerifierParameters memory) {
        return super.getVerifierParameters(stateSlot);
    }
}
