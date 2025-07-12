// SPDX-License-Identifier: MIT
pragma solidity 0.8.27;

import "forge-std/Script.sol";
import "forge-std/console.sol";
import {stdJson} from "forge-std/StdJson.sol";
import {Sp1LidoAccountingReportContract} from "../src/Sp1LidoAccountingReportContract.sol";

// forge script --chain $EVM_CHAIN script/Deploy.s.sol:Deploy --rpc-url $EXECUTION_LAYER_RPC --broadcast --verify
contract Deploy is Script {
    using stdJson for string;

    struct DeployManifesto {
        string network;
        address verifier;
        bytes32 vkey;
        bytes32 withdrawal_credentials;
        address withdrawal_vault_address;
        uint256 genesis_timestamp;
        Sp1LidoAccountingReportContract.LidoValidatorState initial_lido_validator_state;
        address owner;
    }

    function stringsEqual(string memory _a, string memory _b) public pure returns(bool) {
        return keccak256(abi.encodePacked(_a)) == keccak256(abi.encodePacked(_b));
    }

    function readManifesto(string memory network) public view returns (DeployManifesto memory) {
        string memory root = vm.projectRoot();
        string memory path = string.concat(root, "/../data/deploy/", network, "-deploy.json");
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
                json.readAddress(".withdrawal_vault_address"),
                json.readUint(".genesis_timestamp"),
                Sp1LidoAccountingReportContract.LidoValidatorState(
                    json.readUint(".initial_validator_state.slot"),
                    json.readBytes32(".initial_validator_state.merkle_root")
                ),
                json.readAddress(".admin")
        );

        // sanity check
        require(stringsEqual(manifesto.network, network), "Networks not equal");
        return manifesto;
    }

    function run() external {
        uint256 deployerPrivateKey = vm.envUint("PRIVATE_KEY");
        string memory network = vm.envString("EVM_CHAIN");

        vm.startBroadcast(deployerPrivateKey);

        DeployManifesto memory manifesto = readManifesto(network);

        console.logString("Deploying contract");
        Sp1LidoAccountingReportContract accounting_contract = new Sp1LidoAccountingReportContract(
            manifesto.verifier,
            manifesto.vkey,
            manifesto.withdrawal_credentials,
            manifesto.withdrawal_vault_address,
            manifesto.genesis_timestamp,
            manifesto.initial_lido_validator_state,
            manifesto.owner
        );

        vm.stopBroadcast();
    }
}
