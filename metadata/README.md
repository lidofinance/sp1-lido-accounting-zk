# Methodology 

* Used [scc][scc] to compute LOC, SLOC, etc. metrics for Rust and [solidity-metrics][solidity-metrics] for Solidity.

[scc]: https://github.com/boyter/scc
[solidity-metrics]: https://github.com/ConsenSys/solidity-metrics

# Scope
* For Solidity code, see [solidity-report.md](solidity-report.md)
* For Rust code, see [audit_sloc.txt](audit_sloc.txt) (covers solidity too, for convenience only; `solidity-report` 
is the source of truth; difference is negligible anyway)

# What's not included and why

`audit_sloc.txt` covers all code parts subject to the audit, but not **all code**; specifically:

* Tests - for obvious reasons
* `contract/scripts/Deploy.s.sol` - deployment script
* Everyithng in `script`,  `service` - the main requirement for this solution is to operate correctly even if offchain part 
is executed by a compromised/maliciious actor - i.e. the solution correctness should not depend on the offchain part.
* Everything in `dev_scripts` - these are development scripts aiming to automate some development and debugging work.