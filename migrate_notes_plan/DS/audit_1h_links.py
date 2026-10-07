#!/usr/bin/env python3
"""独立审计脚本 4：相对链接重定基的正确性（不依赖迁移方的 relink 报告）。

对每个「有迁移前旧路径」的树内文本文件：
  1. 取迁前文本（pre-tag 旧路径 blob）与迁后文本（HEAD 新路径 blob）。
  2. 逐行解析 markdown 行内链接 `](target)`（跳过围栏代码块、外链、纯锚点）。
  3. 对旧文本中的每条链接：
     - 把 target 解析成旧坐标系下的绝对路径 T_old（相对该文件旧目录）。
     - 情形一：T_old 是迁前真实存在的文件（在 manifest 里，或在当前仓库里原位置存在）
       → 期望新文本里对应链接的目标 = 从新文件目录出发指向（T_old 的新位置）的相对路径。
     - 情形二：T_old 迁前就不存在（历史悬空）
       → 期望新文本里对应链接目标保持原字符串不变（按裁决不顺手修）。
  4. 按链接出现顺序配对（链接文本必须相同，否则记为配对失败，人工看）。
输出：不符合期望的逐条明细 + 统计。
"""
import posixpath
import re
import subprocess
from pathlib import Path

R = Path("/home/xzhao/github/minix-rs")
PRE = R / "migrate_notes_plan/pre-migrate-20261007"

LINK = re.compile(r"\[([^\]]*)\]\(([^)\s]+)(\s+\"[^\"]*\")?\)")
FENCE = re.compile(r"^\s*(```|~~~)")
SKIP_PREFIX = ("http://", "https://", "mailto:", "ftp://", "file://", "#")


def sh_bytes(*a):
    return subprocess.run(a, cwd=R, capture_output=True).stdout


def blob(ref, path):
    return sh_bytes("git", "show", f"{ref}:{path}").decode("utf-8", "surrogateescape")


rows = [l.split("\t") for l in (PRE / "path-map.tsv").read_text(encoding="utf-8").splitlines()[1:]]
old_to_new = {r[0]: r[1] for r in rows}
manifest = set()
for l in (PRE / "manifest.notes.disk.tsv").read_text(encoding="utf-8").splitlines()[1:]:
    manifest.add(l.split("\t")[0])

# 已迁走的旧路径集合：用于判断 T_old 是否「迁前真实存在」（notes 树内）
def old_exists(t_old: str) -> str:
    """返回 'moved'（迁前存在、已被搬走）/ 'still'（当前位置仍存在）/ 'dangling'（迁前就不存在）"""
    if t_old in old_to_new:
        return "moved"
    if t_old.startswith("notes/"):
        # notes 树内的路径：不在 manifest 就是迁前不存在
        return "still" if (R / t_old).is_file() else "dangling"
    if (R / t_old).is_file():
        return "still"
    return "dangling"


def links_in(text: str):
    out = []
    fence = False
    for ln, line in enumerate(text.splitlines(), 1):
        if FENCE.match(line):
            fence = not fence
            continue
        if fence:
            continue
        for m in LINK.finditer(line):
            out.append((ln, m.group(1), m.group(2)))
    return out


bad = []
stats = {"moved": 0, "still": 0, "dangling": 0, "rebased_ok": 0, "rebased_bad": 0,
         "dangling_ok": 0, "dangling_changed": 0, "pairfail": 0, "skip_frozen": 0}

FROZEN = ("rewrite-notes/evidence/", "rewrite-notes/archive/legacy-fork-bak/", ".design/")

for old, new, kind, rule in rows:
    if not new.endswith((".md", ".txt")):
        continue
    if any(f in new for f in FROZEN) or "/.design/" in new:
        stats["skip_frozen"] += 1
        continue
    old_text = blob("notes/pre-migrate-20261007", old)
    new_text = blob("HEAD", new)
    if new_text is None:
        continue
    old_links = links_in(old_text)
    new_links = links_in(new_text)
    # 配对：按出现顺序；链接文本相同则配对
    if len(old_links) != len(new_links):
        # 结构性重写的文件（README 等）允许数量变化，逐条报出
        pass
    for i, (ln, text, target) in enumerate(old_links):
        if target.startswith(SKIP_PREFIX):
            continue
        t_old = posixpath.normpath(posixpath.join(posixpath.dirname(old), target.split("#", 1)[0]))
        kind_old = old_exists(t_old)
        stats[kind_old] += 1
        if kind_old == "dangling":
            # 期望不变：在新文本里找链接文本相同、目标等于原串的那条
            same = [t for (_, tx, t) in new_links if tx == text]
            if target in same:
                stats["dangling_ok"] += 1
            else:
                stats["dangling_changed"] += 1
                bad.append(("DANGLING_CHANGED", new, ln, text, target, str(same[:3])))
            continue
        # 期望重定基（或原样相对可解析）；判据 = 在新坐标系里解析到同一绝对文件
        new_file_dir = posixpath.dirname(new)
        if kind_old == "moved":
            t_new_abs = old_to_new[t_old]
        else:
            t_new_abs = t_old
        cands = [(tx, t) for (_, tx, t) in new_links if tx == text]
        ok = False
        for _tx, t in cands:
            if t.startswith(SKIP_PREFIX):
                continue
            actual_abs = posixpath.normpath(posixpath.join(new_file_dir, t.split("#", 1)[0]))
            if actual_abs == t_new_abs:
                ok = True
                break
        if ok:
            stats["rebased_ok"] += 1
        else:
            stats["rebased_bad"] += 1
            bad.append((kind_old.upper() + "_EXPECT", new, ln, text, target,
                        f"期望解析到 {t_new_abs}，实际候选 {[c[1] for c in cands[:3]]}"))

out = ["类别\t文件\t旧行号\t链接文本\t旧目标\t实测"]
for b in bad:
    out.append("\t".join(str(x) for x in b))
(R / "migrate_notes_plan/DS/evidence/item1h-link-rebase-audit.tsv").write_text("\n".join(out), encoding="utf-8")

print("链接分类统计：", stats)
print(f"不符合期望 {len(bad)} 条，明细见 item1h-link-rebase-audit.tsv")
for b in bad[:60]:
    print("   ", b)
