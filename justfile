set dotenv-load := true
set dotenv-required := false

local_verify_proof:="false"
verify_contract:="false"

# need to limit number of concurrent compile and test threads to avoid OOM during build and execution
compile_threads:="8"
test_threads:="16"
tests_log_level:="info"

verify_contract_cmd:=if verify_contract == "true" { "--verify" } else { "" }
local_verify_cmd:=if local_verify_proof == "true" { "--local-verify" } else { "" }

build:
    cargo build --release --locked -j {{compile_threads}}

switch_env env:
    rm -f .env && ln -s envs/.env.network.{{env}} .env


# Running
submit target_slot previous_slot='0': build
    ./target/release/submit {{local_verify_cmd}} --target-ref-slot {{target_slot}} {{ if previous_slot != "0" { "--previous-ref-slot "+previous_slot } else { "" } }}

execute target_slot previous_slot='0': build
    ./target/release/execute {{local_verify_cmd}} --target-ref-slot {{target_slot}} {{ if previous_slot != "0" { "--previous-ref-slot "+previous_slot } else { "" } }}

service_run: build
    ./target/release/service

service_health:
    curl -X GET $SERVICE_BIND_TO_ADDR/health

service_report_def:
    curl -X POST -d '{}' $SERVICE_BIND_TO_ADDR/run-report

service_report target_slot='null' previous_slot='null':
    curl -X POST -H "Content-Type: application/json" -d '{"previous_ref_slot": {{previous_slot}}, "target_ref_slot": {{target_slot}}}' $SERVICE_BIND_TO_ADDR/run-report

service_read_report target_slot='null':
    curl -X GET $SERVICE_BIND_TO_ADDR/get-report?{{if target_slot!='null' { "target_slot="+target_slot } else { "" } }}

service_stats:
    curl -X GET $SERVICE_BIND_TO_ADDR/metrics

# Deploy
anvil_run block_time='0':
    anvil --fork-url $FORK_URL {{ if block_time != '0' { "--block-time "+block_time} else { "" } }}

write_manifesto target_slot: build
    ./target/release/deploy --target-slot {{target_slot}} --store "data/deploy/${EVM_CHAIN}-deploy.json" --dry-run

anvil_mine number='1':
    #!/usr/bin/env bash
    set -euxo pipefail
    for i in $(seq 1 {{number}}); do
        curl -X POST http://localhost:8545   -H "Content-Type: application/json" -d '{"jsonrpc":"2.0","method":"evm_mine","params":[],"id":0}'
    done

### Contract interactions ###
# These implicitly depends on run_anvil, but we don't want to start anvil each time - it should be running
[working-directory: 'contracts']
contract_deploy:
    forge script --chain $EVM_CHAIN_ID script/Deploy.s.sol:Deploy --rpc-url $EXECUTION_LAYER_RPC --broadcast {{verify_contract_cmd}}

[working-directory: 'test_contracts']
block_root_mock_deploy:
    forge script --chain $EVM_CHAIN_ID script/Deploy.s.sol:Deploy --rpc-url $EXECUTION_LAYER_RPC --broadcast

contract_read_last_report_slot:
    cast call $CONTRACT_ADDRESS "getLatestLidoValidatorStateSlot()(uint256)" --rpc-url $EXECUTION_LAYER_RPC

contract_read_last_report:
    #!/usr/bin/env bash
    set -euxo pipefail
    target_slot=$(cast --json call $CONTRACT_ADDRESS "getLatestLidoValidatorStateSlot()(uint256)"  --rpc-url $EXECUTION_LAYER_RPC| jq ".[0] | tonumber")
    cast call $CONTRACT_ADDRESS "getReport(uint256)(bool,uint256,uint256,uint256,uint256)" $target_slot --rpc-url $EXECUTION_LAYER_RPC

contract_read_report target_slot:
    cast call $CONTRACT_ADDRESS "getReport(uint256)(bool,uint256,uint256,uint256,uint256)" "{{target_slot}}" --rpc-url $EXECUTION_LAYER_RPC

contract_get_block_hash target_slot:
    cast call $CONTRACT_ADDRESS "getBeaconBlockHash(uint256 slot)" "{{target_slot}}" --rpc-url $EXECUTION_LAYER_RPC

block_root_mock_setup:
    #!/usr/bin/env bash
    set -euxo pipefail
    bytecode=$(cat test_contracts/out/BeaconRootsMock.sol/BeaconRootsMock.json | jq ".deployedBytecode.object")
    curl $EXECUTION_LAYER_RPC  -H "Content-Type: application/json" -d '{ "jsonrpc": "2.0", "method": "anvil_setCode", "params": ["0x000F3df6D732807Ef1319fB7B8bB8522d0Beac02", '${bytecode#0x}'], "id": 1 }'

block_root_mock_set_block_hash timestamp hash:
    cast send "0x000F3df6D732807Ef1319fB7B8bB8522d0Beac02" "setRoot(uint256 timestamp, bytes32 root)" "{{timestamp}}" "{{hash}}" --rpc-url $EXECUTION_LAYER_RPC --private-key $PRIVATE_KEY

block_root_mock_get_block_hash timestamp:
    cast call "0x000F3df6D732807Ef1319fB7B8bB8522d0Beac02" "beacon_block_hashes(uint256)(bytes32)" "{{timestamp}}" --rpc-url $EXECUTION_LAYER_RPC
    cast call "0x000F3df6D732807Ef1319fB7B8bB8522d0Beac02" $(cast abi-encode "f(uint256)" {{timestamp}}) --rpc-url $EXECUTION_LAYER_RPC
### Contract interactions ###

### Development ###
store_report target_slot previous_slot: build
    ./target/release/store_report --target-ref-slot {{target_slot}} --previous-ref-slot {{previous_slot}}

download_state target_slot format="ssz":
    curl -H {{ if format == "ssz" { "'Accept:application/octet-stream'" } else { "'Accept:application/json'" } }} ${CONSENSUS_LAYER_RPC}/eth/v2/debug/beacon/states/{{target_slot}} > temp/beacon_states/$EVM_CHAIN/bs_{{target_slot}}.{{ if format == "ssz" { "ssz" } else { "json" } }}

download_header target_slot:
    curl -H "Accept:application/json" ${CONSENSUS_LAYER_RPC}/eth/v1/beacon/headers/{{target_slot}} | jq ".data.header.message" > temp/beacon_states/$EVM_CHAIN/bs_{{target_slot}}_header.json

download_bs target_slot format="ssz": (download_state target_slot) (download_header target_slot)

add_test_bs target_slot format="ssz": (download_bs target_slot) (download_bs target_slot)
    cp temp/beacon_states/$EVM_CHAIN/bs_{{target_slot}}_header.json crates/script/tests/data/beacon_states/bs_{{target_slot}}_header.json
    cp temp/beacon_states/$EVM_CHAIN/bs_{{target_slot}}.{{ if format == "ssz" { "ssz" } else { "json" } }} crates/script/tests/data/beacon_states/bs_{{target_slot}}.{{ if format == "ssz" { "ssz" } else { "json" } }}

read_validators target_slot:
    curl $CONSENSUS_LAYER_RPC/eth/v1/beacon/states/{{target_slot}}/validators > temp/vals_bals/$EVM_CHAIN/validators_{{target_slot}}.json
    curl $CONSENSUS_LAYER_RPC/eth/v1/beacon/states/{{target_slot}}/validator_balances > temp/vals_bals/$EVM_CHAIN/balances_{{target_slot}}.json

### Testing ###
test_update_fixtures target_slot='0' previous_slot='0': build
    ./target/release/write_test_fixture {{ if target_slot != "0" { "--target-ref-slot "+target_slot } else { "" } }} {{ if previous_slot != "0" { "--previous-ref-slot "+previous_slot } else { "" } }}

    
[working-directory: 'contracts']
test_contracts:
    forge test

[working-directory: 'crates/shared']
test_shared:
    cargo test

[working-directory: 'crates/program']
test_program:
    cargo test

# building scripts starts multiple builds in parallel and often OOMs
# -j 5 limits the concurrency for building (but not running) and avoids that
# --test-threads 5 limits concurrency for running tests (sometimes it gets excited and runs too
# many in parallel, consuming all the memory and grinding to a halt)
[working-directory: 'crates/script']
test_script test_args="":
    SP1_SKIP_PROGRAM_BUILD=true RUST_LOG={{tests_log_level}} cargo test -j {{compile_threads}} --no-fail-fast -- --test-threads={{test_threads}} {{test_args}}

[working-directory: 'crates/script']
integration_test test_args="":
    SP1_SKIP_PROGRAM_BUILD=true RUST_LOG={{tests_log_level}} cargo test -j {{compile_threads}} --no-fail-fast -- --test-threads {{test_threads}} --include-ignored  {{test_args}} 2>&1 | tee test.log

test: test_contracts test_shared test_script


### Updating metadata ###
update_meta:
    cargo license --color never > metadata/deps_licenses.txt
    solidity-code-metrics contracts/src/*.sol  > metadata/solidity_report.md
    scc --by-file --format-multi "wide:metadata/audit_sloc.txt,json:metadata/audit_sloc.json" contracts/src crates/program/src/ crates/shared/src/lib crates/macros/**/src/
    scc --by-file --format-multi "wide:metadata/total_sloc.txt,json:metadata/total_sloc.json" ./


### Docker
docker_build *args:
    docker build -t lido_sp1_oracle . --platform linux/amd64 --build-arg VERGEN_GIT_SHA=$(git rev-parse HEAD) {{args}} --debug

docker_build_print_elf_sha: (docker_build "--build-arg PRINT_ELF_SHA=$(date +%s) --progress plain")

# network: host is to allow connecting to anvil when run locally
# Practically just docker-compose for lazy
docker_run:    
    docker run --env-file .env --platform linux/amd64 -p 8080:8080 -v $BS_FILE_STORE:/usr/data/sp1-lido-zk/$BS_FILE_STORE lido_sp1_oracle:latest

docker_shell:
    docker run --env-file .env --platform linux/amd64 -it --network host -v $BS_FILE_STORE:/usr/data/sp1-lido-zk/$BS_FILE_STORE --rm --entrypoint /bin/bash lido_sp1_oracle:latest 

docker_env:
    docker run --env-file .env -it --rm lido_sp1_oracle:latest env
