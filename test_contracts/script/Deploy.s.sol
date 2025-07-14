// SPDX-License-Identifier: MIT
pragma solidity 0.8.27;

import "forge-std/Script.sol";
import "forge-std/console.sol";
import {BeaconRootsMock} from "../src/BeaconRootsMock.sol";

// forge script --chain $EVM_CHAIN_ID script/Deploy.s.sol:Deploy --rpc-url $EXECUTION_LAYER_RPC --broadcast --verify
contract Deploy is Script {
    function stringsEqual(string memory _a, string memory _b) public pure returns(bool) {
        return keccak256(abi.encodePacked(_a)) == keccak256(abi.encodePacked(_b));
    }

    function run() external {
        uint256 deployerPrivateKey = vm.envUint("PRIVATE_KEY");
        vm.startBroadcast(deployerPrivateKey);

        console.logString("Deploying contract");
        BeaconRootsMock accounting_contract = new BeaconRootsMock();

        vm.stopBroadcast();
    }
}
