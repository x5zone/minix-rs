#!/usr/bin/env python3
"""独立审计 item1d：内容有变化的文件，逐 hunk 分类残差。

old = git show notes/pre-migrate-20261007:<old_path>
new = git show HEAD:<new_path>
分类：
  PREFIX — 新行 = 旧行做已知旧路径前缀替换后的结果（合同内）
  RELINK — 新行与旧行只差相对链接的 `../` 层数（合同内，Phase 3 重定基）
  OTHER  — 其余，需人工复核
只读脚本，输出到 stdout；明细写 Mimo/evidence/item1d-other-lines.txt。
"""
import difflib
import re
import subprocess
import sys
from collections import Counter
from pathlib import Path

R = Path(__file__).resolve().parents[2]
PRE = "notes/pre-migrate-20261007"

PREFIX_RULES = [
    ("notes/rewrite/fork-syscall-rewrite/", "rewrite-notes/"),
    ("notes/rewrite/fork-syscall-rewrite", "rewrite-notes"),
    ("notes/rewrite/", "rewrite-notes/"),
    ("notes/rewrite", "rewrite-notes"),
    ("notes/study/", "study-notes/"),
    ("notes/study", "study-notes"),
    ("notes/redesign/", "redesign-notes/"),
    ("notes/redesign", "redesign-notes"),
]

LINK_RE = re.compile(r"\]\(([^)\s]+)\)")


def apply_prefix(line: str) -> str:
    out = line
    for a, b in PREFIX_RULES:
        out = out.replace(a, b)
    return out


def git_show(spec: str) -> bytes:
    p = subprocess.run(["git", "show", spec], cwd=R, capture_output=True)
    if p.returncode != 0:
        raise RuntimeError(p.stderr.decode("utf-8", "replace"))
    return p.stdout


def relink_ok(old: str, new: str) -> bool:
    ol, nl = LINK_RE.findall(old), LINK_RE.findall(new)
    if len(ol) != len(nl):
        return False
    if LINK_RE.sub("()", old) != LINK_RE.sub("()", new):
        return False
    for a, b in zip(ol, nl):
        if a == b:
            continue
        if a.startswith(("http://", "https://", "#", "mailto:")) or b.startswith(
            ("http://", "https://", "#", "mailto:")
        ):
            return False
        if a.split("/")[-1] != b.split("/")[-1]:
            return False
        if a.replace("../", "") != b.replace("../", ""):
            return False
    return True


def main() -> int:
    changed = [tuple(l.split("\t")[:2]) for l in
               Path("/tmp/opencode/changed-list.tsv").read_text().splitlines()]
    print(f"changed files: {len(changed)}")

    file_cls = Counter()
    hunk_cls = Counter()
    others = []
    for old, new in changed:
        ob = git_show(f"{PRE}:{old}")
        nb = git_show(f"HEAD:{new}")
        try:
            o = ob.decode("utf-8").splitlines()
            n = nb.decode("utf-8").splitlines()
        except UnicodeDecodeError:
            file_cls["BINARY"] += 1
            others.append((old, new, "BINARY", "<binary>", "<binary>"))
            continue
        sm = difflib.SequenceMatcher(a=o, b=n, autojunk=False)
        worst = "PREFIX"
        for tag, i1, i2, j1, j2 in sm.get_opcodes():
            if tag == "equal":
                continue
            if tag == "replace" and (i2 - i1) == (j2 - j1):
                for oi, ni in zip(range(i1, i2), range(j1, j2)):
                    ol, nl = o[oi], n[ni]
                    if apply_prefix(ol) == nl:
                        c = "PREFIX"
                    elif relink_ok(ol, nl):
                        c = "RELINK"
                    else:
                        c = "OTHER"
                        others.append((old, new, c, ol, nl))
                    hunk_cls[c] += 1
                    if c == "OTHER":
                        worst = "OTHER"
                    elif c == "RELINK" and worst == "PREFIX":
                        worst = "RELINK"
            else:
                hunk_cls["OTHER-SHAPE"] += 1
                worst = "OTHER"
                for oi in range(i1, i2):
                    others.append((old, new, "DEL", o[oi], ""))
                for ni in range(j1, j2):
                    others.append((old, new, "ADD" if tag == "insert" else "OTHER-SHAPE", "", n[ni]))
        file_cls[worst] += 1

    print("file-level classification:", dict(file_cls))
    print("line/hunk classification:", dict(hunk_cls))
    print(f"non-contract-explained line instances: {len(others)}")
    out = Path(__file__).with_name("evidence") / "item1d-other-lines.txt"
    with open(out, "w", encoding="utf-8") as f:
        for old, new, c, ol, nl in others:
            f.write(f"--- {old} -> {new} [{c}]\n-{ol}\n+{nl}\n")
    print(f"detail -> {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
