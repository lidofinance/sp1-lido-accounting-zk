// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Script.sol";
import "forge-std/console.sol";
import {stdJson} from "forge-std/StdJson.sol";
import {Sp1LidoAccountingReportContract, LidoValidatorState} from "../src/Sp1LidoAccountingReportContract.sol";

contract Deploy is Script {
    using stdJson for string;

    struct DeployManifesto {
        string network;
        address verifier;
        bytes32 vkey;
        bytes32 withdrawal_credentials;
        uint256 genesis_timestamp;
        LidoValidatorState initial_lido_validator_state;
    }

    function stringsEqual(string memory _a, string memory _b) public pure returns(bool) {
        return keccak256(abi.encodePacked(_a)) == keccak256(abi.encodePacked(_b));
    }

    function readManifesto(string memory network) public view returns (DeployManifesto memory) {
        string memory root = vm.projectRoot();
        string memory path = string.concat(root, "/script/deploy_manifesto_", network, ".json");
        console.logString(string.concat("Reading manifesto from file", path));
        string memory json = vm.readFile(path);
        // bytes memory jsonBytes = json.parseRaw(".");
        // This should be
        // return abi.decode(jsonBytes, (DeployManifesto));
        // ... but it reverts with no explanation - so just doing it manually
        DeployManifesto memory manifesto = DeployManifesto(
                json.readString(".network"),
                json.readAddress(".verifier"),
                json.readBytes32(".vkey"),
                json.readBytes32(".withdrawal_credentials"),
                json.readUint(".genesis_timestamp"),
                LidoValidatorState(
                    json.readUint(".initial_validator_state.slot"),
                    json.readBytes32(".initial_validator_state.merkle_root")
                )
        );

        // sanity check
        require(stringsEqual(manifesto.network, network));
        return manifesto;
    }

    function run() external {
        uint256 deployerPrivateKey = vm.envUint("PRIVATE_KEY");
        string memory network = vm.envString("EVM_CHAIN");

        vm.startBroadcast(deployerPrivateKey);

        DeployManifesto memory manifesto = readManifesto(network);

        Sp1LidoAccountingReportContract accounting_contract = new Sp1LidoAccountingReportContract(
            manifesto.verifier,
            manifesto.vkey,
            manifesto.withdrawal_credentials,
            manifesto.genesis_timestamp,
            manifesto.initial_lido_validator_state
        );

        vm.stopBroadcast();
    }
}