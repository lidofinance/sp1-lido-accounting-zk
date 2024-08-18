import dataclasses

import dataclasses_json
import ssz
from hexbytes import HexBytes

import constants
from eth_consensus_layer import BeaconState, BeaconBlockHeader

class WithdrawalCreds:
    Lido = HexBytes("010000000000000000000000b9d7934878b5fb9610b3fe8a5e441e8fad7e293f")
    Other = HexBytes(b"\01" * 32)

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
    lido_total_validators: int
    lido_deposited_validators: int
    lido_future_deposit_validators: int
    lido_exited_validators: int
    lido_withdrawal_credentials: bytes

    @classmethod
    def create(cls, beacon_state: BeaconState, beacon_block_header: BeaconBlockHeader) -> "Report":
        total_balance, lido_cl_balance = 0, 0
        lido_validators, deposited_validators, future_deposit_validators, exited_validators = 0, 0, 0, 0

        epoch = beacon_state.slot // constants.SLOTS_PER_EPOCH

        for validator, balance in zip(beacon_state.validators, beacon_state.balances):
            total_balance += balance
            if validator.withdrawal_credentials == WithdrawalCreds.Lido:
                lido_validators += 1
                lido_cl_balance += balance
                if epoch >= validator.activation_eligibility_epoch:
                    deposited_validators += 1
                else:
                    future_deposit_validators += 1
                if epoch >= validator.exit_epoch:
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
            total_validators = len(beacon_state.validators),
            lido_total_validators = lido_validators,
            lido_deposited_validators = deposited_validators,
            lido_future_deposit_validators = future_deposit_validators,
            lido_exited_validators = exited_validators,
            lido_withdrawal_credentials = WithdrawalCreds.Lido,
        )
