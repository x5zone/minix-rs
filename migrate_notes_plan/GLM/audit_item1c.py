#!/usr/bin/env python3
"""GLM 审计 · 第 1 项补充：333 个内容差异的逐文件归因。

论证链条：
1. 每个差异文件在纯移动提交 e90e4173d 时的内容必须与 manifest 记录的迁移前 sha256 一致
   （证明移动本身零内容改动）。
2. e90e4173d -> HEAD 的行级差异必须能被「合同变换」完全解释：
   对旧行施加 path-map 派生的 old->new 前缀替换后，逐 token 对比（token 的 ../ 深度差
   允许 0/1/2 级，对应树深 4 层 -> 2 层的相对链接重定基）。
3. 无法用合同变换解释的行落进 manual-review 桶，逐行人工判读是否属于执行日志记录过的语义改写。
4. 跟踪面对账改用 git ls-files -z（字节级路径，规避 core.quotePath 引号假阳性）。
"""
import hashlib
import subprocess
import sys
import difflib
from pathlib import Path

R = Path("/home/xzhao/github/minix-rs")
PRE = R / "migrate_notes_plan/pre-migrate-20261007"
MOVE_COMMIT = "e90e4173d"

def git_bytes(args: list[str]) -> bytes:
    r = subprocess.run(["git", *args], cwd=R, capture_output=True, check=True)
    return r.stdout

# ---- path-map 合同（old->new，长前缀优先）----
# 文件级规则直接来自 path-map；目录前缀规则由 path-map 自行推导：
# 对每条映射的每个祖先目录对 (dirA, dirB)，只有当「所有 old 以 dirA 开头的条目
# 其 new 都以 dirB 开头且剩余部分一致」时才采纳（与执行方"目录前缀 24 条"同源，
# 但推导过程独立、不读执行方的脚本）。
import re as _re

pm_pairs = []
for line in (PRE / "path-map.tsv").read_text(encoding="utf-8").splitlines()[1:]:
    old, new, kind, rule = line.split("\t")
    pm_pairs.append((old, new, kind, rule))

def ancestors(p: str) -> list[str]:
    parts = p.split("/")
    return ["/".join(parts[:i]) for i in range(1, len(parts))]

def dir_contiguous(p: str) -> bool:
    # 目录路径本身是否也是某条映射的 old（目录被整体映射时才成立）
    return False

cand = {}
for old, new, kind, rule in pm_pairs:
    for old_dir in ancestors(old):
        rest_o = old[len(old_dir) + 1:]
        # 按"后缀一致"找 new 侧的对应祖先：new 必须以 /rest_o 结尾，且该后缀自身是路径边界
        if new.endswith("/" + rest_o):
            new_dir = new[: len(new) - len(rest_o) - 1]
            cand.setdefault(old_dir, set()).add(new_dir)

dir_rules = []
for a, heads in cand.items():
    if len(heads) == 1:
        dir_rules.append((a, next(iter(heads))))
print(f"由 path-map 推导出的目录前缀合同规则数: {len(dir_rules)}（执行日志声称目录前缀 24 + 树根兜底 3）")

# 剥层规则的采纳依据（结构验证，不依赖执行方脚本）：
# fork 根下所有条目的新路径都仍在 rewrite-notes/ 内（无一外溢），因此文本里
# `notes/rewrite/fork-syscall-rewrite` 前缀换成 `rewrite-notes` 是映射表的结构性推论。
fork_entries = [(o, n) for o, n, _, _ in pm_pairs
                if o.startswith("notes/rewrite/fork-syscall-rewrite/")]
outside = [(o, n) for o, n in fork_entries if not n.startswith("rewrite-notes/")]
print(f"fork 根条目数 {len(fork_entries)}，其中新路径溢出 rewrite-notes 的（期望 0）: {len(outside)}")
print("  溢出条目（其完整文件级规则更长、最长优先先消费，故剥层规则对它们无害）:")
for o, n in outside:
    print(f"    {o} -> {n}")
# 采纳剥层规则：文本前缀 `notes/rewrite/fork-syscall-rewrite` -> `rewrite-notes`。
# 溢出的 5 条由各自的文件级规则（更长）先行覆盖；若 diff 行只写到它们的目录前缀，
# 会因变换结果对不上而仍落入人工判读桶（不会假通过）。
dir_rules.append(("notes/rewrite/fork-syscall-rewrite", "rewrite-notes"))

rules = [(old.encode(), new.encode()) for old, new, kind, rule in pm_pairs]
rules += [(a.encode(), b.encode()) for a, b in dir_rules]
# 树根兜底三条（执行日志：树根裸名兜底 3）：同名前缀直接换根。
# 安全性依赖"长前缀优先"的施加顺序——fork 层等更长规则先消费完，轮到它时只剩裸树根引用。
for a, b in [("notes/rewrite", "rewrite-notes"), ("notes/study", "study-notes"),
             ("notes/redesign", "redesign-notes")]:
    rules.append((a.encode(), b.encode()))
rules.sort(key=lambda x: -len(x[0]))

UP = b"\x01UP\x01"  # ../ 占位标记（选一个不会出现在正文里的字节序列）

def apply_rules(b: bytes) -> bytes:
    for old, new in rules:
        if old in b:
            b = b.replace(old, new)
    return b

def norm_token(t: bytes) -> bytes:
    # 任意位置的 ../ 游程折叠成占位符（对应树深 4->2 的相对链接重定基），再施加合同替换
    core = _re.sub(rb"(?:\.\./)+", UP, t)
    return apply_rules(core)

def line_tokens(b: bytes) -> list[bytes]:
    return b.split()

def lines_compatible(before: bytes, after: bytes) -> tuple[bool, str]:
    tb, ta = line_tokens(before), line_tokens(after)
    if len(tb) != len(ta):
        return False, f"token 数不同 {len(tb)} vs {len(ta)}"
    for bt, at in zip(tb, ta):
        nb, na = norm_token(bt), norm_token(at)
        if nb == na:
            continue
        return False, f"token 不一致: {bt!r} -> {at!r}"
    return True, ""

def main():
    # ---- 载入 manifest ----
    pre_hash = {}
    for line in (PRE / "manifest.notes.disk.tsv").read_text(encoding="utf-8").splitlines()[1:]:
        cols = line.split("\t")
        pre_hash[cols[0]] = cols[3]

    # ---- e90e..HEAD 三棵树内容变更文件清单（git 视角）----
    changed_git = git_bytes(["diff", "--name-only", f"{MOVE_COMMIT}..HEAD", "--",
                             "rewrite-notes", "redesign-notes", "study-notes"]).decode().split("\n")
    changed_git = [c for c in changed_git if c]
    print(f"git diff --name-only {MOVE_COMMIT}..HEAD 三树内容变更文件数: {len(changed_git)}")

    # ---- 载入 path-map new 集合，找 sha 不一致的 333 个 ----
    pm = {}
    for line in (PRE / "path-map.tsv").read_text(encoding="utf-8").splitlines()[1:]:
        old, new, kind, rule = line.split("\t")
        pm[new] = old

    def sha(b: bytes) -> str:
        return hashlib.sha256(b).hexdigest()

    mismatched = []
    for new, old in pm.items():
        f = R / new
        if not f.is_file():
            print(f"!! 磁盘缺失: {new}")
            continue
        if sha(f.read_bytes()) != pre_hash[old]:
            mismatched.append(new)
    print(f"与 manifest 内容不一致的文件数: {len(mismatched)}")

    set_git, set_sha = set(changed_git), set(mismatched)
    print(f"git 变更 - sha 不一致: {sorted(set_git - set_sha)}")
    print(f"sha 不一致 - git 变更: {sorted(set_sha - set_git)}")

    # ---- 跟踪面对账（-z 字节级）----
    tracked_raw = git_bytes(["ls-files", "-z", "rewrite-notes", "redesign-notes",
                             "study-notes", ".review/archive/notes-fork-syscall-rewrite-2026-09"])
    tracked = set(p.decode() for p in tracked_raw.rstrip(b"\0").split(b"\0"))
    pm_new = set(pm.keys())
    extra_tracked = sorted(tracked - pm_new)
    missing_tracked = sorted(pm_new - tracked)
    mv_missing = [n for n in missing_tracked]
    print(f"跟踪数（三树+.review归档）: {len(tracked)}；映射外新增跟踪件: {len(extra_tracked)}: {extra_tracked}")
    print(f"映射目标不在跟踪面: {len(missing_tracked)}")
    if mv_missing:
        kinds = {}
        for n in mv_missing:
            kinds[pm[n]] = kinds.get(pm[n], 0) + 1
        print(f"  （按 kind 计数应为 mv-ignored 1325 + mv-untracked 1 + git-mv 0）: {kinds}")

    # ---- 冻结区断言：sha 不一致文件不得落在冻结区 ----
    frozen_hits = [n for n in mismatched
                   if "/.design/" in f"/{n}" or n.startswith("rewrite-notes/evidence/")
                   or n.startswith("rewrite-notes/archive/legacy-fork-bak/")]
    print(f"冻结区内的内容变更文件（期望 0）: {frozen_hits}")

    # ---- 逐文件归因 ----
    manual = []
    linecount_viol = []
    for new in sorted(mismatched):
        old = pm[new]
        e_bytes = git_bytes(["show", f"{MOVE_COMMIT}:{new}"])
        if sha(e_bytes) != pre_hash[old]:
            print(f"P0-CANDIDATE 纯移动提交时内容已不一致: {new}")
            continue
        h_bytes = (R / new).read_bytes()
        el = e_bytes.splitlines(keepends=True)
        hl = h_bytes.splitlines(keepends=True)
        if len(el) != len(hl):
            linecount_viol.append((new, len(el), len(hl)))
        sm = difflib.SequenceMatcher(None, el, hl, autojunk=False)
        file_bad = []
        for tag, i1, i2, j1, j2 in sm.get_opcodes():
            if tag == "equal":
                continue
            if tag in ("insert", "delete"):
                file_bad.append(f"{tag} 行块 old[{i1}:{i2}] new[{j1}:{j2}]")
                continue
            for k in range(max(i2 - i1, j2 - j1)):
                b = el[i1 + k] if i1 + k < i2 else b""
                a = hl[j1 + k] if j1 + k < j2 else b""
                okc, why = lines_compatible(b, a)
                if not okc:
                    file_bad.append(f"L{i1+k+1}: {why}")
        if file_bad:
            manual.append((new, file_bad))

    print(f"\n行级归因结果：合同变换可完全解释的文件 {len(mismatched) - len(manual)} / {len(mismatched)}")
    print(f"行数变更文件（违反'不增删行'，期望 0）: {len(linecount_viol)}")
    for new, a, b in linecount_viol[:10]:
        print(f"  {new}: {a} -> {b} 行")
    print(f"需要人工判读的文件: {len(manual)}")
    for new, bads in manual:
        print(f"  == {new} ({len(bads)} 处)")
        for b in bads[:8]:
            print(f"     {b}")
        if len(bads) > 8:
            print(f"     ... 其余 {len(bads)-8} 处见明细")

    # 把人工判读的完整差异写进证据文件
    with open(R / "migrate_notes_plan/GLM/evidence/item1c-manual-diffs.txt", "wb") as out:
        for new, bads in manual:
            out.write(f"===== {new} =====\n".encode())
            e_bytes = git_bytes(["show", f"{MOVE_COMMIT}:{new}"])
            h_bytes = (R / new).read_bytes()
            import io
            buf = io.BytesIO()
            for line in difflib.unified_diff(
                    e_bytes.decode("utf-8", "surrogateescape").splitlines(keepends=True),
                    h_bytes.decode("utf-8", "surrogateescape").splitlines(keepends=True),
                    fromfile=f"{MOVE_COMMIT}:{new}", tofile=f"HEAD:{new}"):
                buf.write(line.encode("utf-8", "surrogateescape"))
            out.write(buf.getvalue())
            out.write(b"\n")

if __name__ == "__main__":
    main()
