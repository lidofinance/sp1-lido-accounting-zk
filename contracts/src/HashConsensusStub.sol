// SPDX-FileCopyrightText: 2025 Lido <info@lido.fi>
// SPDX-License-Identifier: GPL-3.0
pragma solidity 0.8.27;

contract HashConsensusStub {

    uint256 public _refSlot;
    uint256 public _reportProcessingDeadlineSlot;

    constructor(
        uint256 refSlot,
        uint256 reportProcessingDeadlineSlot
    ) {
        _refSlot = refSlot;
        _reportProcessingDeadlineSlot = reportProcessingDeadlineSlot;
    }

    function setCurrentFrame(
        uint256 refSlot,
        uint256 reportProcessingDeadlineSlot
    ) external {
        _refSlot = refSlot;
        _reportProcessingDeadlineSlot = reportProcessingDeadlineSlot;
    }

    function getCurrentFrame() external view returns (
        uint256 refSlot,
        uint256 reportProcessingDeadlineSlot
    ) {
        refSlot = _refSlot;
        reportProcessingDeadlineSlot = _reportProcessingDeadlineSlot;
    }

}
