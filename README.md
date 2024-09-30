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

* SP1_PROVER - could be `network` or `local`. Local proover requires 128+Gb memory and is not tested.
* SP1_PRIVATE_KEY - private key, granting access to the Succinct prover network. See [setup instructions][sp1-key-instructions]
* BEACON_STATE_RPC - the oracle needs access to full BeaconState at the `target_slot` to prepare the 
  report and proof. BEACON_STATE_RPC

[sp1-key-instructions]: https://docs.succinct.xyz/prover-network/setup.html#key-setup

Examples:

* Run oracle for slot `5994112`, store report and proof, submit to EVM contract: `submit --target-slot 5994112 --store`
* Run oracle for slot `5994112`, locally verify the proof and public values (will crash if verification fails), submit to EVM contract: `submit --target-slot 5994112 --local_verify`
* Run oracle for slot `5994112`, use `5993824` as a previous report slot, submit to EVM contract: `submit --target-slot 5994112`


## Development

### Requirements

- [Rust](https://rustup.rs/)
- [SP1](https://succinctlabs.github.io/sp1/getting-started/install.html)
- [Foundry](https://book.getfoundry.sh/getting-started/installation)
- [Docker](https://docs.docker.com/get-started/get-docker/) - optional, for verifying proofs locally

TBD, but nothing unexpected: `cargo test` to run tests, `cargo run` to run binaries.