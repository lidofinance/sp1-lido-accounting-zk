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
    if [ {{previous_slot}} -ne '0']; then \
        ./target/release/submit {{local_verify_cmd}} --target-ref-slot {{target_slot}} --previous_slot {{previous_slot}};\
    else \
        ./target/release/submit {{local_verify_cmd}} --target-ref-slot {{target_slot}};\
    fi

# Deploy
start_anvil:
    RUST_LOG=info anvil --fork-url $FORK_URL

write_manifesto target_slot: build
    ./target/release/deploy --target-slot {{target_slot}} --store x"../data/deploy/${EVM_CHAIN}-deploy.json" --dry-run

deploy:
    forge script --chain $CHAIN_ID script/Deploy.s.sol:Deploy --rpc-url $EXECUTION_LAYER_RPC --broadcast {{verify_contract_cmd}}

# Development
update_fixtures target_slot previous_slot='0': build
    if [ {{previous_slot}} -ne '0']; then \
        ./target/release/write_test_fixture --target-ref-slot {{target_slot}} --previous-ref-slot {{previous_slot}};\
    else \
        ./target/release/write_test_fixture --target-ref-slot {{target_slot}};\
    fi

[working-directory: 'contracts']
test_contracts:
    forge test

[working-directory: 'shared']
test_shared:
    cargo test

[working-directory: 'program']
test_program:
    cargo test

[working-directory: 'script']
test_script:
    # building scripts starts multiple builds in parallel and often OOMs
    # -j 5 limits the concurrency for building (but not running) and avoids that
    cargo test -j 5

[working-directory: 'script']
integration_test:
    cargo test -j 5 --include-ignored

test: test_contracts test_shared test_program test_script

update_meta:
    cargo license --color never > metadata/deps_licenses.txt
    solidity-code-metrics contracts/src/*.sol  > metadata/solidity_report.md
    scc --by-file --format-multi "wide:metadata/audit_sloc.txt,json:metadata/audit_sloc.json" contracts/src program/src/ shared/src/lib macros/**/src/
    scc --by-file --format-multi "wide:metadata/total_sloc.txt,json:metadata/total_sloc.json" ./