#!/usr/bin/env python3
"""独立审计 item1f：以「真实改动」（old→new 逐行 diff）为基准，判每处改动是否可由迁移合同解释。

old = git show notes/pre-migrate-20261007:<old_path>
new = git show HEAD:<new_path>
对每一处真实改动（difflib 对齐后的 replace/insert/delete）：
  - 若 new_line == 合同机械变换(old_line) → 合同内（路径映射 + 前缀替换 + 相对链接重定基）
  - 否则 → 真实内容编辑（需对应执行日志里的授权动作）
明细写 evidence/item1f-real-edits.txt。
"""
import difflib
import re
import subprocess
import sys
from collections import Counter
from pathlib import Path, PurePosixPath

R = Path(__file__).resolve().parents[2]
PRE = "notes/pre-migrate-20261007"
PRE_DIR = R / "migrate_notes_plan/pre-migrate-20261007"

MAP = []
for line in (PRE_DIR / "path-map.tsv").read_text(encoding="utf-8").splitlines()[1:]:
    old, new, kind, rule = line.split("\t")
    MAP.append((old, new))
MAP.sort(key=lambda p: -len(p[0]))
MAP_DICT = dict(MAP)

DIR_RULES = [
    ("notes/rewrite/fork-syscall-rewrite/", "rewrite-notes/"),
    ("notes/rewrite/fork-syscall-rewrite", "rewrite-notes"),
    ("notes/rewrite/", "rewrite-notes/"),
    ("notes/rewrite", "rewrite-notes"),
    ("notes/study/", "study-notes/"),
    ("notes/study", "study-notes"),
    ("notes/redesign/", "redesign-notes/"),
    ("notes/redesign", "redesign-notes"),
    ("fork-syscall-rewrite/", "rewrite-notes/"),
    ("fork-syscall-rewrite", "rewrite-notes"),
]
LINK_RE = re.compile(r"\]\(([^)\s]+)\)")


def map_path(p: str) -> str:
    if p in MAP_DICT:
        return MAP_DICT[p]
    for a, b in DIR_RULES:
        if p.startswith(a):
            return b + p[len(a):]
    return p


def normalize(path: str) -> str:
    parts = []
    for seg in PurePosixPath(path).parts:
        if seg == "..":
            if parts:
                parts.pop()
        elif seg != ".":
            parts.append(seg)
    return "/".join(parts)


def transform_line(line: str, old_dir: str, new_dir: str) -> str:
    out = line
    for a, b in MAP:
        if a in out:
            out = out.replace(a, b)
    for a, b in DIR_RULES:
        out = out.replace(a, b)

    def fix(m: re.Match) -> str:
        t = m.group(1)
        if t.startswith(("http://", "https://", "#", "mailto:")) or t == "":
            return m.group(0)
        frag = ""
        if "#" in t:
            t, frag = t.split("#", 1)
            frag = "#" + frag
        if t == "":
            return m.group(0)
        old_abs = normalize(str(PurePosixPath(old_dir) / t))
        new_abs = map_path(old_abs)
        start = [s for s in PurePosixPath(new_dir).parts if s not in (".",)]
        tgt = list(PurePosixPath(new_abs).parts)
        i = 0
        while i < len(start) and i < len(tgt) and start[i] == tgt[i]:
            i += 1
        relpath = "/".join([".."] * (len(start) - i) + tgt[i:]) or "."
        return f"]({relpath}{frag})"

    return LINK_RE.sub(fix, out)


def git_show(spec: str) -> bytes:
    p = subprocess.run(["git", "show", spec], cwd=R, capture_output=True)
    if p.returncode != 0:
        raise RuntimeError(p.stderr.decode("utf-8", "replace"))
    return p.stdout


def main() -> int:
    changed = [tuple(l.split("\t")[:2]) for l in
               Path("/tmp/opencode/changed-list.tsv").read_text().splitlines()]
    in_contract = Counter()
    real_files = []
    real_lines = []
    for old, new in changed:
        ob = git_show(f"{PRE}:{old}").decode("utf-8", "replace").splitlines()
        nb = git_show(f"HEAD:{new}").decode("utf-8", "replace").splitlines()
        old_dir = str(PurePosixPath(old).parent)
        new_dir = str(PurePosixPath(new).parent)
        sm = difflib.SequenceMatcher(a=ob, b=nb, autojunk=False)
        file_real = []
        for tag, i1, i2, j1, j2 in sm.get_opcodes():
            if tag == "equal":
                continue
            if tag == "replace" and (i2 - i1) == (j2 - j1):
                for oi, ni in zip(range(i1, i2), range(j1, j2)):
                    ol, nl = ob[oi], nb[ni]
                    if transform_line(ol, old_dir, new_dir) == nl:
                        in_contract["replace-contract"] += 1
                    else:
                        in_contract["replace-REAL"] += 1
                        file_real.append(("R", ol, nl))
            elif tag == "insert":
                for ni in range(j1, j2):
                    in_contract["insert-REAL"] += 1
                    file_real.append(("A", "", nb[ni]))
            elif tag == "delete":
                for oi in range(i1, i2):
                    in_contract["delete-REAL"] += 1
                    file_real.append(("D", ob[oi], ""))
            else:  # unequal replace
                for oi in range(i1, i2):
                    in_contract["block-del-REAL"] += 1
                    file_real.append(("D", ob[oi], ""))
                for ni in range(j1, j2):
                    in_contract["block-add-REAL"] += 1
                    file_real.append(("A", "", nb[ni]))
        if file_real:
            real_files.append((old, new, file_real))
            real_lines.extend(file_real)

    print("changed line classification:", dict(in_contract))
    print(f"files with REAL edits (not explained by contract transform): {len(real_files)}")
    print(f"REAL edit line instances: {len(real_lines)}")
    out = Path(__file__).with_name("evidence") / "item1f-real-edits.txt"
    with open(out, "w", encoding="utf-8") as f:
        for old, new, items in real_files:
            f.write(f"===== {old} -> {new}  ({len(items)} real edits)\n")
            for k, ol, nl in items:
                f.write(f"[{k}]\n-{ol}\n+{nl}\n")
    print(f"detail -> {out}")
    c = Counter(old for old, new, _ in real_files)
    for k, v in c.most_common():
        print(f"  {v:3d} {k}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
