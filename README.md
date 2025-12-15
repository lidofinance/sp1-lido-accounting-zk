# SP1 Lido Accounting Oracle

This is an implementation of LIDO [LIP-23][lip-23] sanity check oracle, using Succinct [SP1][sp1]. 
Repository contains the following:

* [contracts](contracts) - on-chain, LIP-23 compatible contract.
* [program](crates/program) - ZK circuit implementation
* [script](crates/script) - offchain oracle implementation (`submit.rs`)
* [service](crates/service) - offchain oracle implementation (`submit.rs`) + some development scripts (scripts/src/dev)
* [shared](crates/shared) - common code shared between ZK (program) and oracle (script).
* [dev_script](crates/dev_script/) - development scripts
* [macros](crates/macros/) - macros implementations

[lip-23]: https://github.com/lidofinance/lido-improvement-proposals/blob/develop/LIPS/lip-23.md
[sp1]: https://github.com/succinctlabs

## Running oracle

The oracle comes in two forms:
* CLI interface (`submit`)
* Service (`service`) with internal scheduler (optional, controlled by env vars) and HTTP endpoint to trigger running a report

Most of the configuration is shared between the two (in fact, both are just thin wrappers around common underlying logic) and
delivered via env vars - see [.env.example](.env.example) (and comments in it) for the list of required settings.

## CLI interface
* (OPTIONAL) target_ref_slot: int - slot number for report; if not set, determined from Lido's HashConsensus contract for Accounting Oracle
* (OPTIONAL) previous_ref_slot:int - slot number for the previous report and cached validator state. If omitted, read from the contract `getLatestValidatorStateSlot`.
* (OPTIONAL) dry_run: bool - if set, prepares the input for proving, but do not request the proof (and hence do not sumbit the report for on-chain verification). Default: false
* (OPTIONAL) verify_input: bool - if set, verifies the input for consistency/correctness. Default: false.
* (OPTIONAL) verify_proof: bool - if set, verifies the proof locally. Default: false. **Note:** local proving requires docker to run.
* (OPTIONAL) report_cycles: bool - if set, measures the SP1 cycles require to generate the proof - by locally "executing" (in SP1 terms) the program. Default: false.

**Examples:**

* Run oracle for slot `5994112`, submit to EVM contract: `submit --target-ref-slot 5994112`
* Run oracle for slot `5994112`, locally verify the proof and public values (will crash if verification fails), submit to EVM contract: `submit --target-ref-slot 5994112 --local_verify`
* Run oracle for slot `5994112`, use `5993824` as a previous report slot, submit to EVM contract: `submit --target-ref-slot 5994112 --previous-ref-slot 5993824`

### Service API endpoints

* GET `/health` - used for healthcheck, just returns 'ok' when healthy
* GET `/metrics` - returns Prometheus metrics
* GET `/get-report?target-slot=$d` - reads report for a given slot from the contract. If `target-slot` omitted, obtains the latest report from the contract
* POST `/run-report` - reads report for a given slot from the contract. If `target-slot` omitted, obtains the latest report from the contract


## Development

**NOTE:** This project uses gitmodules. Please make sure to clone with `--recurse-submodules` and/or 
`git submodules update --init --recursive`, and/or any other means to make sure `contracts/lib/sp1-contracts` exists
and is not empty. Otherwise, building contracts (`forge build`, also run by cargo build via `crates/script/build.rs`)
will not produce the `contracts/out`, which in turn will fail generating code for accessing the contract (`eth_client.rs`).

### Requirements

- [Rust](https://rustup.rs/)
- [SP1](https://succinctlabs.github.io/sp1/getting-started/install.html)
- [Foundry](https://book.getfoundry.sh/getting-started/installation)
- [Docker](https://docs.docker.com/get-started/get-docker/) - optional, for verifying proofs locally

### Running tests

#### Local Testing

`cargo test` runs all unit (in `cfg(test)` blocks) and integration (in `tests` folders) tests, except:
* integration tests that take very long time to run (minutes)
* end-to-end tests that interact with the SP1 prover network (and hence incur real-life costs)

To run those, use `cargo test -- --include-ignored` - note it will use Sepolia testnet and SP1 prover network. As such,
env variables needed to access those (`CONSENSUS_LAYER_RPC`, `BEACON_STATE_RPC`, `NETWORK_PRIVATE_KEY`, etc.) need to be
set for those tests to work.

#### Docker Testing (Recommended for Reproducibility)

For platform-independent, reproducible testing:

```bash
# Run all tests in Docker
just docker_test

# Run integration tests
just docker_integration_test

# Generate fixtures (ensures consistency)
# Note: Skips local verification in Docker to avoid Docker-in-Docker issues
just docker_generate_fixtures

# Generate fixtures locally (with verification)
just test_update_fixtures

# Open interactive shell for debugging
just docker_test_shell
```

**Why Docker testing?**
- **Reproducible ELF builds**: Ensures consistent binary generation across different platforms (macOS, Linux, Windows)
- **Platform independence**: Tests run identically regardless of host architecture
- **Isolated environment**: No interference from local toolchain versions
- **CI/CD ready**: Same environment used in continuous integration

See [docs/DOCKER_TESTING.md](docs/DOCKER_TESTING.md) for detailed documentation.

### Environment Variables

Key environment variables for development:

- **`SP1_SKIP_LOCAL_PROOF_VERIFICATION`** (default: `false`): Skip local proof verification. Set to `true` when running in Docker to avoid Docker-in-Docker issues with SP1's local verification. Automatically set when using `just docker_generate_fixtures`.
  
  ```bash
  # Skip verification (useful in Docker)
  SP1_SKIP_LOCAL_PROOF_VERIFICATION=true just test_update_fixtures
  
  # Normal verification (default, for local use)
  just test_update_fixtures
  ```

See `.env.example` for all available environment variables.

### Development scripts

`crates/dev_scripts/src/bin` hosts a few scripts to support development and deployment workflows. 

* `execute.rs` - prepares the input and runs ZK circuit simulation, outputting number of cycles and instructions used.
Does **not** interact with the prover network, safe to run to quickly check changes and estimate cycle count.
* `write_test_fixture.rs` - updates the test fixtures used in scripts integration tests and contract tests. Needs to 
be run when `vkey` changes (basically, any code or dependency change in `program` and `shared`).
* `deploy.rs` - have two orthogonal features: (1) write deploy manifesto (`--store`) and (2) deploy contract (`--dry-run`).
**Note:** deployment works, but doesn't perform code verfication yet. Preferred deployment workflow is to run `deploy.rs`
with `--dry-run --store "../data/deploy/${EVM_CHAIN}-deploy.json"` - and then run deployment script in `contracts/script/Deploy.s.sol`
that automatically picks the deploy manifesto from that location.
* `store_report.rs` - generates report and stores it on disk for inspection and debugging.
* `sumbit_cached.rs` - intended to be used with `store_report.rs` script. Allows submitting a cached proof
and report to the verification contract - skipping on the (most time-consuming) proof generation stage.
