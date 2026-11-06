#!/usr/bin/env python3
"""
trait-audit.py — Rust trait 实现面的事实供给

用途：给 review-loop 的 P-DESIGN 角色提供“哪些 trait 只有单实现、零实现或很少被引用”的读数。
它只给事实，不做裁决；单实现 trait 不一定坏，可能是测试接缝或未来扩展点，必须由设计角色结合语义判断。

扫描内容：
  - trait 定义（行首可带 pub / unsafe）；
  - trait 实现（`impl ... Trait for Type`，单行形态）；
  - trait 名称在全部 .rs 文件中的词频；
  - `dyn Trait` 出现次数。

启发式裁决：
  - NO-IMPL                 零实现
  - SINGLE-IMPL-LOW-USE     单实现且词频不超过 2（定义与实现各一次）
  - SINGLE-IMPL             单实现
  - MULTI-IMPL              两个及以上实现
  - 名称以 Manager/Service/Handler/Factory/Provider 结尾时追加 NAME-CANDIDATE

已知盲区（输出中的 [SCOPE] 行会重复声明）：
  - 宏生成的 impl、cfg 门控的 impl、跨行写法的 impl 可能漏计；
  - 同名 trait 定义在不同模块时，词频按全局名称统计，可能高估；
  - 词频包含注释、字符串与文档，可能高估引用；
  - 泛型约束、trait 别名、类型别名背后的 trait object 可能漏计。

用法：
    python3 tools/trait-audit.py
    python3 tools/trait-audit.py --root os --json
    python3 tools/trait-audit.py --include-tests
    python3 tools/trait-audit.py --self-test

退出码：
    0 = 正常运行
    2 = 用法或路径错误
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import tempfile
from collections import Counter
from pathlib import Path

TRAIT_DEF_RE = re.compile(
    r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:unsafe\s+)?trait\s+([A-Za-z_][A-Za-z0-9_]*)"
)
IMPL_RE = re.compile(
    r"^\s*impl(?:<[^>]*>)?\s+(?:([A-Za-z_][A-Za-z0-9_:<>]*)\s+for\s+)?([A-Za-z_][A-Za-z0-9_:<>]*)"
)
WORD_RE = re.compile(r"\b[A-Za-z_][A-Za-z0-9_]*\b")
NAME_CANDIDATE_RE = re.compile(r"(Manager|Service|Handler|Factory|Provider)$")

VERDICT_RANK = {
    "NO-IMPL": 0,
    "SINGLE-IMPL-LOW-USE": 1,
    "SINGLE-IMPL": 2,
    "MULTI-IMPL": 3,
}


def last_segment(path: str) -> str:
    return path.split("::")[-1].split("<")[0].strip()


SKIP_PARTS = {"target", ".git", ".dockercargo", ".cargo", "vendor"}


def collect_rs_files(root: Path, include_tests: bool) -> list[Path]:
    files: list[Path] = []
    for path in sorted(root.rglob("*.rs")):
        parts = set(path.parts)
        if parts & SKIP_PARTS:
            continue
        # 跳过 cargo registry 的第三方源码（os/.dockercargo/registry/src/...）
        if "registry" in parts and "src" in parts:
            continue
        if not include_tests and ("tests" in parts or path.name.endswith("_test.rs")):
            continue
        files.append(path)
    return files


def audit(root: Path, include_tests: bool) -> list[dict]:
    files = collect_rs_files(root, include_tests)
    word_counter: Counter[str] = Counter()
    definitions: list[dict] = []
    impl_counts: Counter[str] = Counter()
    dyn_counts: Counter[str] = Counter()

    for path in files:
        try:
            text = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue
        word_counter.update(WORD_RE.findall(text))
        for line_no, line in enumerate(text.splitlines(), start=1):
            def_match = TRAIT_DEF_RE.match(line)
            if def_match:
                definitions.append(
                    {
                        "name": def_match.group(1),
                        "path": str(path),
                        "line": line_no,
                    }
                )
                continue
            impl_match = IMPL_RE.match(line)
            if impl_match and impl_match.group(1):
                trait_name = last_segment(impl_match.group(1))
                impl_counts[trait_name] += 1
        for dyn_match in re.finditer(r"\bdyn\s+([A-Za-z_][A-Za-z0-9_]*)", text):
            dyn_counts[dyn_match.group(1)] += 1

    rows: list[dict] = []
    for definition in definitions:
        name = definition["name"]
        impl_count = impl_counts.get(name, 0)
        raw_refs = word_counter.get(name, 0) - 1 - impl_count
        ref_count = max(raw_refs, 0)
        if impl_count == 0:
            verdict = "NO-IMPL"
        elif impl_count == 1 and ref_count <= 2:
            verdict = "SINGLE-IMPL-LOW-USE"
        elif impl_count == 1:
            verdict = "SINGLE-IMPL"
        else:
            verdict = "MULTI-IMPL"
        if NAME_CANDIDATE_RE.search(name):
            verdict = verdict + "+NAME-CANDIDATE"
        rows.append(
            {
                "name": name,
                "path": definition["path"],
                "line": definition["line"],
                "impls": impl_count,
                "refs": ref_count,
                "dyn": dyn_counts.get(name, 0),
                "verdict": verdict,
            }
        )

    rows.sort(
        key=lambda row: (
            VERDICT_RANK.get(row["verdict"].split("+")[0], 99),
            row["impls"],
            row["refs"],
            row["path"],
            row["line"],
        )
    )
    return rows


def print_markdown(rows: list[dict], root: Path) -> None:
    print(
        f"[SCOPE] root={root} 文件={len(collect_rs_files(root, False))} "
        "只统计单行 trait 定义与单行 impl；宏、cfg、跨行写法可能漏计。"
    )
    print("")
    print("| trait | 位置 | 实现 | 引用 | dyn | 判定 |")
    print("|---|---|---:|---:|---:|---|")
    for row in rows:
        print(
            f"| `{row['name']}` | `{row['path']}:{row['line']}` | {row['impls']} | "
            f"{row['refs']} | {row['dyn']} | {row['verdict']} |"
        )
    print("")
    counts = Counter(row["verdict"].split("+")[0] for row in rows)
    print(
        "汇总: "
        + " / ".join(f"{name} {counts.get(name, 0)}" for name in VERDICT_RANK)
    )
    print("说明：判定只是事实分档，不是删除建议；单实现 trait 可能是测试接缝或扩展点。")


def self_test() -> int:
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        (root / "dead.rs").write_text(
            "pub trait DeadTrait {\n    fn x(&self);\n}\n", encoding="utf-8"
        )
        (root / "clock.rs").write_text(
            "pub trait ClockSource {\n    fn read(&self) -> u64;\n}\n"
            "pub struct Hpet;\n"
            "impl ClockSource for Hpet {\n    fn read(&self) -> u64 { 0 }\n}\n"
            "pub fn now(c: &Hpet) -> u64 { c.read() }\n",
            encoding="utf-8",
        )
        (root / "multi.rs").write_text(
            "pub trait Multi {\n    fn x(&self);\n}\n"
            "pub struct A;\npub struct B;\n"
            "impl Multi for A {\n    fn x(&self) {}\n}\n"
            "impl Multi for B {\n    fn x(&self) {}\n}\n",
            encoding="utf-8",
        )
        (root / "manager.rs").write_text(
            "pub trait ProcessManager {\n    fn get(&self);\n}\n"
            "pub struct Table;\n"
            "impl ProcessManager for Table {\n    fn get(&self) {}\n}\n"
            "pub fn use_it(t: &Table) { t.get(); }\n",
            encoding="utf-8",
        )
        rows = audit(root, include_tests=False)
        by_name = {row["name"]: row for row in rows}
        expected = {
            "DeadTrait": ("NO-IMPL", 0),
            "ClockSource": ("SINGLE-IMPL-LOW-USE", 1),
            "Multi": ("MULTI-IMPL", 2),
            "ProcessManager": ("SINGLE-IMPL", 1),
        }
        for name, (verdict, impls) in expected.items():
            row = by_name.get(name)
            if row is None:
                print(f"self-test FAIL: 没有发现 {name}")
                return 1
            if not row["verdict"].startswith(verdict):
                print(f"self-test FAIL: {name} 判定 {row['verdict']}，期望 {verdict}")
                return 1
            if row["impls"] != impls:
                print(f"self-test FAIL: {name} 实现数 {row['impls']}，期望 {impls}")
                return 1
        if "NAME-CANDIDATE" not in by_name["ProcessManager"]["verdict"]:
            print("self-test FAIL: ProcessManager 没有命中 NAME-CANDIDATE")
            return 1
    print("self-test PASS: 零实现、单实现、低引用、多实现与名称候选均符合预期")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="Rust trait 实现面事实供给")
    parser.add_argument("--root", default="os", help="扫描根目录，默认 os")
    parser.add_argument("--json", action="store_true", help="输出 JSON")
    parser.add_argument("--include-tests", action="store_true", help="包含 tests 目录")
    parser.add_argument("--self-test", action="store_true", help="运行内置自测")
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    root = Path(args.root)
    if not root.is_dir():
        print(f"错误: 目录不存在: {root}", file=sys.stderr)
        return 2

    rows = audit(root, args.include_tests)
    if args.json:
        print(json.dumps(rows, ensure_ascii=False, indent=2))
    else:
        print_markdown(rows, root)
    return 0


if __name__ == "__main__":
    sys.exit(main())
