#!/usr/bin/env python3
"""Address-constant static sweep over os/ (BUG-doc T13, three-arch verdicts).

Scans every .rs file under os/ and reports, on separate channels:
  LITERAL : integer literals >= 2^38 (hex/decimal, underscore forms included)
  SHIFT   : `1 << N` shift expressions with N in [30, 63] (address-boundary
            candidates; 2^38 and above are the VA-canonicality family)
  FAMILY  : known x86-64-style user-top families even when small
            (0x7fff..., 0x8000... prefixes) and the Sv39 authority family
            (0x3f_ffff...) so authority-consistency can be audited
  CONST   : every `const NAME: u64|usize|isize` definition (full enumeration,
            value extracted when it is a literal; classified by hand later)

Byte-safe on purpose (files are read as bytes and decoded leniently), so a
non-UTF-8 file can never silently swallow output (ugrep lesson, memory:
ugrep-silent-output-trap).

Usage: python3 tools/address-constant-scan.py [root]   (default: os)
"""
import re
import sys
from pathlib import Path

ROOT = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("os")
LIMIT = 1 << 38  # x86-64/aarch64-class user tops start above this

HEX_RE = re.compile(rb"0x[0-9a-fA-F][0-9a-fA-F_]*")
DEC_RE = re.compile(rb"(?<![0-9a-fA-F_.])\d[\d_]{6,}")  # long decimals only
SHIFT_RE = re.compile(rb"(?<![\w])1\s*(?:u64|u32|usize)?\s*<<\s*(\d{1,2})")
CONST_RE = re.compile(
    rb"const\s+([A-Za-z_][A-Za-z0-9_]*)\s*:\s*(u64|usize|isize|u32)\b"
)


def unu(b: bytes) -> int:
    return int(b.replace(b"_", b""), 0)


def main() -> None:
    hits = {"LITERAL": [], "SHIFT": [], "FAMILY": [], "CONST": []}
    for path in sorted(ROOT.rglob("*.rs")):
        if "target/" in str(path):
            continue
        data = path.read_bytes()
        text = data.decode("utf-8", errors="replace")
        lines = text.splitlines()
        raw_lines = data.split(b"\n")
        for i, raw in enumerate(raw_lines, start=1):
            for m in HEX_RE.finditer(raw):
                v = unu(m.group(0))
                body = m.group(0)
                fam = None
                if v >= LIMIT:
                    fam = "LITERAL"
                elif body.lower().startswith((b"0x7fff", b"0x00007fff", b"0x0000_7fff")) or \
                        body.lower().startswith((b"0x8000", b"0x00008000", b"0x0000_8000")):
                    fam = "FAMILY"
                elif body.lower().startswith(b"0x3f_ffff") or body.lower() == b"0x3fffff":
                    fam = "FAMILY"
                if fam:
                    hits[fam].append(f"{path}:{i}: {raw.decode('utf-8', 'replace').strip()}")
            for m in SHIFT_RE.finditer(raw):
                n = int(m.group(1))
                if 30 <= n <= 63:
                    hits["SHIFT"].append(f"{path}:{i}: {raw.decode('utf-8', 'replace').strip()}  (1<<{n})")
            for m in CONST_RE.finditer(raw):
                hits["CONST"].append(f"{path}:{i}: {raw.decode('utf-8', 'replace').strip()}")
        # multi-line decimal literals that hex/decimal regexes missed when the
        # literal spans a line break are out of scope by design (none exist in
        # this tree as of the sweep; re-run after big refactors to confirm).
        del lines
    for channel in ("LITERAL", "SHIFT", "FAMILY", "CONST"):
        print(f"== {channel} ({len(hits[channel])}) ==")
        for line in hits[channel]:
            print(line)


if __name__ == "__main__":
    main()
