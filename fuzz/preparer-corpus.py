#!/usr/bin/env python3
"""Matérialise le corpus binaire depuis les extraits hexadécimaux relisibles."""

import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parent


def write(target, name, data):
    if re.fullmatch(r"[a-z0-9_]+", name) is None:
        raise ValueError("nom de graine invalide")
    path = ROOT / "corpus" / target / name
    path.parent.mkdir(parents=True, exist_ok=True)
    try:
        with path.open("xb") as output:
            output.write(data)
    except FileExistsError:
        # Les ajouts de libFuzzer sont conservés ; aucune suppression.
        if path.read_bytes() != data:
            raise ValueError(f"graine existante différente : {target}/{name}")


def main():
    seeds = json.loads((ROOT / "corpus-initial.json").read_text(encoding="utf-8"))
    frames = []
    for seed in seeds["frames"]:
        data = bytes.fromhex(seed["hex"])
        if len(data) < 8 or len(data) != 8 + data[3]:
            raise ValueError("trame initiale incomplète")
        frames.append(data)
        name = seed["name"]
        write("frame_decode", name, data)
        write("messages", name, data[8:])
        write("frame_stream", name + "_octets", b"\x00" + data)
        write("frame_stream", name + "_morceaux", b"\x03\x00\x02\x06" + data)
    # Même flux collé que captures.rs:decoder_splits_glued_frames_and_skips_garbage.
    write("frame_stream", "collees_et_parasites", b"\x00\x00\x11" + frames[0] + frames[1])
    for seed in seeds["payloads"]:
        write("messages", seed["name"], bytes.fromhex(seed["hex"]))
    for seed in seeds["sdp"]:
        write("sdp_element", seed["name"], bytes.fromhex(seed["hex"]))
    print("Corpus initial prêt : trames, flux découpés, SDP et messages.")


if __name__ == "__main__":
    main()
