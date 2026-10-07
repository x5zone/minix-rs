#!/usr/bin/env python3
"""独立审计 item1e：对 333 个内容变化文件做「合同机械变换」还原并查残差。

机械变换 = 
  1) 路径映射表（path-map.tsv）里的完整旧路径 → 新路径 字面替换（长路径优先）
  2) 目录前缀替换（含裸 fork-syscall-rewrite 提法）
  3) 相对链接重定基：把链接按旧文件坐标解析成仓库路径，映射到新坐标，再相对新文件目录还原
若 还原(old) == new，则该文件的变化完全由迁移合同解释；否则把残差行打印出来人工判读。
只读脚本。
"""
import re
import subprocess
import sys
from collections import Counter
from pathlib import Path, PurePosixPath

R = Path(__file__).resolve().parents[2]
PRE = "notes/pre-migrate-20261007"
PRE_DIR = R / "migrate_notes_plan/pre-migrate-20261007"

# 载入映射表
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
    """把仓库相对旧路径映射到新路径：先查表，再按前缀规则。"""
    if p in MAP_DICT:
        return MAP_DICT[p]
    for a, b in DIR_RULES:
        if p.startswith(a):
            return b + p[len(a):]
    for a, b in DIR_RULES:
        if p.startswith(a.rstrip("/")):
            rest = p[len(a.rstrip("/")):]
            return b.rstrip("/") + rest
    return p


def transform_line(line: str, old_dir: str, new_dir: str) -> str:
    # 1) 完整路径字面替换
    out = line
    for a, b in MAP:
        if a in out:
            out = out.replace(a, b)
    # 2) 目录前缀
    for a, b in DIR_RULES:
        out = out.replace(a, b)
    # 3) 相对链接重定基
    def fix(m: re.Match) -> str:
        t = m.group(1)
        if t.startswith(("http://", "https://", "#", "mailto:")):
            return m.group(0)
        frag = ""
        if "#" in t:
            t, frag = t.split("#", 1)
            frag = "#" + frag
        if t == "":
            return m.group(0)
        old_abs = str(PurePosixPath(old_dir) / t)
        parts = []
        for seg in PurePosixPath(old_abs).parts:
            if seg == "..":
                if parts:
                    parts.pop()
            elif seg != ".":
                parts.append(seg)
        new_abs = map_path("/".join(parts))
        # 新链接相对新文件所在目录
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
    exact = []
    residual = []
    for old, new in changed:
        ob = git_show(f"{PRE}:{old}").decode("utf-8", "replace").splitlines()
        nb = git_show(f"HEAD:{new}").decode("utf-8", "replace").splitlines()
        old_dir = str(PurePosixPath(old).parent)
        new_dir = str(PurePosixPath(new).parent)
        tb = [transform_line(l, old_dir, new_dir) for l in ob]
        if tb == nb:
            exact.append((old, new))
        else:
            diff_lines = []
            import difflib
            for line in difflib.unified_diff(tb, nb, lineterm="", n=0):
                if line.startswith(("---", "+++", "@@")):
                    continue
                diff_lines.append(line)
            residual.append((old, new, diff_lines))
    print(f"mechanically-explained files: {len(exact)}")
    print(f"files with residual edits: {len(residual)}")
    out = Path(__file__).with_name("evidence") / "item1e-residual.txt"
    with open(out, "w", encoding="utf-8") as f:
        for old, new, d in residual:
            f.write(f"===== {old} -> {new}  ({len(d)} residual lines)\n")
            for l in d:
                f.write(l + "\n")
    print(f"residual detail -> {out}")
    # 统计残差行形态
    c = Counter()
    for old, new, d in residual:
        for l in d:
            c[l[0]] += 1
    print("residual line kinds:", dict(c))
    return 0


if __name__ == "__main__":
    sys.exit(main())
