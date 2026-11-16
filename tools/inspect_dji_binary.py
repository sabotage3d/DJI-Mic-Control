#!/usr/bin/env python3
"""Offline, read-only triage of DJI Mimo APKs and receiver firmware packages.

Never opens USB devices, sends commands, patches firmware, or executes input.
Outputs file hashes, container members, string-token counts, and byte offsets.
This is a *triage* tool, not a firmware decoder or an RF-disconnect command.
"""
from __future__ import annotations

import argparse
import collections
import hashlib
import math
from pathlib import Path
import re
import sys
import zipfile

# Labels are clues, NOT verified control opcodes. Short tokens ("rf", "tx",
# "rx") are intentionally excluded to keep results meaningful.
KEYWORDS = (
    "wireless", "standby", "sleep", "disconnect", "unpair", "pairing",
    "linking", "transmitter", "receiver", "power_off", "poweroff",
    "shutdown", "rf_enable", "rf_disable", "radio_off", "radio_on",
    "osmoaudio", "mic mini",
)
MAX_HITS_PER_TERM = 8


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def file_kind(data: bytes) -> str:
    if data.startswith(b"xV4"):
        return "DJI xV4-style container (not necessarily Mic-compatible)"
    if data.startswith(b"IM*H"):
        return "DJI image header candidate"
    if data.startswith(b"\x7fELF"):
        return "ELF executable/library"
    if data.startswith(b"PK\x03\x04"):
        return "ZIP/APK archive"
    if data.startswith(b"\x1f\x8b"):
        return "gzip"
    if data.startswith(b"\xfd7zXZ\x00"):
        return "XZ"
    return "unidentified binary/data"


def entropy_sample(data: bytes) -> float:
    sample = data[:min(len(data), 1024 * 1024)]
    if not sample:
        return 0.0
    counts = collections.Counter(sample)
    return -sum((n / len(sample)) * math.log2(n / len(sample))
                for n in counts.values())


def scan_tokens(data: bytes) -> dict[str, list[int]]:
    """ASCII/UTF-16LE string occurrences, reported as offsets only.

    A match is not evidence of an RF-control instruction. We do not print
    surrounding bytes, which may contain serials, API keys or identifiers.
    """
    findings = {}
    for word in KEYWORDS:
        offsets = []
        for encoded in (word.encode("ascii"), word.encode("utf-16le")):
            for match in re.finditer(re.escape(encoded), data, flags=re.IGNORECASE):
                offsets.append(match.start())
                if len(offsets) >= MAX_HITS_PER_TERM:
                    break
            if len(offsets) >= MAX_HITS_PER_TERM:
                break
        if offsets:
            findings[word] = sorted(set(offsets))
    return findings


def analyze(name: str, data: bytes) -> tuple[int, str]:
    hits = scan_tokens(data)
    names = ", ".join(f"{key}@{','.join(hex(x) for x in offsets[:3])}"
                      for key, offsets in hits.items())
    print(f"  {name}: size={len(data):,} kind={file_kind(data)} "
          f"entropy(sample)={entropy_sample(data):.2f}/8")
    if hits:
        print("    text clues (byte offsets only): " + names)
    else:
        print("    no relevant plain-text clues; content may be stripped, "
              "compressed, or encrypted")
    return len(hits), name


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("file", type=Path,
                        help="APK/ZIP or local firmware package to inspect")
    parser.add_argument("--max-member-mb", type=int, default=128,
                        help="max uncompressed ZIP member size (default 128 MiB)")
    parser.add_argument("--max-members", type=int, default=5000,
                        help="max ZIP entries inspected (default 5000)")
    parser.add_argument("--show-empty", action="store_true",
                        help="also list ZIP members without relevant strings")
    args = parser.parse_args(argv)
    path: Path = args.file
    if not path.is_file():
        parser.error(f"not a regular file: {path}")
    if args.max_member_mb < 1 or args.max_members < 1:
        parser.error("limits must be positive")
    print(f"Input: {path.name} ({path.stat().st_size:,} bytes)")
    print("SHA-256:", sha256_file(path))
    cap = args.max_member_mb * 1024 * 1024
    if zipfile.is_zipfile(path):
        with zipfile.ZipFile(path) as archive:
            entries = [i for i in archive.infolist() if not i.is_dir()]
            print(f"ZIP entries: {len(entries)} (inspect up to {args.max_members})")
            ranked = []
            skipped = 0
            for item in entries[:args.max_members]:
                if item.file_size > cap:
                    skipped += 1
                    continue
                with archive.open(item) as source:
                    payload = source.read(cap + 1)
                if len(payload) > cap:
                    skipped += 1
                    continue
                found = scan_tokens(payload)
                if found or args.show_empty:
                    score, name = analyze(item.filename, payload)
                    ranked.append((score, name))
            if skipped:
                print(f"Skipped {skipped} oversized entries (increase --max-member-mb if needed)")
            if not ranked:
                print("No clues found in inspected ZIP entries.")
            else:
                print("Candidate archive members (ranked by distinct keyword matches):")
                for count, name in sorted(ranked, key=lambda x: (-x[0], x[1]))[:20]:
                    print(f"  {count:2d}  {name}")
    else:
        if path.stat().st_size > cap:
            parser.error("binary exceeds --max-member-mb limit; increase deliberately")
        analyze(path.name, path.read_bytes())
    print("Read-only inspection complete. No USB communication or firmware writes.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
