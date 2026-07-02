#!/usr/bin/env python3

from __future__ import annotations

import pathlib
import sys
import zlib


ROOT = pathlib.Path(__file__).resolve().parent
JOBS = (
    ("styles", "styles_zlib"),
    ("locales", "locales_zlib"),
)


def compress_tree(src_name: str, dst_name: str) -> tuple[int, int]:
    src_dir = ROOT / src_name
    dst_dir = ROOT / dst_name
    dst_dir.mkdir(parents=True, exist_ok=True)

    count = 0
    total_in = 0
    total_out = 0

    for src_path in sorted(src_dir.glob("*.cbor")):
        raw = src_path.read_bytes()
        compressed = zlib.compress(raw, level=9)
        dst_path = dst_dir / f"{src_path.name}.zlib"
        dst_path.write_bytes(compressed)

        count += 1
        total_in += len(raw)
        total_out += len(compressed)

    print(
        f"{src_name} -> {dst_name}: {count} files, "
        f"{total_in} bytes -> {total_out} bytes"
    )
    return total_in, total_out


def main() -> int:
    grand_in = 0
    grand_out = 0

    for src_name, dst_name in JOBS:
        total_in, total_out = compress_tree(src_name, dst_name)
        grand_in += total_in
        grand_out += total_out

    if grand_in:
        ratio = grand_out / grand_in
        print(
            f"total: {grand_in} bytes -> {grand_out} bytes "
            f"({ratio:.2%} of original)"
        )
    else:
        print("no .cbor files found")

    return 0


if __name__ == "__main__":
    sys.exit(main())
