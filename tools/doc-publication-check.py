#!/usr/bin/env python3
"""doc-publication-check.py — 正式文档出版标准门（V3.7 / DP-1..DP-5）

用户的出版标准（2026-11-21 裁决，原话要点）：
  1. 正式文档必须自包含自解释；只允许引用正式文档、代码路径、C 源；
     **不允许引用中间文档**（DECISIONS.md / OPEN-DEBT / runs/ / .design/ / prompt/ / tmp/）。
  2. **不允许出现标号**——D-061 / I-29 / F-R34-01 / R36 / Phase A 对读者没有意义。
  3. **不允许出现迭代历史**——读者只关心现状。若一段历史有教学价值，必须改写成
     反事实设计讨论（"如果设计 X 会怎么样，所以直接设计 Z"），而不是"最初…后来删了"。

判据（全部可机械判定）：
  DP-1 过程编号：D-nnn / I-n / F-Rnn-nn / DESIGN-* / Rnn / Tn / Phase X
  DP-2 提交指纹：散文里的 7–40 位十六进制串（代码块内豁免；0x 字面量天然不匹配）
  DP-3 中间产物引用：.design/ runs/ DECISIONS.md OPEN-DEBT prompt/ tmp/ coordination/
  DP-4 编号图例：出现「编号图例」本身即违规（它是 DP-1 的产物）
  DP-5 迭代叙事：旧版/旧形态/原实现/最初/曾经/后来/删除前/已删除/不再使用/改成/替代/
       升级为/废弃/放弃/回退到/修复前/已修复…
       豁免：反事实设计句（含 如果/若/假设/可以设计 + 会/不会/不能/反而），
       或显式标记 `<!-- publication-ok: counterfactual -->`。

基线机制（与 pattern-gate.sh 同构）：存量违规不阻断并发工作；新增即红。
  --baseline <file>  默认 tools/publication-baseline/doc.txt
  --update-baseline  把当前全部命中写入基线（只应在用户批准后执行）
  --self-test        正反例判别矩阵

用法：
  python3 tools/doc-publication-check.py                     # 默认范围：正式文档
  python3 tools/doc-publication-check.py FILE [FILE...]
  python3 tools/doc-publication-check.py --self-test
"""
from __future__ import annotations

import argparse
import hashlib
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_BASELINE = ROOT / "tools/publication-baseline/doc.txt"

# 正式文档 = rewrite-notes/<stage>/NN-<name>.md，排除评审/待办/设计/取证等辅助件。
# 正式篇目只在 stage 树里；00-master-plan 是规划稿，不属出版范围。
FORMAL_GLOB = "rewrite-notes/*-stage-*/[0-9][0-9]-*.md"
# 正式篇目 = `NN-<slug>.md`，且 slug 不以辅助前缀开头。
# 依据：01-stage-kernel 下的实际布局（boot-shim / higher-half / kmain / platform /
# clock / proc-init 六篇为正式篇；adjudication / design / forensics / review /
# evidence / todo / backlog / history / learn / brief / note 均为辅助件）。
AUX_PREFIX = (
    "adjudication", "design", "forensics", "review", "evidence", "todo",
    "backlog", "history", "learn", "brief", "note", "migration", "code-design",
)
PROCESS_PATTERNS = (
    ("DP-1", re.compile(r"\bD-\d{3}\b")),
    ("DP-1", re.compile(r"\bI-\d+\b")),
    ("DP-1", re.compile(r"\bF-R\d{1,2}-\d+\b")),
    ("DP-1", re.compile(r"\bDESIGN-[A-Za-z0-9-]+\b")),
    ("DP-1", re.compile(r"\bR(?:1[3-9]|[2-9]\d)\b")),
    ("DP-1", re.compile(r"\bT\d{1,2}\b")),
    ("DP-1", re.compile(r"\bPhase [A-Z](?:\.\d+)?\b")),
    ("DP-2", re.compile(r"\b[0-9a-f]{7,40}\b")),
)
INTERMEDIATE_RE = re.compile(
    r"\.design/|(?<![\w/])runs/|DECISIONS\.md|OPEN-DEBT|(?<![\w/])prompt/|(?<![\w/])tmp/|coordination/"
)
LEGEND_RE = re.compile(r"编号图例")
ITER_RE = re.compile(
    r"旧版|旧形态|旧实现|原实现|前版|原先|最初|曾经|后来|过去|当年|当初|删除前|已删除|"
    r"已被删除|不再使用|改成|替代|替换为|升级为|废弃|放弃|回退到|修复前|已修复"
)
COUNTERFACTUAL_RE = re.compile(
    r"(?:如果|若|假设|可以设计|把[^。\n]{0,30}改成[^。\n]{0,20}会)"
    r"[^。\n]{0,80}(?:会怎样|会有什么|会出问题|会失效|不会|不能|反而)"
)
OK_MARK = "<!-- publication-ok: counterfactual -->"
FENCE_RE = re.compile(r"^\s*(```|~~~)")


def iter_formal_docs() -> list[Path]:
    docs: list[Path] = []
    for p in sorted(ROOT.glob(FORMAL_GLOB)):
        slug = p.stem.split("-", 1)[1] if "-" in p.stem else p.stem
        if any(slug.startswith(a) for a in AUX_PREFIX):
            continue
        docs.append(p)
    return docs


def scan_doc(path: Path) -> list[dict]:
    hits: list[dict] = []
    in_fence = False
    for i, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if FENCE_RE.match(line):
            in_fence = not in_fence
            continue
        if in_fence:
            continue
        stripped = line.strip()
        for kind, pat in PROCESS_PATTERNS:
            for m in pat.finditer(line):
                hits.append({"kind": kind, "line": i, "text": line.strip(), "token": m.group(0)})
        for m in INTERMEDIATE_RE.finditer(line):
            hits.append({"kind": "DP-3", "line": i, "text": stripped, "token": m.group(0)})
        if LEGEND_RE.search(line):
            hits.append({"kind": "DP-4", "line": i, "text": stripped, "token": "编号图例"})
        if ITER_RE.search(line) and OK_MARK not in line and not COUNTERFACTUAL_RE.search(line):
            hits.append(
                {"kind": "DP-5", "line": i, "text": stripped,
                 "token": ITER_RE.search(line).group(0)}
            )
    return hits


def key_for(path: Path, hit: dict) -> str:
    """基线键：文件 + 类别 + 该行规范化内容的哈希（行号漂移不失效）。"""
    norm = re.sub(r"\s+", " ", hit["text"]).strip()
    digest = hashlib.sha1(norm.encode("utf-8")).hexdigest()[:16]
    rel = path.relative_to(ROOT) if path.is_relative_to(ROOT) else path
    return f"{rel}|{hit['kind']}|{digest}"


def load_baseline(path: Path) -> set[str]:
    if not path.is_file():
        return set()
    return {ln.strip() for ln in path.read_text(encoding="utf-8").splitlines() if ln.strip()}


def run(docs: list[Path], baseline: Path, update: bool) -> int:
    known = load_baseline(baseline)
    new_hits: list[tuple[Path, dict]] = []
    known_count = 0
    all_keys: list[str] = []
    per_doc: dict[Path, dict[str, int]] = {}
    for doc in docs:
        for hit in scan_doc(doc):
            k = key_for(doc, hit)
            all_keys.append(k)
            per_doc.setdefault(doc, {}).setdefault(hit["kind"], 0)
            per_doc[doc][hit["kind"]] += 1
            if k in known:
                known_count += 1
            else:
                new_hits.append((doc, hit))
    if update:
        baseline.parent.mkdir(parents=True, exist_ok=True)
        baseline.write_text("\n".join(sorted(set(all_keys))) + "\n", encoding="utf-8")
        print(f"baseline 写入 {baseline}（{len(set(all_keys))} 条）")
        return 0
    # 报告
    for doc in docs:
        counts = per_doc.get(doc, {})
        if not counts:
            continue
        rel = doc.relative_to(ROOT) if doc.is_relative_to(ROOT) else doc
        summary = "  ".join(f"{k}={v}" for k, v in sorted(counts.items()))
        print(f"[统计] {rel}: {summary}")
    if known_count:
        print(f"[基线] 已有 {known_count} 处（不阻断，列入 burn-down）")
    if new_hits:
        for doc, hit in new_hits[:80]:
            rel = doc.relative_to(ROOT) if doc.is_relative_to(ROOT) else doc
            print(f"[FAIL] {rel}:{hit['line']} {hit['kind']} `{hit['token']}` — {hit['text'][:100]}")
        if len(new_hits) > 80:
            print(f"... 另有 {len(new_hits) - 80} 处新增")
        print(f"doc-publication-check: {len(new_hits)} 处**新增**违规（基线内 {known_count} 处）")
        return 1
    print("doc-publication-check: 无新增违规")
    return 0


def self_test() -> int:
    import tempfile

    with tempfile.TemporaryDirectory() as td:
        root = Path(td)
        good = root / "good.md"
        good.write_text(
            "# 好文档\n\n"
            "内核把启动信息按字段名交给下一阶段，后者只读这些字段。\n"
            "如果把四个位置实参直接传下去会怎样：交换两个同类型参数仍能编译，"
            "所以这里用一个命名结构体。\n"
            "参见 [`bootface`](<os/kernel-image/src/bootface.rs:fn bootface>)。\n"
            "\n```rust\n// D-061 在代码块内不影响出版标准\nvoid f(void);\n```\n",
            encoding="utf-8",
        )
        bad = root / "bad.md"
        bad.write_text(
            "# 坏文档\n\n"
            "> **编号图例**：D01-Fnn、R36、D-061 是过程编号。\n\n"
            "Rust 侧最初把固件收进一个 trait，它现在已被删除（见 D-061，提交 5ed051a53）。\n"
            "详见 DECISIONS.md。\n",
            encoding="utf-8",
        )
        # 直接测 scan_doc，绕开路径相对化
        good_hits = scan_doc(good)
        bad_hits = scan_doc(bad)
        if good_hits:
            print(f"self-test FAIL: 合规文档被判违规: {good_hits[:2]}")
            return 1
        kinds = {h["kind"] for h in bad_hits}
        need = {"DP-1", "DP-3", "DP-4", "DP-5"}
        if not need.issubset(kinds):
            print(f"self-test FAIL: 违规文档漏检类别 {need - kinds}（实际 {kinds}）")
            return 1
        # 代码块豁免
        fenced = root / "fenced.md"
        fenced.write_text("```\nR36 D-061 5ed051a53 旧版\n```\n", encoding="utf-8")
        if scan_doc(fenced):
            print("self-test FAIL: 代码块未豁免")
            return 1
    print("self-test PASS: 过程编号/中间引用/图例/迭代叙事全检；反事实句与代码块豁免")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description="正式文档出版标准门（V3.7 DP-1..DP-5）")
    ap.add_argument("files", nargs="*")
    ap.add_argument("--baseline", default=str(DEFAULT_BASELINE))
    ap.add_argument("--update-baseline", action="store_true")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        return self_test()
    docs = [Path(f) for f in args.files] if args.files else iter_formal_docs()
    return run(docs, Path(args.baseline), args.update_baseline)


if __name__ == "__main__":
    sys.exit(main())
