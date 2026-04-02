#!/bin/bash

set -euo pipefail

: "${SP1_VERSION:?SP1_VERSION must be set (derived from sp1-zkvm version in Cargo.toml)}"
: "${FOUNDRY_VERSION:=1.4.4}"

# Install SP1 to compile ZK program
curl -fsSL https://sp1.succinct.xyz -o ./install_sp1up.sh
chmod u+x ./install_sp1up.sh
./install_sp1up.sh
source /root/.bashrc
sp1up --version "${SP1_VERSION}"

# Install foundry to compile contracts
curl -fsSL https://foundry.paradigm.xyz -o ./install_foundryup.sh
chmod u+x ./install_foundryup.sh
./install_foundryup.sh
source /root/.bashrc
foundryup --install "${FOUNDRY_VERSION}"
