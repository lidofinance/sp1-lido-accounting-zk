#!/bin/bash
cargo license --color never > metadata/deps_licenses.txt
solidity-code-metrics contracts/src/*.sol  > metadata/solidity_report.md
scc --by-file --format-multi "wide:metadata/audit_sloc.txt,json:metadata/audit_sloc.json" contracts/src program/src/ shared/src macros/**/src/
scc --by-file --format-multi "wide:metadata/total_sloc.txt,json:metadata/total_sloc.json" ./