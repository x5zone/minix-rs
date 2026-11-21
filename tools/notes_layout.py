#!/usr/bin/env python3
"""notes_layout.py — Python 侧的 meta 路径解析（与 tools/notes-layout.conf 同源）。

只定义路径，不做 I/O。shell 工具 source `notes-layout.conf`；Python 工具 import 本模块。
两处路径漂移由 `tools/check-review-rules.sh` 的 meta 断言兜底。
"""
from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
META_DIR = ROOT / "rewrite-notes/meta"
COORD_DIR = META_DIR / "coordination"
TODO_LEDGER_DIR = COORD_DIR / "todo-ledger"
REVIEW_EVAL_DIR = COORD_DIR / "review-eval"
REVIEW_STATE_DIR = META_DIR / "review/state"
DESIGN_ROOT = META_DIR / "design"


def design_dir(stage: str) -> Path:
    return DESIGN_ROOT / stage


def review_state_path(stage: str, doc_stem: str) -> Path:
    return REVIEW_STATE_DIR / stage / f"{doc_stem}.yaml"


def self_test() -> int:
    """与 tools/notes-layout.conf 的路径变量逐项对齐（防两处漂移）。"""
    conf = ROOT / "tools/notes-layout.conf"
    vals = {}
    for line in conf.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if line.startswith("NOTES_") and "=" in line and not line.startswith("NOTES_TREES"):
            k, v = line.split("=", 1)
            vals[k] = v.strip().strip('"')
    for _ in range(5):
        changed = False
        for k, v in list(vals.items()):
            for ref, rv in vals.items():
                if f"${ref}" in v:
                    v = v.replace(f"${ref}", rv)
                    changed = True
            if v != vals[k]:
                vals[k] = v
        if not changed:
            break
    checks = {
        "NOTES_META": META_DIR,
        "NOTES_COORD": COORD_DIR,
        "NOTES_TODO_LEDGER": TODO_LEDGER_DIR,
        "NOTES_REVIEW_EVAL": REVIEW_EVAL_DIR,
        "NOTES_REVIEW_STATE": REVIEW_STATE_DIR,
        "NOTES_DESIGN_ROOT": DESIGN_ROOT,
    }
    bad = []
    for key, path in checks.items():
        want = vals.get(key)
        got = str(path.relative_to(ROOT))
        if want != got:
            bad.append(f"{key}: conf={want!r} python={got!r}")
    if bad:
        print("notes_layout self-test FAIL:\n  " + "\n  ".join(bad))
        return 1
    print("notes_layout self-test PASS: Python 路径与 notes-layout.conf 一致")
    return 0


if __name__ == "__main__":
    import sys
    sys.exit(self_test())
