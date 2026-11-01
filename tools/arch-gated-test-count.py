#!/usr/bin/env python3
"""arch-gated-test-count.py — SD-34 防回长门：统计"宿主跑不到"的架构门 `#[test]`。

背景（结构债寄存器 SD-34）：`#[cfg(target_arch = "aarch64"|"riscv64")]` 门住的 `#[test]`
在 x86_64 宿主测试形态里**根本不编译**，套件却照样报绿——「绿」证明不了这些门后的断言
跑过。2026-10 的清扫把这类测试从 ~90 迁成 `<crate>/tests/*.rs` 的形状/文本 pin（31/86
起做），本工具是那条清扫的**防回长半**：每个 crate 的此类计数冻结进基线，任何新增
（或未同步基线的挪动）都让门变红，而不是静默地少跑几个测试。

判据（两种形态都算，都是「宿主不编译」的形状）：
1. 直接叠在 `#[test]` 上的属性栈里有 `target_arch`（含 `#[cfg(all(...))]` 形式）；
2. `#[cfg(...target_arch...)]` 直接门住一个 `mod` 块，块内的 `#[test]` 全部计入。

用法:
  tools/arch-gated-test-count.py                 # 对账（基线外有新增/缺失即退 1）
  tools/arch-gated-test-count.py --list          # 只打印计数
  tools/arch-gated-test-count.py --update-baseline
  tools/arch-gated-test-count.py --self-test     # 正反例夹具自检
退出码: 0=通过 1=不一致 2=用法错误
"""
import argparse
import os
import re
import shutil
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BASELINE = os.path.join(ROOT, "tools", "arch-gated-test-count-baseline.txt")
SCAN_ROOT = os.path.join(ROOT, "os")
SKIP_DIRS = {"target", "target_smp", "tmp"}

ATTR = re.compile(r"^\s*#\[")
TEST = re.compile(r"^\s*#\[test\]\s*$")
ARCH = re.compile(r"target_arch")
MOD_OPEN = re.compile(r"^\s*(pub(\([^)]*\))?\s+)?mod\s+\w+\s*\{")
CFG_ARCH = re.compile(r"^\s*#\[cfg\((?:[^]]*target_arch[^]]*)\)\]\s*$")


def count_file(text):
    """Return (direct, gated_mod) counts for one file."""
    lines = text.split("\n")
    direct = 0
    for i, line in enumerate(lines):
        if not TEST.match(line):
            continue
        j = i - 1
        gated = False
        while j >= 0:
            stripped = lines[j].strip()
            if stripped == "" or stripped.startswith("//"):
                j -= 1
                continue
            if ATTR.match(lines[j]):
                if ARCH.search(lines[j]):
                    gated = True
                j -= 1
                continue
            break
        if gated:
            direct += 1
    # 模块级：cfg(target_arch) 后（可隔其它属性/注释）紧跟 mod xxx {
    in_mod = 0
    i = 0
    while i < len(lines):
        if CFG_ARCH.match(lines[i]):
            j = i + 1
            saw_mod = False
            while j < len(lines):
                stripped = lines[j].strip()
                if stripped == "" or stripped.startswith("//") or ATTR.match(lines[j]):
                    j += 1
                    continue
                saw_mod = bool(MOD_OPEN.match(lines[j]))
                break
            if saw_mod:
                depth = 0
                for k in range(j, len(lines)):
                    depth += lines[k].count("{") - lines[k].count("}")
                    if depth <= 0 and k > j - 1:
                        end = k
                        break
                else:
                    end = len(lines) - 1
                in_mod += sum(1 for line in lines[j:end + 1] if TEST.match(line))
                i = end
        i += 1
    return direct, in_mod


def scan(root):
    counts = {}
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = sorted(d for d in dirnames if d not in SKIP_DIRS)
        for name in sorted(filenames):
            if not name.endswith(".rs"):
                continue
            path = os.path.join(dirpath, name)
            with open(path, encoding="utf-8", errors="replace") as handle:
                direct, gated_mod = count_file(handle.read())
            if direct or gated_mod:
                rel = os.path.relpath(path, root)
                counts[rel] = direct + gated_mod
    return counts


def load_baseline(path):
    base = {}
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            rel, count = line.split("\t")
            base[rel] = int(count)
    return base


def write_baseline(path, counts):
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(
            "# SD-34 架构门测试计数基线（tools/arch-gated-test-count.py --update-baseline 生成）\n"
            "# 每行：文件相对 os/ 的路径 TAB 计数。新增架构门测试要么迁成 tests/ 形状 pin，\n"
            "# 要么在这里写明理由后抬基线；不许让它静默地不跑。\n"
        )
        for rel in sorted(counts):
            handle.write(f"{rel}\t{counts[rel]}\n")


def check(root, baseline_path, listing=False):
    counts = scan(root)
    if listing:
        for rel in sorted(counts):
            print(f"{counts[rel]}\t{rel}")
        print(f"总计 {sum(counts.values())} 枚（{len(counts)} 文件）")
        return 0
    base = load_baseline(baseline_path)
    problems = []
    for rel in sorted(set(counts) | set(base)):
        now = counts.get(rel, 0)
        was = base.get(rel, 0)
        if now != was:
            problems.append(f"  {rel}: 基线 {was} → 现在 {now}")
    if problems:
        print("❌ 架构门测试计数与基线不一致（SD-34：宿主编译不到的 #[test] 不许静默增减）")
        print("\n".join(problems))
        print(f"若为已迁移/已审阅的改动：python3 {os.path.relpath(__file__)} --update-baseline")
        return 1
    total = sum(counts.values())
    print(f"✅ 架构门测试计数与基线一致：{total} 枚 / {len(counts)} 文件（SD-34）")
    return 0


def self_test():
    temp = tempfile.mkdtemp(prefix="arch-gated-")
    try:
        os.makedirs(os.path.join(temp, "os", "a", "src"))
        with open(os.path.join(temp, "os", "a", "src", "lib.rs"), "w", encoding="utf-8") as handle:
            handle.write(
                "mod tests {\n"
                "    #[cfg(target_arch = \"aarch64\")]\n"
                "    #[test]\n"
                "    fn arm_only() {}\n"
                "    #[cfg(all(test, target_arch = \"riscv64\"))]\n"
                "    #[test]\n"
                "    fn rv_only() {}\n"
                "    #[test]\n"
                "    fn host_runs() {}\n"
                "    #[cfg(target_arch = \"aarch64\")]\n"
                "    mod arm_helpers {\n"
                "        #[test]\n"
                "        fn nested_arm_only() {}\n"
                "    }\n"
                "    #[cfg(test)]\n"
                "    mod plain {\n"
                "        #[test]\n"
                "        fn plain_test() {}\n"
                "    }\n"
                "}\n"
            )
        counts = scan(os.path.join(temp, "os"))
        got = counts.get(os.path.join("a", "src", "lib.rs"), 0)
        assert got == 3, f"自检失败：期望 3 枚（两枚直接 + 一枚模块级），实际 {got}"
        # 基线对账：同数通过，加一枚则红。
        base_path = os.path.join(temp, "baseline.txt")
        write_baseline(base_path, counts)
        assert check(os.path.join(temp, "os"), base_path) == 0, "同数基线该通过"
        with open(os.path.join(temp, "os", "a", "src", "lib.rs"), "a", encoding="utf-8") as handle:
            handle.write(
                "mod more {\n"
                "    #[cfg(target_arch = \"riscv64\")]\n"
                "    #[test]\n"
                "    fn new_rv_only() {}\n"
                "}\n"
            )
        assert check(os.path.join(temp, "os"), base_path) == 1, "新增门后测试该让门变红"
        print("✅ 自检通过：直接/模块级/gated(all(...)) 都计入，宿主可跑的不计，新增即红")
        return 0
    finally:
        shutil.rmtree(temp, ignore_errors=True)


def main():
    parser = argparse.ArgumentParser(add_help=True)
    parser.add_argument("--update-baseline", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--list", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    if args.update_baseline:
        counts = scan(SCAN_ROOT)
        write_baseline(BASELINE, counts)
        print(f"基线已写入 {os.path.relpath(BASELINE, ROOT)}：{sum(counts.values())} 枚 / {len(counts)} 文件")
        return 0
    return check(SCAN_ROOT, BASELINE, args.list)


if __name__ == "__main__":
    sys.exit(main())
