set dotenv-load := true
set dotenv-required := false

local_verify_proof:="false"
verify_contract:="false"

verify_contract_cmd:=if verify_contract == "true" { "--verify" } else { "" }
local_verify_cmd:=if local_verify_proof == "true" { "--local-verify" } else { "" }

build:
    cargo build --release

switch_env env:
    rm -f .env && ln -s envs/.env.network.{{env}} .env


# Running
submit target_slot previous_slot='0': build
    ./target/release/submit {{local_verify_cmd}} --target-ref-slot {{target_slot}} {{ if previous_slot != "0" { "--previous-ref-slot "+previous_slot } else { "" } }}

execute target_slot previous_slot='0': build
    ./target/release/execute {{local_verify_cmd}} --target-ref-slot {{target_slot}} {{ if previous_slot != "0" { "--previous-ref-slot "+previous_slot } else { "" } }}

run_service: build
    ./target/release/service

service_report_def:
    curl -X POST -d '{}' 127.0.0.1:8080/run-report

service_report target_slot='null' previous_slot='null':
    curl -X POST -d '{"previous_ref_slot": {{previous_slot}}, "target_ref_slot": {{target_slot}}}' 127.0.0.1:8080/run-report

service_stats:
    curl -X GET 127.0.0.1:8080/metrics

# Deploy
run_anvil:
    anvil --fork-url $FORK_URL

write_manifesto target_slot: build
    ./target/release/deploy --target-slot {{target_slot}} --store "data/deploy/${EVM_CHAIN}-deploy.json" --dry-run

### Contract interactions ###
# These implicitly depends on run_anvil, but we don't want to start anvil each time - it should be running
[working-directory: 'contracts']
deploy:
    forge script --chain $CHAIN_ID script/Deploy.s.sol:Deploy --rpc-url $EXECUTION_LAYER_RPC --broadcast {{verify_contract_cmd}}

read_last_report_slot:
    cast call $CONTRACT_ADDRESS "getLatestLidoValidatorStateSlot()(uint256)"

read_last_report:
    #!/usr/bin/env bash
    set -euxo pipefail
    target_slot=$(cast --json call $CONTRACT_ADDRESS "getLatestLidoValidatorStateSlot()(uint256)" | jq ".[0] | tonumber")
    cast call $CONTRACT_ADDRESS "getReport(uint256)(bool,uint256,uint256,uint256,uint256)" $target_slot

read_report target_slot:
    cast call $CONTRACT_ADDRESS "getReport(uint256)(bool,uint256,uint256,uint256,uint256)" "{{target_slot}}"
### Contract interactions ###

# Development
update_fixtures target_slot previous_slot='0': build
    ./target/release/write_test_fixture --target-ref-slot {{target_slot}} {{ if previous_slot != "0" { "--previous-ref-slot "+previous_slot } else { "" } }}

download_state target_slot format="ssz":
    curl -H {{ if format == "ssz" { "'Accept:application/octet-stream'" } else { "'Accept:application/json'" } }} ${CONSENSUS_LAYER_RPC}/eth/v2/debug/beacon/states/{{target_slot}} > temp/beacon_states/$EVM_CHAIN/bs_{{target_slot}}.{{ if format == "ssz" { "ssz" } else { "json" } }}

download_header target_slot:
    curl -H "Accept:application/json" ${CONSENSUS_LAYER_RPC}/eth/v1/beacon/headers/{{target_slot}} | jq ".data.header.message" > temp/beacon_states/$EVM_CHAIN/bs_{{target_slot}}_header.json

download_bs target_slot format="ssz": (download_state target_slot) (download_header target_slot)

add_test_bs target_slot format="ssz": (download_bs target_slot) (download_bs target_slot)
    cp temp/beacon_states/$EVM_CHAIN/bs_{{target_slot}}_header.json script/tests/data/beacon_states/bs_{{target_slot}}_header.json
    cp temp/beacon_states/$EVM_CHAIN/bs_{{target_slot}}.{{ if format == "ssz" { "ssz" } else { "json" } }} script/tests/data/beacon_states/bs_{{target_slot}}.{{ if format == "ssz" { "ssz" } else { "json" } }}

read_validators target_slot:
    curl $CONSENSUS_LAYER_RPC/eth/v1/beacon/states/{{target_slot}}/validators > temp/vals_bals/$EVM_CHAIN/validators_{{target_slot}}.json
    curl $CONSENSUS_LAYER_RPC/eth/v1/beacon/states/{{target_slot}}/validator_balances > temp/vals_bals/$EVM_CHAIN/balances_{{target_slot}}.json

[working-directory: 'contracts']
test_contracts:
    forge test

[working-directory: 'crates/shared']
test_shared:
    cargo test

[working-directory: 'crates/program']
test_program:
    cargo test

[working-directory: 'crates/script']
test_script:
    # building scripts starts multiple builds in parallel and often OOMs
    # -j 5 limits the concurrency for building (but not running) and avoids that
    # --test-threads 5 limits concurrecny for running tests (sometimes it gets excited and runs too
    # many in parallel, consuming all the memory and grinding to a halt)
    RUST_LOG=infor cargo test -j 5 -- --test-threads=5 --nocapture

[working-directory: 'crates/script']
integration_test:
    cargo test -j 5 --include-ignored

test: test_contracts test_shared test_program test_script

update_meta:
    cargo license --color never > metadata/deps_licenses.txt
    solidity-code-metrics contracts/src/*.sol  > metadata/solidity_report.md
    scc --by-file --format-multi "wide:metadata/audit_sloc.txt,json:metadata/audit_sloc.json" contracts/src crates/program/src/ crates/shared/src/lib crates/macros/**/src/
    scc --by-file --format-multi "wide:metadata/total_sloc.txt,json:metadata/total_sloc.json" ./