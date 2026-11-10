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
# 新文档（02 起）从 R1 就会使用 V2.5 状态机；不能靠绝对轮号 >=25 判定，
# 否则 02 的 R1 会落回旧 schema，重新允许 ③/无未闭环/无限全量轮。
SCHEMA_RE = re.compile(
    r"^\s*(?:[-*]\s*)?(?:schema|规则版本)\s*[:：]\s*V(\d+)\.(\d+)"
    r"(?:\s*from\s*R(\d+))?",
    re.MULTILINE | re.IGNORECASE,
)


def schema_start_round(text: str | None) -> int | None:
    """Return the first round governed by the V3+ schema markers.

    A layered document carries one marker per schema generation
    (`schema: V3.0 from R26`, then `schema: V3.3 from R31`). Each marker
    governs *from* its round, so the V3 base rules bind from the **minimum**
    `from R<N>` across V3+ markers — a V3.3 marker must not unscope the
    rounds the V3.0 marker already covered. A V2.5→V3 transition document
    keeps the historical newest-version-wins behavior for its V2.5 segment.
    """
    if not text:
        return None
    v3_starts = [
        int(m.group(3) or 1)
        for m in SCHEMA_RE.finditer(text)
        if (int(m.group(1)), int(m.group(2))) >= (3, 0)
    ]
    if v3_starts:
        return min(v3_starts)
    best: tuple[tuple[int, int], int] | None = None
    for m in SCHEMA_RE.finditer(text):
        version = (int(m.group(1)), int(m.group(2)))
        if version < (2, 5):
            continue
        start = int(m.group(3) or 1)
        if best is None or version > best[0] or (
            version == best[0] and start > best[1]
        ):
            best = (version, start)
    return best[1] if best else None


def schema_version(text: str | None) -> tuple[int, int] | None:
    if not text:
        return None
    best: tuple[int, int] | None = None
    for m in SCHEMA_RE.finditer(text):
        version = (int(m.group(1)), int(m.group(2)))
        if best is None or version > best:
            best = version
    return best


def is_v3_schema(text: str | None) -> bool:
    version = schema_version(text)
    return version is not None and version >= (3, 0)


def origin_round(origin: str) -> int | None:
    """Extract the first `R<N>` from an `origin` cell.

    V3 writes `origin: R<首见>` or `origin: R<首见>；last_seen: R<本轮>`.
    The first number is the identity round; the rest is history.
    """
    m = re.search(r"\bR(\d+)\b", origin or "")
    return int(m.group(1)) if m else None


V33_MARKER_RE = re.compile(
    r"schema\s*[:：]\s*V3\.3(?:\s+from\s+R(\d+))?", re.IGNORECASE
)


def v33_start_round(text: str | None, fallback: int | None = None) -> int | None:
    """First round governed by the V3.3 additions (four-state lifecycle,
    confirmation rounds, reader-P3 admission, origin/severity semantics).

    Parsed from a `schema: V3.3 from R<N>` marker; without `from R<N>` the
    V3.3 rules bind from the document's existing schema start.
    """
    if not text:
        return fallback
    best: int | None = None
    for m in V33_MARKER_RE.finditer(text):
        start = int(m.group(1)) if m.group(1) else (fallback or 1)
        if best is None or start < best:
            best = start
    return best if best is not None else fallback


ROUND_TYPE_V33 = {"full", "structural", "confirmation"}
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


def parse_ledger(
    text: str, report: Report, v3: bool = False, v3_from_round: int | None = None
) -> tuple[
    list[str],
    dict[str, dict],
    dict[str, set[str]],
    dict[str, dict[str, str]],
    dict[str, dict],
]:
    lines = text.splitlines()
    ids: list[str] = []
    rows: dict[str, dict] = {}
    section_ids: dict[str, set[str]] = {}
    section_classes: dict[str, dict[str, str]] = {}
    root_rows: dict[str, dict] = {}
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
                    "root": cells.index("根因 ID") if "根因 ID" in cells else -1,
                    "origin": cells.index("origin") if "origin" in cells else -1,
                    "sev": cells.index("严重度") if "严重度" in cells else -1,
                    "role": (
                        cells.index("来源角色")
                        if "来源角色" in cells
                        else (cells.index("角色") if "角色" in cells else -1)
                    ),
                }
                table_seen = set()
                range_covered = set()
                section_round = None
                section_match = re.match(r"^R(\d+)\b", section)
                if section_match:
                    section_round = int(section_match.group(1))
                # V3 ledger requirements start at schema_start; historical
                # R17–R25 tables (V2.5) do not have the new columns and must
                # not turn the transition into a permanent red gate.
                current["v3_scope"] = int(
                    v3
                    and (
                        v3_from_round is None
                        or (
                            section_round is not None
                            and section_round >= v3_from_round
                        )
                    )
                )
                if current["v3_scope"] and (current["root"] < 0 or current["origin"] < 0):
                    report.fail(
                        "ledger",
                        f"V3 台账缺少 `根因 ID` 或 `origin` 列（节：{section or '?'}；"
                        "§4.0c：一行一个根因，复发只更新 last_seen）",
                    )
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
        root_id = cells[current["root"]].strip() if current["root"] >= 0 else ""
        origin = cells[current["origin"]].strip() if current["origin"] >= 0 else ""
        severity = cells[current["sev"]].strip() if current["sev"] >= 0 else ""
        role = cells[current["role"]].strip() if current["role"] >= 0 else ""
        if current.get("v3_scope"):
            if not root_id:
                report.fail(
                    "ledger",
                    f"{'、'.join(row_ids)} 缺少 `根因 ID`（V3 一行一个根因）",
                )
            if not origin:
                report.fail(
                    "ledger",
                    f"{'、'.join(row_ids)} 缺少 `origin`（V3 首次出现轮次）",
                )
            else:
                row_round_match = re.match(r"^R(\d+)\b", section)
                row_round = int(row_round_match.group(1)) if row_round_match else None
                origin_n = origin_round(origin)
                if (
                    row_round is not None
                    and origin_n is not None
                    and origin_n > row_round
                ):
                    report.fail(
                        "ledger",
                        f"{root_id or row_ids[0]} 的 origin R{origin_n} 晚于首次出现节 "
                        f"R{row_round}；origin 必须是首见轮",
                    )
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
                "root": root_id,
                "origin": origin,
                "severity": severity,
                "role": role,
                "section": section,
            }
            section_ids.setdefault(section, set()).add(row_id)
            section_classes.setdefault(section, {})[row_id] = canonical
            ids.append(row_id)
        if root_id:
            if root_id in root_rows and root_rows[root_id]["section"] == section:
                report.fail("ledger", f"同一节重复根因 ID: {root_id}")
            root_rows[root_id] = {
                "class": canonical,
                "action": action,
                "verify": verify,
                "evidence": evidence,
                "root": root_id,
                "origin": origin,
                "role": role,
                "finding_ids": list(row_ids),
                "section": section,
            }
    report.ok("ledger", f"台账解析到 {len(ids)} 条处置")
    return ids, rows, section_ids, section_classes, root_rows


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


def check_metrics(
    text: str,
    report: Report,
    round_tag: str | None = None,
    expected: dict[str, int] | None = None,
    actual_open: int | None = None,
    schema_start: int | None = None,
    v3: bool = False,
    root_count: int | None = None,
    new_roots: int | None = None,
    v33_start: int | None = None,
) -> None:
    body = _pick_metric_block(text, report, round_tag)
    total_match = METRIC_TOTAL_RE.search(body)
    counts = {m.group(1): int(m.group(2)) for m in METRIC_COUNT_RE.finditer(body)}
    round_num = int(round_tag.lstrip("R")) if round_tag else None
    if v3:
        legacy = False
    else:
        new_schema = (
            (schema_start is not None and (round_num is None or round_num >= schema_start))
            or (round_num is not None and round_num >= 25)
            or ("未闭环" in body)
        )
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

    declared_open: int | None = None
    if not legacy:
        open_match = re.search(r"^-\s*未闭环\s*[:：]\s*(\d+)", body, re.MULTILINE)
        if open_match is None:
            report.fail("metrics", "metrics.md 缺少 `- 未闭环: <整数>`（R25+ 强制）")
        else:
            declared_open = int(open_match.group(1))
            if actual_open is not None and declared_open != actual_open:
                report.fail(
                    "metrics",
                    f"metrics 写未闭环 {declared_open}，但台账本轮实际未闭环 {actual_open}；"
                    "禁止手填 0 掩盖 ⑤/未批准 ②/未实施 ⑥/复验写“未修”的行",
                )
            elif declared_open != 0:
                report.fail(
                    "metrics",
                    f"本轮仍有未闭环 {declared_open} 项；本轮不得关闭，禁止派下一轮面板",
                )
            else:
                report.ok("metrics", "未闭环 = 0，本轮可关闭")

    if v3:
        round_type = re.search(r"^-\s*轮型\s*[:：]\s*(\S+)", body, re.MULTILINE)
        if round_type is None:
            report.fail("metrics", "V3 metrics 缺少 `- 轮型: full`")
        elif round_type.group(1) not in ROUND_TYPE_V33:
            report.fail(
                "metrics",
                f"V3 轮型只能是 full/structural/confirmation，实际 `{round_type.group(1)}`；"
                "discovery/verification 轮型已删除",
            )
        if re.search(r"^-\s*edition\s*[:：]\s*V\d+\.\d+", body, re.MULTILINE) is None:
            report.fail("metrics", "V3 metrics 缺少 `- edition: V3.x`")
        for key in ["唯一根因数", "本轮新准入", "certify 新准入", "重复合并", "自引入"]:
            if re.search(rf"^-\s*{re.escape(key)}\s*[:：]\s*(\d+)", body, re.MULTILINE) is None:
                report.fail("metrics", f"V3 metrics 缺少 `- {key}: <整数>`")
        certify = re.search(r"^-\s*certify 新准入\s*[:：]\s*(\d+)", body, re.MULTILINE)
        if certify is not None and declared_open == 0 and int(certify.group(1)) != 0:
            report.fail(
                "metrics",
                f"certify 新准入 = {certify.group(1)}；本轮不得关闭（必须在本轮内修完再认证）",
            )
        unique = re.search(r"^-\s*唯一根因数\s*[:：]\s*(\d+)", body, re.MULTILINE)
        if root_count is not None and unique is not None and int(unique.group(1)) != root_count:
            report.fail(
                "metrics",
                f"唯一根因数 {unique.group(1)} 与台账根因 ID 数 {root_count} 不一致",
            )
        new_match = re.search(r"^-\s*本轮新准入\s*[:：]\s*(\d+)", body, re.MULTILINE)
        if new_roots is not None and new_match is not None and int(new_match.group(1)) != new_roots:
            report.fail(
                "metrics",
                f"本轮新准入 {new_match.group(1)} 与台账 origin=本轮 的根因数 {new_roots} 不一致",
            )
        # V3.3 four-state: every bound round declares its closing result.
        # `CONVERGED` here is cross-checked by check_v33_lifecycle (round type
        # + certify 新准入); the whitelist blocks free-form status prose.
        if v33_start is not None and (round_num is None or round_num >= v33_start):
            result_key = re.search(r"^-\s*轮结果\s*[:：]\s*(\S+)", body, re.MULTILINE)
            if result_key is None:
                report.fail(
                    "metrics",
                    "V3.3 metrics 缺少 `- 轮结果: ROUND-CLOSED|CONVERGED|NOT CONVERGED"
                    "|NOT CONVERGING|FREEZE-WITH-BACKLOG|BLOCKED`（§6A 四态模型）",
                )
            elif result_key.group(1) not in ROUND_RESULT_V33:
                report.fail(
                    "metrics",
                    f"- 轮结果 `{result_key.group(1)}` 不合法；允许：{sorted(ROUND_RESULT_V33)}",
                )
    elif not legacy:
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

def check_round_type_sequence(
    text: str, report: Report, schema_start: int | None = None, v3: bool = False
) -> None:
    """Enforce the V2.4 state machine on metrics blocks with explicit `## R<N>`
    headings: at most one discovery, and **no round after a verification**.

    Without this, clearing `未闭环` and then labeling the next round
    `discovery` silently reopens the full panel — the exact 25-round loop the
    state machine exists to stop. A verification round that finds new defects
    must fix them in the same round (max 2 deltas); it may not hand them to a
    later round.

    V3 removes both round types. In V3 every round must say `轮型: full`;
    there is no discovery/verification sequence to enforce.
    """
    if v3:
        for title, body in _metric_blocks(text):
            m = re.search(r"\bR(\d+)\b", title)
            if not m:
                continue
            n = int(m.group(1))
            if schema_start is not None and n < schema_start:
                continue
            rt = re.search(r"^-\s*轮型\s*[:：]\s*(\S+)", body, re.MULTILINE)
            if rt is None:
                report.fail("round-type", f"{title} 缺少 `- 轮型: full`（V3）")
            elif rt.group(1) not in ROUND_TYPE_V33:
                report.fail(
                    "round-type",
                    f"{title} 的轮型是 `{rt.group(1)}`；V3 只允许 full/structural/confirmation"
                    "（discovery/verification 已删除）",
                )
        return
    seq: list[tuple[int, str, str]] = []
    for title, body in _metric_blocks(text):
        m = re.search(r"\bR(\d+)\b", title)
        if not m:
            continue
        n = int(m.group(1))
        if schema_start is not None and n < schema_start:
            continue
        rt = re.search(r"^-\s*轮型\s*[:：]\s*(discovery|verification)", body, re.MULTILINE)
        if rt:
            seq.append((n, rt.group(1), title))
        elif schema_start is not None:
            report.fail(
                "round-type",
                f"{title} 缺少 `- 轮型: discovery|verification`；V2.5 schema 下从 R{schema_start} 起每一轮都必须声明轮型",
            )
    if not seq:
        return
    discoveries = [x for x in seq if x[1] == "discovery"]
    if len(discoveries) > 1:
        report.fail(
            "round-type",
            "metrics 出现多个 discovery 轮（"
            + "、".join(f"R{n}" for n, _, _ in discoveries)
            + "）；一篇文档只允许一次全量发现，后续必须是 verification",
        )
    for n, rtype, _title in seq:
        if rtype != "verification":
            continue
        later = sorted(x for x in seq if x[0] > n)
        if later:
            report.fail(
                "round-type",
                f"R{n} 是 verification，却出现 R{later[0][0]}；验证轮的新缺陷必须在同一轮 delta 内修完，"
                "不得开新轮（否则收敛又变成无限循环）",
            )
    if any(x[1] == "discovery" for x in seq) and seq[0][1] == "verification":
        report.fail("round-type", "metrics 在 discovery 之前出现了 verification；轮型顺序非法")


def check_verify_templates(rows: dict[str, dict], report: Report) -> None:
    """Warn when many rows reuse the same `复验` text (template evidence).

    The tool cannot judge whether a grep/test output is genuine, but a single
    sentence repeated across five or more fixes is almost always a template
    (R25's `doc 五门 PASS；宿主/QEMU 见 loop §5`). WARN, not FAIL: the meta
    audit and the per-root evidence requirement remain the authority.
    """
    from collections import Counter

    counts = Counter(row["verify"] for row in rows.values() if row.get("verify"))
    for text, count in counts.items():
        if count >= 5 and len(text) >= 20:
            report.warn(
                "verify-template",
                f"{count} 条记录复用同一复验文本（疑似模板）：{text[:48]}"
                "…；逐根因证据才算 100% 复验",
            )


ROW_LEVEL_TOKEN_RE = re.compile(
    r"os/[\w./-]+\.rs(?::|\s)|[\w./-]+\.(?:rs|md|sh|py):\d+|"
    r"tests?::[A-Za-z_]|cargo test|grep|旧串|commit\s+[0-9a-f]{7,}|git show|"
    r"冷读|回读|冷启动|CLOSED|STILL-BROKEN",
    re.IGNORECASE,
)
COLD_READ_TOKEN_RE = re.compile(r"冷读|回读|冷启动|CLOSED|STILL-BROKEN", re.IGNORECASE)
CONTRADICTION_RE = re.compile(r"矛盾|打架|对不上|不一致|自相矛盾")
B_CLASS_ROLE_RE = re.compile(r"^(reader-|design-)", re.IGNORECASE)


def _section_round(section: str) -> int | None:
    m = re.match(r"^R(\d+)\b", section or "")
    return int(m.group(1)) if m else None


def check_v3_evidence(
    ledger_rows: dict[str, dict],
    report: Report,
    round_tag: str | None = None,
    schema_start: int | None = None,
) -> None:
    """Enforce per-root evidence, B-class cold reads and no templates (V3).

    `复验` must carry a row-level token (path:line / test::name / grep / old
    string / commit / cold-read verdict). A round-level `五门 PASS` or
    `见 loop §N` is not evidence for an individual root. Reader/design rows
    are B-class and additionally require a cold-read `CLOSED`/`STILL-BROKEN`
    receipt; a FACT row whose evidence says "矛盾/打架" is treated as B-class.
    """
    from collections import Counter

    scope_round = int(round_tag.lstrip("R")) if round_tag else None
    scoped: list[tuple[str, dict]] = []
    for fid, row in ledger_rows.items():
        if row.get("class") not in {"①", "④", "⑥"}:
            continue
        section_round = _section_round(row.get("section", ""))
        if scope_round is not None:
            # A round-scoped check must not let a section without a round
            # number (e.g. a historical "RB13" or "R6 面板明细" table) leak
            # into R26 evidence checks.
            if section_round != scope_round:
                continue
        elif schema_start is not None and section_round is not None and section_round < schema_start:
            continue
        scoped.append((fid, row))

    for fid, row in scoped:
        verify = row.get("verify", "")
        role = row.get("role", "")
        if not ROW_LEVEL_TOKEN_RE.search(verify):
            report.fail(
                "v3-evidence",
                f"{fid} 复验没有行级证据（需 path:line / test::name / grep / 旧串 / "
                "commit / 冷读回执）；`五门 PASS`、`见 loop §N` 不算",
            )
        if B_CLASS_ROLE_RE.match(role) and not COLD_READ_TOKEN_RE.search(verify):
            report.fail(
                "v3-evidence",
                f"{fid} 来源 {role} 是 B 类（读者/设计），复验缺冷读 "
                "CLOSED/STILL-BROKEN 回执",
            )
        if (
            role.lower().startswith("fact-")
            and CONTRADICTION_RE.search(row.get("evidence", "") + row.get("action", ""))
            and not COLD_READ_TOKEN_RE.search(verify)
        ):
            report.fail(
                "v3-evidence",
                f"{fid} 是 FACT 的矛盾类（证据含矛盾/打架/对不上），复验缺冷读回执",
            )

    for key, label in (("action", "处置"), ("verify", "复验")):
        counts = Counter(row.get(key, "") for _, row in scoped if row.get(key))
        for text, count in counts.items():
            if count > 3 and len(text) >= 10:
                report.fail(
                    "v3-template",
                    f"V3 本轮 {label} 列同一文本出现 {count} 次（>3），疑似模板："
                    f"{text[:48]}",
                )


def audit_template_sections(ledger_text: str, report: Report) -> None:
    """Historical audit: flag template repetition in every ledger section.

    This is not part of the normal `--round` gate (R24/R25 are historical);
    run it explicitly with `--audit-templates` to see what the old rounds
    actually recorded.

    Parsed independently from `parse_ledger`: that function keys rows by
    finding ID, and R24/R25 reuse IDs like `FACT-A-01`, so a global dict
    would let R25 overwrite R24 and undercount the history (R24 114 rows
    collapsed to 5). The audit must keep every row per section.
    """
    from collections import Counter

    lines = ledger_text.splitlines()
    section = ""
    current: dict[str, int] | None = None
    by_section: dict[str, list[dict]] = {}
    for line in lines:
        if line.startswith("## "):
            section = line.strip("# ").strip()
            current = None
            continue
        stripped = line.strip()
        if not stripped.startswith("|"):
            current = None
            continue
        cells = [cell.strip() for cell in stripped.strip("|").split("|")]
        if all(SEP_RE.fullmatch(cell) for cell in cells):
            continue
        if "发现 ID" in cells and "处置" in cells and "复验" in cells:
            current = {
                "id": cells.index("发现 ID"),
                "act": cells.index("处置"),
                "ver": cells.index("复验"),
            }
            continue
        if current is None or len(cells) <= max(current.values()):
            continue
        if not cells[current["id"]]:
            continue
        by_section.setdefault(section, []).append(
            {"act": cells[current["act"]], "ver": cells[current["ver"]]}
        )
    for section, rows in by_section.items():
        for key, label in (("act", "处置"), ("ver", "复验")):
            counts = Counter(row[key] for row in rows if row[key])
            for text, count in counts.items():
                if count > 3 and len(text) >= 10:
                    report.fail(
                        "template-audit",
                        f"{section} 的 {label} 列同一文本出现 {count} 次："
                        f"{text[:48]}",
                    )


def check_todo_ledger(path: Path, report: Report) -> None:
    """Reconcile a document-level todo ledger with the review disposition set.

    `01-todo.md` is outside the run directory and was invisible to the gate.
    It may legitimately carry user-owned or deferred items, but every open
    item needs a legal disposition: `②` (user approval + date) or `⑤`
    (evidence path / blocked). `允许开放` is not in the disposition schema and
    must not be used to keep a newly registered in-scope task out of the
    review ledger.
    """
    if not path.is_file():
        report.fail("todo-ledger", f"todo 台账不存在: {path}")
        return
    lines = path.read_text(encoding="utf-8").splitlines()
    current: dict[str, int] | None = None
    open_items = 0
    for line in lines:
        stripped = line.strip()
        if not stripped.startswith("|"):
            current = None
            continue
        cells = [cell.strip() for cell in stripped.strip("|").split("|")]
        if all(SEP_RE.fullmatch(cell) for cell in cells):
            continue
        if "条目" in cells and "当前状态" in cells:
            current = {
                "id": cells.index("条目"),
                "status": cells.index("当前状态"),
            }
            continue
        if current is None or len(cells) <= max(current.values()):
            continue
        item = cells[current["id"]]
        status = cells[current["status"]]
        if not item:
            continue
        # `允许开放` is illegal regardless of other cells: a grouped row can
        # contain ✅ for most items and `允许开放` for one (TODO-05).
        if any("允许开放" in cell for cell in cells):
            open_items += 1
            report.fail(
                "todo-ledger",
                f"{item} 使用非法状态 `允许开放`；映射为 ②（用户批准 + 日期）"
                "或 ⑤（取证路径 / 阻塞），不得作为第七个处置桶",
            )
            continue
        # The overview table is free-form: some rows put `✅ 已实施` in the
        # type column and details in the status column. Treat the whole row
        # as done if any cell carries a completion token.
        if any(
            token in cell
            for cell in cells
            for token in ("✅", "已实施", "已修", "已闭合", "closed", "完成")
        ):
            continue
        open_items += 1
        if any(
            token in status
            for token in ("用户批准", "用户裁决", "用户侧", "DEFERRED", "取证", "阻塞")
        ):
            report.warn(
                "todo-ledger",
                f"{item} 开放（{status}）；确认批准日期或取证路径",
            )
        else:
            report.warn(
                "todo-ledger",
                f"{item} 未闭合且无合法处置（{status}）",
            )
    report.ok("todo-ledger", f"todo 台账开放项 {open_items} 条（逐条见上）")


def check_edition_trial(
    text: str, report: Report, schema_start: int | None = None
) -> None:
    """Warn when one edition has exhausted its 3-round trial on this doc.

    V3 §7.5: an edition gets at most 3 full rounds on the same document. The
    boundary is a process action, not a document defect, so this is a WARN;
    the main agent must run L7/EDITION-RETRO instead of opening a 4th round.
    """
    stats: dict[str, dict] = {}
    for title, body in _metric_blocks(text):
        m = re.search(r"\bR(\d+)\b", title)
        if not m:
            continue
        n = int(m.group(1))
        if schema_start is not None and n < schema_start:
            continue
        ed = re.search(r"^-\s*edition\s*[:：]\s*(V[\d.]+)", body, re.MULTILINE)
        rt = re.search(r"^-\s*轮型\s*[:：]\s*(\S+)", body, re.MULTILINE)
        if not ed or not rt or rt.group(1) not in {"full", "structural"}:
            continue
        stat = stats.setdefault(ed.group(1), {"rounds": 0, "last_open": None})
        stat["rounds"] += 1
        op = re.search(r"^-\s*未闭环\s*[:：]\s*(\d+)", body, re.MULTILINE)
        stat["last_open"] = int(op.group(1)) if op else None
    # V3.3: confirmation rounds are excluded by design (they are not
    # modification transactions), but structural rounds count like full.
    for edition, stat in stats.items():
        if stat["rounds"] >= 3 and (stat["last_open"] is None or stat["last_open"] > 0):
            report.warn(
                "edition-trial",
                f"edition {edition} 在本篇已跑 {stat['rounds']} 轮且最新一轮未闭环；"
                "按 §7.5 应执行 L7 边界评估或判 NOT CONVERGING，不得开第 4 轮",
            )
        else:
            report.ok(
                "edition-trial",
                f"edition {edition} 试验窗口 {stat['rounds']}/3",
            )


DEBT_DONE_TOKENS = (
    "✅",
    "已修",
    "已实施",
    "已闭合",
    "已完成",
    "已覆盖",
    "已拒绝",
    "superseded",
    "dup-of",
)
DEBT_DEADLINE_RE = re.compile(r"截止\s*R(\d+)")


def check_open_debt(path: Path, report: Report, round_tag: str | None) -> int:
    """V3.3 §4.0f: OPEN-DEBT rows are part of the single open count.

    A debt row has exactly two legal exits: implemented (completion token /
    `dup-of` pointing at the covering root) or a DECISIONS.md decision row
    (user adjudication). Anything else keeps the round open. A `截止 R<N>`
    whose N is below the closing round is overdue and said so by name.

    Returns the open-row count so the caller folds it into `未闭环`.
    """
    if not path.is_file():
        return 0
    lines = path.read_text(encoding="utf-8").splitlines()
    current: dict[str, int] | None = None
    open_rows: list[tuple[str, str]] = []
    closing_round = int(round_tag.lstrip("R")) if round_tag else None
    for line in lines:
        stripped = line.strip()
        if not stripped.startswith("|"):
            current = None
            continue
        cells = [cell.strip() for cell in stripped.strip("|").split("|")]
        if all(SEP_RE.fullmatch(cell) for cell in cells):
            continue
        if any("ID" == cell or "条目" == cell for cell in cells):
            current = {"id": 0}
            continue
        if current is None or not cells or not cells[0]:
            continue
        item = cells[0]
        if item.startswith("#") or item in {"ID", "条目"}:
            continue
        row_text = " | ".join(cells)
        if any(token in row_text for token in DEBT_DONE_TOKENS):
            continue
        deadline = DEBT_DEADLINE_RE.search(row_text)
        deadline_note = ""
        if deadline and closing_round is not None and int(deadline.group(1)) < closing_round:
            deadline_note = f"；已逾期（截止 R{deadline.group(1)}）"
        open_rows.append((item, deadline_note))
    for item, note in open_rows:
        report.fail(
            "open-debt",
            f"{item} 未销账{note}；只有两个出口：实施（附代码证据或 dup-of）"
            "或 DECISIONS.md 判决账行——没有第三个状态（§4.0f）",
        )
    if open_rows:
        report.fail(
            "open-debt",
            f"OPEN-DEBT 有 {len(open_rows)} 行未销账；未闭环口径包含本表，"
            "本轮不得关闭、不得派下一轮面板（§4.0f 单一未闭环口径）",
        )
    else:
        report.ok("open-debt", "OPEN-DEBT 无未销账行")
    return len(open_rows)


ROUND_RESULT_V33 = {
    "ROUND-CLOSED",
    "CONVERGED",
    "NOT CONVERGED",
    "NOT CONVERGING",
    "FREEZE-WITH-BACKLOG",
    "BLOCKED",
}


def _panel_severities(panel_dir: Path | None) -> dict[tuple[int, str], str]:
    """Map `(round, FINDING ID)` → `P0..P3` from panel headers (`（P0-fact）`).

    Keyed per round because short finding IDs repeat across rounds
    (READER-A-16 has seven lives in doc 01's history).
    """
    if panel_dir is None or not panel_dir.is_dir():
        return {}
    out: dict[tuple[int, str], str] = {}
    for path in sorted(panel_dir.rglob("*.md")):
        round_match = re.search(r"round(\d+)", str(path.parent))
        round_num = int(round_match.group(1)) if round_match else 0
        for m in re.finditer(
            r"^#{2,3}\s+FINDING\s+([A-Za-z][A-Za-z0-9-]*)\s*[（(]?\s*(P[0-3])",
            path.read_text(encoding="utf-8"),
            re.MULTILINE,
        ):
            out.setdefault((round_num, m.group(1)), m.group(2))
    return out


def check_v33_admission(
    ledger_rows: dict[str, dict],
    panel_severities: dict[tuple[int, str], str],
    report: Report,
    round_tag: str | None,
    v33_start: int | None,
) -> None:
    """V3.3 admission rules, scoped to rounds >= v33_start.

    1. Reader-role P3 rows must not enter the ledger as blocking (①/④):
       they belong in suggestions.md unless they pass the "读者卡住无法
       继续" admission test (G3).
    2. Ledger severity must match the panel FINDING header (R30 recorded a
       P0 finding as P1/P2 rows — the columns diverged).
    """
    for fid, row in ledger_rows.items():
        section_round = _section_round(row.get("section", ""))
        if v33_start is None or section_round is None or section_round < v33_start:
            continue
        role = (row.get("role") or "").lower()
        severity = (row.get("severity") or "").strip()
        cls = row.get("class")
        if role.startswith("reader") and severity == "P3" and cls in {"①", "④"}:
            report.fail(
                "admission-v33",
                f"{fid} 是读者 P3 却按 {cls} 准入；G3（V3.3）：措辞/指针类 P3 进"
                " suggestions.md，只有通过「读者卡住无法继续」测试的才入台账",
            )
        panel_sev = panel_severities.get((section_round, fid))
        if panel_sev and severity and panel_sev != severity:
            report.fail(
                "severity-sync",
                f"{fid} 台账严重度 {severity} 与面板标题 {panel_sev} 不一致；"
                "severity 列必须从面板 FINDING 标题机械同步",
            )


def check_v33_lifecycle(
    metrics_text: str,
    ledger_rows: dict[str, dict],
    section_classes: dict[str, dict[str, str]],
    report: Report,
    schema_start: int | None,
    v33_start: int | None,
) -> None:
    """V3.3 four-state lifecycle (§6A), anchored on the metrics `- 轮结果:` key
    (not on loop.md §7 prose — the prose legitimately mentions CONVERGED when
    narrating a previous round's overturned claim, which false-positives).

    - full/structural rounds close as ROUND-CLOSED at most; a CONVERGED
      result on a modification round is a false positive (R26/R30 判例).
    - CONVERGED requires a `confirmation` round with `certify 新准入: 0`;
      at most two confirmation rounds; none after a sealed one.
    - A confirmation round implements no ⑥: design items go to OPEN-DEBT
      for the next design batch, otherwise the baseline moves under the
      confirmation itself.
    """
    confirmations = 0
    sealed_at: int | None = None
    for title, body in _metric_blocks(metrics_text):
        m = re.search(r"\bR(\d+)\b", title)
        if not m:
            continue
        n = int(m.group(1))
        if v33_start is not None and n < v33_start:
            continue
        if schema_start is not None and n < schema_start:
            continue
        rt = re.search(r"^-\s*轮型\s*[:：]\s*(\S+)", body, re.MULTILINE)
        rtype = rt.group(1) if rt else None
        result = re.search(r"^-\s*轮结果\s*[:：]\s*(\S+)", body, re.MULTILINE)
        result_val = result.group(1) if result else None
        certify = re.search(r"^-\s*certify 新准入\s*[:：]\s*(\d+)", body, re.MULTILINE)
        certify_new = int(certify.group(1)) if certify else None
        if rtype == "confirmation":
            confirmations += 1
            if confirmations > 2:
                report.fail(
                    "lifecycle-v33",
                    f"R{n} 是第 {confirmations} 次确认轮；确认轮全局最多 2 次（§6A）",
                )
            if certify_new == 0:
                sealed_at = n
            for section, mapping in section_classes.items():
                if not section.lower().startswith(f"r{n}".lower()):
                    continue
                for fid, cls in mapping.items():
                    row = ledger_rows.get(fid)
                    if (
                        row is not None
                        and row.get("class") == "⑥"
                        and _section_round(row.get("section", "")) == n
                    ):
                        report.fail(
                            "lifecycle-v33",
                            f"{fid} 在确认轮实施 ⑥；确认轮只修新准入的 P0/P1/P2 事实与同步项，"
                            "设计项登记 OPEN-DEBT 到下一个设计批（§4.0d）",
                        )
        if result_val == "CONVERGED":
            if rtype != "confirmation":
                report.fail(
                    "lifecycle-v33",
                    f"R{n} 是 {rtype or '?'} 轮却宣告 CONVERGED；修改轮最多 ROUND-CLOSED / "
                    "CONVERGENCE-CANDIDATE，收敛声明权只在确认轮（R26/R30 两次假阳性判例）",
                )
            elif certify_new != 0:
                report.fail(
                    "lifecycle-v33",
                    f"R{n} 确认轮 certify 新准入 = {certify_new}；非 0 不得宣告 CONVERGED",
                )
        if sealed_at is not None and n > sealed_at:
            report.fail(
                "lifecycle-v33",
                f"R{sealed_at} 确认通过（新准入=0）之后出现 R{n}；CONVERGED 后不得再开轮，"
                "新证据走重开决策（§6A）",
            )


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


# 两种轮次标题：旧格式 `# Round 13`，以及 R14 起实际使用的
# `## R25 面板轮`。只认旧格式会让 R14–R25 整段逃过 L6/L6.5 收口合同
# （2026-11-10 回归检查实证：R15–R25 全部缺 L6/L6.5，旧正则零报警）。
ROUND_HEADER_RE = re.compile(
    r"^(?:#{1,2}\s+Round\s+(\d+)|##\s+R(\d+)\b)", re.MULTILINE
)


def _round_header_number(m: re.Match) -> int:
    return int(m.group(1) or m.group(2))


def check_round_closure(
    loop_text: str,
    report: Report,
    round_tag: str | None = None,
    v3: bool = False,
    v3_from_round: int | None = None,
) -> None:
    """六步收口合同（DECISIONS-PENDING 第 5 项裁决）：编号 >= 13 的
    轮次小节必须包含 `### L6`（meta 记录）与 `### L6.5`（三问收口
    记录）两个三级小节；R0–R12 为历史轮次，豁免。

    V3 没有历史豁免：每一轮都是交易，关闭时必须有 L6/L6.5 收口记录。
    `round_tag` 指定时只检查该轮：轮次关账只对本轮负责；全历史缺失
    是归档审计项，不能永远阻断当前轮（2026-11-10 用户裁决：流程必须
    能走向收敛，历史欠账不得成为当前轮的永久红灯）。"""
    matches = list(ROUND_HEADER_RE.finditer(loop_text))
    rounds = []
    want = int(round_tag.lstrip("R")) if round_tag else None
    for i, m in enumerate(matches):
        start = m.start()
        end = matches[i + 1].start() if i + 1 < len(matches) else len(loop_text)
        num = _round_header_number(m)
        v3_round = v3 and (v3_from_round is None or num >= v3_from_round)
        if (num >= 13 or v3_round) and (want is None or num == want):
            rounds.append((str(num), start, end))
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


def _round_open_items(
    ledger_rows: dict[str, dict],
    section_classes: dict[str, dict[str, str]],
    schema_start: int | None = None,
    v3: bool = False,
) -> dict[int, list[str]]:
    """Return open IDs per ledger round for the rounds under the new schema.

    V3 counts open **root causes**, not raw finding IDs: one root row may
    cover several panel findings, and the round closes only when every root
    is closed. V2.5 keeps the historical per-finding count.
    """
    open_rounds: dict[int, list[str]] = {}
    for section, mapping in section_classes.items():
        m = re.match(r"^R(\d+)\b", section)
        if not m:
            continue
        n = int(m.group(1))
        if schema_start is not None:
            if n < schema_start:
                continue
        elif n < 25:
            continue
        opened: set[str] = set()
        for fid, cls in mapping.items():
            row = ledger_rows.get(fid)
            if row is None:
                continue
            is_open = False
            if cls in {"LEGACY", "⑤"}:
                is_open = True
            elif cls == "②":
                action = row["action"]
                if "用户批准" not in action or re.search(r"20\d{2}-\d{2}-\d{2}", action) is None:
                    is_open = True
            elif cls in {"①", "④", "⑥"} and re.search(
                r"未修|待修|未实施|TODO", row["verify"], re.IGNORECASE
            ):
                is_open = True
            if is_open:
                if v3 and row.get("root"):
                    opened.add(row["root"])
                else:
                    opened.add(fid)
        if opened:
            open_rounds[int(m.group(1))] = sorted(opened)
    return open_rounds


def _loop_round_numbers(loop_text: str) -> set[int]:
    nums = {int(n) for n in re.findall(r"^##\s+R(\d+)\b", loop_text, re.MULTILINE)}
    nums |= {int(n) for n in re.findall(r"^#{1,2}\s+Round\s+(\d+)", loop_text, re.MULTILINE)}
    return nums


def check_round_monotonicity(
    loop_text: str,
    ledger_rows: dict[str, dict],
    section_classes: dict[str, dict[str, str]],
    report: Report,
    schema_start: int | None = None,
    v3: bool = False,
) -> None:
    """A round with open items must not have a later round section."""
    open_rounds = _round_open_items(ledger_rows, section_classes, schema_start, v3)
    loop_rounds = _loop_round_numbers(loop_text)
    ledger_rounds = {
        int(m.group(1))
        for section in section_classes
        if (m := re.match(r"^R(\d+)\b", section))
    }
    all_rounds = loop_rounds | ledger_rounds
    for r, ids in sorted(open_rounds.items()):
        later = sorted(x for x in all_rounds if x > r)
        if later:
            report.fail(
                "round-order",
                f"R{r} 有未闭环 {len(ids)} 项（{', '.join(ids[:5])}{'…' if len(ids) > 5 else ''}），"
                f"却已出现 R{later[0]}；禁止带病进入下一轮，必须在 R{r} 内修完",
            )
        else:
            report.ok("round-order", f"R{r} 未闭环项未泄漏到下一轮")


def enforce_adjacent_only_dispositions(
    ledger_rows: dict[str, dict],
    report: Report,
    schema_start: int | None,
) -> None:
    """V3.2 §4.1a: `②` is adjacent-scope only; `② 裁决` is illegal.

    A same-document pending-design item must be ⑥ implemented in-round (proxy
    adjudication) or the round goes BLOCKED — never transferred into the next
    round. Sections before `schema_start` are historical and exempt.
    """
    for fid, row in ledger_rows.items():
        if row.get("class") != "②":
            continue
        section = row.get("section", "")
        section_round = _section_round(section)
        if (
            schema_start is not None
            and section_round is not None
            and section_round < schema_start
        ):
            continue
        action = row.get("action", "")
        if re.match(r"^\s*②\s*相邻", action) is None:
            preview = action[:36] + ("…" if len(action) > 36 else "")
            report.fail(
                "disposition-v32",
                f"{fid} 的 ② 不是相邻范围形态（`{preview}`）；V3.2 起同篇待裁决项"
                "只能 ⑥ 本轮实施或 BLOCKED，不得转出（§4.1a）",
            )


def check_run_dir(
    run_dir: Path,
    panel_dir: Path | None = None,
    round_tag_override: str | None = None,
    audit_templates: bool = False,
) -> Report:
    report = Report()
    if not run_dir.is_dir():
        report.fail("file", f"run 目录不存在: {run_dir}")
        return report

    loop_text = read_text(run_dir / "loop.md", report, "loop.md")
    metrics_text = read_text(run_dir / "metrics.md", report, "metrics.md")
    ledger_text = read_text(run_dir / "defect-ledger.md", report, "defect-ledger.md")
    schema_start = schema_start_round(loop_text)
    if schema_start is None and metrics_text is not None:
        schema_start = schema_start_round(metrics_text)
    v3_doc = is_v3_schema(loop_text) or is_v3_schema(metrics_text)
    if round_tag_override and schema_start is None:
        report.fail(
            "schema",
            "缺少 `schema: V2.5|V3.0 from R<N>` 标记；轮次关账必须声明本文档从哪一轮起受"
            "状态机约束（01 写 `from R25`，02 起写 `from R1`，否则新文档会落回旧 schema"
            "无限全量轮）",
        )
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
    if round_tag_override:
        round_tag = round_tag_override
        if panel_dir is not None:
            # `--round R25` 自动收窄到 panel/round25；只有显式存在该
            # 子目录时才切换，避免把单文件测试夹具改坏。
            scoped_panel = panel_dir / f"round{round_tag.lstrip('R').lstrip('0') or '0'}"
            if scoped_panel.is_dir():
                panel_dir = scoped_panel
    else:
        round_match = re.search(r"round(\d+)", panel_dir.name) if panel_dir else None
        round_tag = f"R{int(round_match.group(1))}" if round_match else None
    # A doc may transition from V2.5 to V3 at a specific round (01: R26).
    # `--round R25` on such a doc must still run the V2.5 checks.
    v3 = v3_doc and (
        round_tag is None
        or schema_start is None
        or int(round_tag.lstrip("R")) >= schema_start
    )
    v33_doc_flag = (schema_version(loop_text) or (0, 0)) >= (3, 3)
    v33_from = (
        v33_start_round(loop_text, fallback=schema_start) if v33_doc_flag else None
    )
    if v3 and loop_text is not None:
        if re.search(
            r"^\s*(?:[-*]\s*)?edition\s*[:：]\s*V\d+\.\d+",
            loop_text,
            re.MULTILINE | re.IGNORECASE,
        ) is None:
            report.fail(
                "schema",
                "V3 loop.md §0 缺少 `edition: V3.x` 标记（轮次必须绑定到规则版本）",
            )
    panel_ids = collect_panel_findings(panel_dir, report)
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
            # V3: a panel may legitimately report only `## SUGGESTION`
            # (out-of-rubric preferences that do not enter the ledger).
            has_suggestion = bool(
                re.search(r"^##\s+SUGGESTION\b", text, re.MULTILINE)
            )
            if has_finding or has_suggestion:
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
        check_round_closure(
            loop_text, report, round_tag, v3=v3, v3_from_round=schema_start
        )
        check_placeholders(loop_text, report, "loop.md")
    expected_counts = None
    ledger_rows: dict[str, dict] = {}
    section_classes: dict[str, dict[str, str]] = {}
    root_count: int | None = None
    new_roots: int | None = None
    if ledger_text is not None:
        check_placeholders(ledger_text, report, "defect-ledger.md")
        (
            ledger_ids,
            ledger_rows,
            section_ids,
            section_classes,
            root_rows,
        ) = parse_ledger(ledger_text, report, v3=v3, v3_from_round=schema_start)
        if (schema_version(loop_text) or (0, 0)) >= (3, 2):
            enforce_adjacent_only_dispositions(ledger_rows, report, schema_start)
        # V3.3 additions: four-state lifecycle, confirmation rounds, reader-P3
        # admission, origin/severity semantics. Scoped from the V3.3 marker's
        # `from R<N>` so historical rounds stay exempt (01: R31 grandfathered).
        if v33_doc_flag:
            panel_severities = _panel_severities(panel_dir)
            check_v33_admission(
                ledger_rows, panel_severities, report, round_tag, v33_from
            )
            check_v33_lifecycle(
                metrics_text or "",
                ledger_rows,
                section_classes,
                report,
                schema_start,
                v33_from,
            )
        if v3:
            check_v3_evidence(ledger_rows, report, round_tag, schema_start)
        else:
            check_verify_templates(ledger_rows, report)
        if audit_templates:
            audit_template_sections(ledger_text, report)
        if v3:
            if round_tag:
                current_round = int(round_tag.lstrip("R"))
                scoped_roots = {
                    row["root"]
                    for row in ledger_rows.values()
                    if row.get("root")
                    and row.get("section", "").lower().startswith(round_tag.lower())
                }
                root_count = len(scoped_roots)
                new_roots = sum(
                    1
                    for root in scoped_roots
                    if origin_round(root_rows.get(root, {}).get("origin", ""))
                    == current_round
                )
            else:
                root_count = len(root_rows)
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
            round_n = int(round_tag.lstrip("R"))
            if (schema_start is not None and round_n >= schema_start) or round_n >= 25:
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

    actual_open = None
    if round_tag and ledger_rows:
        open_map = _round_open_items(
            ledger_rows, section_classes, schema_start, v3=v3
        )
        actual_open = len(open_map.get(int(round_tag.lstrip("R")), []))
    # V3.3 §4.0f: OPEN-DEBT is part of the single open count. An unclosed
    # same-document debt row blocks the round regardless of ledger state.
    open_debt_path = run_dir / "OPEN-DEBT.md"
    if open_debt_path.is_file():
        debt_open = check_open_debt(open_debt_path, report, round_tag)
        if actual_open is not None:
            actual_open += debt_open
        elif debt_open:
            report.fail(
                "open-debt",
                f"OPEN-DEBT 未销账 {debt_open} 行且未指定 --round；全档审计同样不得视为已收敛",
            )
    if metrics_text is not None:
        check_metrics(
            metrics_text,
            report,
            round_tag,
            expected_counts,
            actual_open,
            schema_start,
            v3=v3,
            root_count=root_count,
            new_roots=new_roots,
            v33_start=v33_from,
        )
        # The all-round checks follow the document's newest schema; the
        # per-round checks above use `v3` (the selected round's segment).
        check_round_type_sequence(metrics_text, report, schema_start, v3=v3_doc)
        if v3_doc:
            check_edition_trial(metrics_text, report, schema_start)

    if loop_text is not None:
        check_round_monotonicity(
            loop_text, ledger_rows, section_classes, report, schema_start, v3=v3
        )

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
    loop = (
        "## R25 面板轮\n\n"
        + GOOD_LOOP.replace("## ", "### ")
        + "\n\n### L6\nmeta 结论与 BLIND-SPOT-DELTA。\n"
        + "\n### L6.5\n三问三去处记录。\n"
    )
    (run_dir / "loop.md").write_text(loop, encoding="utf-8")
    (run_dir / "defect-ledger.md").write_text(ledger, encoding="utf-8")
    (run_dir / "metrics.md").write_text(metrics, encoding="utf-8")
    (panel_dir / "fact.md").write_text(
        "# 面板\n\n## FINDING FACT-01\n- 位置：§1\n- 问题：示例\n",
        encoding="utf-8",
    )
    return run_dir, panel_dir


V3_LOOP = (
    "schema: V3.0 from R1\n\n"
    "edition: V3.0\n\n"
    "## R1 面板轮\n\n"
    + GOOD_LOOP.replace(
        "## §5 复验\n本节的占位内容。",
        "## §5 复验\n- certify 新准入: 0。",
    ).replace("## ", "### ")
    + "\n\n### L6\nmeta 结论与 BLIND-SPOT-DELTA。\n"
    + "\n### L6.5\n三问三去处记录。\n"
)

V3_LEDGER = """# 台账

## R1 面板轮

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD-001 | FACT-01 | R1 | P-FACT | P1 | 事实 | ① | 修 §1 | E1 | 旧串 grep 归零 |
"""

V3_METRICS = """# 指标

- 发现总数: 1
- 唯一根因数: 1
- 本轮新准入: 1
- certify 新准入: 0
- 重复合并: 0
- 自引入: 0
- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0
- 未闭环: 0
- 轮型: full
- edition: V3.0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 1 / token 1 / 耗时 1min
"""


def write_v3_fixture(
    root: Path,
    loop: str = V3_LOOP,
    ledger: str = V3_LEDGER,
    metrics: str = V3_METRICS,
    panel: str = "# 面板\n\n## FINDING FACT-01\n- 位置：§1\n- 问题：示例\n",
) -> tuple[Path, Path]:
    run_dir = root / "run"
    panel_dir = run_dir / "panel" / "round1"
    panel_dir.mkdir(parents=True)
    (run_dir / "loop.md").write_text(loop, encoding="utf-8")
    (run_dir / "defect-ledger.md").write_text(ledger, encoding="utf-8")
    (run_dir / "metrics.md").write_text(metrics, encoding="utf-8")
    (panel_dir / "fact.md").write_text(panel, encoding="utf-8")
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

    # 验证轮：零新 in-scope 真缺陷 → 允许 CONVERGED
    with tempfile.TemporaryDirectory() as tmp:
        ledger_v = """# 台账

## R25 面板轮

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-01 | P-FACT | P1 | 事实 | ① | 修复 | E1 | 测试通过 |
"""
        metrics_v = """# 指标

- 发现总数: 1
- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0
- 未闭环: 0
- 轮型: verification
- 新 in-scope 真缺陷: 0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 1 / token 1 / 耗时 1min
"""
        run_dir, panel_dir = write_round_fixture(Path(tmp), ledger_v, metrics_v)
        good = check_run_dir(run_dir, panel_dir)
        if good.failed:
            print("self-test FAIL: verification 零新增被判 FAIL")
            print_report(good, False)
            return 1

    # 验证轮：有新 in-scope 真缺陷 → 不得 CONVERGED
    with tempfile.TemporaryDirectory() as tmp:
        metrics_bad = metrics_v.replace("新 in-scope 真缺陷: 0", "新 in-scope 真缺陷: 1")
        run_dir, panel_dir = write_round_fixture(Path(tmp), ledger_v, metrics_bad)
        bad = check_run_dir(run_dir, panel_dir)
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed or "不得 CONVERGED" not in messages:
            print("self-test FAIL: verification 有新发现未被拦截")
            print_report(bad, False)
            return 1

    # 上一轮未闭环时出现下一轮小节 → 必须拦截（禁止带病进下一轮）
    with tempfile.TemporaryDirectory() as tmp:
        ledger_open = """# 台账

## R25 面板轮

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-01 | P-FACT | P1 | 事实 | ⑤ | 缺源码核实路径：R26 取证 |  | 未闭环 |
"""
        metrics_open = """# 指标

- 发现总数: 1
- 处置: ① 0 / ② 0 / ④ 0 / ⑤ 1 / ⑥ 0 / FP 0
- 未闭环: 1
- 轮型: discovery
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 1 / token 1 / 耗时 1min
"""
        run_dir, panel_dir = write_round_fixture(Path(tmp), ledger_open, metrics_open)
        loop = (run_dir / "loop.md").read_text()
        (run_dir / "loop.md").write_text(loop + "\n## R26 面板轮\n\n### §0 基线\n占位\n", encoding="utf-8")
        bad = check_run_dir(run_dir, panel_dir)
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed or "禁止带病进入下一轮" not in messages:
            print("self-test FAIL: 上一轮未闭环却出现 R26 未被拦截")
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

    # 六步收口合同：现代标题 `## R25 面板轮` 同样受约束（旧正则只认
    # `# Round`，R14–R25 曾整段逃检）
    with tempfile.TemporaryDirectory() as tmp:
        loop_modern = (
            "## R25 面板轮\n\n"
            + GOOD_LOOP.replace("## ", "### ")
            + "\n\n### L6\nmeta 结论。\n"
        )
        bad_dir = write_fixture(Path(tmp), loop_modern, GOOD_LEDGER, GOOD_METRICS, GOOD_PANEL)
        bad = check_run_dir(bad_dir)
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed or "Round 25" not in messages or "L6.5" not in messages:
            print("self-test FAIL: 现代 R25 标题缺 ### L6.5 未被拦截")
            print_report(bad, False)
            return 1

    # metrics 手填 未闭环: 0 掩盖台账 ⑤ → 必须按台账实际数拦截
    with tempfile.TemporaryDirectory() as tmp:
        ledger_lie = """# 台账

## R25 面板轮

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-01 | P-FACT | P1 | 事实 | ⑤ | 还缺源码核实，取证路径 X | panel | 未闭环 |
"""
        metrics_lie = """# 指标

- 发现总数: 1
- 处置: ① 0 / ② 0 / ④ 0 / ⑤ 1 / ⑥ 0 / FP 0
- 未闭环: 0
- 轮型: discovery
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 1 / token 1 / 耗时 1min
"""
        run_dir, panel_dir = write_round_fixture(Path(tmp), ledger_lie, metrics_lie)
        bad = check_run_dir(run_dir, panel_dir)
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed or "实际未闭环 1" not in messages:
            print("self-test FAIL: metrics 手填 0 未被台账实际数拦截")
            print_report(bad, False)
            return 1

    # 轮型状态机：verification 之后不得再出现新轮
    with tempfile.TemporaryDirectory() as tmp:
        ledger_one = """# 台账

## R25 面板轮

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-01 | P-FACT | P1 | 事实 | ① | 修复 | E1 | 测试通过 |
"""
        def metrics_block(round_no: int, rtype: str, extra: str = "") -> str:
            return f"""## R{round_no}

- 发现总数: 1
- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0
- 未闭环: 0
- 轮型: {rtype}
{extra}- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 1 / token 1 / 耗时 1min
"""
        metrics_seq = "# 指标\n\n" + metrics_block(25, "discovery") + metrics_block(
            26, "verification", "- 新 in-scope 真缺陷: 0\n"
        ) + metrics_block(27, "verification", "- 新 in-scope 真缺陷: 0\n")
        run_dir, panel_dir = write_round_fixture(Path(tmp), ledger_one, metrics_seq)
        bad = check_run_dir(run_dir, panel_dir)
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed or "不得开新轮" not in messages:
            print("self-test FAIL: verification 之后出现 R27 未被拦截")
            print_report(bad, False)
            return 1

    # 轮型状态机：一篇文档只允许一次 discovery
    with tempfile.TemporaryDirectory() as tmp:
        metrics_two_discovery = "# 指标\n\n" + metrics_block(25, "discovery") + metrics_block(
            26, "discovery"
        )
        run_dir, panel_dir = write_round_fixture(Path(tmp), ledger_one, metrics_two_discovery)
        bad = check_run_dir(run_dir, panel_dir)
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed or "多个 discovery" not in messages:
            print("self-test FAIL: 第二次 discovery 未被拦截")
            print_report(bad, False)
            return 1

    # 新文档（02 起）从 R1 就必须受 V2.5 状态机约束：不能靠轮号 >=25
    with tempfile.TemporaryDirectory() as tmp:
        run_dir = Path(tmp) / "run"
        panel_dir = run_dir / "panel" / "round1"
        panel_dir.mkdir(parents=True)
        loop_r1 = (
            "schema: V2.5 from R1\n\n## R1 面板轮\n\n"
            + GOOD_LOOP.replace("## ", "### ")
        )
        ledger_r1 = """# 台账

## R1 面板轮

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-01 | P-FACT | P1 | 事实 | ⑤ | 还缺源码核实，取证路径 X | panel | 未闭环 |
"""
        metrics_r1 = """# 指标

- 发现总数: 1
- 处置: ① 0 / ② 0 / ④ 0 / ⑤ 1 / ⑥ 0 / FP 0
- 未闭环: 1
- 轮型: discovery
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 1 / token 1 / 耗时 1min
"""
        (run_dir / "loop.md").write_text(loop_r1, encoding="utf-8")
        (run_dir / "defect-ledger.md").write_text(ledger_r1, encoding="utf-8")
        (run_dir / "metrics.md").write_text(metrics_r1, encoding="utf-8")
        (panel_dir / "fact.md").write_text(
            "# 面板\n\n## FINDING FACT-01\n- 位置：§1\n- 问题：x\n", encoding="utf-8"
        )
        bad = check_run_dir(run_dir, panel_dir, "R1")
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed or "R1" not in messages or "未闭环" not in messages:
            print("self-test FAIL: 02 起 R1 的 V2.5 schema 未生效")
            print_report(bad, False)
            return 1

    # 新文档不写 schema 标记 → --round 必须拒绝，避免落回旧 schema
    with tempfile.TemporaryDirectory() as tmp:
        run_dir = Path(tmp) / "run"
        panel_dir = run_dir / "panel" / "round1"
        panel_dir.mkdir(parents=True)
        loop_no_schema = "## R1 面板轮\n\n" + GOOD_LOOP.replace("## ", "### ")
        (run_dir / "loop.md").write_text(loop_no_schema, encoding="utf-8")
        (run_dir / "defect-ledger.md").write_text(ledger_r1, encoding="utf-8")
        (run_dir / "metrics.md").write_text(metrics_r1, encoding="utf-8")
        (panel_dir / "fact.md").write_text(
            "# 面板\n\n## FINDING FACT-01\n- 位置：§1\n- 问题：x\n", encoding="utf-8"
        )
        bad = check_run_dir(run_dir, panel_dir, "R1")
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed or "缺少 `schema: V2.5" not in messages:
            print("self-test FAIL: 缺 schema 标记的新文档未被拦截")
            print_report(bad, False)
            return 1

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

    # V3：完整交易轮（full + root/origin + certify 0）→ 通过
    with tempfile.TemporaryDirectory() as tmp:
        run_dir, panel_dir = write_v3_fixture(Path(tmp))
        good = check_run_dir(run_dir, panel_dir, "R1")
        if good.failed:
            print("self-test FAIL: V3 完整交易轮被判 FAIL")
            print_report(good, False)
            return 1

    # V3：verification 轮型 → 拦截
    with tempfile.TemporaryDirectory() as tmp:
        metrics = V3_METRICS.replace("轮型: full", "轮型: verification")
        run_dir, panel_dir = write_v3_fixture(Path(tmp), metrics=metrics)
        bad = check_run_dir(run_dir, panel_dir, "R1")
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed or "V3 轮型只能是 full" not in messages:
            print("self-test FAIL: V3 verification 轮型未被拦截")
            print_report(bad, False)
            return 1

    # V3：缺少 certify 新准入 → 拦截
    with tempfile.TemporaryDirectory() as tmp:
        metrics = V3_METRICS.replace("- certify 新准入: 0\n", "")
        run_dir, panel_dir = write_v3_fixture(Path(tmp), metrics=metrics)
        bad = check_run_dir(run_dir, panel_dir, "R1")
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed or "certify 新准入" not in messages:
            print("self-test FAIL: V3 缺 certify 新准入未被拦截")
            print_report(bad, False)
            return 1

    # V3：台账缺 根因 ID/origin → 拦截
    with tempfile.TemporaryDirectory() as tmp:
        ledger = """# 台账

## R1 面板轮

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-01 | P-FACT | P1 | 事实 | ① | 修 §1 | E1 | 旧串 grep 归零 |
"""
        run_dir, panel_dir = write_v3_fixture(Path(tmp), ledger=ledger)
        bad = check_run_dir(run_dir, panel_dir, "R1")
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed or "V3 台账缺少" not in messages:
            print("self-test FAIL: V3 台账缺根因列未被拦截")
            print_report(bad, False)
            return 1

    # V3：唯一根因数与台账不一致 → 拦截
    with tempfile.TemporaryDirectory() as tmp:
        metrics = V3_METRICS.replace("- 唯一根因数: 1", "- 唯一根因数: 2")
        run_dir, panel_dir = write_v3_fixture(Path(tmp), metrics=metrics)
        bad = check_run_dir(run_dir, panel_dir, "R1")
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed or "唯一根因数 2 与台账根因 ID 数 1 不一致" not in messages:
            print("self-test FAIL: V3 唯一根因数不一致未被拦截")
            print_report(bad, False)
            return 1

    # V3：certify 新准入 >0 且未闭环=0 → 不得关闭
    with tempfile.TemporaryDirectory() as tmp:
        metrics = V3_METRICS.replace("- certify 新准入: 0", "- certify 新准入: 1")
        run_dir, panel_dir = write_v3_fixture(Path(tmp), metrics=metrics)
        bad = check_run_dir(run_dir, panel_dir, "R1")
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed or "不得关闭" not in messages:
            print("self-test FAIL: V3 certify 新准入>0 未被拦截")
            print_report(bad, False)
            return 1

    # V3 过渡：schema: V3.0 from R2 时，历史 R1 台账没有根因列不能被判红；
    # 只检查 R2 起的新表。
    with tempfile.TemporaryDirectory() as tmp:
        run_dir = Path(tmp) / "run"
        panel_dir = run_dir / "panel" / "round2"
        panel_dir.mkdir(parents=True)
        sections = GOOD_LOOP.replace(
            "## §5 复验\n本节的占位内容。",
            "## §5 复验\n- certify 新准入: 0。",
        ).replace("## ", "### ")
        sections_r1 = GOOD_LOOP.replace("## ", "### ")
        loop = (
            "schema: V3.0 from R2\n\n"
            "edition: V3.0\n\n"
            "## R1 面板轮\n\n"
            + sections_r1
            + "\n\n## R2 面板轮\n\n"
            + sections
            + "\n\n### L6\nmeta 结论。\n\n### L6.5\n三问记录。\n"
        )
        ledger = """# 台账

## R1 面板轮

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-01 | P-FACT | P1 | 事实 | ① | 修 | E1 | grep 归零 |

## R2 面板轮

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD-001 | FACT-02 | R2 | P-FACT | P1 | 事实 | ① | 修 | E1 | grep 归零 |
"""
        metrics = """# 指标

## R2

- 发现总数: 1
- 唯一根因数: 1
- 本轮新准入: 1
- certify 新准入: 0
- 重复合并: 0
- 自引入: 0
- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0
- 未闭环: 0
- 轮型: full
- edition: V3.0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 1 / token 1 / 耗时 1min
"""
        (run_dir / "loop.md").write_text(loop, encoding="utf-8")
        (run_dir / "defect-ledger.md").write_text(ledger, encoding="utf-8")
        (run_dir / "metrics.md").write_text(metrics, encoding="utf-8")
        (panel_dir / "fact.md").write_text(
            "# 面板\n\n## FINDING FACT-02\n- 位置：§1\n- 问题：示例\n",
            encoding="utf-8",
        )
        good = check_run_dir(run_dir, panel_dir, "R2")
        if good.failed:
            print("self-test FAIL: V3 过渡轮（R1 历史表无根因列）被判 FAIL")
            print_report(good, False)
            return 1

    # V3：同一 edition 第 3 轮仍未闭环 → WARN 要求走 L7/EDITION-RETRO
    metrics_trial = "# 指标\n\n" + "\n\n".join(
        f"## R{i}\n\n- 未闭环: 1\n- 轮型: full\n- edition: V3.0"
        for i in (1, 2, 3)
    )
    trial_report = Report()
    check_edition_trial(metrics_trial, trial_report, 1)
    if not any(
        item["level"] == "WARN" and "第 4 轮" in item["message"]
        for item in trial_report.items
    ):
        print("self-test FAIL: 同一 edition 第 3 轮未闭环未触发 L7 WARN")
        return 1

    # V3：R1 的根因未闭环（⑤）却出现 R2 → 必须拦截（按根因，不按原始 ID）
    with tempfile.TemporaryDirectory() as tmp:
        run_dir = Path(tmp) / "run"
        panel_dir = run_dir / "panel" / "round2"
        panel_dir.mkdir(parents=True)
        sections = GOOD_LOOP.replace(
            "## §5 复验\n本节的占位内容。",
            "## §5 复验\n- certify 新准入: 0。",
        ).replace("## ", "### ")
        loop = (
            "schema: V3.0 from R1\n\n"
            "edition: V3.0\n\n"
            "## R1 面板轮\n\n"
            + sections
            + "\n\n### L6\nmeta 结论。\n\n### L6.5\n三问记录。\n\n"
            "## R2 面板轮\n\n"
            + sections
            + "\n\n### L6\nmeta 结论。\n\n### L6.5\n三问记录。\n"
        )
        ledger = """# 台账

## R1 面板轮

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD-001 | FACT-01 | R1 | P-FACT | P1 | 事实 | ⑤ | 还缺源码核实，取证路径 X | panel | 未闭环 |

## R2 面板轮

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD-002 | FACT-02 | R2 | P-FACT | P1 | 事实 | ① | 修 | E1 | grep 归零 |
"""
        metrics = """# 指标

## R1

- 发现总数: 1
- 唯一根因数: 1
- 本轮新准入: 1
- certify 新准入: 0
- 重复合并: 0
- 自引入: 0
- 处置: ① 0 / ② 0 / ④ 0 / ⑤ 1 / ⑥ 0 / FP 0
- 未闭环: 1
- 轮型: full
- edition: V3.0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 1 / token 1 / 耗时 1min

## R2

- 发现总数: 1
- 唯一根因数: 1
- 本轮新准入: 1
- certify 新准入: 0
- 重复合并: 0
- 自引入: 0
- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0
- 未闭环: 0
- 轮型: full
- edition: V3.0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 1 / token 1 / 耗时 1min
"""
        (run_dir / "loop.md").write_text(loop, encoding="utf-8")
        (run_dir / "defect-ledger.md").write_text(ledger, encoding="utf-8")
        (run_dir / "metrics.md").write_text(metrics, encoding="utf-8")
        (panel_dir / "fact.md").write_text(
            "# 面板\n\n## FINDING FACT-02\n- 位置：§1\n- 问题：示例\n",
            encoding="utf-8",
        )
        bad = check_run_dir(run_dir, panel_dir, "R2")
        messages = " ".join(item["message"] for item in bad.items)
        if not bad.failed or "禁止带病进入下一轮" not in messages:
            print("self-test FAIL: V3 R1 根因未闭环却出现 R2 未被拦截")
            print_report(bad, False)
            return 1

    # V3：同一根因跨轮复发（origin=R1）不计“本轮新准入”，R2 可关闭
    with tempfile.TemporaryDirectory() as tmp:
        run_dir = Path(tmp) / "run"
        panel_dir = run_dir / "panel" / "round2"
        panel_dir.mkdir(parents=True)
        sections = GOOD_LOOP.replace(
            "## §5 复验\n本节的占位内容。",
            "## §5 复验\n- certify 新准入: 0。",
        ).replace("## ", "### ")
        loop = (
            "schema: V3.0 from R1\n\n"
            "edition: V3.0\n\n"
            "## R1 面板轮\n\n"
            + sections
            + "\n\n### L6\nmeta 结论。\n\n### L6.5\n三问记录。\n\n"
            "## R2 面板轮\n\n"
            + sections
            + "\n\n### L6\nmeta 结论。\n\n### L6.5\n三问记录。\n"
        )
        ledger = """# 台账

## R1 面板轮

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD-001 | FACT-01 | R1 | P-FACT | P1 | 事实 | ① | 修 | E1 | grep 归零 |

## R2 面板轮

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD-001 | FACT-02 | 复发：origin R1；last_seen R2 | P-FACT | P1 | 事实 | ① | 修 | E1 | grep 归零 |
"""
        metrics = """# 指标

## R1

- 发现总数: 1
- 唯一根因数: 1
- 本轮新准入: 1
- certify 新准入: 0
- 重复合并: 0
- 自引入: 0
- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0
- 未闭环: 0
- 轮型: full
- edition: V3.0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 1 / token 1 / 耗时 1min

## R2

- 发现总数: 1
- 唯一根因数: 1
- 本轮新准入: 0
- certify 新准入: 0
- 重复合并: 1
- 自引入: 0
- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0
- 未闭环: 0
- 轮型: full
- edition: V3.0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 1 / token 1 / 耗时 1min
"""
        (run_dir / "loop.md").write_text(loop, encoding="utf-8")
        (run_dir / "defect-ledger.md").write_text(ledger, encoding="utf-8")
        (run_dir / "metrics.md").write_text(metrics, encoding="utf-8")
        (panel_dir / "fact.md").write_text(
            "# 面板\n\n## FINDING FACT-02\n- 位置：§1\n- 问题：示例\n",
            encoding="utf-8",
        )
        good = check_run_dir(run_dir, panel_dir, "R2")
        if good.failed:
            print("self-test FAIL: 同一根因跨轮复发被误判为新准入")
            print_report(good, False)
            return 1

    # V3：B 类行只写“五门 PASS” → 行级证据与冷读双 FAIL
    evidence_report = Report()
    check_v3_evidence(
        {
            "READER-A-01": {
                "class": "④",
                "action": "修",
                "verify": "doc 五门 PASS",
                "role": "reader-a",
                "section": "R1 面板轮",
                "evidence": "§1 读者卡住",
            }
        },
        evidence_report,
        "R1",
        1,
    )
    evidence_messages = " ".join(item["message"] for item in evidence_report.items)
    if "行级证据" not in evidence_messages or "B 类" not in evidence_messages:
        print("self-test FAIL: V3 B 类行只写五门 PASS 未被拦截")
        print_report(evidence_report, False)
        return 1

    # V3：同一处置/复验文本 >3 次 → 模板串 FAIL
    template_report = Report()
    check_v3_evidence(
        {
            f"FACT-0{i}": {
                "class": "①",
                "action": "按面板最小修复落地",
                "verify": "doc 五门 PASS；见 loop §5",
                "role": "P-FACT",
                "section": "R1 面板轮",
                "evidence": "",
            }
            for i in range(5)
        },
        template_report,
        "R1",
        1,
    )
    template_messages = " ".join(item["message"] for item in template_report.items)
    if "疑似模板" not in template_messages:
        print("self-test FAIL: V3 模板串重复未被拦截")
        print_report(template_report, False)
        return 1

    # todo 台账：`允许开放` 是非法第七桶，必须被拦截
    with tempfile.TemporaryDirectory() as tmp:
        todo = Path(tmp) / "todo.md"
        todo.write_text(
            "# todo\n\n"
            "| 条目 | 一句话 | 类型 | 当前状态 | 去向 |\n"
            "|---|---|---|---|---|\n"
            "| I-30 | 新登记教学任务 | 文档教学 | 允许开放（刚登记） | §5 |\n"
            "| D-1 | 已完成项 | 代码 | ✅ 已实施 | §2 |\n",
            encoding="utf-8",
        )
        todo_report = Report()
        check_todo_ledger(todo, todo_report)
        todo_messages = " ".join(item["message"] for item in todo_report.items)
        if "非法状态" not in todo_messages:
            print("self-test FAIL: todo 台账的 `允许开放` 未被拦截")
            print_report(todo_report, False)
            return 1

    # V3.3：OPEN-DEBT 未销账行 → FAIL 并计入未闭环
    with tempfile.TemporaryDirectory() as tmp:
        debt = Path(tmp) / "OPEN-DEBT.md"
        debt.write_text(
            "# open-debt\n\n"
            "| ID | 严重度 | 处置/档期 |\n"
            "|---|---|---|\n"
            "| R26-design-batch | P1 | 待实施（R27）；截止 R27 边界 |\n"
            "| F-R17-02 | P2 | ✅ 已修：已完成 |\n",
            encoding="utf-8",
        )
        debt_report = Report()
        open_count = check_open_debt(debt, debt_report, "R31")
        debt_messages = " ".join(item["message"] for item in debt_report.items)
        if open_count != 1 or "未销账" not in debt_messages or "已逾期" not in debt_messages:
            print("self-test FAIL: OPEN-DEBT 未销账行未被拦截或逾期未点名")
            print_report(debt_report, False)
            return 1

    # V3.3：读者 P3 按 ①/④ 准入 → FAIL；severity 与面板不一致 → FAIL
    admission_report = Report()
    check_v33_admission(
        {
            "READER-A-01": {
                "class": "④",
                "severity": "P3",
                "role": "reader-a",
                "section": "R31 面板轮",
            },
            "FACT-B-01": {
                "class": "①",
                "severity": "P1",
                "role": "P-FACT-B",
                "section": "R31 面板轮",
            },
        },
        {(31, "FACT-B-01"): "P0"},
        admission_report,
        "R31",
        31,
    )
    admission_messages = " ".join(item["message"] for item in admission_report.items)
    if "suggestions" not in admission_messages or "面板标题" not in admission_messages:
        print("self-test FAIL: V3.3 读者 P3 准入 / severity 同步未被拦截")
        print_report(admission_report, False)
        return 1

    # V3.3：修改轮自判 CONVERGED → FAIL（收敛声明权只在确认轮）
    lifecycle_loop = (
        "## R31 完整 survey\n\n### R31 §7 状态\n\n`CERTIFY PASS / CONVERGED（按代理口径 A）`\n"
    )
    lifecycle_metrics = """# 指标

## R31 完整 survey

- 发现总数: 1
- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0
- 未闭环: 0
- certify 新准入: 0
- 轮型: full
- 轮结果: CONVERGED
- edition: V3.1
"""
    lifecycle_report = Report()
    check_v33_lifecycle(
        lifecycle_metrics,
        {},
        {},
        lifecycle_report,
        26,
        31,
    )
    lifecycle_messages = " ".join(item["message"] for item in lifecycle_report.items)
    if "收敛声明权" not in lifecycle_messages:
        print("self-test FAIL: 修改轮自判 CONVERGED 未被拦截")
        print_report(lifecycle_report, False)
        return 1

    # V3.3：确认轮通过（新准入=0）合法；确认轮实施 ⑥ → FAIL
    confirm_metrics = lifecycle_metrics.replace("- 轮型: full", "- 轮型: confirmation")
    confirm_ledger_rows = {
        "RD-R31-01": {
            "class": "⑥",
            "action": "实施",
            "verify": "cargo test",
            "root": "RD-R31-01",
            "origin": "R31",
            "severity": "P2",
            "role": "design-d2",
            "section": "R31 完整 survey",
        }
    }
    confirm_sections = {"R31 完整 survey": {"RD-R31-01": "⑥"}}
    confirm_report = Report()
    check_v33_lifecycle(
        confirm_metrics,
        confirm_ledger_rows,
        confirm_sections,
        confirm_report,
        26,
        31,
    )
    confirm_messages = " ".join(item["message"] for item in confirm_report.items)
    if "确认轮实施 ⑥" not in confirm_messages:
        print("self-test FAIL: 确认轮实施 ⑥ 未被拦截")
        print_report(confirm_report, False)
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
    parser.add_argument(
        "--round",
        dest="round_tag",
        metavar="R25",
        help="只检查指定轮次（覆盖范围、收口、未闭环、round-order）；轮次关账用这个",
    )
    parser.add_argument("--json", action="store_true", help="输出 JSON")
    parser.add_argument(
        "--audit-templates",
        action="store_true",
        help="审计历史轮次的处置/复验模板串重复（R24/R25 的隐式带病）",
    )
    parser.add_argument(
        "--todo-file",
        help="同时核对该文档的 todo 台账（如 rewrite-notes/01-stage-kernel/01-todo.md）",
    )
    parser.add_argument("--self-test", action="store_true", help="运行内置自测")
    args = parser.parse_args()

    if args.self_test:
        return self_test()
    if not args.run_dir:
        parser.print_help()
        return 2

    panel_dir = Path(args.panel_dir) if args.panel_dir else None
    report = check_run_dir(
        Path(args.run_dir), panel_dir, args.round_tag, args.audit_templates
    )
    if args.todo_file:
        check_todo_ledger(Path(args.todo_file), report)
    print_report(report, args.json)
    return 1 if report.failed else 0


if __name__ == "__main__":
    sys.exit(main())
