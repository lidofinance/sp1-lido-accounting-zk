import json
import os
import pathlib
import ssz
from eth_consensus_layer import BeaconState, BeaconBlockHeader
from report import Report

THIS_FOLDER = os.path.dirname(__file__)
PROJECT_ROOT = os.path.dirname(THIS_FOLDER)

slot = 9760032

beacon_state_file = pathlib.Path(PROJECT_ROOT) / f"temp/bs_{slot}.ssz"
beacon_header_file = pathlib.Path(PROJECT_ROOT) / f"temp/bs_{slot}_header.ssz"

def main():
    with open(beacon_header_file, "rb") as target:
        bh_payload = json.load(target)
        bh_data = bh_payload["data"]["header"]["message"]
        print(bh_data)
        bh = BeaconBlockHeader.from_json(bh_data)

    print(f"Read beacon header: slot {bh.slot}, beacon_state_hash: {bh.state_root}, block_hash: {ssz.get_hash_tree_root(bh)}")

    print("Reading beacon state (this should take a while...)")
    with open(beacon_state_file, "rb") as target:
        bs = ssz.decode(target.read(), BeaconState)

    print(f"Beacon state for slot {bs.slot} with {len(bs.validators)} validators")
    report = Report.create(bs, bh)
    print(f"Report: {report}")


if __name__ == "__main__":
    main()