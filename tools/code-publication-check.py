#!/usr/bin/env python3
"""code-publication-check.py — 生产代码出版标准门（V3.7 / CP-1..CP-6）

用户的出版标准（2026-11-21 裁决）：代码与文档一样，必须达到「可直接出版/教学」的标准，
不允许把未完成、历史叙事、过程编号留在生产代码里。defer 是返工窗口，一律禁止。

判据（生产代码 = 非 `#[cfg(test)]` 区段）：
  CP-1 占位实现：`todo!` / `unimplemented!`；`unreachable!` 无现场理由注释
       （同行或上一行含 `// SAFETY:` / `// justified:` / `// unreachable because`）
  CP-2 静默抑制：`#[allow(dead_code)]` / `#[allow(unused…)]`
  CP-3 过程编号：注释里的 `D-nnn` / `I-n` / `F-Rnn-nn` / `Rnn` / 含字母的 7–40 位
       十六进制串（纯十进制常量如 `0170000` 不算提交号，不误报）
  CP-4 迭代叙事：注释里的 formerly / previously / originally / used to / was removed /
       no longer / legacy / 旧版 / 最初 / 曾经 / 后来 / 删除前 / 已删除 / 不再 / 改成 / 替代
  CP-5 未完成标记：注释里的 TODO / FIXME / XXX / HACK
  CP-6 被禁测试：`#[ignore]` 且无同行理由注释

基线机制：存量不阻断（当前树未达出版标准），新增即红；每篇迭代时把本篇关联代码的
基线清到 0 才算 CONVERGED。

用法：
  python3 tools/code-publication-check.py                  # 默认：os/ 下全部跟踪的 .rs
  python3 tools/code-publication-check.py FILE [FILE...]
  python3 tools/code-publication-check.py --self-test
"""
from __future__ import annotations

import argparse
import hashlib
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_BASELINE = ROOT / "tools/publication-baseline/code.txt"

STUB_RE = re.compile(r"\b(todo|unimplemented)!\s*\(")
UNREACH_RE = re.compile(r"\bunreachable!\s*\(")
ALLOW_RE = re.compile(r"#\[allow\((?:dead_code|unused(?:_\w+)?)\)\]")
PROCESS_RES = (
    re.compile(r"\bD-\d{3}\b"),
    re.compile(r"\bI-\d+\b"),
    re.compile(r"\bF-R\d{1,2}-\d+\b"),
    re.compile(r"\bR(?:1[3-9]|[2-9]\d)\b"),
    # 提交号候选：7–40 位十六进制，且至少含一个 a–f 字母。纯数字的
    # `0170000` 这类常量是十进制值，不是提交号；若允许纯数字，门会把
    # 正常的内存偏移/容量全判成过程编号。`0b…`/`0o…` 是二进制/八进制
    # 字面量，也不是提交号。
    re.compile(
        r"\b(?=[0-9a-f]{7,40}\b)(?=[0-9a-f]*[a-f])(?!0[bB])(?!0[oO])[0-9a-f]{7,40}\b"
    ),
)
ITER_RE = re.compile(
    r"\b(?:formerly|previously|originally|was removed|were removed|removed in|"
    r"no longer)\b|旧版|最初|曾经|后来|删除前|已删除|不再|改成|替代",
    re.IGNORECASE,
)
TODO_RE = re.compile(r"\b(?:TODO|FIXME|XXX|HACK)\b")
IGNORE_RE = re.compile(r"#\[ignore\b")
JUSTIFY_RE = re.compile(r"//\s*(?:SAFETY|justified|unreachable because)", re.IGNORECASE)
CFG_TEST_RE = re.compile(r"#\[cfg\(test\)\]")
COMMENT_RE = re.compile(r"//")


def tracked_rs() -> list[Path]:
    try:
        out = subprocess.run(
            ["git", "-C", str(ROOT), "ls-files", "--", "os/*.rs", "os/**/*.rs"],
            capture_output=True, text=True, check=True,
        ).stdout
    except Exception:
        return sorted(ROOT.glob("os/**/*.rs"))
    return [ROOT / ln for ln in out.splitlines() if ln.strip().endswith(".rs")]


def scan_file(path: Path) -> list[dict]:
    """扫一个 .rs 文件，跳过 `#[cfg(test)]` 修饰的项（用花括号深度跟踪）。

    `#[cfg(test)]` 既可能挂在 `mod tests { ... }` 上，也可能挂在单行
    `const` / `fn` 上。旧实现只在“深度回落到属性行之前”时退出测试区，
    单行项永远不会回落，于是文件剩余部分全被当成测试区（I-25 判例：
    `os/kernel/src/lib.rs` 有 13 处违规因此被假阴性藏掉）。这里显式区分
    三个状态：正常扫描、等待属性修饰的项、跳过测试项。
    """
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except Exception:
        return []
    hits: list[dict] = []
    depth = 0
    # normal：正常扫描；test_attr：已见 #[cfg(test)]，等待它修饰的项；
    # test_item：正在跳过该项。test_depth 是项开始前的花括号深度。
    state = "normal"
    test_depth = 0
    item_opened = False
    for i, line in enumerate(lines, 1):
        stripped = line.strip()
        delta = line.count("{") - line.count("}")

        if state == "test_attr":
            # 空行、注释与其它属性仍属于同一个项的前导。
            if not stripped or stripped.startswith("#[") or stripped.startswith("//"):
                depth += delta
                continue
            state = "test_item"
            test_depth = depth
            item_opened = "{" in line
            depth += delta
            if item_opened:
                if depth <= test_depth:
                    state = "normal"
            elif stripped.endswith(";"):
                state = "normal"
            continue

        if state == "test_item":
            if "{" in line:
                item_opened = True
            depth += delta
            if item_opened and depth <= test_depth:
                state = "normal"
            elif not item_opened and stripped.endswith(";"):
                state = "normal"
            continue

        # state == "normal"
        cfg = CFG_TEST_RE.match(stripped)
        if cfg:
            rest = stripped[cfg.end():].strip()
            depth += delta
            if not rest or rest.startswith("#["):
                state = "test_attr"
            elif "{" in rest:
                # 属性与项同行且带块：块在关闭行结束时退出测试区。
                test_depth = depth - delta
                item_opened = True
                state = "normal" if depth <= test_depth else "test_item"
            else:
                # 属性与无花括号项同行（`const` / `use` / `mod x;`）。
                state = "normal"
            continue

        prev = lines[i - 2] if i >= 2 else ""
        comment = COMMENT_RE.search(line)
        if STUB_RE.search(line):
            hits.append({"kind": "CP-1", "line": i, "text": stripped,
                         "token": STUB_RE.search(line).group(0)})
        elif UNREACH_RE.search(line) and not (JUSTIFY_RE.search(line) or JUSTIFY_RE.search(prev)):
            hits.append({"kind": "CP-1", "line": i, "text": stripped, "token": "unreachable!"})
        if ALLOW_RE.search(line):
            hits.append({"kind": "CP-2", "line": i, "text": stripped, "token": "allow"})
        if IGNORE_RE.search(line) and not re.search(r"//", line):
            hits.append({"kind": "CP-6", "line": i, "text": stripped, "token": "#[ignore]"})
        if comment:
            tail = line[comment.start():]
            for pat in PROCESS_RES:
                m = pat.search(tail)
                if m:
                    hits.append({"kind": "CP-3", "line": i, "text": stripped,
                                 "token": m.group(0)})
                    break
            m = ITER_RE.search(tail)
            if m:
                hits.append({"kind": "CP-4", "line": i, "text": stripped, "token": m.group(0)})
            m = TODO_RE.search(tail)
            if m:
                hits.append({"kind": "CP-5", "line": i, "text": stripped, "token": m.group(0)})
        depth += delta
    return hits


def key_for(path: Path, hit: dict) -> str:
    norm = re.sub(r"\s+", " ", hit["text"]).strip()
    digest = hashlib.sha1(norm.encode("utf-8")).hexdigest()[:16]
    rel = path.relative_to(ROOT) if path.is_relative_to(ROOT) else path
    return f"{rel}|{hit['kind']}|{digest}"


def load_baseline(path: Path) -> set[str]:
    if not path.is_file():
        return set()
    return {ln.strip() for ln in path.read_text(encoding="utf-8").splitlines() if ln.strip()}


def run(files: list[Path], baseline: Path) -> int:
    known = load_baseline(baseline)
    new_hits: list[tuple[Path, dict]] = []
    known_count = 0
    per_kind: dict[str, int] = {}
    for f in files:
        for hit in scan_file(f):
            k = key_for(f, hit)
            per_kind[hit["kind"]] = per_kind.get(hit["kind"], 0) + 1
            if k in known:
                known_count += 1
            else:
                new_hits.append((f, hit))
    print(f"[统计] 扫描 {len(files)} 个 .rs；命中 " +
          "  ".join(f"{k}={v}" for k, v in sorted(per_kind.items())))
    if known_count:
        print(f"[基线] 已有 {known_count} 处（不阻断，列入 burn-down）")
    if new_hits:
        for f, hit in new_hits[:60]:
            rel = f.relative_to(ROOT) if f.is_relative_to(ROOT) else f
            print(f"[FAIL] {rel}:{hit['line']} {hit['kind']} `{hit['token']}` — {hit['text'][:100]}")
        if len(new_hits) > 60:
            print(f"... 另有 {len(new_hits) - 60} 处新增")
        print(f"code-publication-check: {len(new_hits)} 处**新增**违规（基线内 {known_count} 处）")
        return 1
    print("code-publication-check: 无新增违规")
    return 0


def self_test() -> int:
    import tempfile

    with tempfile.TemporaryDirectory() as td:
        root = Path(td)
        good = root / "good.rs"
        good.write_text(
            "//! 模块文档\n"
            "fn f(x: u32) -> u32 { x + 1 }\n"
            "#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() { todo!() }\n}\n",
            encoding="utf-8",
        )
        bad = root / "bad.rs"
        bad.write_text(
            "// D-061 裁决过；最初写成 trait，后来删除\n"
            "fn f() { unimplemented!() }\n"
            "#[allow(dead_code)]\nfn g() {}\n"
            "// TODO: 以后补\n"
            "fn h() { unreachable!() }\n",
            encoding="utf-8",
        )
        if scan_file(good):
            print(f"self-test FAIL: 合规文件被判违规 {scan_file(good)[:2]}")
            return 1
        kinds = {h["kind"] for h in scan_file(bad)}
        need = {"CP-1", "CP-2", "CP-3", "CP-4", "CP-5"}
        if not need.issubset(kinds):
            print(f"self-test FAIL: 漏检 {need - kinds}（实际 {kinds}）")
            return 1
        justified = root / "justified.rs"
        justified.write_text(
            "// justified: 该分支由构造器排他，unreachable 只防未来改动\n"
            "fn f() { unreachable!() }\n",
            encoding="utf-8",
        )
        if scan_file(justified):
            print("self-test FAIL: 有现场理由的 unreachable! 被误判")
            return 1
        inline = root / "inline.rs"
        inline.write_text(
            "#[cfg(test)]\n"
            "mod tests {\n"
            "    #[test]\n"
            "    fn t() { todo!() }\n"
            "}\n"
            "#[cfg(test)]\n"
            "const TEST_LIMIT: u32 = 0x1000;\n"
            "#[cfg(test)]\n"
            "fn helper(\n"
            "    x: u32,\n"
            ") -> u32 {\n"
            "    todo!()\n"
            "}\n"
            "// D-061 生产区过程编号\n"
            "fn f() {}\n",
            encoding="utf-8",
        )
        inline_hits = scan_file(inline)
        if not any(h["kind"] == "CP-3" and "D-061" in h["text"] for h in inline_hits):
            print("self-test FAIL: 无花括号 cfg(test) 之后的生产违规未被检出（I-25 判例）")
            return 1
        if any("todo!" in h["text"] for h in inline_hits):
            print("self-test FAIL: cfg(test) 项内的 todo! 被当成生产违规")
            return 1
        hexfile = root / "hex.rs"
        hexfile.write_text(
            "// 0170000 是十进制偏移，不是提交号\n"
            "// 0b010000 是二进制字面量，不是提交号\n"
            "// c9f4e544c 是提交号\n"
            "fn f() {}\n",
            encoding="utf-8",
        )
        hex_tokens = {h["token"] for h in scan_file(hexfile) if h["kind"] == "CP-3"}
        if "0170000" in hex_tokens:
            print("self-test FAIL: 纯十进制常量被当成十六进制提交号")
            return 1
        if "0b010000" in hex_tokens:
            print("self-test FAIL: 二进制字面量被当成十六进制提交号")
            return 1
        if "c9f4e544c" not in hex_tokens:
            print("self-test FAIL: 真实十六进制提交号未被检出")
            return 1
    print("self-test PASS: 占位/抑制/过程编号/迭代叙事/TODO 全检；测试区与理由注释豁免")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description="生产代码出版标准门（V3.7 CP-1..CP-6）")
    ap.add_argument("files", nargs="*")
    ap.add_argument("--baseline", default=str(DEFAULT_BASELINE))
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        return self_test()
    files = [Path(f) for f in args.files] if args.files else tracked_rs()
    return run(files, Path(args.baseline))


if __name__ == "__main__":
    sys.exit(main())
