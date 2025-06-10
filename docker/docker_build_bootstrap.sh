#!/bin/bash

# Install SP1 to compile ZK program
curl -L https://sp1.succinct.xyz > ./install_sp1up.sh
chmod u+x ./install_sp1up.sh
./install_sp1up.sh
source /root/.bashrc
sp1up

# Install foundry to compile contracts
curl -L https://foundry.paradigm.xyz > ./install_foundryup.sh
chmod u+x ./install_foundryup.sh
./install_foundryup.sh
source /root/.bashrc
foundryup