// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "./Sp1LidoAccountingReportContractBase.sol";

contract Sp1LidoAccountingReportContract is
    Sp1LidoAccountingReportContractBase
{
    /// @notice Seconds per slot
    uint256 public immutable SECONDS_PER_SLOT = 12;

    /// @notice The genesis block timestamp.
    uint256 public immutable GENESIS_BLOCK_TIMESTAMP;

    /// @notice The address of the beacon roots precompile.
    /// @dev https://eips.ethereum.org/EIPS/eip-4788
    address internal constant BEACON_ROOTS =
        0x000F3df6D732807Ef1319fB7B8bB8522d0Beac02;

    /// @notice The length of the beacon roots ring buffer.
    uint256 internal constant BEACON_ROOTS_HISTORY_BUFFER_LENGTH = 8191;

    /// @dev Timestamp out of range for the the beacon roots precompile.
    error TimestampOutOfRange();

    /// @dev No block root is found using the beacon roots precompile.
    error NoBlockRootFound();

    constructor(
        address _verifier,
        bytes32 _vkey,
        bytes32 _lidowithdrawalCredentials,
        uint256 _genesis_timestamp,
        LidoValidatorState memory _initial_state
    ) Sp1LidoAccountingReportContractBase(_verifier, _vkey, _lidowithdrawalCredentials, _initial_state) {
        GENESIS_BLOCK_TIMESTAMP = _genesis_timestamp;
    }

    /// @notice Attempts to find the block root for the given slot.
    /// @param slot The slot to get the block root for.
    /// @return blockRoot The beacon block root of the given slot.
    /// @dev BEACON_ROOTS returns a block root for a given parent block's timestamp. To get the block root for slot
    ///      N, you use the timestamp of slot N+1. If N+1 is not avaliable, you use the timestamp of slot N+2, and
    //       so on.
    function _getBeaconBlockHash(
        uint256 slot
    ) internal view override returns (bytes32) {
        uint256 currBlockTimestamp = GENESIS_BLOCK_TIMESTAMP +
            ((slot + 1) * SECONDS_PER_SLOT);

        uint256 earliestBlockTimestamp = block.timestamp -
            (BEACON_ROOTS_HISTORY_BUFFER_LENGTH * SECONDS_PER_SLOT);
        if (currBlockTimestamp <= earliestBlockTimestamp) {
            revert TimestampOutOfRange();
        }

        while (currBlockTimestamp <= block.timestamp) {
            (bool success, bytes memory result) = BEACON_ROOTS.staticcall(
                abi.encode(currBlockTimestamp)
            );
            if (success && result.length > 0) {
                return abi.decode(result, (bytes32));
            }

            unchecked {
                currBlockTimestamp += 12;
            }
        }

        revert NoBlockRootFound();
    }
}
