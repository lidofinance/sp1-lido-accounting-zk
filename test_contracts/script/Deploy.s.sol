// SPDX-License-Identifier: MIT
pragma solidity 0.8.27;

import "forge-std/Script.sol";
import "forge-std/console.sol";
import {BeaconRootsMock} from "../src/BeaconRootsMock.sol";
import {SP1VerifierMock} from "../src/SP1VerifierMock.sol";

// forge script --chain $EVM_CHAIN_ID script/Deploy.s.sol:Deploy --rpc-url $EXECUTION_LAYER_RPC --broadcast --verify
contract Deploy is Script {
    function stringsEqual(string memory _a, string memory _b) public pure returns(bool) {
        return keccak256(abi.encodePacked(_a)) == keccak256(abi.encodePacked(_b));
    }

    function run() external {
        uint256 deployerPrivateKey = vm.envUint("PRIVATE_KEY");
        vm.startBroadcast(deployerPrivateKey);

        console.logString("Deploying BeaconRootsMock");
        BeaconRootsMock beacon_roots_mock = new BeaconRootsMock();
        console.log("BeaconRootsMock deployed at:", address(beacon_roots_mock));

        console.logString("Deploying SP1VerifierMock");
        SP1VerifierMock verifier_mock = new SP1VerifierMock();
        console.log("SP1VerifierMock deployed at:", address(verifier_mock));

        vm.stopBroadcast();
    }
}
