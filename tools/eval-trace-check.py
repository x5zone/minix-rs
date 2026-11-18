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
METRIC_COUNT_RE = re.compile(r"([①②④⑤⑥③]|FP|SUGG)\s*(\d+)")
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
    # V3.3 D-047: triage-out class — reader/design P3 findings routed to
    # suggestions.md. Visible in the ledger, never open, never in G1-G8.
    "SUGG": "SUGG",
    "sugg": "SUGG",
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


def _pick_metric_block(
    text: str, report: Report, round_tag: str | None, strict: bool = False
) -> tuple[str, str]:
    blocks = _metric_blocks(text)
    if round_tag:
        for title, body in blocks:
            if _round_key_matches(title, round_tag):
                return title, body
        # r8（对抗判例）：V3 的 `--round` 模式不得回退到最后一块——块缺失（含被
        # 写成 `# R<N>` 单级标题的旁路）必须由调用方判 FAIL；V2.5 历史夹具保留
        # 回退语义。
        if strict:
            return "", ""
        return blocks[-1] if blocks else ("", text)
    # No round heading: a single-block file is the normal case; otherwise the
    # last block is the newest round.
    return blocks[-1] if blocks else ("", text)


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
    v34: bool = False,
) -> None:
    block_title, body = _pick_metric_block(text, report, round_tag, strict=v3)
    total_match = METRIC_TOTAL_RE.search(body)
    counts = {m.group(1): int(m.group(2)) for m in METRIC_COUNT_RE.finditer(body)}
    if round_tag:
        round_num = int(round_tag.lstrip("R"))
    else:
        # Whole-file mode: judge each block by its OWN round number, so the
        # V3.3 keys bind from v33_start and never leak onto the last
        # historical block (archival audit must not grow new reds).
        m = re.search(r"\bR(\d+)\b", block_title)
        round_num = int(m.group(1)) if m else None
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
    # V3.3: triage-out findings (reader/design P3 → suggestions) are disposed
    # as SUGG rows — required in the disposition sum for bound rounds.
    if v33_start is not None and (round_num is None or round_num >= v33_start):
        required.append("SUGG")
    missing = [k for k in required if k not in counts]
    if missing:
        report.fail("metrics", "metrics.md 缺少处置计数: " + "、".join(missing))
    if not legacy and "③" in counts:
        report.fail("metrics", "R25+ 禁止使用已删除的 ③；误报写 FP 并附反证，否则写 ⑤ 未闭环")
    if v3 and total_match is None:
        # r10（P0-② 判例）：缺 `发现总数` 键会让和校验、不变式、分档对账、INDEX
        # 对账四个校验静默全跳——V3 轮必须有该键。
        report.fail("metrics", "V3 metrics 缺少 `- 发现总数: <整数>` 键")
    if total_match is not None and not missing:
        total = int(total_match.group(1))
        parts = [counts[k] for k in required]
        _new = re.search(r"^-\s*本轮新准入\s*[:：]\s*(\d+)", body, re.MULTILINE)
        if _new is not None and total < int(_new.group(1)):
            report.fail(
                "metrics",
                f"发现总数 {total} < 本轮新准入 {_new.group(1)}（r9 判例：三账必须同源）",
            )
        _uniq = re.search(r"^-\s*唯一根因数\s*[:：]\s*(\d+)", body, re.MULTILINE)
        if _uniq is not None and total < int(_uniq.group(1)):
            report.fail(
                "metrics",
                f"发现总数 {total} < 唯一根因数 {_uniq.group(1)}（r9 判例）",
            )
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
            result_key = re.search(
                r"^-\s*轮结果\s*[:：]\s*(ROUND-CLOSED|CONVERGED|NOT CONVERGED"
                r"|NOT CONVERGING|FREEZE-WITH-BACKLOG|BLOCKED)\b",
                body,
                re.MULTILINE,
            )
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
            # V3.3 D-047 治理仪表：代裁决授权与漂移可见性
            for key in ["代裁决", "ARCH 提案", "用户否决"]:
                if re.search(rf"^\-\s*{re.escape(key)}\s*[:：]\s*(\d+)", body, re.MULTILINE) is None:
                    report.fail("metrics", f"V3.3 metrics 缺少治理仪表 `- {key}: <整数>`")
            # V3.4 最小内核（P1-7 判例：D03/D04 的「certify 待跑」与 CONVERGED 同轮
            # 并存且门放行）：闭合声明不得与 certify 未完成标记同存。作用域限定
            # `schema: V3.4`——01–04 已 FREEZE 的存量记录不追溯（r5 回归审查 H1）。
            if (
                v34
                and result_key is not None
                and result_key.group(1) in {"CONVERGED", "ROUND-CLOSED"}
                and re.search(
                    r"certify[^\n]{0,16}(?:待跑|未跑|待补|待安排|未完成|未执行|没跑|延后|排期未定|待复跑|未复跑|尚未启动|回读未做|pending|not run|推迟)",
                    body,
                    re.IGNORECASE,
                )
            ):
                report.fail(
                    "metrics",
                    "轮结果为闭合态，但记录仍写「certify 待跑/未跑」；闭合声明缺闭合"
                    "证据——先跑完 certify 再改轮结果（P1-7）",
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
        # §4.0f exit 2: a DECISIONS.md-recorded user adjudication — including
        # an explicitly approved deferral — resolves the row. Approval without
        # a date is not an adjudication (mirrors the ② gate).
        if "用户批准" in row_text and re.search(r"20\d{2}-\d{2}-\d{2}", row_text):
            report.warn(
                "open-debt",
                f"{item} 已批准延期；确认 DECISIONS.md 判决账行存在（含日期与去向）",
            )
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
        if (
            (role.startswith("reader") or role.startswith("design"))
            and severity == "P3"
            and cls in {"①", "④", "⑥"}
        ):
            report.fail(
                "admission-v33",
                f"{fid} 是 {role} P3 却按 {cls} 准入；G3（V3.3）：改标 SUGG 并登记"
                " suggestions.md，只有通过「读者卡住无法继续」或「正确性缺陷」测试的"
                "才按 ①/④ 入台账",
            )
        if cls == "SUGG" and not (
            (role.startswith("reader") or role.startswith("design"))
            and severity == "P3"
        ):
            report.fail(
                "admission-v33",
                f"{fid} 按 SUGG 分流但来源/严重度为 {role or '?'}/{severity or '?'}；"
                "SUGG 仅限读者/设计的 P3——P0-P2 不得借 SUGG 逃出修复队列（D-047）",
            )
        panel_sev = panel_severities.get((section_round, fid))
        if panel_sev and severity and panel_sev != severity:
            report.fail(
                "severity-sync",
                f"{fid} 台账严重度 {severity} 与面板标题 {panel_sev} 不一致；"
                "severity 列必须从面板 FINDING 标题机械同步",
            )
        # V3.3 D-047: design decisions must declare their class. Unlabeled
        # implementations are unclassifiable and treated as ARCH (blocked) —
        # the burden of proof sits on the proxy side (C anchor required).
        if role.startswith("design") and cls == "⑥":
            row_text = " ".join(
                (row.get("action") or "", row.get("evidence") or "", row.get("verify") or "")
            )
            tag = re.search(r"\[(MODERN|DEL|ARCH)\]", row_text)
            if tag is None:
                report.fail(
                    "design-class",
                    f"{fid} 设计实施未分类（需 [MODERN]/[DEL]/[ARCH] 标记）；"
                    "未分类按 ARCH 阻断——举证责任在代裁侧：[MODERN]/[DEL] 必须能锚定 "
                    "Minix3 C 语义（D-047）",
                )
            elif tag.group(1) == "ARCH" and not (
                re.search(r"D-\d+", row_text)
                and "用户批准" in row_text
                and re.search(r"20\d{2}-\d{2}-\d{2}", row_text)
            ):
                report.fail(
                    "design-class",
                    f"{fid} 标记 [ARCH] 却已实施且无判决行引用（需 D-xxx + 用户批准 + 日期）；"
                    "ARCH 类必须先落 DECISIONS.md 判决账行（用户裁决）——即便口头说继续"
                    "也必须落行（D-047）",
                )


def _has_round_ledger(n: int, section_classes: dict[str, dict[str, str]]) -> bool:
    """r8（对抗判例）：`发现总数: 0` 两行伪造仍可满足 mod_closed——真实修改轮的
    台账必须有该轮的行。计入要件 = metrics 骨架 + 台账该轮节且非空。"""
    return any(
        _round_key_matches(sec, f"R{n}") and bool(mapping)
        for sec, mapping in section_classes.items()
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
    prev_edition: str | None = None
    # V3.4 最小内核（r6 R6-a 判例）：确认轮验证的是修改轮的收口——此前必须存在
    # 已关闭的修改轮（full/structural 且 ROUND-CLOSED 且未闭环=0），否则「确认」
    # 是伪造收敛。按文档计（不按 edition）：R34 型跨 edition 确认合法（D-057 口径）。
    mod_closed: list[int] = []
    for title, body in _metric_blocks(metrics_text):
        m = re.search(r"\bR(\d+)\b", title)
        if not m:
            continue
        n = int(m.group(1))
        # mod_closed 追踪在 edition/era 过滤之前：已关闭的修改轮按文档计，
        # 跨 edition（甚至 V3.3 前）的收口也算数——D-057 的 R34 型口径。
        _rt_early = re.search(r"^\s*-\s*轮型\s*[:：]\s*(\S+)", body, re.MULTILINE)
        _res_early = re.search(
            r"^\s*-\s*轮结果\s*[:：]\s*(ROUND-CLOSED|CONVERGED|NOT CONVERGED"
            r"|NOT CONVERGING|FREEZE-WITH-BACKLOG|BLOCKED)\b",
            body,
            re.MULTILINE,
        )
        _open_early = re.search(r"^\s*-\s*未闭环\s*[:：]\s*(\d+)", body, re.MULTILINE)
        # r7（对抗判例）：两行伪造的 `full`+`ROUND-CLOSED` 空块也计入 mod_closed——
        # 计入要件加「块内有 `发现总数` 键」（真实轮必有；伪造须造全套 metrics，
        # 而全套伪造在 N0 双源与 coverage 门射程内）。
        _has_total = re.search(r"^\s*-\s*发现总数\s*[:：]", body, re.MULTILINE)
        if (
            _rt_early is not None
            and _rt_early.group(1) in {"full", "structural"}
            and _res_early is not None
            and _res_early.group(1) == "ROUND-CLOSED"
            and _has_total is not None
            and _has_round_ledger(n, section_classes)
            and (_open_early is None or int(_open_early.group(1)) == 0)
        ):
            mod_closed.append(n)
        if v33_start is not None and n < v33_start:
            continue
        if schema_start is not None and n < schema_start:
            continue
        # Per-CANDIDATE counters: a new edition (after NOT CONVERGING, or a
        # user-approved reopen of a sealed doc) starts a fresh confirmation
        # allowance and unseals. Without this, any document whose first
        # confirmation fails twice can never converge (K=3 editions would
        # exhaust the global counter).
        ed = re.search(r"^-\s*edition\s*[:：]\s*(V[\d.]+)", body, re.MULTILINE)
        edition = ed.group(1) if ed else None
        if edition is not None and prev_edition is not None and edition != prev_edition:
            confirmations = 0
            sealed_at = None
        if edition is not None:
            prev_edition = edition
        rt = re.search(r"^-\s*轮型\s*[:：]\s*(\S+)", body, re.MULTILINE)
        rtype = rt.group(1) if rt else None
        result = re.search(
            r"^-\s*轮结果\s*[:：]\s*(ROUND-CLOSED|CONVERGED|NOT CONVERGED"
            r"|NOT CONVERGING|FREEZE-WITH-BACKLOG|BLOCKED)\b",
            body,
            re.MULTILINE,
        )
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
            if result_val == "CONVERGED" and not mod_closed:
                report.fail(
                    "lifecycle-v33",
                    f"R{n} 确认轮宣告 CONVERGED，但本文档此前没有任何已关闭的修改轮"
                    "（full/structural 且 ROUND-CLOSED 且未闭环=0）；确认轮验证的是修改轮"
                    "的收口，无前置修改轮的「确认」是伪造收敛（r6 R6-a）",
                )
            if certify_new == 0:
                sealed_at = n
            for section, mapping in section_classes.items():
                if not _round_key_matches(section, f"R{n}"):
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
        # V3.3 D-047: an ARCH-blocked round (⑤ rows awaiting user adjudication)
        # must declare `轮结果: BLOCKED` — the hard stop is a recorded state,
        # not a chat understanding. 口头"继续"不解锁：解除只认判决行。
        arch_blocked = [
            fid
            for section, mapping in section_classes.items()
            if _round_key_matches(section, f"R{n}")
            for fid, cls in mapping.items()
            if cls == "⑤"
            and "ARCH" in (
                (ledger_rows.get(fid, {}).get("action") or "")
                + (ledger_rows.get(fid, {}).get("verify") or "")
                + (ledger_rows.get(fid, {}).get("evidence") or "")
            )
            # V3.4-r8（修复回归 F6）：已批准延期（用户批准 + 日期 + D-xxx 判决行
            # 引用）不触发 ARCH 硬阻断——否则「延期」是死选项。
            and not (
                "用户批准" in (ledger_rows.get(fid, {}).get("action") or "")
                and re.search(r"20\d{2}-\d{2}-\d{2}", ledger_rows.get(fid, {}).get("action") or "")
                and re.search(r"\bD-\d{2,3}\b", ledger_rows.get(fid, {}).get("action") or "")
            )
        ]
        if arch_blocked and result_val not in (None, "BLOCKED"):
            report.fail(
                "lifecycle-v33",
                f"R{n} 存在等待用户 ARCH 裁决的 ⑤ 行（{', '.join(arch_blocked[:3])}…），"
                f"但轮结果为 `{result_val}`；必须 `BLOCKED`，且解除只认判决账行"
                "（D-047：口头继续不解除）",
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
    paths = [
        p
        for p in sorted(panel_dir.glob("*.md"))
        + sorted(p for p in panel_dir.glob("*/*.md") if p.is_file())
        if p.name != "INDEX.md"
    ]
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
        # r10（P0-① 判例）：带 ID 的 SUGGESTION 也是面板发现——SUGG 行入台账并
        # 计入 metrics 后，覆盖率门必须能看见它们，否则合规记账双 FAIL。
        found += re.findall(
            r"^##\s+SUGGESTION\s+([A-Za-z][A-Za-z0-9-]*)", text, re.MULTILINE
        )
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
                # V3.3 D-047: a ⑤ row whose action/verify records an approved
                # deferral (用户批准 + date) is resolved-by-decision.
                if "用户批准" in (row["action"] + row["verify"]) and re.search(
                    r"20\d{2}-\d{2}-\d{2}", row["action"] + row["verify"]
                ):
                    is_open = False
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


def _round_key_matches(key: str, target: str) -> bool:
    """`R1` 不得匹配 `R10`/`R12`（r7 前缀串扰判例：--round R1 把 R10+ 台账节并入
    作用域，产出假 FAIL；反向则污染分档聚合）。"""
    return re.match(rf"{re.escape(target.lower())}\b", key.lower()) is not None


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
    metrics_text: str | None = None,
    round_tag: str | None = None,
) -> None:
    """A round with open items must not have a later round section."""
    open_rounds = _round_open_items(ledger_rows, section_classes, schema_start, v3)
    loop_rounds = _loop_round_numbers(loop_text)
    ledger_rounds = {
        int(m.group(1))
        for section in section_classes
        if (m := re.match(r"^R(\d+)\b", section))
    }
    # V3.4 最小内核（N0/R31 判例）：轮序门原只从台账行推 open 集——「不写台账的
    # 未关闭轮」结构性逃逸。双源补洞：metrics 块（须含真实账目键，防一行空标题
    # 绕过；区间标题按全部轮号登记）也算账。
    metrics_rounds: set[int] = set()
    metrics_open: set[int] = set()
    if metrics_text:
        for title, body in _metric_blocks(metrics_text):
            if not re.search(r"发现总数|轮结果|未闭环", body):
                continue
            ns = [int(n) for n in re.findall(r"\bR(\d+)\b", title)]
            metrics_rounds.update(ns)
            # r7（对抗判例）：双源只看「有没有账」不看「账说什么」——metrics 自报
            # 未闭环>0 的轮同样不得静默越序。
            _mo = re.search(r"^\s*-\s*未闭环\s*[:：]\s*(\d+)", body, re.MULTILINE)
            # r8（对抗判例）：缺键不得默认已关闭——无 `未闭环` 键的轮按开放计。
            if _mo is None or int(_mo.group(1)) > 0:
                metrics_open.update(ns)
    all_rounds = loop_rounds | ledger_rounds | metrics_rounds
    want = int(round_tag.lstrip("R")) if round_tag else None
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
    # 双源断链检查：loop 有轮次小节但台账与 metrics 双缺 = 未闭合轮，不得静默
    # 越序；`--round R<N>` 只对本轮及更早负责——更晚的断链在该轮时点尚不存在，
    # 不得追溯判红（r5 回归审查 H5）。
    if schema_start is not None:
        for r in sorted(loop_rounds):
            if r < schema_start:
                continue
            if want is not None and r > want:
                continue
            if r in ledger_rounds or (r in metrics_rounds and r not in metrics_open):
                continue
            later = sorted(x for x in all_rounds if x > r)
            if later:
                why = (
                    "metrics 自报未闭环>0" if r in metrics_open
                    else "既无台账节也无 metrics 块（未闭合轮）"
                )
                report.fail(
                    "round-order",
                    f"R{r} 在 loop.md 有轮次小节，但{why}；"
                    f"却已出现 R{later[0]}——未关闭轮不得静默越序（N0）",
                )


def enforce_adjacent_only_dispositions(
    ledger_rows: dict[str, dict],
    report: Report,
    schema_start: int | None,
) -> None:
    """V3.2 §4.1a: `②` is adjacent-scope only; `② 裁决` is illegal.

    A same-document pending-design item must be ⑥ implemented in-round (proxy
    adjudication) or the round goes BLOCKED — never transferred into the next
    round. Sections before `schema_start` are historical and exempt.

    A `②` row carrying `用户批准` + date is a historical adjudicated transfer
    (legal under V3.0, e.g. R26's D-043 batch): it is exempt here because its
    debt is separately enforced by the `open-debt` gate — double-flagging the
    same escape only buries the live signal.
    """
    for fid, row in ledger_rows.items():
        if row.get("class") != "②":
            continue
        section = row.get("section", "")
        section_round = _section_round(section)
        if section_round is None:
            # Tables under unparseable headings (处置索引 / Round N 明细) are
            # pre-V3 history: they cannot be dated, so they are never flagged.
            continue
        if (
            schema_start is not None
            and section_round < schema_start
        ):
            continue
        action = row.get("action", "")
        if "用户批准" in action and re.search(r"20\d{2}-\d{2}-\d{2}", action):
            continue
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
        if (schema_version(loop_text) or (0, 0)) >= (3, 4):
            # V3.5 制品链：V3.4 起面板必须入库在 run 目录；tmp 只作工作副本。
            pass
        elif not panel_dir.is_dir():
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
    _combined_ver = max(
        schema_version(loop_text) or (0, 0),
        schema_version(metrics_text) or (0, 0),
    )
    v33_doc_flag = _combined_ver >= (3, 3)
    v33_from = (
        v33_start_round(loop_text, fallback=schema_start) if v33_doc_flag else None
    )
    v34_doc_flag = _combined_ver >= (3, 4)
    # r9（考场判例）：制品链四项加固——
    # ① 适用面按组合 schema（loop ∪ metrics，防载体分脑）与 v34 标记起点轮
    #    （历史层回放不追溯）；② FREEZE 豁免要求文件非空且含 CONVERGED/
    #    FREEZE-WITH-BACKLOG（0 字节或自相矛盾的 FREEZE 不解锁）；③ 空制品不算
    #    证据（面板文件必须非空且含 FINDING/SUGGESTION/CLAIM 或「无发现」声明）；
    #    ④ INDEX 计数与文件内容、与 metrics `发现总数` 对账；--panel-dir 不得替代
    #    入库面板；certify 回执要求非空且无否定性结论（内容细审仍归 §4.7 人工）。
    _freeze_ok = False
    _freeze_path = run_dir / "FREEZE.md"
    if _freeze_path.is_file():
        _ft = _freeze_path.read_text(encoding="utf-8")
        # r10（P0-③ 判例）：子串匹配让「NOT CONVERGED」「尚未 CONVERGED」
        # 「历史曾 CONVERGED」的笔记解锁全链豁免——只认**正向状态行**。
        _pos = [
            ln
            for ln in _ft.splitlines()
            if re.search(r"\bCONVERGED\b|\bFREEZE-WITH-BACKLOG\b", ln)
            and not re.search(r"NOT\s+CONVERGED|尚未|曾|未", ln)
        ]
        _freeze_ok = len(_ft.strip()) > 0 and bool(_pos)
    _v34_from = (
        v33_start_round(loop_text, fallback=schema_start)
        if (schema_version(loop_text) or (0, 0)) >= (3, 4)
        else None
    )
    _round_no = int(round_tag_override.lstrip("R")) if round_tag_override else None
    _artifact_required = False
    if round_tag_override:
        if _combined_ver >= (3, 4):
            _artifact_required = (
                _v34_from is None or _round_no is None or _round_no >= _v34_from
            )
        elif _combined_ver >= (3, 3):
            _artifact_required = not _freeze_ok
    if _artifact_required and round_tag_override:
        _rk = round_tag_override.lstrip("R").lstrip("0") or "0"
        _scoped = run_dir / "panel" / f"round{_rk}"
        if panel_dir is not None and run_dir.resolve() not in panel_dir.resolve().parents:
            report.fail(
                "panel-artifact",
                "面板目录位于 run 目录之外（tmp 工作副本或外部 --panel-dir）："
                "V3.5 制品链要求以入库面板 panel/round<N>/ 为准，外部目录不得替代",
            )
        if not (_scoped.is_dir() and any(_scoped.glob("*.md"))):
            report.fail(
                "panel-artifact",
                f"面板制品未入库：runs/<stage>/<doc>/panel/round{_rk}/ 缺失或为空"
                "（V3.5 制品链；tmp 只作工作副本）",
            )
        else:
            _idx = _scoped / "INDEX.md"
            if not _idx.is_file():
                report.fail(
                    "panel-artifact",
                    f"panel/round{_rk}/INDEX.md 缺失（python3 tools/panel-index.py 生成）",
                )
            else:
                import hashlib as _hashlib

                _rows: dict[str, dict] = {}
                for _line in _idx.read_text(encoding="utf-8").splitlines():
                    _m = re.match(
                        r"^\|\s*([^|]+?)\s*\|\s*([0-9a-f]{64})\s*\|\s*(\d+)\s*\|\s*(\d+)\s*\|\s*(\d+)\s*\|\s*([a-z-]+)\s*\|$",
                        _line,
                    )
                    if _m:
                        _rows[_m.group(1)] = {
                            "sha256": _m.group(2),
                            "findings": int(_m.group(3)),
                            "suggestions": int(_m.group(4)),
                            "claims": int(_m.group(5)),
                        }
                _files = {f.name: f for f in _scoped.glob("*.md") if f.name != "INDEX.md"}
                if not _rows:
                    report.fail("panel-artifact", f"panel/round{_rk}/INDEX.md 无有效行")
                _panel_total = 0
                for _name, _rec in _rows.items():
                    if _name not in _files:
                        report.fail(
                            "panel-artifact",
                            f"panel/round{_rk}/{_name} 在索引中但文件不存在",
                        )
                        continue
                    _fpath = _files[_name]
                    _cur = _hashlib.sha256(_fpath.read_bytes()).hexdigest()
                    if _cur != _rec["sha256"]:
                        report.fail(
                            "panel-artifact",
                            f"panel/round{_rk}/{_name} sha256 与 INDEX.md 不一致"
                            "（面板被改动而未重生成索引）",
                        )
                    _text = _fpath.read_text(encoding="utf-8")
                    if len(_text.strip()) == 0:
                        report.fail(
                            "panel-artifact",
                            f"panel/round{_rk}/{_name} 是空文件（空制品不算面板证据）",
                        )
                        continue
                    _nf = len(re.findall(r"^##\s+FINDING\b", _text, re.MULTILINE))
                    _nc = len(re.findall(r"^##\s+CLAIM\s+\d+", _text, re.MULTILINE))
                    _ns = len(re.findall(r"^##\s+SUGGESTION\b", _text, re.MULTILINE))
                    if _nf + _nc + _ns == 0 and "无发现" not in _text:
                        report.fail(
                            "panel-artifact",
                            f"panel/round{_rk}/{_name} 既无 FINDING/SUGGESTION/CLAIM "
                            "也无「无发现」声明（空壳面板）",
                        )
                    if (
                        _nf != _rec["findings"]
                        or _ns != _rec["suggestions"]
                        or _nc != _rec["claims"]
                    ):
                        report.fail(
                            "panel-artifact",
                            f"panel/round{_rk}/{_name} 计数与 INDEX 不一致"
                            f"（FINDING {_nf}≠{_rec['findings']} / SUGG {_ns}≠{_rec['suggestions']}"
                            f" / CLAIM {_nc}≠{_rec['claims']}；重跑 panel-index.py）",
                        )
                    _panel_total += _nf + _nc + _ns
                for _name in _files:
                    if _name not in _rows:
                        report.fail(
                            "panel-artifact",
                            f"panel/round{_rk}/{_name} 存在但未入 INDEX.md",
                        )
                # INDEX 原始发现总数 vs metrics `发现总数`（r9 O5：不变式对账）
                if metrics_text is not None:
                    _mb = _pick_metric_block(metrics_text, report, round_tag_override, strict=True)[1]
                    _mt = re.search(r"^-\s*发现总数\s*[:：]\s*(\d+)", _mb, re.MULTILINE)
                    if _mt is not None and int(_mt.group(1)) != _panel_total:
                        report.fail(
                            "panel-artifact",
                            f"面板原始发现数 {_panel_total}（INDEX FINDING+SUGGESTION+CLAIM，均须带 ID）与 metrics "
                            f"发现总数 {_mt.group(1)} 不一致（r9 判例：三账必须同源）",
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
    if v3_doc and round_tag_override:
        # V3 轮关闭必须有非空面板目录（mkdir 空目录不算）——r8 判例；r9 复核。
        _empty_panel = panel_dir is None or not panel_dir.is_dir()
        if not _empty_panel:
            _empty_panel = not any(panel_dir.rglob("*.md"))
        if _empty_panel:
            report.fail(
                "panel",
                "V3 轮关闭要求面板证据：找不到 panel 目录或目录为空（至少一个角色报告；"
                "run-dir/panel 或 tmp/review-eval/<stage>/<doc>/panel）",
            )
    panel_ids = collect_panel_findings(panel_dir, report)

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
                    and _round_key_matches(row.get("section", ""), round_tag)
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
                if _round_key_matches(section, round_tag)
                for fid in fids
            }
            # No fallback to the whole ledger: if this round's section is
            # missing, every panel finding must be reported missing.
            coverage_ledger = scoped
            from collections import Counter
            expected_counts: dict[str, int] = {}
            counter: Counter[str] = Counter()
            for section, mapping in section_classes.items():
                if _round_key_matches(section, round_tag):
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
            v34=v34_doc_flag,
        )
        # The all-round checks follow the document's newest schema; the
        # per-round checks above use `v3` (the selected round's segment).
        check_round_type_sequence(metrics_text, report, schema_start, v3=v3_doc)
        if v3_doc:
            check_edition_trial(metrics_text, report, schema_start)
        # V3.4 最小内核（r5 回归审查 B4）：P1-7 同样覆盖 loop 轮次小节——D04 的
        # 「certify 待跑」写在 loop §5 而非 metrics，只扫 metrics 会漏。
        if _artifact_required and round_tag and loop_text is not None:
            _bt, _bb = _pick_metric_block(metrics_text, report, round_tag)
            _closes = re.search(
                r"^-\s*轮结果\s*[:：]\s*(CONVERGED|ROUND-CLOSED)\b",
                _bb,
                re.MULTILINE,
            )
            if _closes:
                _want = int(round_tag.lstrip("R"))
                _cert_dir = run_dir / "certify"
                _cert_files = (
                    list(_cert_dir.glob(f"round{_want}-*.md"))
                    if _cert_dir.is_dir()
                    else []
                )
                if not _cert_files:
                    report.fail(
                        "certify-artifact",
                        f"{round_tag} 为闭合态，certify 独立回执未入库"
                        f"（runs/<stage>/<doc>/certify/round{_want}-*.md；V3.5 制品链）",
                    )
                else:
                    _bad_certs = []
                    for _cert in _cert_files:
                        _ct = _cert.read_text(encoding="utf-8")
                        # r10（P0-④ 判例）：字节数被纯标点绕过——按**有效字符**
                        # （字母/数字/汉字）计数；否定词表补 结论：REOPEN/未通过。
                        _eff = len(
                            re.sub(r"[^\w\u4e00-\u9fff]", "", _ct)
                        )
                        if _eff < 20:
                            _bad_certs.append(
                                f"{_cert.name} 有效字符不足（{_eff}）"
                            )
                        elif re.search(
                            r"NOT\s*PASS|未通过|结论\s*[:：]\s*(FAIL|REOPEN)",
                            _ct,
                            re.IGNORECASE,
                        ):
                            _bad_certs.append(
                                f"{_cert.name} 含否定性结论（NOT PASS/FAIL/REOPEN/未通过）"
                            )
                    if _bad_certs:
                        report.fail(
                            "certify-artifact",
                            f"{round_tag} 的 certify 回执不合格："
                            + "；".join(_bad_certs)
                            + "（内容细审归 §4.7 人工，此处只拦空壳与否定结论）",
                        )
                _matches = list(ROUND_HEADER_RE.finditer(loop_text))
                for _i, _m in enumerate(_matches):
                    if _round_header_number(_m) != _want:
                        continue
                    _end = (
                        _matches[_i + 1].start()
                        if _i + 1 < len(_matches)
                        else len(loop_text)
                    )
                    if re.search(
                        r"certify[^\n]{0,16}(?:待跑|未跑|待补|待安排|未完成|未执行|没跑|延后|排期未定|待复跑|未复跑|尚未启动|回读未做|pending|not run|推迟|todo)",
                        loop_text[_m.start() : _end],
                        re.IGNORECASE,
                    ):
                        report.fail(
                            "round-closure",
                            f"{round_tag} 的 loop 记录写「certify 待跑/未跑」而 metrics "
                            "轮结果为闭合态；闭合声明缺闭合证据（P1-7，loop 侧）",
                        )

    if loop_text is not None:
        check_round_monotonicity(
            loop_text,
            ledger_rows,
            section_classes,
            report,
            schema_start,
            v3=v3,
            metrics_text=metrics_text,
            round_tag=round_tag,
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

## R1 面板轮

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

    # V3.3：OPEN-DEBT 未销账行 → FAIL 并计入未闭环；用户批准延期 → WARN 放行
    with tempfile.TemporaryDirectory() as tmp:
        debt = Path(tmp) / "OPEN-DEBT.md"
        debt.write_text(
            "# open-debt\n\n"
            "| ID | 严重度 | 处置/档期 |\n"
            "|---|---|---|\n"
            "| R26-design-batch | P1 | 待实施（R27）；截止 R27 边界 |\n"
            "| F-X-99 | P2 | ② 用户批准 2026-11-15 延期至 02 篇（D-047） |\n"
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
        if "已批准延期" not in debt_messages:
            print("self-test FAIL: 用户批准的延期出口未被识别")
            print_report(debt_report, False)
            return 1

    # V3.3：读者/设计 P3 按 ①/④/⑥ 准入 → FAIL；severity 与面板不一致 → FAIL；
    # 设计 ⑥ 未分类 → FAIL；[ARCH] 无判决行 → FAIL
    admission_report = Report()
    check_v33_admission(
        {
            "READER-A-01": {
                "class": "④",
                "severity": "P3",
                "role": "reader-a",
                "section": "R31 面板轮",
            },
            "DESIGN-D2-01": {
                "class": "⑥",
                "severity": "P3",
                "role": "design-d2",
                "section": "R31 面板轮",
                "action": "实施 [MODERN] BootStorage 内部提交断言（外部行为不变）",
                "evidence": "cargo test",
            },
            "DESIGN-D2-02": {
                "class": "⑥",
                "severity": "P2",
                "role": "design-d2",
                "section": "R31 面板轮",
                "action": "实施某项改进",
                "evidence": "cargo test",
            },
            "DESIGN-D2-03": {
                "class": "⑥",
                "severity": "P1",
                "role": "design-d2",
                "section": "R31 面板轮",
                "action": "实施 [ARCH] 借槽语义改 directMap",
                "evidence": "cargo test",
            },
            "READER-A-02": {
                "class": "SUGG",
                "severity": "P3",
                "role": "reader-a",
                "section": "R31 面板轮",
            },
            "FACT-B-02": {
                "class": "SUGG",
                "severity": "P2",
                "role": "P-FACT-B",
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
    if (
        "suggestions" not in admission_messages
        or "面板标题" not in admission_messages
        or "design-d2" not in admission_messages
    ):
        print("self-test FAIL: V3.3 读者/设计 P3 准入 / severity 同步未被拦截")
        print_report(admission_report, False)
        return 1
    class_messages = " ".join(
        item["message"]
        for item in admission_report.items
        if item["check"] == "design-class"
    )
    if "DESIGN-D2-02" not in class_messages or "未分类" not in class_messages:
        print("self-test FAIL: 设计 ⑥ 未分类未被拦截")
        print_report(admission_report, False)
        return 1
    if "DESIGN-D2-03" in class_messages and "判决行" not in class_messages:
        print("self-test FAIL: [ARCH] 无判决行未被拦截")
        print_report(admission_report, False)
        return 1
    if "DESIGN-D2-01" in class_messages:
        print("self-test FAIL: 已分类的设计行被误判")
        print_report(admission_report, False)
        return 1
    sugg_escape = any(
        item["check"] == "admission-v33" and "FACT-B-02" in item["message"]
        for item in admission_report.items
    )
    if not sugg_escape:
        print("self-test FAIL: P2 借 SUGG 逃逸分流未被拦截")
        print_report(admission_report, False)
        return 1
    reader_sugg_ok = not any(
        item["check"] == "admission-v33" and "READER-A-02" in item["message"]
        for item in admission_report.items
    )
    if not reader_sugg_ok:
        print("self-test FAIL: 合法 SUGG 分流被误判")
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
    # （r6 R6-a：确认轮前置须有已关闭修改轮——按文档计，故补一个 R30 收口轮）
    _closed_mod = (
        "## R30 修改轮\n\n- 发现总数: 1\n- 未闭环: 0\n- 轮型: full\n- 轮结果: ROUND-CLOSED\n\n"
    )
    confirm_metrics = lifecycle_metrics.replace(
        "- 轮型: full", "- 轮型: confirmation"
    ).replace("# 指标\n\n", "# 指标\n\n" + _closed_mod, 1)
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
    confirm_sections = {
        "R31 完整 survey": {"RD-R31-01": "⑥"},
        "R30 修改轮": {"RD-30": "①"},
    }
    _closed_rows = {
        "RD-30": {"class": "①", "action": "修", "verify": "grep x", "root": "RD-30",
                  "origin": "R30", "severity": "P2", "role": "fact",
                  "section": "R30 修改轮"},
    }
    confirm_report = Report()
    check_v33_lifecycle(
        confirm_metrics,
        {**confirm_ledger_rows, **_closed_rows},
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

    # V3.3：确认轮计数按 CANDIDATE（edition）重置——跨 edition 的第 3 次确认
    # 不再触发"最多 2 次"；CONVERGED 后的新 edition（批准重开）也不再 FAIL
    cross_edition_metrics = (
        lifecycle_metrics.replace(
            "- 轮型: full", "- 轮型: confirmation"
        ).replace("# 指标\n\n", "# 指标\n\n" + _closed_mod, 1)
        + "\n## R32 确认轮\n\n"
        + "- 发现总数: 0\n"
        + "- 处置: ① 0 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0\n"
        + "- 未闭环: 0\n"
        + "- certify 新准入: 0\n"
        + "- 轮型: confirmation\n"
        + "- 轮结果: CONVERGED\n"
        + "- edition: V3.3\n"
    )
    cross_report = Report()
    cross_sections = {"R30 修改轮": {"RD-30": "①"}}
    check_v33_lifecycle(
        cross_edition_metrics,
        _closed_rows,
        cross_sections,
        cross_report,
        26,
        31,
    )
    cross_messages = " ".join(item["message"] for item in cross_report.items)
    if cross_report.failed:
        print("self-test FAIL: 跨 edition 的确认轮被误判（计数应按 CANDIDATE 重置）")
        print_report(cross_report, False)
        return 1
    if "lifecycle-v33" in cross_messages:
        print("self-test FAIL: 跨 edition 确认轮误报 lifecycle")
        return 1

    # ===== V3.4 最小内核夹具（r6 重实施） =====
    # R6-a：确认轮前置修改轮。
    _r6a_bad = (
        "# 指标\n\n## R1 确认轮\n\n- 轮型: confirmation\n- 轮结果: CONVERGED\n"
        "- certify 新准入: 0\n"
    )
    rep_r6a = Report()
    check_v33_lifecycle(_r6a_bad, {}, {}, rep_r6a, 1, 1)
    if not any("R6-a" in i["message"] for i in rep_r6a.items if i["level"] == "FAIL"):
        print("self-test FAIL: 无前置修改轮的确认轮 CONVERGED 未被拦截")
        return 1
    _r6a_good = (
        "# 指标\n\n## R1 修改轮\n\n- 发现总数: 1\n- 轮型: full\n- 轮结果: ROUND-CLOSED\n- 未闭环: 0\n\n"
        "## R2 确认轮\n\n- 轮型: confirmation\n- 轮结果: CONVERGED\n- certify 新准入: 0\n"
    )
    rep_r6a2 = Report()
    _r6a_rows = {
        "RD-1": {"class": "①", "action": "修", "verify": "grep x", "root": "RD-1",
                 "origin": "R1", "severity": "P2", "role": "fact",
                 "section": "R1 修改轮"},
    }
    _r6a_sections = {"R1 修改轮": {"RD-1": "①"}}
    check_v33_lifecycle(_r6a_good, _r6a_rows, _r6a_sections, rep_r6a2, 1, 1)
    if any("R6-a" in i["message"] for i in rep_r6a2.items if i["level"] == "FAIL"):
        print("self-test FAIL: 有前置修改轮的确认轮被误拦")
        return 1

    # P1-7：certify 待跑 × 闭合态（限 schema V3.4）。
    _p17_metrics = (
        "## R1 确认轮\n"
        "- 发现总数: 1\n- 唯一根因数: 1\n- 本轮新准入: 1\n- certify 新准入: 0\n"
        "- 重复合并: 0\n- 自引入: 0\n"
        "- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 0\n- 未闭环: 0\n"
        "- 轮型: confirmation\n- 轮结果: CONVERGED\n- 代裁决: 0\n- ARCH 提案: 0\n"
        "- 用户否决: 0\n- edition: V3.4\n- re-litigation: 0\n- adjacent 阻塞: 0\n"
        "- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0\n- 成本: agent 1 / token 1 / 耗时 1min\n"
        "- 回读与回归: certify 待跑\n"
    )
    rep_p17 = Report()
    check_metrics(_p17_metrics, rep_p17, "R1", None, 0, 1, v3=True,
                  root_count=None, new_roots=None, v33_start=1, v34=True)
    if not any("P1-7" in i["message"] for i in rep_p17.items if i["level"] == "FAIL"):
        print("self-test FAIL: V3.4 下「certify 待跑」并存未被拦截")
        return 1
    rep_p17b = Report()
    check_metrics(_p17_metrics, rep_p17b, "R1", None, 0, 1, v3=True,
                  root_count=None, new_roots=None, v33_start=1, v34=False)
    if any("P1-7" in i["message"] for i in rep_p17b.items if i["level"] == "FAIL"):
        print("self-test FAIL: V3.3 schema 被 P1-7 追溯判红")
        return 1

    # N0：双源断链 + --round 作用域。
    _loop_n0 = (
        "schema: V3.4 from R1\n\nedition: V3.4\n\n"
        "## R1 面板轮\n\n### §7 状态\nCONVERGED\n\n"
        "## R2 面板轮\n\n### §7 状态\nCONVERGED\n\n"
        "## R3 面板轮\n\n### §7 状态\nCONVERGED\n"
    )
    _metrics_n0 = (
        "# 指标\n\n## R1 面板轮\n\n- 发现总数: 0\n- 未闭环: 0\n\n"
        "## R3 面板轮\n\n- 发现总数: 0\n- 未闭环: 0\n"
    )
    rep_n0 = Report()
    check_round_monotonicity(_loop_n0, {}, {}, rep_n0, schema_start=1, v3=True,
                             metrics_text=_metrics_n0)
    if not any("N0" in i["message"] for i in rep_n0.items if i["level"] == "FAIL"):
        print("self-test FAIL: 无台账无 metrics 的未闭合轮越序未被拦截")
        return 1
    rep_scope = Report()
    check_round_monotonicity(_loop_n0, {}, {}, rep_scope, schema_start=1, v3=True,
                             metrics_text=_metrics_n0, round_tag="R1")
    if any("N0" in i["message"] for i in rep_scope.items if i["level"] == "FAIL"):
        print("self-test FAIL: --round R1 追溯判了 R2 断链（作用域过宽）")
        return 1
    rep_scope2 = Report()
    check_round_monotonicity(_loop_n0, {}, {}, rep_scope2, schema_start=1, v3=True,
                             metrics_text=_metrics_n0, round_tag="R3")
    if not any("N0" in i["message"] for i in rep_scope2.items if i["level"] == "FAIL"):
        print("self-test FAIL: --round R3 未抓到 R2 断链")
        return 1

    # r7：R6-a 实质要件——无发现总数键的两行伪造不得计入 mod_closed。
    _forge = (
        "# 指标\n\n## R1 修改轮\n\n- 轮型: full\n- 轮结果: ROUND-CLOSED\n\n"
        "## R2 确认轮\n\n- 轮型: confirmation\n- 轮结果: CONVERGED\n"
        "- certify 新准入: 0\n"
    )
    _fr = Report()
    check_v33_lifecycle(_forge, {}, {}, _fr, 1, 1)
    if not any("R6-a" in i["message"] for i in _fr.items if i["level"] == "FAIL"):
        print("self-test FAIL: 无发现总数键的伪造修改轮仍计入 mod_closed")
        return 1

    # r7：N0 读 metrics 未闭环值——自报 open 的轮不得静默越序。
    _mo_loop = (
        "schema: V3.4 from R1\n\nedition: V3.4\n\n## R1 面板轮\n\n### §7 状态\nNOT CONVERGED\n\n"
        "## R2 面板轮\n\n### §7 状态\nCONVERGED\n"
    )
    _mo_metrics = (
        "# 指标\n\n## R1 面板轮\n\n- 发现总数: 1\n- 未闭环: 1\n\n"
        "## R2 面板轮\n\n- 发现总数: 0\n- 未闭环: 0\n"
    )
    _mr = Report()
    check_round_monotonicity(_mo_loop, {}, {}, _mr, schema_start=1, v3=True,
                             metrics_text=_mo_metrics)
    if not any("N0" in i["message"] for i in _mr.items if i["level"] == "FAIL"):
        print("self-test FAIL: metrics 自报未闭环>0 的轮静默越序")
        return 1

    # r7：前缀串扰——词边界助手双向判定。
    if _round_key_matches("R10 面板轮", "R1"):
        print("self-test FAIL: R1 仍匹配 R10（词边界失效）")
        return 1
    if not _round_key_matches("R1 面板轮", "R1"):
        print("self-test FAIL: R1 不再匹配自身")
        return 1
    if _round_key_matches("R12 面板轮", "R1"):
        print("self-test FAIL: R1 仍匹配 R12")
        return 1

    # ===== V3.4-r8 判别夹具（删除对应新逻辑必须变红）=====
    # (1) mod_closed 台账要件：有发现总数但无台账节的伪造轮不得计入。
    _forge3 = (
        "# 指标\n\n## R1 修改轮\n\n- 发现总数: 0\n- 轮型: full\n"
        "- 轮结果: ROUND-CLOSED\n- 未闭环: 0\n\n"
        "## R2 确认轮\n\n- 轮型: confirmation\n- 轮结果: CONVERGED\n"
        "- certify 新准入: 0\n"
    )
    _f3 = Report()
    check_v33_lifecycle(_forge3, {}, {}, _f3, 1, 1)
    if not any("R6-a" in i["message"] for i in _f3.items if i["level"] == "FAIL"):
        print("self-test FAIL: 有发现总数但无台账节的伪造轮仍计入 mod_closed")
        return 1

    # (2) N0 缺 `未闭环` 键按开放计。
    _nk_loop = (
        "schema: V3.4 from R1\n\nedition: V3.4\n\n## R1 面板轮\n\n### §7 状态\nNOT CONVERGED\n\n"
        "## R2 面板轮\n\n### §7 状态\nCONVERGED\n"
    )
    _nk_metrics = (
        "# 指标\n\n## R1 面板轮\n\n- 发现总数: 1\n\n"
        "## R2 面板轮\n\n- 发现总数: 0\n- 未闭环: 0\n"
    )
    _nkr = Report()
    check_round_monotonicity(_nk_loop, {}, {}, _nkr, schema_start=1, v3=True,
                             metrics_text=_nk_metrics)
    if not any("N0" in i["message"] for i in _nkr.items if i["level"] == "FAIL"):
        print("self-test FAIL: metrics 缺未闭环键的轮未按开放计")
        return 1

    # (3) P1-7 词表扩充生效。
    _p17b = (
        "## R1 确认轮\n"
        "- 发现总数: 1\n- 唯一根因数: 1\n- 本轮新准入: 1\n- certify 新准入: 0\n"
        "- 重复合并: 0\n- 自引入: 0\n"
        "- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 0\n- 未闭环: 0\n"
        "- 轮型: confirmation\n- 轮结果: CONVERGED\n- 代裁决: 0\n- ARCH 提案: 0\n"
        "- 用户否决: 0\n- edition: V3.4\n- re-litigation: 0\n- adjacent 阻塞: 0\n"
        "- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0\n- 成本: agent 1\n"
        "- 回读与回归: certify 待复跑\n"
    )
    _p17r = Report()
    check_metrics(_p17b, _p17r, "R1", None, 0, 1, v3=True,
                  root_count=None, new_roots=None, v33_start=1, v34=True)
    if not any("P1-7" in i["message"] for i in _p17r.items if i["level"] == "FAIL"):
        print("self-test FAIL: P1-7 词表扩充未生效（certify 待复跑）")
        return 1

    # (4) strict 不回退：块标题写成一/三级 `# R2` 时 --round 必须 FAIL。
    _strictr = Report()
    check_metrics("# 指标\n\n# R2 面板轮\n\n- 发现总数: 0\n", _strictr, "R2",
                  None, 0, 1, v3=True, root_count=None, new_roots=None,
                  v33_start=1, v34=True)
    if not _strictr.failed:
        print("self-test FAIL: 轮块标题旁路（# R2）未被 strict 模式拦住")
        return 1

    # (5) 零/空面板关轮必须 FAIL。
    with tempfile.TemporaryDirectory() as t8:
        r8d = Path(t8) / "run"
        r8d.mkdir()
        (r8d / "loop.md").write_text(
            "schema: V3.4 from R1\n\nedition: V3.4\n\n## R1 面板轮\n\n"
            "### §7\nx\n\n### L6\na\n\n### L6.5\nb\n", encoding="utf-8")
        (r8d / "defect-ledger.md").write_text(
            "## R1 面板轮\n\n| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |\n"
            "|---|---|---|---|---|---|---|---|\n| F-1 | fact | P2 | 事实 | ① | 修 | e | g |\n",
            encoding="utf-8")
        (r8d / "metrics.md").write_text(
            "# 指标\n\n## R1 面板轮\n\n- 发现总数: 1\n"
            "- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 0\n- 未闭环: 0\n"
            "- 轮型: full\n- 轮结果: ROUND-CLOSED\n- 代裁决: 0\n- ARCH 提案: 0\n"
            "- 用户否决: 0\n- edition: V3.4\n", encoding="utf-8")
        _pr = check_run_dir(r8d, None, "R1")
        if not any("面板证据" in i["message"] for i in _pr.items if i["level"] == "FAIL"):
            print("self-test FAIL: 零面板关轮未被拦截")
            return 1

    # (6) ARCH ⑤ 行：批准延期（用户批准+日期+D 行）不触发 BLOCKED；未批准触发。
    _arch_metrics = (
        "# 指标\n\n## R1 面板轮\n\n- 发现总数: 1\n"
        "- 处置: ① 0 / ② 0 / ④ 0 / ⑤ 1 / ⑥ 0 / FP 0 / SUGG 0\n- 未闭环: 1\n"
        "- 轮型: full\n- 轮结果: ROUND-CLOSED\n- 代裁决: 0\n- ARCH 提案: 0\n"
        "- 用户否决: 0\n- edition: V3.4\n"
    )
    _arch_secs = {"R1 面板轮": {"F-9": "⑤"}}
    _arch_pending = {"F-9": {"class": "⑤", "action": "[ARCH] 待用户裁决",
                             "verify": "-", "root": "F-9", "origin": "R1",
                             "severity": "P1", "role": "design-d2",
                             "section": "R1 面板轮"}}
    _ar = Report()
    check_v33_lifecycle(_arch_metrics, _arch_pending, _arch_secs, _ar, 1, 1)
    if not any("ARCH" in i["message"] for i in _ar.items if i["level"] == "FAIL"):
        print("self-test FAIL: 未批准的 ARCH ⑤ 行未触发 BLOCKED")
        return 1
    _arch_ok = {"F-9": {"class": "⑤", "action": "[ARCH] 待用户裁决；用户批准 2026-11-18；D-056",
                        "verify": "-", "root": "F-9", "origin": "R1",
                        "severity": "P1", "role": "design-d2",
                        "section": "R1 面板轮"}}
    _ar2 = Report()
    check_v33_lifecycle(_arch_metrics, _arch_ok, _arch_secs, _ar2, 1, 1)
    if any("ARCH" in i["message"] for i in _ar2.items if i["level"] == "FAIL"):
        print("self-test FAIL: 已批准延期的 ARCH 行仍被硬阻断")
        return 1

    # ===== V3.5 制品链夹具 =====
    import hashlib as _hl

    def _write_artifact_run(root: Path) -> Path:
        d = root / "run"
        d.mkdir()
        (d / "loop.md").write_text(
            "schema: V3.4 from R1\n\nedition: V3.4\n\n## R1 面板轮\n\n"
            "### §7\nx\n\n### L6\na\n\n### L6.5\nb\n", encoding="utf-8")
        (d / "defect-ledger.md").write_text(
            "## R1 面板轮\n\n| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |\n"
            "|---|---|---|---|---|---|---|---|\n| F-1 | fact | P2 | 事实 | ① | 修 | e | g |\n",
            encoding="utf-8")
        (d / "metrics.md").write_text(
            "# 指标\n\n## R1 面板轮\n\n- 发现总数: 1\n"
            "- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 0\n- 未闭环: 0\n"
            "- 轮型: full\n- 轮结果: ROUND-CLOSED\n- 代裁决: 0\n- ARCH 提案: 0\n"
            "- 用户否决: 0\n- edition: V3.4\n", encoding="utf-8")
        return d

    with tempfile.TemporaryDirectory() as tv:
        rv = _write_artifact_run(Path(tv))
        rp = check_run_dir(rv, None, "R1")
        if not any("面板制品未入库" in i["message"] for i in rp.items if i["level"] == "FAIL"):
            print("self-test FAIL: V3.5 面板制品缺失未拦截")
            return 1
    with tempfile.TemporaryDirectory() as tv:
        rv = _write_artifact_run(Path(tv))
        pd = rv / "panel" / "round1"
        pd.mkdir(parents=True)
        body = "# fact\n\n## FINDING F-1\n- 位置：§1\n"
        (pd / "fact.md").write_text(body, encoding="utf-8")
        h = _hl.sha256(body.encode()).hexdigest()
        (pd / "INDEX.md").write_text(
            "# 面板制品索引（panel-index，生成物——勿手改）\n\n- 文件数: 1\n\n"
            "| 文件 | sha256 | FINDING | SUGGESTION | CLAIM | 形态 |\n"
            "|---|---|---|---|---|---|\n"
            f"| fact.md | {h} | 1 | 0 | 0 | finding |\n", encoding="utf-8")
        rp = check_run_dir(rv, None, "R1")
        msgs = [i["message"] for i in rp.items if i["level"] == "FAIL"]
        if any("面板制品未入库" in x or "INDEX.md" in x for x in msgs):
            print("self-test FAIL: 合法制品链被误拦")
            return 1
        if not any("certify 独立回执未入库" in x for x in msgs):
            print("self-test FAIL: 闭合轮 certify 回执缺失未拦截")
            return 1
        cd = rv / "certify"
        cd.mkdir()
        (cd / "round1-1.md").write_text(
            "# 独立冷读回执\n\n逐条回读 R1 全部修复 hunk；结论：CLOSED。\n" * 2,
            encoding="utf-8")
        rp2 = check_run_dir(rv, None, "R1")
        if any("certify 独立回执未入库" in i["message"] for i in rp2.items if i["level"] == "FAIL"):
            print("self-test FAIL: 已入库的 certify 回执仍被拦")
            return 1

    # ===== r9 考场判别夹具 =====
    # (a) strict：完整合法 metrics 写成单级 `# R2` 标题 → 必须 FAIL。
    _strict_complete = (
        "# 指标\n\n# R2 确认轮\n\n- 发现总数: 0\n- 唯一根因数: 0\n"
        "- 本轮新准入: 0\n- certify 新准入: 0\n- 重复合并: 0\n- 自引入: 0\n"
        "- 处置: ① 0 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 0\n- 未闭环: 0\n"
        "- 轮型: confirmation\n- 轮结果: CONVERGED\n"
        "- 代裁决: 0\n- ARCH 提案: 0\n- 用户否决: 0\n- edition: V3.4\n"
        "- re-litigation: 0\n- adjacent 阻塞: 0\n- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0\n"
        "- 成本: agent 1\n"
    )
    _scr = Report()
    check_metrics(_strict_complete, _scr, "R2", None, 0, 1, v3=True,
                  root_count=None, new_roots=None, v33_start=1, v34=True)
    if not _scr.failed:
        print("self-test FAIL: strict 未拦住单级标题的完整块")
        return 1

    # (b) 空面板目录（存在但为空）→ 必须 FAIL。
    with tempfile.TemporaryDirectory() as te:
        re_ = _write_artifact_run(Path(te))
        (re_ / "panel" / "round1").mkdir(parents=True)
        rpe = check_run_dir(re_, None, "R1")
        if not any("目录为空" in i["message"] for i in rpe.items if i["level"] == "FAIL"):
            print("self-test FAIL: 空面板目录未被拦截")
            return 1

    # (c) INDEX 计数漂移 → 必须 FAIL。
    with tempfile.TemporaryDirectory() as tc:
        rc = _write_artifact_run(Path(tc))
        pd = rc / "panel" / "round1"
        pd.mkdir(parents=True)
        body = "# fact\n\n## FINDING F-1\n- 位置：§1\n"
        (pd / "fact.md").write_text(body, encoding="utf-8")
        h = _hl.sha256(body.encode()).hexdigest()
        (pd / "INDEX.md").write_text(
            "# 面板制品索引\n\n| 文件 | sha256 | FINDING | SUGGESTION | CLAIM | 形态 |\n"
            "|---|---|---|---|---|---|\n"
            f"| fact.md | {h} | 2 | 0 | 0 | finding |\n", encoding="utf-8")
        rpc = check_run_dir(rc, None, "R1")
        if not any("计数与 INDEX 不一致" in i["message"] for i in rpc.items if i["level"] == "FAIL"):
            print("self-test FAIL: INDEX 计数漂移未被拦截")
            return 1

    # (d) 缺 INDEX → 必须 FAIL。
    with tempfile.TemporaryDirectory() as td:
        rd = _write_artifact_run(Path(td))
        pd = rd / "panel" / "round1"
        pd.mkdir(parents=True)
        (pd / "fact.md").write_text("## FINDING F-1\n", encoding="utf-8")
        rpd = check_run_dir(rd, None, "R1")
        if not any("INDEX.md 缺失" in i["message"] for i in rpd.items if i["level"] == "FAIL"):
            print("self-test FAIL: 缺 INDEX.md 未被拦截")
            return 1

    # (e) certify 回执含否定结论 → 必须 FAIL。
    with tempfile.TemporaryDirectory() as tf:
        rf = _write_artifact_run(Path(tf))
        pd = rf / "panel" / "round1"
        pd.mkdir(parents=True)
        body = "# fact\n\n## FINDING F-1\n- 位置：§1\n"
        (pd / "fact.md").write_text(body, encoding="utf-8")
        h = _hl.sha256(body.encode()).hexdigest()
        (pd / "INDEX.md").write_text(
            "# 面板制品索引\n\n| 文件 | sha256 | FINDING | SUGGESTION | CLAIM | 形态 |\n"
            "|---|---|---|---|---|---|\n"
            f"| fact.md | {h} | 1 | 0 | 0 | finding |\n", encoding="utf-8")
        cd = rf / "certify"
        cd.mkdir()
        (cd / "round1-1.md").write_text(
            "# 独立冷读回执\n\n逐条回读完成。\n\n结论：NOT PASS——REOPEN 两处未落地。\n" * 2,
            encoding="utf-8")
        rpf = check_run_dir(rf, None, "R1")
        if not any("否定性结论" in i["message"] for i in rpf.items if i["level"] == "FAIL"):
            print("self-test FAIL: 含否定结论的回执未被拦截")
            return 1

    # (f) 空 FREEZE 不解锁制品豁免（V3.3 未冻结文档）。
    with tempfile.TemporaryDirectory() as tg:
        rg = Path(tg) / "run"
        rg.mkdir()
        (rg / "loop.md").write_text(
            "schema: V3.3 from R1\n\nedition: V3.3\n\n## R1 面板轮\n\n"
            "### §7\nx\n\n### L6\na\n\n### L6.5\nb\n", encoding="utf-8")
        (rg / "defect-ledger.md").write_text(
            "## R1 面板轮\n\n| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |\n"
            "|---|---|---|---|---|---|---|---|\n| F-1 | fact | P2 | 事实 | ① | 修 | e | g |\n",
            encoding="utf-8")
        (rg / "metrics.md").write_text(
            "# 指标\n\n## R1 面板轮\n\n- 发现总数: 1\n"
            "- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 0\n- 未闭环: 0\n"
            "- 轮型: full\n- 轮结果: ROUND-CLOSED\n", encoding="utf-8")
        (rg / "FREEZE.md").write_text("", encoding="utf-8")
        rpg = check_run_dir(rg, None, "R1")
        if not any("面板制品未入库" in i["message"] for i in rpg.items if i["level"] == "FAIL"):
            print("self-test FAIL: 空 FREEZE 解锁了制品豁免")
            return 1
        (rg / "FREEZE.md").write_text("CONVERGED（测试夹具）\n", encoding="utf-8")
        rpg2 = check_run_dir(rg, None, "R1")
        if any("面板制品未入库" in i["message"] for i in rpg2.items if i["level"] == "FAIL"):
            print("self-test FAIL: 有效 FREEZE 未豁免制品链")
            return 1

    # ===== r10 判别夹具（四个 P0）=====
    # (1) SUGG 账目：合规记账（面板 FINDING+SUGG、台账两行、metrics 总数 2）不得双 FAIL。
    with tempfile.TemporaryDirectory() as ts:
        rs = _write_artifact_run(Path(ts))
        pd = rs / "panel" / "round1"
        pd.mkdir(parents=True)
        body = (
            "# fact\n\n## FINDING F-1\n- 位置：§1\n\n"
            "## SUGGESTION RD-S-01\n- 建议：措辞\n"
        )
        (pd / "fact.md").write_text(body, encoding="utf-8")
        h = _hl.sha256(body.encode()).hexdigest()
        (pd / "INDEX.md").write_text(
            "# 面板制品索引\n\n| 文件 | sha256 | FINDING | SUGGESTION | CLAIM | 形态 |\n"
            "|---|---|---|---|---|---|\n"
            f"| fact.md | {h} | 1 | 1 | 0 | finding |\n", encoding="utf-8")
        lt = rs / "defect-ledger.md"
        lt.write_text(
            "## R1 面板轮\n\n| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |\n"
            "|---|---|---|---|---|---|---|---|---|---|\n"
            "| RD-1 | F-1 | R1 | fact | P2 | 事实 | ① | 修 | e | grep x |\n"
            "| RD-2 | RD-S-01 | R1 | fact | P3 | 建议 | SUGG | 登记 | e | 无 |\n",
            encoding="utf-8")
        (rs / "metrics.md").write_text(
            "# 指标\n\n## R1 面板轮\n\n- 发现总数: 2\n- 唯一根因数: 2\n"
            "- 本轮新准入: 2\n- certify 新准入: 0\n- 重复合并: 0\n- 自引入: 0\n"
            "- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 1\n- 未闭环: 0\n"
            "- 轮型: full\n- 轮结果: ROUND-CLOSED\n- 代裁决: 0\n- ARCH 提案: 0\n"
            "- 用户否决: 0\n- edition: V3.4\n", encoding="utf-8")
        rps = check_run_dir(rs, None, "R1")
        _msgs = [i["message"] for i in rps.items if i["level"] == "FAIL"]
        if any("面板原始发现数" in x for x in _msgs):
            print("self-test FAIL: 合规 SUGG 记账仍触发三账对账 FAIL（r10 P0-①）")
            return 1
        if any("分档 SUGG" in x or "SUGG=0" in x for x in _msgs):
            print("self-test FAIL: 分档聚合仍看不见 SUGG 行（r10 P0-①）")
            return 1

    # (2) 缺 `发现总数` 键必须 FAIL。
    _mt = Report()
    check_metrics(
        "## R1 面板轮\n- 处置: ① 1 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 0\n"
        "- 未闭环: 0\n- 轮型: full\n- 轮结果: ROUND-CLOSED\n- 代裁决: 0\n"
        "- ARCH 提案: 0\n- 用户否决: 0\n- edition: V3.4\n",
        _mt, "R1", None, 0, 1, v3=True, root_count=None, new_roots=None,
        v33_start=1, v34=True,
    )
    if not any("发现总数" in i["message"] for i in _mt.items if i["level"] == "FAIL"):
        print("self-test FAIL: 缺发现总数键未被判 FAIL（r10 P0-②）")
        return 1

    # (3) 否定措辞的 FREEZE 不解锁豁免。
    with tempfile.TemporaryDirectory() as tn:
        rn = _write_artifact_run(Path(tn))
        # 改成 V3.3 未冻结文档（制品要求来自 V3.3+未FREEZE 分支）
        (rn / "loop.md").write_text(
            "schema: V3.3 from R1\n\nedition: V3.3\n\n## R1 面板轮\n\n"
            "### §7\nx\n\n### L6\na\n\n### L6.5\nb\n", encoding="utf-8")
        (rn / "FREEZE.md").write_text("状态：NOT CONVERGED——尚有 53 项未清点。\n", encoding="utf-8")
        rpn = check_run_dir(rn, None, "R1")
        if not any("面板制品未入库" in i["message"] for i in rpn.items if i["level"] == "FAIL"):
            print("self-test FAIL: 否定措辞 FREEZE 仍解锁豁免（r10 P0-③）")
            return 1

    # (4) 纯标点回执与 结论：REOPEN 型回执必须 FAIL。
    with tempfile.TemporaryDirectory() as tp:
        rp4 = _write_artifact_run(Path(tp))
        pd4 = rp4 / "panel" / "round1"
        pd4.mkdir(parents=True)
        body = "# fact\n\n## FINDING F-1\n- 位置：§1\n"
        (pd4 / "fact.md").write_text(body, encoding="utf-8")
        h = _hl.sha256(body.encode()).hexdigest()
        (pd4 / "INDEX.md").write_text(
            "# 面板制品索引\n\n| 文件 | sha256 | FINDING | SUGGESTION | CLAIM | 形态 |\n"
            "|---|---|---|---|---|---|\n"
            f"| fact.md | {h} | 1 | 0 | 0 | finding |\n", encoding="utf-8")
        cd4 = rp4 / "certify"
        cd4.mkdir()
        (cd4 / "round1-1.md").write_text("。" * 50, encoding="utf-8")
        rp4a = check_run_dir(rp4, None, "R1")
        if not any("有效字符不足" in i["message"] for i in rp4a.items if i["level"] == "FAIL"):
            print("self-test FAIL: 纯标点回执未被拦（r10 P0-④）")
            return 1
        (cd4 / "round1-2.md").write_text(
            "# 独立冷读回执\n\n" + "逐条回读完成。" * 5 + "\n\n结论：REOPEN——两处未落地。\n",
            encoding="utf-8")
        rp4b = check_run_dir(rp4, None, "R1")
        if not any("否定性结论" in i["message"] for i in rp4b.items if i["level"] == "FAIL"):
            print("self-test FAIL: 结论 REOPEN 型回执未被拦（r10 P0-④/F-r10-1）")
            return 1

    # (5) 空文件面板 + 外部 --panel-dir（r10 补充判别）。
    with tempfile.TemporaryDirectory() as tq:
        rq = _write_artifact_run(Path(tq))
        pdq = rq / "panel" / "round1"
        pdq.mkdir(parents=True)
        (pdq / "empty.md").write_text("", encoding="utf-8")
        hq = _hl.sha256(b"").hexdigest()
        (pdq / "INDEX.md").write_text(
            "# 面板制品索引\n\n| 文件 | sha256 | FINDING | SUGGESTION | CLAIM | 形态 |\n"
            "|---|---|---|---|---|---|\n"
            f"| empty.md | {hq} | 0 | 0 | 0 | no-finding |\n", encoding="utf-8")
        rpq = check_run_dir(rq, None, "R1")
        if not any("是空文件" in i["message"] for i in rpq.items if i["level"] == "FAIL"):
            print("self-test FAIL: 空文件面板未被拦（r9-③/r10 复核）")
            return 1
    with tempfile.TemporaryDirectory() as tr:
        rr = _write_artifact_run(Path(tr))
        external = Path(tr) / "external-panel"
        external.mkdir()
        (external / "fact.md").write_text("## FINDING F-1\n", encoding="utf-8")
        rpr = check_run_dir(rr, external, "R1")
        if not any("run 目录之外" in i["message"] for i in rpr.items if i["level"] == "FAIL"):
            print("self-test FAIL: 外部 --panel-dir 未被拦（r10 复核）")
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
