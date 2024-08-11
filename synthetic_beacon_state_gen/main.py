import os
import json
import itertools
import random
from typing import Generator

import click
import dataclasses
import dataclasses_json
import pathlib
import enum


import ssz
from hexbytes import HexBytes

from eth_ssz_utils import (
    make_validator,
    make_beacon_block_state,
    Constants,
)
from eth_consensus_layer import BeaconState, BeaconBlockHeader, JustificationBits
import constants

THIS_FOLDER = os.path.dirname(__file__)
PROJECT_ROOT = os.path.dirname(THIS_FOLDER)


class WithdrawalCreds:
    Lido = HexBytes("010000000000000000000000b9d7934878b5fb9610b3fe8a5e441e8fad7e293f")
    Other = HexBytes(b"\01" * 32)


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


def create_beacon_state(
    slot: int,
    epoch: int,
    total_validators: int,
    lido_validators: int,
    balance_generator: Generator[int, None, None],
) -> BeaconState:
    assert lido_validators <= total_validators
# withdrawal_credentials: bytes, activation_eligibility_epoch: int, activation_epoch: int,
        # exit_epoch
    balances = list(itertools.islice(balance_generator, total_validators))
    validators = [
        make_validator(
            withdrawal_credentials = WithdrawalCreds.Lido, 
            activation_eligibility_epoch = epoch, 
            activation_epoch = epoch + 1, 
            exit_epoch=None, 
            pubkey=b"\x01" * 48
        )
        for i in range(lido_validators)
    ] + [
        make_validator(
            withdrawal_credentials = WithdrawalCreds.Other, 
            activation_eligibility_epoch = epoch, 
            activation_epoch = epoch + 1, 
            exit_epoch=None, 
            pubkey=b"\x01" * 48
        )
        for i in range(lido_validators, total_validators)
    ]

    lido_sum = sum(balances[:lido_validators])

    return make_beacon_block_state(
        slot, epoch, Constants.Genesis.BLOCK_ROOT, validators, balances
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
FIXED_BALANCE = 16 * GWEI_IN_1_ETH


@dataclasses_json.dataclass_json
@dataclasses.dataclass
class Report:
    slot: int
    epoch: int
    beacon_block_hash: bytes
    beacon_state_hash: bytes
    total_balance: int
    lido_cl_balance: int
    total_validators: int
    lido_validators: int
    lido_exited_validators: int
    lido_withdrawal_credentials: bytes

    @classmethod
    def create(cls, epoch: int, beacon_state: BeaconState, beacon_block_header: BeaconBlockHeader) -> "Report":
        total_balance, lido_cl_balance, validators, exited_validators = 0, 0, 0, 0

        beacon_state_slot = beacon_state.slot // constants.SLOTS_PER_EPOCH

        for validator, balance in zip(beacon_state.validators, beacon_state.balances):
            total_balance += balance
            if validator.withdrawal_credentials == WithdrawalCreds.Lido:
                validators += 1
                lido_cl_balance += balance
                if validator.exit_epoch <= beacon_state_slot:
                    exited_validators += 1

        beacon_state_hash = HexBytes(ssz.get_hash_tree_root(beacon_state))
        beacon_block_hash = HexBytes(ssz.get_hash_tree_root(beacon_block_header))

        return cls(
            beacon_state.slot,
            epoch,
            beacon_block_hash,
            beacon_state_hash,
            total_balance,
            lido_cl_balance,
            len(beacon_state.validators),
            validators,
            exited_validators,
            WithdrawalCreds.Lido,
        )
    

BALANCE_MODES = [mode.value for mode in BalanceMode]

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
    "-v", "--validators", type=int, default=2**10, help="Total number of validators"
)
@click.option(
    "-l",
    "--lido_validators",
    type=int,
    default=2**5,
    help="Total number of Lido validators",
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
def main(
    file: pathlib.Path,
    validators: int,
    lido_validators: int,
    balances_mode: str,
    slot: int,
    check: bool,
):
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

    beacon_state = create_beacon_state(
        slot, epoch, validators, lido_validators, balance_gen
    )
    beacon_state_hash = ssz.get_hash_tree_root(beacon_state)
    balances_hash = ssz.get_hash_tree_root(beacon_state.balances)

    beacon_block_header = create_beacon_block_header(slot, beacon_state_hash)
    report = Report.create(epoch, beacon_state, beacon_block_header)
    
    print(f"Beacon State hash: {report.beacon_block_hash.hex()}")
    print(f"Balances hash: {balances_hash.hex()}")
    print(f"Expected report: {report}")

    manifesto = {
        "report": report.to_dict(),
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
    beacon_header_file = file.with_stem(f"{file.stem}_header")
    beacon_header_file.write_bytes(ssz.encode(beacon_block_header))

    manifesto_file = file.with_name(f"{file.stem}_manifesto.json")
    with open(manifesto_file, "w") as manifesto_fp:
        json.dump(manifesto, manifesto_fp, cls=BytesHexEncoder, indent=2)

    if check:
        with open(file, "rb") as target:
            reread_beacon_state = ssz.decode(target.read(), BeaconState)

        assert reread_beacon_state.slot == beacon_state.slot
        reread_beacon_state_hash = ssz.get_hash_tree_root(reread_beacon_state)
        assert beacon_state_hash == reread_beacon_state_hash


if __name__ == "__main__":
    main()
