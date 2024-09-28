// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

interface SecondOpinionOracle { 
	function getReport(uint256 refSlot) external view returns ( 
		bool success, 
		uint256 clBalanceGwei, 
		uint256 withdrawalVaultBalanceWei, 
		uint256 totalDepositedValidators, 
		uint256 totalExitedValidators 
	); 
}