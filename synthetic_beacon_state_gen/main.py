import dataclasses
import os
from typing import Generator
import random

import itertools

import click
import pathlib
import enum


import ssz
from hexbytes import HexBytes

from eth_ssz_utils import (
    make_validator,
    make_beacon_block_state,
    Constants,
)
from eth_consensus_layer import BeaconState
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


def create_beacon_state(
    slot: int,
    epoch: int,
    total_validators: int,
    lido_validators: int,
    balance_generator: Generator[int, None, None],
) -> BeaconState:
    assert lido_validators <= total_validators

    balances = list(itertools.islice(balance_generator, total_validators))
    validators = [
        make_validator(WithdrawalCreds.Lido, i, i + 1, None, pubkey=b"\x01" * 48)
        for i in range(lido_validators)
    ] + [
        make_validator(WithdrawalCreds.Other, i, i + 1, None, pubkey=b"\x01" * 48)
        for i in range(lido_validators, total_validators)
    ]

    lido_sum = sum(balances[:lido_validators])

    return make_beacon_block_state(
        slot, epoch, Constants.Genesis.BLOCK_ROOT, validators, balances
    )


GWEI_IN_1_ETH = 10**9
MILLIETH = 10**6
FIXED_BALANCE = 16 * GWEI_IN_1_ETH


@dataclasses.dataclass
class Report:
    beacon_block_hash: bytes
    lido_cl_balance: int
    total_validators: int
    lido_validators: int
    lido_exited_validators: int

    @classmethod
    def from_beacon_state(cls, beacon_state: BeaconState) -> "Report":
        cl_balance, validators, exited_validators = 0, 0, 0

        beacon_state_slot = beacon_state.slot // constants.SLOTS_PER_EPOCH

        for validator, balance in zip(beacon_state.validators, beacon_state.balances):
            if validator.withdrawal_credentials == WithdrawalCreds.Lido:
                validators += 1
                cl_balance += balance
                if validator.exit_epoch >= beacon_state_slot:
                    exited_validators += 1

        beacon_block_hash = ssz.get_hash_tree_root(beacon_state)

        return cls(
            beacon_block_hash,
            cl_balance,
            len(beacon_state.validators),
            validators,
            exited_validators,
        )


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
    type=click.Choice(BalanceMode),
    help="Balance generation mode",
    default=BalanceMode.SEQUENTIAL,
)
@click.option("-s", "--slot", type=int, default=123456, help="Slot number")
@click.option("--check", is_flag=True, default=False)
def main(
    file: pathlib.Path,
    validators: int,
    lido_validators: int,
    balances_mode: BalanceMode,
    slot: int,
    check: bool,
):
    if balances_mode == BalanceMode.FIXED:
        balance_gen = itertools.repeat(FIXED_BALANCE)
    elif balances_mode == BalanceMode.SEQUENTIAL:
        balance_gen = itertools.count(1 * GWEI_IN_1_ETH, MILLIETH)
    elif balances_mode == BalanceMode.RANDOM:
        balance_gen = (
            random.randint(1, 100) * GWEI_IN_1_ETH for _ in itertools.repeat(0)
        )

    epoch = slot // constants.SLOTS_PER_EPOCH

    beacon_state = create_beacon_state(
        slot, epoch, validators, lido_validators, balance_gen
    )

    target_dir = os.path.dirname(file)

    if not os.path.exists(target_dir):
        os.makedirs(target_dir)

    beacon_state_hash = ssz.get_hash_tree_root(beacon_state)
    report = Report.from_beacon_state(beacon_state)
    print(f"Beacon State hash: {report.beacon_block_hash.hex()}")
    print(f"Expected report: {report}")

    with open(file, "wb") as target:
        target.write(ssz.encode(beacon_state))

    if check:
        with open(file, "rb") as target:
            reread_beacon_state = ssz.decode(target.read(), BeaconState)

        assert reread_beacon_state.slot == beacon_state.slot
        reread_beacon_state_hash = ssz.get_hash_tree_root(reread_beacon_state)
        assert beacon_state_hash == reread_beacon_state_hash


if __name__ == "__main__":
    main()
