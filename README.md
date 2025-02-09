# SP1 Lido Accounting Oracle

This is an implementation of LIDO [LIP-23][lip-23] sanity check oracle, using Succinct [SP1][sp1]. 
Repository contains the following:

* [program](program) - ZK circuit implementation
* [script](script) - offchain oracle implementation (`submit.rs`) + some development scripts (scripts/src/dev)
* [contracts](contracts) - on-chain, LIP-23 compatible contract.
* [shared](shared) - common code shared between ZK (program) and oracle (script).

[lip-23]: https://github.com/lidofinance/lido-improvement-proposals/blob/develop/LIPS/lip-23.md
[sp1]: https://github.com/succinctlabs

## Running oracle

The oracle comes in a form of an executable (`submit`) that takes the following inputs:

* (REQUIRED) target_slot: int - slot number for report
* (OPTIONAL) previous_slot:int - slot number for the previos report nad cached validator state. If omitted, read from the contract `getLatestValidatorStateSlot`.
* (OPTIONAL) store: bool - if set, stores the report and proof on disk. Default: false. Note: stored reports can
be submitted via `submit_cached` without re-generating the proof on prover network. Requires `PROOF_CACHE_DIR` env var to be set.
* (OPTIONAL) local_verify: bool - if set, stores the report and proof on disk. Default: false. **Note:** local proving requires docker to run.

Rest of the configuration is delivered via environment variables, listed and documented in the [.env.example](.env.example), but to highligh a few important ones:

* SP1_PROVER - could be `network` or `cpu`. CPU proover requires 128+Gb memory and is not tested.
* NETWORK_PRIVATE_KEY - private key, granting access to the Succinct prover network. See [setup instructions][sp1-key-instructions]
* BEACON_STATE_RPC - the oracle needs access to full BeaconState at the `target_slot` to prepare the 
  report and proof. BEACON_STATE_RPC

[sp1-key-instructions]: https://docs.succinct.xyz/prover-network/setup.html#key-setup

Examples:

* Run oracle for slot `5994112`, store report and proof, submit to EVM contract: `submit --target-slot 5994112 --store-proof`
* Run oracle for slot `5994112`, locally verify the proof and public values (will crash if verification fails), submit to EVM contract: `submit --target-slot 5994112 --local_verify`
* Run oracle for slot `5994112`, use `5993824` as a previous report slot, submit to EVM contract: `submit --target-slot 5994112`


## Development

### Requirements

- [Rust](https://rustup.rs/)
- [SP1](https://succinctlabs.github.io/sp1/getting-started/install.html)
- [Foundry](https://book.getfoundry.sh/getting-started/installation)
- [Docker](https://docs.docker.com/get-started/get-docker/) - optional, for verifying proofs locally

### Running tests

`cargo test` runs all unit (in `cfg(test)` blocks) and integration (in `tests` folders) tests, except:
* integration tests that take very long time to run (minutes)
* end-to-end tests that interact with the SP1 prover network (and hence incur real-life costs)

To run those, use `cargo test -- --include-ignored` - note it will use Sepolia testnet and SP1 prover network. As such,
env variables needed to access those (`CONSENSUS_LAYER_RPC`, `BEACON_STATE_RPC`, `NETWORK_PRIVATE_KEY`, etc.) need to be
set for those tests to work.

### Development scripts

`script/src/dev` hosts a few scripts to support development and deployment workflows. 

* `execute.rs` - prepares the input and runs ZK circuit simulation, outputting number of cycles and instructions used.
Does **not** interact with the prover network, safe to run to quickly check changes and estimate cycle count.
* `write_test_fixture.rs` - updates the test fixtures used in scripts integration tests and contract tests. Needs to 
be run when `vkey` changes (basically, any code or dependency change in `program` and `shared`).
* `deploy.rs` - have two orthogonal features: (1) write deploy manifesto (`--store`) and (2) deploy contract (`--dry-run`).
**Note:** deployment works, but doesn't perform code verfication yet. Preferred deployment workflow is to run `deploy.rs`
with `--dry-run --store "../data/deploy/${EVM_CHAIN}-deploy.json"` - and then run deployment script in `contracts/script/Deploy.s.sol`
that automatically picks the deploy manifesto from that location.
* `sumbit_cached.rs` - intended to be used with `submit.rs` script run with `--store-proof` flag. Allows submitting a cached proof
and report to the verification contract - skipping on the (most time-consuming) proof generation stage.

`script/examples` folder contains a number of standalone scripts that exercise various parts of the solution -
technically they are not examples, but ad-hoc tests used during early stages of development. They are provided "as is"
(could be still useful in future + have some development workflows tied to some them - e.g. `gen_synthetic_bs_pair.rs`).
**Note:** `cargo build` doesn't even compile the examples by default, so they might not always be operational.