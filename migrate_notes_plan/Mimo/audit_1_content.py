#!/usr/bin/env python3
"""独立审计 item1：按 path-map 逐条比对迁前 sha256 与迁后当前 sha256。

只读脚本：不写仓库内任何文件（输出到 stdout）。
判据：一致 2428 / 缺失 0 / 不一致 0，且映射双射、现盘三棵树文件集合与映射目标集合完全相等。
"""
import hashlib
import sys
from collections import Counter
from pathlib import Path

R = Path(__file__).resolve().parents[2]
PRE = R / "migrate_notes_plan/pre-migrate-20261007"


def sha(p: Path) -> str:
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def main() -> int:
    pre = {}
    status = Counter()
    for line in (PRE / "manifest.notes.disk.tsv").read_text(encoding="utf-8").splitlines()[1:]:
        p, size, mt, d, st = line.split("\t")
        if p in pre:
            print("DUP manifest path:", p)
        pre[p] = (d, st, int(size))
        status[st] += 1

    mapping = {}
    kinds = Counter()
    dup_new = Counter()
    for line in (PRE / "path-map.tsv").read_text(encoding="utf-8").splitlines()[1:]:
        old, new, kind, rule = line.split("\t")
        if old in mapping:
            print("DUP map old:", old)
        mapping[old] = new
        kinds[kind] += 1
        dup_new[new] += 1

    print(f"manifest entries={len(pre)}  path-map entries={len(mapping)}")
    print(f"manifest git_status distribution: {dict(status)}")
    print(f"path-map kind distribution: {dict(kinds)}")
    print(f"duplicate new_path values: {sum(1 for v in dup_new.values() if v > 1)}")

    only_manifest = set(pre) - set(mapping)
    only_map = set(mapping) - set(pre)
    print(f"in manifest not in map: {len(only_manifest)} {sorted(only_manifest)[:5]}")
    print(f"in map not in manifest: {len(only_map)} {sorted(only_map)[:5]}")

    ok = miss = bad = 0
    badlist = []
    for old, new in mapping.items():
        f = R / new
        if not f.is_file():
            miss += 1
            badlist.append(("MISSING", old, new))
            continue
        if sha(f) == pre[old][0]:
            ok += 1
        else:
            bad += 1
            badlist.append(("CHANGED", old, new))
    print(f"content: OK={ok} MISSING={miss} CHANGED={bad} (expect 2428/0/0)")

    # 现盘三棵树文件集合 vs 映射目标集合（有无漏搬/多搬）
    disk = set()
    for root in ("rewrite-notes", "redesign-notes", "study-notes"):
        for p in (R / root).rglob("*"):
            if p.is_file():
                disk.add(str(p.relative_to(R)))
    targets = set(mapping.values())
    print(f"disk files in 3 trees={len(disk)} targets={len(targets)}")
    print(f"on disk but not a mapping target: {len(disk - targets)} {sorted(disk - targets)[:10]}")
    print(f"mapping target missing on disk: {len(targets - disk)} {sorted(targets - disk)[:10]}")

    # 旧树是否残留
    for old in ("notes/rewrite", "notes/study", "notes/redesign"):
        p = R / old
        print(f"residue {old}: exists={p.exists()}")
    print(f"notes/ exists={(R / 'notes').exists()}")
    if (R / "notes").exists():
        print("notes/ content:", sorted(x.name for x in (R / "notes").iterdir())[:20])

    for kind, old, new in badlist[:20]:
        print(kind, old, "->", new)
    return 0 if (ok == len(mapping) and miss == 0 and bad == 0) else 1


if __name__ == "__main__":
    sys.exit(main())
