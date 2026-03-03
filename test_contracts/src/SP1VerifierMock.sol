// SPDX-FileCopyrightText: 2025 Lido <info@lido.fi>
// SPDX-License-Identifier: GPL-3.0
pragma solidity 0.8.27;

/// @title SP1 Verifier Interface
/// @notice This interface matches the ISP1Verifier interface from sp1-contracts
interface ISP1Verifier {
    /// @notice Verifies a proof with given public values and vkey.
    function verifyProof(
        bytes32 programVKey,
        bytes calldata publicValues,
        bytes calldata proofBytes
    ) external view;
}

/// @title SP1 Verifier Interface With Hash
interface ISP1VerifierWithHash is ISP1Verifier {
    /// @notice Returns the hash of the verifier.
    function VERIFIER_HASH() external pure returns (bytes32);
}

/// @notice Mock SP1 Verifier that always passes verification
/// @dev NOT PART OF THE AUDIT SCOPE, ONLY USED FOR TESTING/DEVNET
///      This contract implements ISP1VerifierWithHash and always returns success
///      for verifyProof calls, making it suitable for testing the main contract
///      without requiring actual SP1 proofs.
contract SP1VerifierMock is ISP1VerifierWithHash {
    /// @notice Returns a mock verifier hash
    /// @dev This can be any value since we're not actually verifying proofs
    function VERIFIER_HASH() external pure override returns (bytes32) {
        return 0x0000000000000000000000000000000000000000000000000000000000000001;
    }

    /// @notice Always succeeds - does not revert
    /// @dev This mock implementation accepts any proof, vkey, and public values
    ///      without performing actual verification. Use only for testing/devnet.
    function verifyProof(
        bytes32 /* programVKey */,
        bytes calldata /* publicValues */,
        bytes calldata /* proofBytes */
    ) external pure override {
        // Always succeed - do nothing, don't revert
        // The main contract uses try-catch, so not reverting means success
    }
}
