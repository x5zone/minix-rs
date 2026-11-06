#!/usr/bin/env python3
"""
eval-trace-check.py — review-loop 轨迹完整性检查

检查对象：一篇循环的 run 目录，至少包含
    loop.md
    metrics.md
    defect-ledger.md
    panel/*.md

检查内容：
  1. loop.md 的九个固定二级标题是否齐全，且每节有内容；
  2. 每个 panel/*.md 里的 `## FINDING <ID>` 是否在 defect-ledger.md 有处置行；
  3. 台账是否有孤儿行（处置了但面板里没有）；
  4. 每行是否有处置类（①②③④⑤ 或 fix/transfer/fp/wording/insufficient）；
  5. 处置类为 ① 或 fix 的行，复验列是否非空且不是占位；
  6. 处置类为 ⑤ 或 insufficient 的行，处置列是否写清还缺什么证据；
  7. metrics.md 的必需键是否齐全；
  8. metrics.md 的 `发现总数` 是否等于五项处置之和；
  9. defect-ledger.md 与 metrics.md 是否残留 TBD/待定 占位。

用法：
    python3 tools/eval-trace-check.py <run-dir>
    python3 tools/eval-trace-check.py <run-dir> --json
    python3 tools/eval-trace-check.py --self-test

退出码：
    0 = 全部通过（允许 WARN）
    1 = 有 FAIL
    2 = 用法或文件缺失错误
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import tempfile
from pathlib import Path

SECTION_TITLES = [
    "§0 基线",
    "§1 面板",
    "§2 发现",
    "§3 归因",
    "§4 修复",
    "§5 复验",
    "§6 指标",
    "§7 状态",
    "§8 给 meta-review 的不确定点",
]

FINDING_RE = re.compile(r"^##\s+FINDING\s+([A-Za-z][A-Za-z0-9_-]*)\s*$", re.MULTILINE)
METRIC_TOTAL_RE = re.compile(r"^-\s*发现总数\s*[:：]\s*(\d+)\s*$", re.MULTILINE)
METRIC_DISPOSITION_RE = re.compile(
    r"^-\s*处置\s*[:：]\s*①\s*(\d+)\s*/\s*②\s*(\d+)\s*/\s*③\s*(\d+)\s*/\s*④\s*(\d+)\s*/\s*⑤\s*(\d+)\s*$",
    re.MULTILINE,
)
PLACEHOLDERS = {"tbd", "待定", "-", "—", ""}
DISPOSITION_TOKENS = {
    "①": "①",
    "②": "②",
    "③": "③",
    "④": "④",
    "⑤": "⑤",
    "fix": "①",
    "transfer": "②",
    "fp": "③",
    "wording": "④",
    "insufficient": "⑤",
}


class Report:
    def __init__(self) -> None:
        self.items: list[dict] = []

    def add(self, level: str, check: str, message: str) -> None:
        self.items.append({"level": level, "check": check, "message": message})

    def fail(self, check: str, message: str) -> None:
        self.add("FAIL", check, message)

    def warn(self, check: str, message: str) -> None:
        self.add("WARN", check, message)

    def ok(self, check: str, message: str) -> None:
        self.add("PASS", check, message)

    @property
    def failed(self) -> bool:
        return any(item["level"] == "FAIL" for item in self.items)

    def as_dict(self) -> dict:
        return {
            "ok": not self.failed,
            "counts": {
                "pass": sum(1 for i in self.items if i["level"] == "PASS"),
                "warn": sum(1 for i in self.items if i["level"] == "WARN"),
                "fail": sum(1 for i in self.items if i["level"] == "FAIL"),
            },
            "items": self.items,
        }


def read_text(path: Path, report: Report, label: str) -> str | None:
    if not path.is_file():
        report.fail("file", f"缺少 {label}: {path}")
        return None
    return path.read_text(encoding="utf-8")


def check_loop_sections(text: str, report: Report) -> None:
    positions: list[int] = []
    missing: list[str] = []
    for title in SECTION_TITLES:
        marker = f"## {title}"
        idx = text.find(marker)
        if idx < 0:
            missing.append(title)
        else:
            positions.append(idx)
    if missing:
        report.fail("loop-sections", "loop.md 缺少固定标题: " + "、".join(missing))
        return
    report.ok("loop-sections", "loop.md 九个固定标题齐全")
    ordered = sorted(positions)
    for i, idx in enumerate(ordered):
        end = ordered[i + 1] if i + 1 < len(ordered) else len(text)
        body = text[idx:end].split("\n", 1)[1] if "\n" in text[idx:end] else ""
        if not body.strip():
            report.fail(
                "loop-sections",
                f"loop.md 的 `{SECTION_TITLES[i]}` 节没有内容",
            )


def parse_ledger(text: str, report: Report) -> tuple[list[str], dict[str, dict]]:
    lines = text.splitlines()
    header_index = None
    headers: list[str] = []
    for i, line in enumerate(lines):
        if line.strip().startswith("|") and "发现 ID" in line:
            header_index = i
            headers = [cell.strip() for cell in line.strip().strip("|").split("|")]
            break
    if header_index is None:
        report.fail("ledger", "defect-ledger.md 找不到含 `发现 ID` 的表头")
        return [], {}

    required = ["发现 ID", "处置类", "处置", "复验"]
    missing = [name for name in required if name not in headers]
    if missing:
        report.fail("ledger", "defect-ledger.md 表头缺少列: " + "、".join(missing))
        return [], {}

    idx_id = headers.index("发现 ID")
    idx_class = headers.index("处置类")
    idx_action = headers.index("处置")
    idx_verify = headers.index("复验")

    ids: list[str] = []
    rows: dict[str, dict] = {}
    for line in lines[header_index + 1 :]:
        stripped = line.strip()
        if not stripped.startswith("|"):
            break
        cells = [cell.strip() for cell in stripped.strip("|").split("|")]
        if all(re.fullmatch(r":?-{2,}:?", cell) for cell in cells):
            continue
        if len(cells) <= max(idx_id, idx_class, idx_action, idx_verify):
            report.fail("ledger", f"台账行列数不足: {line}")
            continue
        raw_ids = cells[idx_id]
        row_ids = [part for part in re.split(r"[、,，\s]+", raw_ids) if part]
        if not row_ids:
            report.fail("ledger", f"台账行没有发现 ID: {line}")
            continue
        disposition_raw = cells[idx_class].strip()
        token = disposition_raw[:1] if disposition_raw else ""
        canonical = DISPOSITION_TOKENS.get(token) or DISPOSITION_TOKENS.get(disposition_raw.lower())
        if canonical is None:
            report.fail(
                "ledger",
                f"{'、'.join(row_ids)} 的处置类无法识别: `{disposition_raw}`",
            )
            continue
        action = cells[idx_action]
        verify = cells[idx_verify]
        if canonical in {"①"} and verify.lower() in PLACEHOLDERS:
            report.fail("ledger", f"{'、'.join(row_ids)} 处置类为 ①，但复验列为空或占位")
        if canonical in {"⑤"} and (
            action.lower() in PLACEHOLDERS or len(action) <= 4
        ):
            report.fail(
                "ledger",
                f"{'、'.join(row_ids)} 处置类为 ⑤，但没有写清还缺什么证据",
            )
        for row_id in row_ids:
            if row_id in rows:
                report.fail("ledger", f"台账里发现 ID 重复: {row_id}")
            rows[row_id] = {
                "class": canonical,
                "action": action,
                "verify": verify,
            }
            ids.append(row_id)
    report.ok("ledger", f"台账解析到 {len(ids)} 条处置")
    return ids, rows


def check_metrics(text: str, report: Report) -> None:
    total_match = METRIC_TOTAL_RE.search(text)
    disposition_match = METRIC_DISPOSITION_RE.search(text)
    if total_match is None:
        report.fail("metrics", "metrics.md 缺少 `- 发现总数: <整数>`")
    if disposition_match is None:
        report.fail("metrics", "metrics.md 缺少标准 `- 处置: ① n / ② n / ③ n / ④ n / ⑤ n` 行")
    if total_match and disposition_match:
        total = int(total_match.group(1))
        parts = [int(disposition_match.group(i)) for i in range(1, 6)]
        if total != sum(parts):
            report.fail(
                "metrics",
                f"发现总数 {total} 与五项处置之和 {sum(parts)} 不一致",
            )
        else:
            report.ok("metrics", f"发现总数与处置之和一致（{total}）")

    required_keys = ["re-litigation", "adjacent 阻塞", "重构", "成本"]
    for key in required_keys:
        if not re.search(rf"^-\s*{re.escape(key)}\s*[:：]", text, re.MULTILINE):
            report.fail("metrics", f"metrics.md 缺少键 `{key}`")

    for line in text.splitlines():
        if line.strip().startswith("-") and re.search(r"TBD|待定", line, re.IGNORECASE):
            report.fail("metrics", f"metrics.md 残留占位: {line.strip()}")


def check_placeholders(text: str, report: Report, label: str) -> None:
    for line in text.splitlines():
        if re.search(r"\bTBD\b|待定", line, re.IGNORECASE):
            report.fail("placeholder", f"{label} 残留占位: {line.strip()}")


def collect_panel_findings(panel_dir: Path, report: Report) -> list[str]:
    if not panel_dir.is_dir():
        report.warn("panel", f"没有 panel 目录: {panel_dir}")
        return []
    ids: list[str] = []
    for path in sorted(panel_dir.glob("*.md")):
        text = path.read_text(encoding="utf-8")
        found = FINDING_RE.findall(text)
        if not found:
            report.warn("panel", f"{path.name} 没有 `## FINDING <ID>` 行")
        ids.extend(found)
    report.ok("panel", f"面板报告解析到 {len(ids)} 条发现")
    return ids


def check_run_dir(run_dir: Path, panel_dir: Path | None = None) -> Report:
    report = Report()
    if not run_dir.is_dir():
        report.fail("file", f"run 目录不存在: {run_dir}")
        return report

    loop_text = read_text(run_dir / "loop.md", report, "loop.md")
    metrics_text = read_text(run_dir / "metrics.md", report, "metrics.md")
    ledger_text = read_text(run_dir / "defect-ledger.md", report, "defect-ledger.md")
    if panel_dir is None:
        panel_dir = run_dir / "panel"
        if not panel_dir.is_dir():
            # 规范布局把原始面板输出放在 tmp/review-eval/<stage>/<doc>/panel；
            # 从 run 目录向上找第一个存在该相对路径的祖先（仓库根）。
            for parent in run_dir.resolve().parents:
                candidate = (
                    parent
                    / "tmp"
                    / "review-eval"
                    / run_dir.parent.name
                    / run_dir.name
                    / "panel"
                )
                if candidate.is_dir():
                    panel_dir = candidate
                    break
    panel_ids = collect_panel_findings(panel_dir, report)

    if loop_text is not None:
        check_loop_sections(loop_text, report)
        check_placeholders(loop_text, report, "loop.md")
    if metrics_text is not None:
        check_metrics(metrics_text, report)
    if ledger_text is not None:
        check_placeholders(ledger_text, report, "defect-ledger.md")
        ledger_ids, _rows = parse_ledger(ledger_text, report)
        panel_set = set(panel_ids)
        ledger_set = set(ledger_ids)
        missing = sorted(panel_set - ledger_set)
        orphan = sorted(ledger_set - panel_set)
        if missing:
            report.fail("coverage", "面板发现没有处置行: " + "、".join(missing))
        else:
            report.ok("coverage", "所有面板发现都有处置行")
        if orphan:
            report.fail("coverage", "台账有孤儿处置行: " + "、".join(orphan))
        else:
            report.ok("coverage", "台账没有孤儿行")

    return report


def print_report(report: Report, as_json: bool) -> None:
    if as_json:
        print(json.dumps(report.as_dict(), ensure_ascii=False, indent=2))
        return
    for item in report.items:
        print(f"[{item['level']:4}] {item['check']}: {item['message']}")
    counts = report.as_dict()["counts"]
    print(
        f"\n汇总: PASS {counts['pass']} / WARN {counts['warn']} / FAIL {counts['fail']}"
    )
    print("结论: " + ("FAIL" if report.failed else "PASS"))


GOOD_LOOP = "\n\n".join(f"## {title}\n本节的占位内容。" for title in SECTION_TITLES)
GOOD_PANEL = """# 面板报告

## FINDING FACT-01
- 位置：§1.1
- 问题：示例
- 影响：示例
- 证据或推理：E1
- 最小修复：示例
- 置信度：高
- 是否需源码核实：否
- 若我错了：示例

## FINDING FACT-02
- 位置：§1.2
- 问题：示例
- 影响：示例
- 证据或推理：E3
- 最小修复：示例
- 置信度：中
- 是否需源码核实：是
- 若我错了：示例
"""
GOOD_LEDGER = """# 缺陷台账

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-01 | P-FACT | P1 | 事实 | ① | 修复 §1.1 | E1 | `grep -n x` 命中 |
| FACT-02 | P-FACT | P2 | 措辞 | ③ | 误报，反证见 §3 | E3 | 不适用 |
"""
GOOD_METRICS = """# 指标

- 发现总数: 2
- 处置: ① 1 / ② 0 / ③ 1 / ④ 0 / ⑤ 0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 1 / token 1000 / 耗时 1min
"""


def write_fixture(root: Path, loop: str, ledger: str, metrics: str, panel: str) -> Path:
    run_dir = root / "run"
    (run_dir / "panel").mkdir(parents=True)
    (run_dir / "loop.md").write_text(loop, encoding="utf-8")
    (run_dir / "defect-ledger.md").write_text(ledger, encoding="utf-8")
    (run_dir / "metrics.md").write_text(metrics, encoding="utf-8")
    (run_dir / "panel" / "fact.md").write_text(panel, encoding="utf-8")
    return run_dir


def self_test() -> int:
    with tempfile.TemporaryDirectory() as tmp:
        good_dir = write_fixture(
            Path(tmp), GOOD_LOOP, GOOD_LEDGER, GOOD_METRICS, GOOD_PANEL
        )
        good = check_run_dir(good_dir)
        if good.failed:
            print("self-test FAIL: 正确样本被判 FAIL")
            print_report(good, False)
            return 1

    with tempfile.TemporaryDirectory() as tmp:
        bad_ledger = GOOD_LEDGER.replace(
            "| FACT-02 | P-FACT | P2 | 措辞 | ③ | 误报，反证见 §3 | E3 | 不适用 |\n", ""
        )
        bad_metrics = GOOD_METRICS.replace("- 发现总数: 2", "- 发现总数: 3")
        bad_dir = write_fixture(
            Path(tmp), GOOD_LOOP, bad_ledger, bad_metrics, GOOD_PANEL
        )
        bad = check_run_dir(bad_dir)
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed:
            print("self-test FAIL: 缺陷样本没有被判 FAIL")
            return 1
        if "FACT-02" not in messages:
            print("self-test FAIL: 没有报出缺失的 FACT-02")
            print_report(bad, False)
            return 1
        if "不一致" not in messages:
            print("self-test FAIL: 没有报出总数与处置之和不一致")
            print_report(bad, False)
            return 1

    print("self-test PASS: 正确样本通过，缺陷样本被拦截")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(
        description="review-loop 轨迹完整性检查",
    )
    parser.add_argument("run_dir", nargs="?", help="run 目录")
    parser.add_argument(
        "--panel-dir",
        help="面板原始输出目录（默认 <run-dir>/panel；规范布局在 tmp/review-eval/ 下）",
    )
    parser.add_argument("--json", action="store_true", help="输出 JSON")
    parser.add_argument("--self-test", action="store_true", help="运行内置自测")
    args = parser.parse_args()

    if args.self_test:
        return self_test()
    if not args.run_dir:
        parser.print_help()
        return 2

    panel_dir = Path(args.panel_dir) if args.panel_dir else None
    report = check_run_dir(Path(args.run_dir), panel_dir)
    print_report(report, args.json)
    return 1 if report.failed else 0


if __name__ == "__main__":
    sys.exit(main())
