import os
import json
import itertools
import random
from typing import Generator, Optional

import click
import pathlib
import enum


import ssz

from eth_ssz_utils import (
    make_validator_simple,
    make_beacon_block_state,
    Constants,
)
from eth_consensus_layer import BeaconState, BeaconBlockHeader, JustificationBits
import constants
from report import Report, WithdrawalCreds

THIS_FOLDER = os.path.dirname(__file__)
PROJECT_ROOT = os.path.dirname(THIS_FOLDER)

class BalanceMode(enum.Enum):
    RANDOM = "random"
    SEQUENTIAL = "sequential"
    FIXED = "fixed"

class BytesHexEncoder(json.JSONEncoder):
    def default(self, o):
        if isinstance(o, bytes):
            return o.hex()
        else:
            return super().default(o)
        

def generate_validators_and_balances(
    slot: int,
    epoch: int,
    non_lido_validators: int,
    deposited_lido_validators: int,
    exited_lido_validators: int,
    future_deposit_lido_validators: int,
    balance_generator: Generator[int, None, None],
    shuffle: bool = False
):
    deposited = [
        make_validator_simple(
            epoch, withdrawal_credentials = WithdrawalCreds.Lido, 
            deposited = True,  active = True, exited = False, pubkey=idx.to_bytes(10, signed=False) + b"\x01" * 38
        )
        for idx in range(deposited_lido_validators)
    ]
    exited = [
        make_validator_simple(
            epoch, withdrawal_credentials = WithdrawalCreds.Lido, 
            deposited = True,  active = True, exited = True, pubkey=idx.to_bytes(10, signed=False) + b"\x01" * 38
        )
        for idx in range(exited_lido_validators)
    ]
    future = [
        make_validator_simple(
            epoch, withdrawal_credentials=WithdrawalCreds.Lido,
            deposited=False, active=False, exited=False, pubkey=idx.to_bytes(10, signed=False) + b"\x01" * 38
        )
        for idx in range(future_deposit_lido_validators)
    ]
    other_validators = [
        make_validator_simple(
            epoch,
            withdrawal_credentials = WithdrawalCreds.Other, 
            deposited = True, 
            active = True,
            exited = False,
            pubkey=b"\x01" * 48
        )
        for i in range(non_lido_validators)
    ]
    validators = deposited + exited + future + other_validators
    balances = [
        next(balance_generator) if not validator.exited(epoch) and validator.deposited(epoch) else 0
        for validator in validators
    ]

    if shuffle:
        vals_and_bals = list(zip(validators, balances))
        random.shuffle(vals_and_bals)
        validators, balances = zip(*vals_and_bals)

    return validators, balances


def create_beacon_state(
    slot: int,
    epoch: int,
    total_validators: int,
    lido_validators: int,
    balance_generator: Generator[int, None, None],
    shuffle: bool = False
) -> BeaconState:
    validators, balances = generate_validators_and_balances(
        slot,
        epoch,
        total_validators,
        lido_validators,
        balance_generator,
        shuffle
    )
        
    return make_beacon_block_state(
        slot, epoch, Constants.Genesis.BLOCK_ROOT, validators, balances
    )

def append_to_existing(
    old_bs: BeaconState,
    slot: int,
    epoch: int,
    total_validators: int,
    lido_validators: int,
    balance_generator: Generator[int, None, None],
    shuffle: bool = False
) -> BeaconState:
    validators, balances = generate_validators_and_balances(
        slot,
        epoch,
        total_validators,
        lido_validators,
        balance_generator,
        shuffle
    )
    return make_beacon_block_state(
        slot, epoch, old_bs.hash_tree_root, old_bs.validators + validators, old_bs.balances + balances
    )


def create_beacon_block_header(
    slot: int,
    beacon_state_hash: bytes
):
    return BeaconBlockHeader.create(
        slot = slot,
        proposer_index = slot+42,
        parent_root = b"\x01\x02"*16,
        state_root = beacon_state_hash,
        body_root = b"\xFC\xFD\xFE\xFF"*8,
    )

GWEI_IN_1_ETH = 10**9
MILLIETH = 10**6
# FIXED_BALANCE = 32 * GWEI_IN_1_ETH
FIXED_BALANCE = 0

BALANCE_MODES = [mode.value for mode in BalanceMode]

def read_bs(file: pathlib.Path) -> BeaconState: 
    with open(file, "rb") as target:
        return ssz.decode(target.read(), BeaconState)

@click.command()
@click.option(
    "-f",
    "--file",
    required=True,
    type=click.Path(
        writable=True, file_okay=True, dir_okay=True, path_type=pathlib.Path
    ),
    default=pathlib.Path(PROJECT_ROOT) / "temp/beacon_block_state.ssz",
)
@click.option(
    "-nl", "--non_lido_validators", type=int, default=2**10, help="Number of non-Lido validators"
)
@click.option(
    "-d",
    "--deposited_lido_validators",
    type=int,
    default=2**5,
    help="Number of deposited Lido validators",
)
@click.option(
    "-e",
    "--exited_lido_validators",
    type=int,
    default=2**3,
    help="Number of exited Lido validators",
)
@click.option(
    "-fd",
    "--future_deposit_lido_validators",
    type=int,
    default=2**2,
    help="Number of created, but not deposited Lido validators",
)
@click.option(
    "-b",
    "--balances_mode",
    type=click.Choice(BALANCE_MODES),
    help="Balance generation mode",
    default=BalanceMode.SEQUENTIAL,
)
@click.option("-s", "--slot", type=int, default=123456, help="Slot number")
@click.option("--check", is_flag=True, default=False)
@click.option("--shuffle", is_flag=True, default=False)
@click.option(
    "--start_from",
    required=False,
    type=click.Path(
        writable=True, file_okay=True, dir_okay=True, path_type=pathlib.Path
    ),
    default=None,
)
def main(
    file: pathlib.Path,
    non_lido_validators: int,
    deposited_lido_validators: int,
    exited_lido_validators: int,
    future_deposit_lido_validators: int,
    balances_mode: str,
    slot: int,
    check: bool = True,
    shuffle: bool = False,
    start_from: Optional[pathlib.Path] = None
):
    # python main.py -nl 64 -d 26 -e 2 -fd 4 -b fixed -s 1000
    mode = BalanceMode(balances_mode)

    if mode == BalanceMode.FIXED:
        balance_gen = itertools.repeat(FIXED_BALANCE)
    elif mode == BalanceMode.SEQUENTIAL:
        balance_gen = itertools.count(1 * GWEI_IN_1_ETH, MILLIETH)
    elif mode == BalanceMode.RANDOM:
        balance_gen = (
            random.randint(1, 100) * GWEI_IN_1_ETH for _ in itertools.repeat(0)
        )

    epoch = slot // constants.SLOTS_PER_EPOCH

    validators_list, balances_list = generate_validators_and_balances(
        slot, epoch, non_lido_validators,
        deposited_lido_validators,
        exited_lido_validators,
        future_deposit_lido_validators, 
        balance_gen, shuffle
    )

    if start_from is not None:
        old_bs = read_bs(start_from)
        beacon_state = make_beacon_block_state(
            slot, epoch, Constants.Genesis.BLOCK_ROOT, 
            old_bs.validators + validators_list, 
            old_bs.balances + balances_list
        )
    else:
        beacon_state = make_beacon_block_state(
            slot, epoch, Constants.Genesis.BLOCK_ROOT, validators_list, balances_list
        )

    beacon_state_hash = ssz.get_hash_tree_root(beacon_state)
    balances_hash = ssz.get_hash_tree_root(beacon_state.balances)

    beacon_block_header = create_beacon_block_header(slot, beacon_state_hash)
    report = Report.create(beacon_state, beacon_block_header)
    
    print(f"Beacon State hash: {report.beacon_block_hash.hex()}")
    print(f"Balances hash: {balances_hash.hex()}")
    print(f"Expected report: {report}")

    manifesto = {
        "report": report.to_dict(),
        ""
        "beacon_block_header": {
            "hash": report.beacon_block_hash.hex(),
            "parts": {
                "slot": ssz.get_hash_tree_root(beacon_block_header.slot, ssz.sedes.uint.uint64),
                "proposer_index": ssz.get_hash_tree_root(beacon_block_header.proposer_index, ssz.sedes.uint.uint64),
                "parent_root": ssz.get_hash_tree_root(beacon_block_header.parent_root, ssz.sedes.byte_vector.bytes32),
                "state_root": ssz.get_hash_tree_root(beacon_block_header.state_root, ssz.sedes.byte_vector.bytes32),
                "body_root": ssz.get_hash_tree_root(beacon_block_header.body_root, ssz.sedes.byte_vector.bytes32),
            }
        },
        "beacon_state": {
            "hash": report.beacon_state_hash.hex(),
            "parts": {
                "genesis_time": ssz.get_hash_tree_root(beacon_state.genesis_time, ssz.sedes.uint.uint64),
                "genesis_validators_root": ssz.get_hash_tree_root(beacon_state.genesis_validators_root, ssz.sedes.byte_vector.bytes32),
                "slot": ssz.get_hash_tree_root(beacon_state.slot, ssz.sedes.uint.uint64),
                "fork": ssz.get_hash_tree_root(beacon_state.fork),
                "latest_block_header": ssz.get_hash_tree_root(beacon_state.latest_block_header),
                "block_roots": ssz.get_hash_tree_root(beacon_state.block_roots),
                "state_roots": ssz.get_hash_tree_root(beacon_state.state_roots),
                "historical_roots": ssz.get_hash_tree_root(beacon_state.historical_roots),
                "eth1_data": ssz.get_hash_tree_root(beacon_state.eth1_data),
                "eth1_data_votes": ssz.get_hash_tree_root(beacon_state.eth1_data_votes),
                "eth1_deposit_index": ssz.get_hash_tree_root(beacon_state.eth1_deposit_index, ssz.sedes.uint.uint64),
                "validators": ssz.get_hash_tree_root(beacon_state.validators),
                "balances": ssz.get_hash_tree_root(beacon_state.balances),
                "randao_mixes": ssz.get_hash_tree_root(beacon_state.randao_mixes),
                "slashings": ssz.get_hash_tree_root(beacon_state.slashings),
                "previous_epoch_participation": ssz.get_hash_tree_root(beacon_state.previous_epoch_participation),
                "current_epoch_participation": ssz.get_hash_tree_root(beacon_state.current_epoch_participation),
                "justification_bits": ssz.get_hash_tree_root(beacon_state.justification_bits, JustificationBits),
                "previous_justified_checkpoint": ssz.get_hash_tree_root(beacon_state.previous_justified_checkpoint),
                "current_justified_checkpoint": ssz.get_hash_tree_root(beacon_state.current_justified_checkpoint),
                "finalized_checkpoint": ssz.get_hash_tree_root(beacon_state.finalized_checkpoint),
                "inactivity_scores": ssz.get_hash_tree_root(beacon_state.inactivity_scores),
                "current_sync_committee": ssz.get_hash_tree_root(beacon_state.current_sync_committee),
                "next_sync_committee": ssz.get_hash_tree_root(beacon_state.next_sync_committee),
                "latest_execution_payload_header": ssz.get_hash_tree_root(beacon_state.latest_execution_payload_header),
                "next_withdrawal_index": ssz.get_hash_tree_root(beacon_state.next_withdrawal_index, ssz.sedes.uint.uint64),
                "next_withdrawal_validator_index": ssz.get_hash_tree_root(beacon_state.next_withdrawal_validator_index, ssz.sedes.uint.uint64),
                "historical_summaries": ssz.get_hash_tree_root(beacon_state.historical_summaries),
            }
        }
    }

    file.parent.mkdir(parents=True, exist_ok=True)
    file.write_bytes(ssz.encode(beacon_state))
    beacon_header_file = file.with_stem(f"{file.stem}_header").with_suffix(".json")
    with open(beacon_header_file, "w") as header_fp:
        json.dump(beacon_block_header.to_json(), header_fp, cls=BytesHexEncoder, indent=2)

    manifesto_file = file.with_name(f"{file.stem}_manifesto.json")
    with open(manifesto_file, "w") as manifesto_fp:
        json.dump(manifesto, manifesto_fp, cls=BytesHexEncoder, indent=2)

    if check:
        reread_beacon_state = read_bs(file)

        assert reread_beacon_state.slot == beacon_state.slot
        reread_beacon_state_hash = ssz.get_hash_tree_root(reread_beacon_state)
        assert beacon_state_hash == reread_beacon_state_hash


if __name__ == "__main__":
    main()
