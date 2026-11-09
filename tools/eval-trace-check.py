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
  4. 每行是否有处置类（①②④⑤⑥FP 或 fix/transfer/wording/insufficient/refactor/false-positive）；R25+ 拒绝已删除的 ③；
  5. 处置类为 ①/④/⑥ 的行，复验列是否非空且不是“未修/待修/未实施”；
  6. 处置类为 ⑤ 的行是否写清取证路径；FP 是否在 `证据` 列给出反证；② 是否有用户批准 + 日期；
  7. metrics.md 的必需键是否齐全；R25+ 必须有 `未闭环: 0`；
  8. metrics.md 的 `发现总数` 是否等于六项处置之和；
  9. defect-ledger.md 与 metrics.md 是否残留 TBD/待定 占位；
  10. 六步收口合同：loop.md 中编号 ≥ 13 的 `# Round` 小节必须含 `### L6` 与
      `### L6.5` 收口小节（R0–R12 为历史轮次，豁免；合同见
      runs/<stage>/<doc>/DECISIONS-PENDING.md 第 5 项裁决）。

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

FINDING_RE = re.compile(r"^#{2,3}\s+FINDING\s+([A-Za-z][A-Za-z0-9_-]*)", re.MULTILINE)
# Round-tagged detailed-section style (fact.md R6 uses `### R6-04　…`): the
# round prefix is part of the ID namespace used in that round's ledger rows.
FINDING_ROUND_RE = re.compile(r"^#{2,3}\s+((?:R|D)\d+-\d+)\s*[　\s]", re.MULTILINE)
METRIC_TOTAL_RE = re.compile(r"^-\s*发现总数\s*[:：]\s*(\d+)", re.MULTILINE)
METRIC_COUNT_RE = re.compile(r"([①②④⑤⑥③]|FP)\s*(\d+)")
PLACEHOLDERS = {"tbd", "待定", "-", "—", "", "（待补）", "待补"}
DISPOSITION_TOKENS = {
    "①": "①",
    "②": "②",
    "④": "④",
    "⑤": "⑤",
    "⑥": "⑥",
    "FP": "FP",
    "fix": "①",
    "transfer": "②",
    "wording": "④",
    "insufficient": "⑤",
    "refactor": "⑥",
    "false-positive": "FP",
    "fp": "FP",
    # Legacy-only token: accepted for rounds before 25, rejected for R25+.
    "③": "LEGACY",
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


def check_loop_sections(text: str, report: Report, round_tag: str | None = None) -> None:
    # Round-scoped: a per-round panel must be answered by that round's loop
    # section. Whole-file search lets a deleted round section pass because the
    # same fixed titles exist in older rounds (B-31 residual).
    if round_tag:
        all_headings = [
            (m.start(), m.end(), m.group(0))
            for m in re.finditer(rf"^## .*{re.escape(round_tag)}.*$", text, re.MULTILINE)
        ]
        headings = [(a, b) for a, b, line in all_headings if "面板轮" in line] or [
            (a, b) for a, b, _line in all_headings
        ]
        if not headings:
            report.fail("loop-sections", f"loop.md 缺少 {round_tag} 面板轮小节")
            return
        if headings:
            start = headings[0][0]
            following = re.search(r"^## ", text[headings[0][1]:], re.MULTILINE)
            end = headings[0][1] + following.start() if following else len(text)
            text = text[start:end]
    positions: list[int] = []
    missing: list[str] = []
    for title in SECTION_TITLES:
        marker = f"§{title.lstrip('§')}"
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


RANGE_RE = re.compile(r"^([A-Z][A-Za-z0-9-]*?)-(\d+)\.\.(\d+)$")


def expand_id_token(token: str) -> list[str]:
    """Expand `READER-A-01..20` into the individual IDs used by panels.

    Range rows keep multi-round ledgers readable; coverage still needs the
    concrete IDs, otherwise a whole reader family silently escapes the check.
    """
    m = RANGE_RE.match(token)
    if not m:
        return [token]
    prefix, start, end = m.group(1), int(m.group(2)), int(m.group(3))
    width = len(m.group(2))
    return [f"{prefix}-{i:0{width}d}" for i in range(start, end + 1)]


RANGE_RE = re.compile(r"^([A-Z][A-Za-z0-9-]*?)-(\d+)\.\.(\d+)$")
SEP_RE = re.compile(r"^:?-{2,}:?$")


def expand_id_token(token: str) -> list[str]:
    """Expand `READER-A-01..20` into the individual IDs used by panels.

    Range rows keep multi-round ledgers readable; coverage still needs the
    concrete IDs, otherwise a whole reader family silently escapes the check.
    """
    m = RANGE_RE.match(token)
    if not m:
        return [token]
    prefix, start, end = m.group(1), int(m.group(2)), int(m.group(3))
    width = len(m.group(2))
    return [f"{prefix}-{i:0{width}d}" for i in range(start, end + 1)]


def _prefix_of(token: str) -> str | None:
    m = re.match(r"^([A-Z][A-Za-z0-9-]*?)-(\d+)$", token)
    return m.group(1) if m else None


def parse_ledger(text: str, report: Report) -> tuple[list[str], dict[str, dict], dict[str, set[str]], dict[str, dict[str, str]]]:
    lines = text.splitlines()
    ids: list[str] = []
    rows: dict[str, dict] = {}
    section_ids: dict[str, set[str]] = {}
    section_classes: dict[str, dict[str, str]] = {}
    current: dict[str, int] | None = None
    section = ""
    table_seen: set[str] = set()
    range_covered: set[str] = set()

    for i, line in enumerate(lines):
        if line.startswith("## "):
            section = line.strip("# ").strip()
        stripped = line.strip()
        if not stripped.startswith("|"):
            continue
        cells = [cell.strip() for cell in stripped.strip("|").split("|")]
        if all(SEP_RE.fullmatch(cell) for cell in cells):
            continue
        # Every markdown table header is followed by a separator row. Use that
        # to reset the column mapping at table boundaries; otherwise the RB
        # tables (4 columns) get parsed with the previous round's indices.
        next_is_sep = False
        if i + 1 < len(lines):
            nxt = lines[i + 1].strip()
            if nxt.startswith("|"):
                next_cells = [c.strip() for c in nxt.strip("|").split("|")]
                next_is_sep = bool(next_cells) and all(
                    SEP_RE.fullmatch(c) for c in next_cells
                )
        if next_is_sep:
            try:
                current = {
                    "id": cells.index("发现 ID"),
                    "cls": cells.index("处置类"),
                    "act": cells.index("处置"),
                    "ver": cells.index("复验"),
                    "ev": cells.index("证据") if "证据" in cells else -1,
                }
                table_seen = set()
                range_covered = set()
            except ValueError:
                current = None
            continue
        if current is None:
            continue
        if len(cells) <= max(current.values()):
            report.fail("ledger", f"台账行列数不足: {line}")
            continue
        raw_ids = re.sub(r"CLAIM\s+(\d+)", lambda m: f"CLAIM-{int(m.group(1)):02d}", cells[current["id"]])
        row_ids: list[str] = []
        last_prefix: str | None = None
        # `DESIGN-D2-01、03、06` carries the prefix across comma-separated
        # bare numbers; `FACT-01、CLAIM 01` must NOT turn the claim number
        # into a second FACT-01, so only a chunk that is a bare number alone
        # inherits the previous prefix.
        for chunk in re.split(r"[、,，]", raw_ids):
            sub = [part for part in chunk.split() if part]
            if not sub:
                continue
            if len(sub) == 1 and re.fullmatch(r"\d{1,2}", sub[0]) and last_prefix:
                sub = [f"{last_prefix}-{int(sub[0]):02d}"]
            for token in sub:
                token = re.sub(r"^R\d+/", "", token)
                expanded = expand_id_token(token)
                if ".." in token:
                    range_covered.update(expanded)
                row_ids.extend(expanded)
                last_prefix = _prefix_of(token) or last_prefix
        if not row_ids:
            report.fail("ledger", f"台账行没有发现 ID: {line}")
            continue
        disposition_raw = cells[current["cls"]].strip()
        token = disposition_raw[:1] if disposition_raw else ""
        canonical = DISPOSITION_TOKENS.get(token) or DISPOSITION_TOKENS.get(
            disposition_raw.lower()
        )
        if canonical is None:
            report.fail(
                "ledger",
                f"{'、'.join(row_ids)} 的处置类无法识别: `{disposition_raw}`",
            )
            continue
        action = cells[current["act"]]
        verify = cells[current["ver"]]
        evidence = cells[current["ev"]] if current["ev"] >= 0 else ""
        if canonical in {"①", "④", "⑥"} and verify.lower() in PLACEHOLDERS:
            report.fail(
                "ledger",
                f"{'、'.join(row_ids)} 处置类为 {canonical}，但复验列为空或占位"
                "（轮转闸：可修项必须有已修证据）",
            )
        if canonical == "FP" and (
            evidence.lower() in PLACEHOLDERS
            or len(evidence) < 20
            or re.search(r"源码|测试|文档原文|refute|cargo|grep|\.rs|§", evidence) is None
            or re.search(r"作者取舍|待裁决|未实施|无新证据", evidence) is not None
        ):
            report.fail(
                "ledger",
                f"{'、'.join(row_ids)} 处置类为 FP，但 `证据` 列没有给出源码/测试/文档原文反证（或仍写作者取舍/待裁决）",
            )
        id_like_row = re.compile(r"^[A-Z]{2,}-[A-Za-z0-9-]+$")
        round_prefix_row = re.compile(r"^(?:R|D)\d+-\d+$")
        for row_id in row_ids:
            is_id_like = bool(id_like_row.match(row_id) or round_prefix_row.match(row_id))
            # A range row (`READER-B-01..20`) followed by a single-ID
            # refinement (`READER-B-15 | ③`) is intentional: the specific
            # disposition overrides the range. Only flag duplicates that are
            # not explained by such a refinement.
            refinement = row_id in range_covered and len(row_ids) == 1
            if is_id_like and row_id in table_seen and not refinement:
                report.fail("ledger", f"台账里发现 ID 重复: {row_id}（节：{section}）")
            table_seen.add(row_id)
            rows[row_id] = {
                "class": canonical,
                "action": action,
                "verify": verify,
                "evidence": evidence,
                "section": section,
            }
            section_ids.setdefault(section, set()).add(row_id)
            section_classes.setdefault(section, {})[row_id] = canonical
            ids.append(row_id)
    report.ok("ledger", f"台账解析到 {len(ids)} 条处置")
    return ids, rows, section_ids, section_classes


def _metric_blocks(text: str) -> list[tuple[str, str]]:
    """Split metrics.md into `##` sections so a multi-round file is checked
    per round instead of always reading the first (oldest) block."""
    blocks: list[tuple[str, str]] = []
    title = ""
    body: list[str] = []
    for line in text.splitlines():
        if line.startswith("## "):
            if title or body:
                blocks.append((title, "\n".join(body)))
            title = line.strip("# ").strip()
            body = [line]
        else:
            body.append(line)
    if title or body:
        blocks.append((title, "\n".join(body)))
    return blocks


def _pick_metric_block(text: str, report: Report, round_tag: str | None) -> str:
    blocks = _metric_blocks(text)
    if round_tag:
        for title, body in blocks:
            if round_tag.lower() in title.lower():
                return body
    # No round heading: a single-block file is the normal case; otherwise the
    # last block is the newest round.
    return blocks[-1][1] if blocks else text


def check_metrics(text: str, report: Report, round_tag: str | None = None, expected: dict[str, int] | None = None) -> None:
    body = _pick_metric_block(text, report, round_tag)
    total_match = METRIC_TOTAL_RE.search(body)
    counts = {m.group(1): int(m.group(2)) for m in METRIC_COUNT_RE.finditer(body)}
    round_num = int(round_tag.lstrip("R")) if round_tag else None
    new_schema = (round_num is not None and round_num >= 25) or ("未闭环" in body)
    legacy = not new_schema
    if legacy:
        required = list("①②③④⑤")
    else:
        required = ["①", "②", "④", "⑤", "⑥", "FP"]
    missing = [k for k in required if k not in counts]
    if missing:
        report.fail("metrics", "metrics.md 缺少处置计数: " + "、".join(missing))
    if not legacy and "③" in counts:
        report.fail("metrics", "R25+ 禁止使用已删除的 ③；误报写 FP 并附反证，否则写 ⑤ 未闭环")
    if total_match is not None and not missing:
        total = int(total_match.group(1))
        parts = [counts[k] for k in required]
        if total != sum(parts):
            report.fail(
                "metrics",
                f"发现总数 {total} 与处置之和 {sum(parts)} 不一致",
            )
        else:
            report.ok("metrics", f"发现总数与处置之和一致（{total}）")
        if expected is not None:
            for token in required:
                want = expected.get(token, 0)
                got = counts.get(token, 0)
                if want != got:
                    report.fail(
                        "metrics",
                        f"metrics 分档 {token}={got} 与台账本轮逐 ID 聚合 {token}={want} 不一致",
                    )
                    break
            else:
                report.ok("metrics", "metrics 各分档与台账本轮逐 ID 聚合一致")

    if not legacy:
        open_match = re.search(r"^-\s*未闭环\s*[:：]\s*(\d+)", body, re.MULTILINE)
        if open_match is None:
            report.fail("metrics", "metrics.md 缺少 `- 未闭环: <整数>`（R25+ 强制）")
        elif int(open_match.group(1)) != 0:
            report.fail(
                "metrics",
                f"本轮仍有未闭环 {open_match.group(1)} 项；本轮不得关闭，禁止派下一轮面板",
            )
        else:
            report.ok("metrics", "未闭环 = 0，本轮可关闭")
        round_type = re.search(r"^-\s*轮型\s*[:：]\s*(discovery|verification)", body, re.MULTILINE)
        if round_type is None:
            report.fail("metrics", "metrics.md 缺少 `- 轮型: discovery|verification`（R25+ 强制）")
        elif round_type.group(1) == "verification":
            new_find = re.search(r"^-\s*新 in-scope 真缺陷\s*[:：]\s*(\d+)", body, re.MULTILINE)
            if new_find is None:
                report.fail("metrics", "verification 轮缺少 `- 新 in-scope 真缺陷: <整数>`")
            elif int(new_find.group(1)) != 0:
                report.fail(
                    "metrics",
                    f"verification 轮仍有 {new_find.group(1)} 条新 in-scope 真缺陷；不得 CONVERGED",
                )
            else:
                report.ok("metrics", "verification 轮新 in-scope 真缺陷 = 0，可判 CONVERGED")

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
    # Panel outputs live both flat (round 0) and in round subdirectories
    # (round1/, round2/, ...) — recurse so every round is checked.
    paths = sorted(panel_dir.glob("*.md")) + sorted(
        p for p in panel_dir.glob("*/*.md") if p.is_file()
    )
    if not paths:
        report.warn("panel", f"面板目录没有任何 .md（含轮次子目录）: {panel_dir}")
        return []
    for path in paths:
        text = path.read_text(encoding="utf-8")
        found = FINDING_RE.findall(text) + FINDING_ROUND_RE.findall(text)
        # Non-`## FINDING` shapes still count as findings in a round's panel:
        # fact.md's secondary bullets and refute.md's numbered claims.
        found += [f"FACT-{n}" for n in re.findall(r"^-\s*\*\*FACT-(\d+)\*\*", text, re.MULTILINE)]
        found += [f"CLAIM-{int(n):02d}" for n in re.findall(r"^##\s+CLAIM\s+(\d+)", text, re.MULTILINE)]
        found += re.findall(r"^\*\*(DESIGN-D2-\d+)", text, re.MULTILINE)
        if not found:
            report.warn("panel", f"{path.relative_to(panel_dir)} 没有 `## FINDING <ID>` 行")
        ids.extend(found)
    seen: set[str] = set()
    ids = [i for i in ids if not (i in seen or seen.add(i))]
    report.ok("panel", f"面板报告解析到 {len(ids)} 条发现（含轮次子目录）")
    return ids


ROUND_HEADER_RE = re.compile(r"^# Round (\d+)", re.MULTILINE)


def check_round_closure(loop_text: str, report: Report) -> None:
    """六步收口合同（DECISIONS-PENDING 第 5 项裁决）：编号 >= 13 的
    `# Round` 小节必须包含 `### L6`（meta 记录）与 `### L6.5`（三问收口
    记录）两个三级小节；R0–R12 为历史轮次，豁免。"""
    matches = list(ROUND_HEADER_RE.finditer(loop_text))
    rounds = []
    for i, m in enumerate(matches):
        start = m.start()
        end = matches[i + 1].start() if i + 1 < len(matches) else len(loop_text)
        num = int(m.group(1))
        if num >= 13:
            rounds.append((m.group(1), start, end))
    if not rounds:
        report.ok("round-closure", "无 R13+ 轮次，收口检查跳过（合同自 R13 起生效）")
        return
    for label, start, end in rounds:
        body = loop_text[start:end]
        missing = [name for name in ("### L6", "### L6.5") if name not in body]
        if missing:
            report.fail(
                "round-closure",
                f"Round {label} 缺少收口小节: {'、'.join(missing)}"
                "（六步收口合同：L6 meta + L6.5 三问记录）",
            )
        else:
            report.ok("round-closure", f"Round {label} 收口小节齐全（### L6 / ### L6.5）")


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
    round_match = re.search(r"round(\d+)", panel_dir.name) if panel_dir else None
    round_tag = f"R{int(round_match.group(1))}" if round_match else None
    if loop_text is not None and panel_dir is not None and panel_dir.is_dir():
        # V2 轨迹门：每个 panel 文件要么有 FINDING，要么在 loop.md §1
        # 显式写“无发现面板：<file>（理由）”豁免。WARN 不再是可接受状态。
        for path in sorted(panel_dir.glob("*.md")):
            text = path.read_text(encoding="utf-8")
            has_finding = bool(
                FINDING_RE.search(text)
                or FINDING_ROUND_RE.search(text)
                or re.search(r"^##\s+CLAIM\s+\d+", text, re.MULTILINE)
                or re.search(r"^-\s*\*\*FACT-\d+\*\*", text, re.MULTILINE)
            )
            if has_finding:
                continue
            exempt = re.search(
                rf"无发现面板[：:][^\n]*{re.escape(path.name)}|{re.escape(path.name)}[^\n]*无发现",
                loop_text,
            )
            if exempt:
                report.ok("panel-exemption", f"{path.name} 无 FINDING，loop.md 已豁免")
            else:
                report.fail(
                    "panel-exemption",
                    f"{path.name} 没有 FINDING，且 loop.md §1 没有“无发现面板：{path.name}”豁免行",
                )

    if loop_text is not None:
        check_loop_sections(loop_text, report, round_tag)
        check_round_closure(loop_text, report)
        check_placeholders(loop_text, report, "loop.md")
    expected_counts = None
    if ledger_text is not None:
        check_placeholders(ledger_text, report, "defect-ledger.md")
        ledger_ids, ledger_rows, section_ids, section_classes = parse_ledger(ledger_text, report)
        panel_set = set(panel_ids)
        ledger_set = set(ledger_ids)
        # Round-scoped coverage: a per-round --panel-dir (round18) must be
        # answered by that round's ledger section. Without this, deleting the
        # newest round's table still passes because short IDs repeat in older
        # rounds (B-51 whole-file loophole).
        if round_tag:
            scoped = {
                fid
                for section, fids in section_ids.items()
                if section.lower().startswith(round_tag.lower())
                for fid in fids
            }
            # No fallback to the whole ledger: if this round's section is
            # missing, every panel finding must be reported missing.
            coverage_ledger = scoped
            from collections import Counter
            expected_counts: dict[str, int] = {}
            counter: Counter[str] = Counter()
            for section, mapping in section_classes.items():
                if section.lower().startswith(round_tag.lower()):
                    counter.update(
                        cls for fid, cls in mapping.items() if fid in panel_set
                    )
            expected_counts = dict(counter)
            if int(round_tag.lstrip("R")) >= 25:
                for fid in sorted(scoped):
                    row = ledger_rows.get(fid)
                    if row is None:
                        continue
                    cls = row["class"]
                    if cls == "LEGACY":
                        report.fail(
                            "round-close",
                            f"{fid} 使用了已删除的 ③；R25+ 只能写 FP（附反证）或 ⑤（未闭环）",
                        )
                    elif cls == "⑤":
                        report.fail(
                            "round-close",
                            f"{fid} 处置类为 ⑤（未闭环）；本轮不得关闭",
                        )
                    elif cls == "②":
                        action = row["action"]
                        if "用户批准" not in action or re.search(r"20\d{2}-\d{2}-\d{2}", action) is None:
                            report.fail(
                                "round-close",
                                f"{fid} 处置类为 ②，但没有用户批准 + YYYY-MM-DD；按未闭环计",
                            )
                    elif cls in {"①", "④", "⑥"} and re.search(
                        r"未修|待修|未实施|TODO", row["verify"], re.IGNORECASE
                    ):
                        report.fail(
                            "round-close",
                            f"{fid} 处置类为 {cls}，但复验写“未修/待修/未实施”；本轮不得关闭",
                        )
        else:
            coverage_ledger = ledger_set
        # Orphan reference set: IDs that appear in ANY panel file (the
        # selected round plus its sibling rounds and the flat files at the
        # parent level). A per-round --panel-dir must not turn other rounds'
        # ledger rows into orphans.
        panel_text_ids = set()
        roots = [panel_dir]
        if panel_dir.parent != panel_dir and panel_dir.parent.is_dir():
            roots.append(panel_dir.parent)
        for root in roots:
            for f in root.rglob("*.md"):
                panel_text_ids.update(
                    re.findall(r"[A-Z]{2,}-[A-Za-z0-9-]+", f.read_text(encoding="utf-8"))
                )
        missing = sorted(panel_set - coverage_ledger)
        # Orphan candidates: only first-cells that look like finding IDs
        # ([A-Z]+-<digits> prefix). Aggregate ledger rows carry prose labels
        # ("断锚簇", "125→124/175→176") that split into non-ID fragments —
        # those are row labels, not orphan findings.
        id_like = re.compile(r"^[A-Z]{2,}-[A-Za-z0-9-]+$")
        orphan = sorted(x for x in (coverage_ledger - panel_set - panel_text_ids) if id_like.match(x))
        if missing:
            report.fail("coverage", "面板发现没有处置行: " + "、".join(missing))
        else:
            report.ok("coverage", "所有面板发现都有处置行")
        if orphan:
            report.fail("coverage", "台账有孤儿处置行: " + "、".join(orphan))
        else:
            report.ok("coverage", "台账没有孤儿行")

    if metrics_text is not None:
        check_metrics(metrics_text, report, round_tag, expected_counts)

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
| FACT-02 | P-FACT | P2 | 措辞 | FP | 反证见 §3 | 文档原文 §3 明确说 x 不是缺陷，且源码未出现该符号 | 不适用 |
| READER-A-01 | reader-a | P3 | 措辞 | ④ | 修 §1.2 | E1 | 旧串 grep 归零 |
"""
GOOD_METRICS = """# 指标

- 发现总数: 3
- 处置: ① 1 / ② 0 / ④ 1 / ⑤ 0 / ⑥ 0 / FP 1
- 未闭环: 0
- 轮型: discovery
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


def write_round_fixture(root: Path, ledger: str, metrics: str) -> tuple[Path, Path]:
    """R25+ fixture whose panel dir carries the round tag."""
    run_dir = root / "run"
    panel_dir = run_dir / "panel" / "round25"
    panel_dir.mkdir(parents=True)
    loop = "## R25 面板轮\n\n" + GOOD_LOOP.replace("## ", "### ")
    (run_dir / "loop.md").write_text(loop, encoding="utf-8")
    (run_dir / "defect-ledger.md").write_text(ledger, encoding="utf-8")
    (run_dir / "metrics.md").write_text(metrics, encoding="utf-8")
    (panel_dir / "fact.md").write_text(
        "# 面板\n\n## FINDING FACT-01\n- 位置：§1\n- 问题：示例\n",
        encoding="utf-8",
    )
    return run_dir, panel_dir


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
            "| FACT-02 | P-FACT | P2 | 措辞 | FP | 反证见 §3 | 文档原文 §3 明确说 x 不是缺陷，且源码未出现该符号 | 不适用 |\n", ""
        )
        bad_metrics = GOOD_METRICS.replace("- 发现总数: 3", "- 发现总数: 4")
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

    # R25+：已删除的 ③ 必须被拦截
    with tempfile.TemporaryDirectory() as tmp:
        ledger_legacy = """# 台账

## R25 面板轮

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-01 | P-FACT | P1 | 事实 | ③ | 作者取舍 | 无 | 无 |
"""
        metrics_legacy = """# 指标

- 发现总数: 1
- 处置: ① 0 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0
- 未闭环: 1
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 1 / token 1 / 耗时 1min
"""
        run_dir, panel_dir = write_round_fixture(Path(tmp), ledger_legacy, metrics_legacy)
        bad = check_run_dir(run_dir, panel_dir)
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed or "已删除的 ③" not in messages:
            print("self-test FAIL: R25+ 的 ③ 未被拦截")
            print_report(bad, False)
            return 1

    # R25+：FP 没有反证必须被拦截
    with tempfile.TemporaryDirectory() as tmp:
        ledger_fp = """# 台账

## R25 面板轮

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-01 | P-FACT | P1 | 事实 | FP | 误报 |  | 不适用 |
"""
        metrics_fp = metrics_legacy.replace("| ③ | 作者取舍 | 无 | 无 |", "").replace(
            "处置: ① 0 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0",
            "处置: ① 0 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 1",
        )
        run_dir, panel_dir = write_round_fixture(Path(tmp), ledger_fp, metrics_fp)
        bad = check_run_dir(run_dir, panel_dir)
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed or "没有给出源码/测试/文档原文反证" not in messages:
            print("self-test FAIL: FP 缺反证未被拦截")
            print_report(bad, False)
            return 1

    # 六步收口合同：R13+ 轮次含 ### L6/### L6.5 → 通过
    with tempfile.TemporaryDirectory() as tmp:
        loop_r13 = (
            GOOD_LOOP
            + "\n\n# Round 13（六步收口首轮）\n\n### L6\n"
            + "meta 结论与 BLIND-SPOT-DELTA。\n\n### L6.5\n三问三去处记录。\n"
        )
        ok_dir = write_fixture(Path(tmp), loop_r13, GOOD_LEDGER, GOOD_METRICS, GOOD_PANEL)
        ok = check_run_dir(ok_dir)
        if ok.failed:
            print("self-test FAIL: 带 L6/L6.5 收口小节的 R13 被判 FAIL")
            print_report(ok, False)
            return 1

    # 六步收口合同：R13+ 轮次缺 ### L6.5 → 拦截
    with tempfile.TemporaryDirectory() as tmp:
        loop_bad = GOOD_LOOP + "\n\n# Round 13（六步收口首轮）\n\n### L6\nmeta 结论。\n"
        bad_dir = write_fixture(Path(tmp), loop_bad, GOOD_LEDGER, GOOD_METRICS, GOOD_PANEL)
        bad = check_run_dir(bad_dir)
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed or "L6.5" not in messages:
            print("self-test FAIL: R13 缺 ### L6.5 未被拦截")
            print_report(bad, False)
            return 1

    # 范围行展开：`READER-A-01..03` 必须覆盖三条具体发现
    with tempfile.TemporaryDirectory() as tmp:
        panel_range = "# 面板\n\n" + "\n\n".join(
            f"## FINDING READER-A-{i:02d}\n- 位置：§1\n- 问题：x\n" for i in (1, 2, 3)
        )
        ledger_range = """# 台账

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| READER-A-01..03 | reader-a | P2 | 结构 | ④ | 修 | E1 | grep |
"""
        metrics_range = """# 指标

- 发现总数: 3
- 处置: ① 0 / ② 0 / ④ 3 / ⑤ 0 / ⑥ 0 / FP 0
- 未闭环: 0
- 轮型: discovery
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 1 / token 1 / 耗时 1min
"""
        run_dir = write_fixture(Path(tmp), GOOD_LOOP, ledger_range, metrics_range, panel_range)
        result = check_run_dir(run_dir)
        if result.failed:
            print("self-test FAIL: 范围行 READER-A-01..03 未被展开覆盖")
            print_report(result, False)
            return 1

    # 轮次作用域：round2 的 FACT-01 不能由 R1 的台账行顶替
    with tempfile.TemporaryDirectory() as tmp:
        run_dir = Path(tmp) / "run"
        panel_dir = run_dir / "panel" / "round2"
        panel_dir.mkdir(parents=True)
        (run_dir / "loop.md").write_text(GOOD_LOOP, encoding="utf-8")
        (panel_dir / "fact.md").write_text(
            "# 面板\n\n## FINDING FACT-01\n- 位置：§1\n- 问题：x\n", encoding="utf-8"
        )
        (run_dir / "metrics.md").write_text(
            GOOD_METRICS, encoding="utf-8"
        )
        (run_dir / "defect-ledger.md").write_text(
            """# 台账

## R1 面板轮

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-01 | fact | P1 | 事实 | ① | 修 | E1 | grep |

## R2 面板轮

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-02 | fact | P1 | 事实 | ① | 修 | E1 | grep |
""",
            encoding="utf-8",
        )
        result = check_run_dir(run_dir, panel_dir)
        messages = " ".join(item["message"] for item in result.items)
        if not result.failed or "FACT-01" not in messages:
            print("self-test FAIL: 轮次作用域未拦截 R1 台账顶替 R2 发现")
            print_report(result, False)
            return 1

    # 轮转闸（2026-11-09）：④ 行没有已修证据 → 拦截
    with tempfile.TemporaryDirectory() as tmp:
        ledger_4 = GOOD_LEDGER.replace(
            "| READER-A-01 | reader-a | P3 | 措辞 | ④ | 修 §1.2 | E1 | 旧串 grep 归零 |",
            "| READER-A-01 | reader-a | P3 | 措辞 | ④ | 修 §1.2 | E1 | （待补） |",
        )
        bad4_dir = write_fixture(Path(tmp), GOOD_LOOP, ledger_4, GOOD_METRICS, GOOD_PANEL)
        bad4 = check_run_dir(bad4_dir)
        messages4 = " ".join(item["message"] for item in bad4.items)
        if not bad4.failed or "轮转闸" not in messages4:
            print("self-test FAIL: ④ 无证据未被轮转闸拦截")
            print_report(bad4, False)
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
