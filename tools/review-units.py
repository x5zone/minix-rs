#!/usr/bin/env python3
"""review-units.py — 单元级 review 状态机（V3.9）

规范源：prompt/review-rules/review-units.md。
单元：`##` = T1 章单元；`###` = T2 节单元；无 `###` 的 `##` 两者兼任。
状态文件：rewrite-notes/meta/review/state/<stage>/<doc-stem>.yaml。

用法：
  review-units.py init <doc> [--stage S] [--edition V3.9] [--force]
  review-units.py check <doc>
  review-units.py sync <doc> [--unfreeze] [--unit ID]
  review-units.py next <doc> [--track t1|t2|t3]
  review-units.py mark <doc> --unit ID --track t1|t2 (--fix|--pass) [--freeze] [--note TEXT]
  review-units.py sample <doc> [--ratio 0.2] [--seed STR]
  review-units.py gate <doc> (--pass|--fail) --evidence PATH [--ledger PATH]
  review-units.py audit <doc> [--ledger PATH]
  review-units.py fingerprint <doc> [--code-list PATH] [--update]
  review-units.py baseline <doc> --evidence PATH
  review-units.py new-edition <doc> --edition V3.x [--unfreeze-all]
  review-units.py begin-confirmation <doc>
  review-units.py end-confirmation <doc>
  review-units.py stats <doc>
  review-units.py --self-test

退出码：0 通过；1 校验失败；2 用法错误。
"""
from __future__ import annotations

import argparse
import hashlib
import math
import re
import subprocess
import sys
import tempfile
from pathlib import Path

try:
    import yaml
except Exception:  # pragma: no cover
    print("需要 PyYAML（python3 -m pip install pyyaml）", file=sys.stderr)
    raise SystemExit(2)

sys.path.insert(0, str(Path(__file__).resolve().parent))
from notes_layout import REVIEW_EVAL_DIR, REVIEW_STATE_DIR, review_state_path  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]

SCHEMA = "review-units/v1"
EXEMPT = re.compile(r"参见|附录|索引|参考文献")
HEAD_RE = re.compile(r"^(#{2,3})\s+(.+?)\s*$")
NUM_RE = re.compile(r"^(\d+(?:\.\d+)*)[.、]?\s")
MAX_ROUNDS = 2


def head_commit() -> str:
    try:
        return subprocess.run(
            ["git", "rev-parse", "HEAD"], capture_output=True, text=True, check=True
        ).stdout.strip()
    except Exception:
        return ""


def sha(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def stage_of(doc: Path) -> str:
    for part in doc.parts:
        if re.fullmatch(r"\d{2}-stage-[^/]+", part):
            return part
    raise SystemExit(f"无法从路径识别 stage：{doc}")


def read_doc(doc: Path) -> list[str]:
    if not doc.is_file():
        raise SystemExit(f"文档不存在：{doc}")
    return doc.read_text(encoding="utf-8").splitlines(keepends=True)


def parse_units(doc: Path) -> list[dict]:
    lines = read_doc(doc)
    heads: list[tuple[int, int, str]] = []
    fence: str | None = None
    for i, line in enumerate(lines):
        s = line.lstrip()
        if s.startswith("```") or s.startswith("~~~"):
            mark = s[:3]
            fence = None if fence == mark else (fence or mark)
            continue
        if fence:
            continue
        m = HEAD_RE.match(line)
        if m:
            heads.append((i, len(m.group(1)), m.group(2).strip()))
    units: list[dict] = []
    chapter_ids: list[str] = []
    used: set[str] = set()
    order = 0

    def unique(base: str) -> str:
        uid, n = base, 2
        while uid in used:
            uid = f"{base}-{n}"
            n += 1
        used.add(uid)
        return uid
    for idx, (start, level, title) in enumerate(heads):
        if level == 2 and EXEMPT.search(title):
            chapter_ids.append("__exempt__")
            continue
        if level == 3 and chapter_ids and chapter_ids[-1] == "__exempt__":
            continue
        # 计算区间
        end = len(lines)
        for j in range(idx + 1, len(heads)):
            if heads[j][1] <= level:
                end = heads[j][0]
                break
        body_start = start + 1
        if level == 2:
            own_end = end
            for j in range(idx + 1, len(heads)):
                if heads[j][1] <= 3:
                    own_end = heads[j][0]
                    break
            own_text = "".join(lines[body_start:own_end])
            children = [
                h[2] for h in heads[idx + 1:]
                if h[1] == 3 and h[0] < end
            ]
            body = own_text + "\n" + "\n".join(children)
            if not own_text.strip(" \t\n-—*") and not children:
                continue
        else:
            body = "".join(lines[start:end])
            if not body[body.find("\n") + 1:].strip(" \t\n-—*"):
                continue
        order += 1
        m = NUM_RE.match(title)
        uid = unique(m.group(1) if m else f"u-{order}")
        if level == 2:
            chapter_ids.append(uid)
            parent = None
        else:
            parent = chapter_ids[-1] if chapter_ids else None
        units.append({
            "id": uid,
            "heading": title,
            "level": level,
            "parent": parent,
            "order": order,
            "text_sha": sha(body),
            "_text": body,
        })
    return units


ANCHOR_RE = re.compile(
    r"`?((?:os|minix3)/[\w./-]+\.(?:rs|c|h)):"
    r"(?:(fn|struct|enum|trait|const|static|type|impl)\s+)?"
    r"([A-Za-z_]\w*(?:::[A-Za-z_]\w*)?)`?"
)


def _brace_hash(text: str, start: int) -> str:
    i = text.find("{", start)
    if i < 0:
        return sha(text[start:].splitlines()[0] if text[start:] else "")
    depth = 0
    for j in range(i, len(text)):
        if text[j] == "{":
            depth += 1
        elif text[j] == "}":
            depth -= 1
            if depth == 0:
                return sha(text[start:j + 1])
    return sha(text[start:])


def _symbol_body(text: str, kind: str, name: str) -> tuple[str | None, bool]:
    """Return (body_sha, fallback). fallback=True for C/heuristic anchors."""
    if kind == "impl" and "::" in name:
        typ, method = name.split("::", 1)
        imp = re.search(r"(?m)^\s*impl\b[^\n{]*\b" + re.escape(typ) + r"\b", text)
        if not imp:
            return None, False
        seg = text[imp.start():]
        m = re.search(r"\bfn\s+" + re.escape(method) + r"\b", seg)
        return (_brace_hash(seg, m.start()), False) if m else (None, False)
    pats = {
        "fn": r"\bfn\s+" + re.escape(name) + r"\b",
        "struct": r"\bstruct\s+" + re.escape(name) + r"\b",
        "enum": r"\benum\s+" + re.escape(name) + r"\b",
        "trait": r"\btrait\s+" + re.escape(name) + r"\b",
        "type": r"\btype\s+" + re.escape(name) + r"\b",
        "const": r"\bconst\s+" + re.escape(name) + r"\b",
        "static": r"\bstatic\s+" + re.escape(name) + r"\b",
        "impl": r"\bimpl\b[^\n{]*\b" + re.escape(name) + r"\b",
    }
    pat = pats.get(kind)
    if pat is None:
        # C anchor: function name only
        m = re.search(r"\b" + re.escape(name) + r"\s*\(", text)
        return (_brace_hash(text, m.start()), True) if m else (None, True)
    m = re.search(pat, text)
    if not m:
        return None, kind in (None, "c")
    if kind in ("const", "static", "type"):
        return sha(text[m.start():].splitlines()[0]), False
    return _brace_hash(text, m.start()), False


def analyze_anchors(text: str) -> tuple[list[str], dict[str, str], str, list[str]]:
    """Extract direct code anchors, hash their bodies, classify anchor quality.

    Returns (refs, hashes, status, errors). status: direct / fallback / none /
    missing / impl. Freeze refuses missing and impl; sample uses 10% only when
    every unit is direct.
    """
    refs: list[str] = []
    hashes: dict[str, str] = {}
    errors: list[str] = []
    fallback = False
    impl = False
    for m in ANCHOR_RE.finditer(text):
        path, kind, name = m.group(1), m.group(2), m.group(3)
        ref = f"{path}:{kind + ' ' if kind else ''}{name}"
        if ref in refs:
            continue
        refs.append(ref)
        if kind == "impl":
            impl = True
        fp = ROOT / path
        if not fp.is_file():
            errors.append(f"锚点文件不存在：{path}")
            continue
        body, fb = _symbol_body(fp.read_text(encoding="utf-8", errors="replace"), kind, name)
        if body is None:
            errors.append(f"锚点符号不存在：{ref}")
            continue
        hashes[ref] = body
        fallback = fallback or fb
    if not refs:
        return refs, hashes, "none", errors
    if impl:
        return refs, hashes, "impl", errors
    if errors:
        return refs, hashes, "missing", errors
    return refs, hashes, "fallback" if fallback else "direct", errors


def new_unit(u: dict) -> dict:
    rec = dict(u)
    rec.pop("_text", None)
    if u["level"] == 2:
        rec.update(t1_state="pending", t1_rounds=0, t2_state="n/a", t2_rounds=0)
    else:
        rec.update(t1_state="n/a", t1_rounds=0, t2_state="pending", t2_rounds=0)
    rec.update(frozen=False, frozen_at=None, rewritten=False, user_approved=False,
               deps=[], reopened=False, stale=False, anchor_refs=[], anchor_hashes={},
               anchor_status="none", note="")
    return rec


def load_state(path: Path) -> dict:
    if not path.is_file():
        print(f"状态文件不存在：{path}（先跑 init）", file=sys.stderr)
        raise SystemExit(2)
    text = path.read_text(encoding="utf-8")
    body = "".join(l for l in text.splitlines(True) if not l.startswith("#"))
    data = yaml.safe_load(body)
    if not isinstance(data, dict) or data.get("schema") != SCHEMA:
        print(f"状态文件 schema 不是 {SCHEMA}：{path}", file=sys.stderr)
        raise SystemExit(2)
    return data


def save_state(path: Path, data: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    header = (
        "# 单元级 review 状态（V3.9）——规范源 prompt/review-rules/review-units.md\n"
        "# 由 tools/review-units.py 维护；冻结状态禁止手改。\n"
    )
    path.write_text(
        header + yaml.safe_dump(data, allow_unicode=True, sort_keys=False, width=120),
        encoding="utf-8",
    )


def cmd_init(doc: Path, state_path: Path, stage: str, edition: str, force: bool) -> int:
    if state_path.is_file() and not force:
        print(f"状态已存在：{state_path}（要重建加 --force）", file=sys.stderr)
        return 2
    if state_path.is_file() and force:
        old = load_state(state_path)
        if old.get("t3", {}).get("passed") or any(u.get("frozen") for u in old.get("units", [])):
            print("已冻结/已过 T3 的状态不得 init --force；请用 new-edition", file=sys.stderr)
            return 2
    units = parse_units(doc)
    data = {
        "schema": SCHEMA,
        "doc": str(doc.relative_to(Path.cwd())) if doc.is_relative_to(Path.cwd()) else str(doc),
        "stage": stage,
        "edition": edition,
        "updated_commit": head_commit(),
        "baseline_full_survey": False,
        "confirmation": {"active": False, "fixes": 0},
        "units": [new_unit(u) for u in units],
        "t3": {"passed": False, "evidence": "", "at_commit": ""},
    }
    save_state(state_path, data)
    print(f"init: {len(units)} 个单元 → {state_path}")
    return 0


def cmd_new_edition(doc: Path, state_path: Path, edition: str, unfreeze_all: bool) -> int:
    data = load_state(state_path)
    old = data.get("edition", "?")
    if edition == old:
        print(f"edition 已是 {edition}", file=sys.stderr)
        return 2
    archive = state_path.parent / "editions" / f"{old}.yaml"
    save_state(archive, data)
    data["edition"] = edition
    data["baseline_full_survey"] = False
    data["confirmation"] = {"active": False, "fixes": 0}
    for u in data["units"]:
        u["t1_rounds"] = 0
        u["t2_rounds"] = 0
        u["rewritten"] = False
        u["user_approved"] = False
        u["reopened"] = False
        if unfreeze_all:
            u["frozen"] = False
            u["frozen_at"] = None
            if u["level"] == 2:
                u["t1_state"] = "pending"
            else:
                u["t2_state"] = "pending"
    save_state(state_path, data)
    print(f"new-edition: {old} → {edition}（归档 {archive}；{'全部解冻' if unfreeze_all else '保留冻结'}）")
    return 0


def cmd_begin_confirmation(doc: Path, state_path: Path) -> int:
    data = load_state(state_path)
    if not all_chapters_frozen(data):
        print("[FAIL] 章尚未全部冻结，不能开始确认轮", file=sys.stderr)
        return 1
    data.setdefault("confirmation", {})
    data["confirmation"] = {"active": True, "fixes": 0}
    save_state(state_path, data)
    print("confirmation: begin")
    return 0


def cmd_end_confirmation(doc: Path, state_path: Path) -> int:
    data = load_state(state_path)
    errors, _ = validate(data, parse_units(doc))
    if errors:
        for e in errors:
            print(f"[FAIL] {e}")
        return 1
    if not all_chapters_frozen(data):
        print("[FAIL] 仍有未冻结单元，不能结束确认轮", file=sys.stderr)
        return 1
    data.setdefault("confirmation", {})
    data["confirmation"]["active"] = False
    save_state(state_path, data)
    print("confirmation: end")
    return 0


def default_ledger(doc: Path, stage: str) -> Path:
    return REVIEW_EVAL_DIR / "runs" / stage / doc.stem / "defect-ledger.md"


PATH_LINE_RE = re.compile(r"([\w./-]+\.(?:rs|c|h|md|sh|py|S|asm)):(\d+)")
TEST_RE = re.compile(r"\btest::([A-Za-z_]\w*)")
DECISION_RE = re.compile(r"\bD-\d{3}\b")
COMMIT_RE = re.compile(r"\b[0-9a-f]{7,40}\b")


DECISIONS_PATH = ROOT / "rewrite-notes/meta/coordination/review-eval/DECISIONS.md"


def _decision_ids() -> set[str] | None:
    if not DECISIONS_PATH.is_file():
        return None
    ids: set[str] = set()
    for line in DECISIONS_PATH.read_text(encoding="utf-8", errors="replace").splitlines():
        m = re.match(r"^##\s+(D-\d{3})\b", line) or re.match(r"^\|\s*(D-\d{3})\b", line)
        if m:
            ids.add(m.group(1))
    return ids


def evidence_nonempty(path: Path) -> bool:
    """evidence 至少要有一行内容；空文件或纯空白不算证据。"""
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return False
    return len("".join(text.split())) >= 5


def evidence_replay_ok(path: Path) -> bool:
    """evidence 至少要有一个可验证的可重放 token。

    接受：path:line（文件存在且行号在界内）、test::name（fn 存在）、真实 D-xxx、
    行首的 rg/grep/cargo test 命令、真实 bash/python3 tools/ 命令、真实 git commit。
    注释行与散文里的“没有运行…”不计。"""
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return False
    decisions = _decision_ids()
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith(("#", "//", "*")):
            continue
        for m in DECISION_RE.finditer(line):
            if decisions is None or m.group(0) in decisions:
                return True
        m = re.match(r"^\s*(?:[-*]\s*)?([\w./-]+\.(?:rs|c|h|md|sh|py|S|asm)):(\d+)", line)
        if m:
            fp = ROOT / m.group(1)
            if fp.is_file():
                try:
                    n = len(fp.read_text(encoding="utf-8", errors="replace").splitlines())
                except OSError:
                    n = 0
                if int(m.group(2)) <= n:
                    return True
            continue
        m = re.match(r"^\s*(?:[-*]\s*)?test::([A-Za-z_]\w*)", line)
        if m:
            name = m.group(1)
            try:
                check = subprocess.run(
                    ["rg", "-q", rf"\bfn\s+{re.escape(name)}\b", "os", "rewrite-notes"],
                    cwd=ROOT, capture_output=True,
                )
            except FileNotFoundError:
                check = subprocess.run(
                    ["grep", "-rq", rf"fn\s\+{re.escape(name)}\b", "os", "rewrite-notes"],
                    cwd=ROOT, capture_output=True,
                )
            if check.returncode == 0:
                return True
            continue
        m = re.match(r"^\s*(?:[-*]\s*)?(?:bash|python3)\s+(tools/[\w./-]+)", line)
        if m and (ROOT / m.group(1)).is_file():
            return True
        if re.match(r"^\s*(?:[-*]\s*)?cargo test\s", line):
            return True
        if re.match(r"^\s*(?:[-*]\s*)?(?:rg|grep)\s+\S", line):
            return True
        m = re.match(r"^\s*(?:[-*]\s*)?([0-9a-f]{7,40})\b", line)
        if m:
            check = subprocess.run(
                ["git", "-C", str(ROOT), "cat-file", "-e", m.group(1) + "^{commit}"],
                capture_output=True,
            )
            if check.returncode == 0:
                return True
    return False


def ledger_open_by_unit(path: Path) -> tuple[dict[str, int], list[str]]:
    """解析 defect-ledger 的 `单元` 列，返回 {unit_id: open_count} 与错误。

    V3.9+ 台账段（`##` 标题或 `- edition:` 行标 V3.9 及以上）必须有 `单元` 列；
    旧 edition 段不追溯。完全没有可解析的处置表（需 `处置类` 与 `复验` 列）时
    仍报错，防止空台账冒充 open=0。"""
    if not path.is_file():
        return {}, [f"ledger 不存在：{path}"]
    lines = path.read_text(encoding="utf-8").splitlines()
    counts: dict[str, int] = {}
    errors: list[str] = []
    header: list[str] | None = None
    idx: dict[str, int] = {}
    found_unit_table = False
    found_defect_table = False
    section_requires_unit = False
    for i, line in enumerate(lines):
        if line.startswith("## "):
            m = re.search(r"V(\d+)\.(\d+)", line)
            if m:
                section_requires_unit = (int(m.group(1)), int(m.group(2))) >= (3, 9)
            continue
        m_ed = re.match(r"\s*-\s*edition:\s*V(\d+)\.(\d+)", line)
        if m_ed:
            section_requires_unit = (int(m_ed.group(1)), int(m_ed.group(2))) >= (3, 9)
            continue
        s = line.strip()
        if not s.startswith("|"):
            continue
        cells = [c.strip() for c in s.strip("|").split("|")]
        nxt = lines[i + 1].strip() if i + 1 < len(lines) else ""
        is_sep = nxt.startswith("|") and all(re.fullmatch(r":?-{2,}:?", c.strip()) for c in nxt.strip("|").split("|") if c.strip())
        if is_sep:
            header = cells
            idx = {}
            for name in ("发现 ID", "单元", "处置类", "处置", "复验"):
                if name in cells:
                    idx[name] = cells.index(name)
            if not ("处置类" in idx and "复验" in idx):
                header = None
                idx = {}
                continue
            found_defect_table = True
            if "单元" not in idx:
                if section_requires_unit:
                    errors.append(f"V3.9+ 台账段缺少 `单元` 列（open 行无法归属）：{path}")
                header = None
                idx = {}
                continue
            found_unit_table = True
            continue
        if header is None:
            continue
        if all(re.fullmatch(r":?-{2,}:?", c) for c in cells):
            continue
        def cell(name: str) -> str:
            j = idx.get(name, -1)
            return cells[j] if 0 <= j < len(cells) else ""
        units = [u.strip("` ") for u in re.split(r"[、,，]", cell("单元")) if u.strip("` ")]
        if not units:
            errors.append(f"ledger 行无单元归属：{cell('发现 ID') or i + 1}")
            continue
        cls = cell("处置类")
        disp = cell("处置")
        ver = cell("复验")
        open_row = False
        if cls in ("FP", "SUGG", "③", "false-positive"):
            open_row = False
        elif cls == "⑤":
            open_row = True
        elif cls in ("①", "④", "⑥"):
            open_row = bool(re.search(r"未修|待修|未实施|未完成|TODO", ver))
        elif cls == "②":
            open_row = not bool(re.search(r"用户批准\s*[:：]?\s*\d{4}-\d{2}-\d{2}", disp))
        else:
            open_row = True
        if open_row:
            for u in units:
                counts[u] = counts.get(u, 0) + 1
    if not found_defect_table:
        errors.append(f"ledger 没有可解析的处置表（需 `处置类` 与 `复验` 列）：{path}")
    return counts, errors


def cmd_audit(doc: Path, state_path: Path, ledger: str | None) -> int:
    data = load_state(state_path)
    path = Path(ledger) if ledger else default_ledger(doc, data["stage"])
    counts, errors = ledger_open_by_unit(path)
    for e in errors:
        print(f"[FAIL] {e}")
    bad = 0
    for u in data["units"]:
        n = counts.get(u["id"], 0)
        print(f"{u['id']}: open={n}")
        bad += n
    if bad or errors:
        print(f"review-units audit: FAIL（open={bad}）")
        return 1
    print("review-units audit: OK（全部单元 open=0）")
    return 0


def structure_fingerprint(doc: Path) -> str:
    units = parse_units(doc)
    parts = []
    for u in units:
        if u["level"] == 2:
            parts.append(f"{u['id']}:{u['heading']}:{u['text_sha']}")
    return sha("|".join(parts))


def code_fingerprint(path: Path | None) -> str:
    if not path or not path.is_file():
        return ""
    h = hashlib.sha256()
    files = []
    for line in path.read_text(encoding="utf-8").splitlines():
        s = line.strip()
        if not s or s.startswith("#"):
            continue
        p = Path(s)
        if not p.is_absolute():
            p = Path.cwd() / p
        if p.is_file():
            files.append(p)
    for p in sorted(files):
        h.update(str(p).encode())
        h.update(p.read_bytes())
    return h.hexdigest()


def cmd_fingerprint(doc: Path, state_path: Path, code_list: str | None, update: bool) -> int:
    data = load_state(state_path)
    s = structure_fingerprint(doc)
    cpath = Path(code_list) if code_list else None
    c = code_fingerprint(cpath)
    old_s = data.get("structure_sha", "")
    old_c = data.get("code_sha", "")
    print(f"structure: {'same' if old_s == s else 'changed'}")
    print(f"code: {'same' if c and old_c == c else ('changed' if c else 'unavailable')}")
    if update:
        data["structure_sha"] = s
        data["code_sha"] = c
        save_state(state_path, data)
    return 0


def all_chapters_frozen(data: dict) -> bool:
    units = data.get("units", [])
    return bool(units) and any(u["level"] == 2 for u in units) and all(
        u.get("frozen") for u in units
    )


def validate(data: dict, current: list[dict]) -> tuple[list[str], list[str]]:
    errors: list[str] = []
    warnings: list[str] = []
    st = {u["id"]: u for u in data.get("units", [])}
    cu = {u["id"]: u for u in current}
    for uid in sorted(set(cu) - set(st)):
        errors.append(f"新单元未入状态：{uid} {cu[uid]['heading']}（跑 sync）")
    for uid in sorted(set(st) - set(cu)):
        errors.append(f"状态中的单元在正文中不存在：{uid} {st[uid].get('heading','')}（跑 sync）")
    for uid in sorted(set(st) & set(cu)):
        s, c = st[uid], cu[uid]
        if s.get("heading") != c["heading"]:
            errors.append(f"标题漂移：{uid} 状态={s.get('heading')!r} 正文={c['heading']!r}（跑 sync）")
        if s.get("text_sha") != c["text_sha"]:
            if s.get("frozen"):
                errors.append(f"冻结单元正文已变：{uid}（先 sync --unfreeze）")
            else:
                warnings.append(f"正文已变：{uid}（跑 sync 更新 hash）")
        if s.get("t1_state") not in ("pending", "passed", "n/a"):
            errors.append(f"{uid}: 非法 t1_state={s.get('t1_state')}")
        if s.get("t2_state") not in ("pending", "passed", "frozen", "n/a"):
            errors.append(f"{uid}: 非法 t2_state={s.get('t2_state')}")
        deps = s.get("deps") or []
        for d in deps:
            if d == uid:
                errors.append(f"{uid}: deps 不能指向自己")
            elif d not in st:
                errors.append(f"{uid}: deps 指向不存在的单元 {d}")
        if s.get("reopened"):
            if s.get("frozen"):
                errors.append(f"{uid}: 冻结单元的依赖已变化（reopened），必须先复核")
            else:
                warnings.append(f"{uid}: 依赖变化后尚未复核（reopened）")
        if s.get("stale"):
            if s.get("frozen"):
                errors.append(f"{uid}: 冻结单元的锚点正文已变（stale），必须先复核")
            else:
                warnings.append(f"{uid}: 锚点正文已变，尚未复核（stale）")
        if s.get("frozen"):
            if "anchor_hashes" not in s:
                errors.append(f"{uid}: 冻结单元缺锚点凭据（旧状态），先跑 check/sync 重开")
            if s.get("anchor_status") in ("missing", "impl"):
                errors.append(f"{uid}: 冻结单元的锚点不可解析（{s.get('anchor_status')}）")
        for track in ("t1", "t2"):
            n = int(s.get(f"{track}_rounds", 0))
            if n > MAX_ROUNDS and not (s.get("rewritten") or s.get("user_approved")):
                errors.append(f"{uid}: {track}_rounds={n} 超过 {MAX_ROUNDS}（需用户批准或整节重写）")
        if s.get("frozen"):
            if s["level"] == 3:
                if s.get("t2_state") != "frozen":
                    errors.append(f"{uid}: 冻结节的 t2_state 必须是 frozen")
                parent = st.get(s.get("parent") or "")
                if parent and parent.get("t1_state") != "passed":
                    errors.append(f"{uid}: 父章 {parent['id']} 的 T1 未通过，子节不得冻结")
            else:
                if s.get("t1_state") != "passed":
                    errors.append(f"{uid}: 冻结章的 t1_state 必须是 passed")
                children = [x for x in data["units"] if x.get("parent") == uid]
                if children and not all(x.get("frozen") for x in children):
                    if data.get("confirmation", {}).get("active"):
                        warnings.append(f"{uid}: 确认轮中章冻结、子节待重新冻结")
                    else:
                        errors.append(f"{uid}: 章冻结但仍有未冻结子节")
                if not children and s.get("t2_state") != "frozen":
                    errors.append(f"{uid}: 无子节的章冻结时 t2_state 必须是 frozen")
    t3 = data.get("t3", {})
    if t3.get("passed") and not all_chapters_frozen(data):
        errors.append("T3 已通过但仍有未冻结的章")
    if all_chapters_frozen(data) and not t3.get("passed"):
        warnings.append("全部章已冻结；T3 尚未通过")
    return errors, warnings


def cmd_check(doc: Path, state_path: Path) -> int:
    data = load_state(state_path)
    current = parse_units(doc)
    reopened = 0
    for u in data.get("units", []):
        if u.get("frozen") and "anchor_hashes" not in u:
            u["frozen"] = False
            u["frozen_at"] = None
            u["reopened"] = True
            u["stale"] = True
            if u["level"] == 3:
                u["t2_state"] = "pending"
            else:
                u["t1_state"] = "pending"
            reopened += 1
    if reopened:
        save_state(state_path, data)
        print(f"[WARN] {reopened} 个旧冻结单元无锚点凭据，已强制重开")
    errors, warnings = validate(data, current)
    for w in warnings:
        print(f"[WARN] {w}")
    for e in errors:
        print(f"[FAIL] {e}")
    frozen = sum(1 for u in data["units"] if u.get("frozen"))
    print(f"units={len(data['units'])} frozen={frozen} chapters={sum(1 for u in data['units'] if u['level']==2)}")
    if errors:
        print("review-units check: FAIL")
        return 1
    print("review-units check: OK")
    return 0


def cmd_sync(doc: Path, state_path: Path, unfreeze: bool, only: str | None, prune: bool = False) -> int:
    data = load_state(state_path)
    current = parse_units(doc)
    cu = {u["id"]: u for u in current}
    changed = 0
    for u in data["units"]:
        if only and u["id"] != only:
            continue
        c = cu.get(u["id"])
        if not c:
            continue
        if u.get("frozen"):
            if "anchor_hashes" not in u:
                u["frozen"] = False
                u["frozen_at"] = None
                u["reopened"] = True
                u["stale"] = True
                if u["level"] == 3:
                    u["t2_state"] = "pending"
                else:
                    u["t1_state"] = "pending"
                print(f"[WARN] {u['id']}: 旧冻结状态无锚点凭据，强制重开")
                changed += 1
            else:
                _, hashes, status, aerrs = analyze_anchors(c.get("_text", ""))
                if status in ("missing", "impl") or hashes != (u.get("anchor_hashes") or {}):
                    u["frozen"] = False
                    u["frozen_at"] = None
                    u["reopened"] = True
                    u["stale"] = True
                    if u["level"] == 3:
                        u["t2_state"] = "pending"
                    else:
                        u["t1_state"] = "pending"
                    print(f"[WARN] {u['id']}: 锚点正文变化（{status}），强制重开")
                    changed += 1
        if c["heading"] != u.get("heading"):
            u["heading"] = c["heading"]
            u["level"] = c["level"]
            u["parent"] = c["parent"]
            if u.get("frozen") and not unfreeze:
                print(f"[FAIL] {u['id']}: 标题漂移且已冻结；加 --unfreeze 才可接受", file=sys.stderr)
                return 1
            if u["level"] == 2:
                u["t1_state"] = "pending"
                u["frozen"] = False
                u["frozen_at"] = None
                for child in data["units"]:
                    if child.get("parent") == u["id"]:
                        child["frozen"] = False
                        child["frozen_at"] = None
                        if child.get("t2_state") == "frozen":
                            child["t2_state"] = "passed"
            else:
                u["t2_state"] = "pending"
            changed += 1
        if c["text_sha"] != u.get("text_sha"):
            if u.get("frozen") and not unfreeze:
                print(f"[FAIL] {u['id']}: 冻结单元正文已变；加 --unfreeze 才可接受", file=sys.stderr)
                return 1
            u["text_sha"] = c["text_sha"]
            if u.get("frozen"):
                u["frozen"] = False
                u["frozen_at"] = None
                if u["level"] == 3:
                    u["t2_state"] = "pending"
                    parent = next((x for x in data["units"] if x["id"] == u.get("parent")), None)
                    if parent and parent.get("frozen"):
                        parent["frozen"] = False
                        parent["frozen_at"] = None
                        print(f"[WARN] 父章 {parent['id']}: 因子节解冻而解除冻结")
                else:
                    u["t1_state"] = "pending"
                print(f"[WARN] {u['id']}: 已解冻（正文变化）")
            elif u["level"] == 3 and u.get("t2_state") == "passed":
                u["t2_state"] = "pending"
            elif u["level"] == 2 and u.get("t1_state") == "passed":
                u["t1_state"] = "pending"
            for other in data["units"]:
                if u["id"] in (other.get("deps") or []) and not other.get("reopened"):
                    other["reopened"] = True
                    print(f"[WARN] {other['id']}: 依赖 {u['id']} 变化，标 reopened")
            changed += 1
    # 新单元：追加；缺失单元保留并报警
    known = {u["id"] for u in data["units"]}
    for c in current:
        if c["id"] not in known:
            data["units"].append(new_unit(c))
            print(f"[WARN] 新增单元 {c['id']} {c['heading']}")
            changed += 1
    missing = [u["id"] for u in data["units"] if u["id"] not in cu]
    for uid in missing:
        print(f"[WARN] 状态中的单元在正文中不存在：{uid}")
    if missing and prune and not only:
        print("[FAIL] --prune 必须与 --unit 同用，禁止一次删除多个单元", file=sys.stderr)
        return 1
    if missing and prune:
        if only is not None and only not in missing:
            print(f"[FAIL] --unit {only} 不是缺失单元；缺失：{missing}", file=sys.stderr)
            return 1
        drop = set(missing)
        frozen_missing = [u["id"] for u in data["units"] if u["id"] in drop and u.get("frozen")]
        if frozen_missing and not unfreeze:
            print(f"[FAIL] 待删除单元仍标记冻结：{frozen_missing}；加 --unfreeze 才可 prune", file=sys.stderr)
            return 1
        data["units"] = [u for u in data["units"] if u["id"] not in drop]
        print(f"[WARN] 已删除 {len(drop)} 个正文中不存在的单元")
        changed += len(drop)
        save_state(state_path, data)
        # prune 改变了子节标题树，父章 hash 必须同批更新
        return cmd_sync(doc, state_path, unfreeze, None, False)
    data["updated_commit"] = head_commit()
    save_state(state_path, data)
    print(f"sync: {changed} 处更新")
    return 0


def cmd_next(doc: Path, state_path: Path, track: str | None) -> int:
    data = load_state(state_path)
    current = parse_units(doc)
    errors, _ = validate(data, current)
    if errors:
        for e in errors:
            print(f"[FAIL] {e}")
        print("先跑 check/sync，再取 next")
        return 1
    units = data["units"]
    lines = []
    if track in (None, "t1"):
        for u in units:
            if u["level"] == 2 and u.get("t1_state") == "pending":
                lines.append(f"T1 {u['id']} {u['heading']}")
    if track in (None, "t2"):
        for u in units:
            if u["level"] == 3 and u.get("t2_state") in ("pending", "passed") and not u.get("frozen"):
                lines.append(f"T2 {u['id']} {u['heading']}")
    if track in (None, "t3"):
        if all_chapters_frozen(data) and not data.get("t3", {}).get("passed"):
            lines.append("T3 <doc> 出版/代码同步门")
    if not lines:
        print("没有待审单元（全部冻结且 T3 已通过）")
    else:
        print("\n".join(lines))
    return 0


def cmd_mark(doc: Path, state_path: Path, uid: str, track: str, fix: bool,
             passed: bool, freeze: bool, note: str,
             rewrite: bool = False, approve: bool = False,
             deps: str | None = None, confirm_fix: bool = False,
             evidence: str | None = None, ledger: str | None = None) -> int:
    data = load_state(state_path)
    unit = next((u for u in data["units"] if u["id"] == uid), None)
    if not unit:
        print(f"找不到单元：{uid}", file=sys.stderr)
        return 2
    if confirm_fix:
        conf = data.get("confirmation", {})
        if not conf.get("active"):
            print("当前不在确认轮；--confirm-fix 仅用于确认轮", file=sys.stderr)
            return 1
        if int(conf.get("fixes", 0)) >= 2:
            print("[FAIL] 确认轮修复已达 2 次上限", file=sys.stderr)
            return 1
        unit["frozen"] = False
        unit["frozen_at"] = None
        if unit["level"] == 3:
            unit["t2_state"] = "pending"
        else:
            unit["t1_state"] = "pending"
        conf["fixes"] = int(conf.get("fixes", 0)) + 1
        unit["note"] = note or unit.get("note", "")
        save_state(state_path, data)
        print(f"confirm-fix: {uid}（不占修改轮；确认轮修复 {conf['fixes']}/2）")
        return 0
    has_children = any(x.get("parent") == uid for x in data["units"])
    if track == "t1" and unit["level"] != 2:
        print("t1 只适用于 ## 章单元", file=sys.stderr)
        return 2
    if track == "t2" and unit["level"] == 2 and has_children:
        print("有子节的章不走 t2；请按其子节审", file=sys.stderr)
        return 2
    if freeze and unit["level"] == 2 and track != "t1":
        print("章冻结只允许 --track t1（T1 章级审）", file=sys.stderr)
        return 2
    key = f"{track}_state"
    rounds = f"{track}_rounds"
    if rewrite:
        if unit.get("rewritten"):
            print(f"[FAIL] {uid}: 本 edition 已整节重写过一次", file=sys.stderr)
            return 1
        unit["rewritten"] = True
        unit[rounds] = 0
        unit[key] = "pending"
        if unit.get("frozen"):
            unit["frozen"] = False
            unit["frozen_at"] = None
    if approve:
        unit["user_approved"] = True
    if (passed or freeze or rewrite or approve) and not evidence:
        print("[FAIL] pass/freeze/rewrite/approve 必须带 --evidence 文件", file=sys.stderr)
        return 1
    if (passed or freeze or rewrite or approve) and evidence and not Path(evidence).is_file():
        print(f"[FAIL] evidence 文件不存在：{evidence}", file=sys.stderr)
        return 1
    if (passed or freeze or rewrite or approve) and evidence and not evidence_nonempty(Path(evidence)):
        print(f"[FAIL] evidence 文件为空或不可读：{evidence}", file=sys.stderr)
        return 1
    if (passed or freeze or rewrite or approve) and evidence and not evidence_replay_ok(Path(evidence)):
        print(f"[FAIL] evidence 缺可重放 token（path:line / test::name / rg|grep / commit）：{evidence}", file=sys.stderr)
        return 1
    if passed or freeze:
        lpath = Path(ledger) if ledger else default_ledger(doc, data["stage"])
        counts, errs = ledger_open_by_unit(lpath)
        if errs:
            for e in errs:
                print(f"[FAIL] {e}")
            return 1
        if counts.get(uid, 0) > 0:
            print(f"[FAIL] {uid} 仍有 {counts[uid]} 条 open finding；先销账再 pass/freeze", file=sys.stderr)
            return 1
        unit["evidence"] = evidence
    if fix:
        unit[rounds] = int(unit.get(rounds, 0)) + 1
        if unit[rounds] > MAX_ROUNDS and not (unit.get("rewritten") or unit.get("user_approved")):
            print(f"[FAIL] {uid}: {rounds}={unit[rounds]} 超过 {MAX_ROUNDS}；需用户批准或 --rewrite", file=sys.stderr)
            return 1
        unit[key] = "pending"
        if unit.get("frozen"):
            unit["frozen"] = False
            unit["frozen_at"] = None
            if unit["level"] == 3:
                parent = next((x for x in data["units"] if x["id"] == unit.get("parent")), None)
                if parent and parent.get("frozen"):
                    parent["frozen"] = False
                    parent["frozen_at"] = None
    if passed:
        unit[key] = "passed"
        unit["reopened"] = False
    if freeze:
        if not passed:
            print("--freeze 需要与 --pass 同用", file=sys.stderr)
            return 2
        cur = next((u for u in parse_units(doc) if u["id"] == uid), None)
        if cur is None:
            print(f"[FAIL] {uid}: 正文中找不到单元，不能冻结", file=sys.stderr)
            return 1
        refs, hashes, status, aerrs = analyze_anchors(cur.get("_text", ""))
        if status in ("missing", "impl"):
            for e in aerrs:
                print(f"[FAIL] {e}", file=sys.stderr)
            print(f"[FAIL] {uid}: 锚点状态 {status}，不得冻结；改成可解析的 fn/struct 直锚或删掉失效锚", file=sys.stderr)
            return 1
        unit["anchor_refs"] = refs
        unit["anchor_hashes"] = hashes
        unit["anchor_status"] = status
        unit["stale"] = False
        if unit["level"] == 3:
            parent = next((u for u in data["units"] if u["id"] == unit.get("parent")), None)
            if parent and parent.get("t1_state") != "passed":
                print(f"[FAIL] 父章 {parent['id']} 的 T1 未通过，子节不得冻结", file=sys.stderr)
                return 1
            unit["t2_state"] = "frozen"
        else:
            children = [u for u in data["units"] if u.get("parent") == uid]
            if children and not all(c.get("frozen") for c in children):
                print(f"[FAIL] {uid}: 仍有未冻结子节", file=sys.stderr)
                return 1
            if not children:
                unit["t2_state"] = "frozen"
            unit["t1_state"] = "passed"
        unit["frozen"] = True
        unit["frozen_at"] = head_commit()
    if deps is not None:
        want = [x.strip() for x in deps.split(",") if x.strip()]
        known = {x["id"] for x in data["units"]}
        bad = [x for x in want if x not in known or x == uid]
        if bad:
            print(f"[FAIL] deps 含不存在或自指单元：{bad}", file=sys.stderr)
            return 1
        unit["deps"] = want
    if note:
        unit["note"] = note
    save_state(state_path, data)
    print(f"mark: {uid} {track} fix={fix} pass={passed} freeze={freeze}")
    return 0


def cmd_sample(doc: Path, state_path: Path, ratio: float | None, seed: str) -> int:
    data = load_state(state_path)
    units = data["units"]
    dirty = [u for u in units if not u.get("frozen")]
    frozen = [u for u in units if u.get("frozen")]
    all_direct = bool(frozen) and all(u.get("anchor_status") == "direct" for u in frozen)
    minimum = 0.1 if all_direct else 0.2
    if ratio is None:
        ratio = minimum
    if ratio < minimum:
        print(f"抽样比例不得低于 {minimum:.0%}（{'全部直锚' if all_direct else '含 fallback/无锚'}；review-units.md §五）", file=sys.stderr)
        return 2
    if not data.get("baseline_full_survey"):
        print("首个 edition 尚未完成整篇 survey（先跑 `review-units.py baseline`）", file=sys.stderr)
        return 1
    if not seed:
        seed = "HEAD"
    ranked = sorted(frozen, key=lambda u: hashlib.sha256(f"{seed}:{u['id']}".encode()).hexdigest())
    n = math.ceil(len(frozen) * ratio) if frozen else 0
    picked = ranked[:n]
    if len(dirty) > 0.3 * max(1, len(units)):
        print(f"[FAIL] 未冻结单元 {len(dirty)}/{len(units)} > 30%；按 review-units.md §五回退整篇 survey，不抽样")
        return 1
    print("## dirty")
    for u in dirty:
        print(f"{u['id']} {u['heading']}")
    print("## frozen-sample")
    for u in picked:
        print(f"{u['id']} {u['heading']}")
    return 0


def cmd_invalidate(doc: Path, state_path: Path, rev: str) -> int:
    """Reopen frozen units whose direct anchor target changed since REV."""
    data = load_state(state_path)
    check = subprocess.run(
        ["git", "-C", str(ROOT), "cat-file", "-e", rev + "^{commit}"],
        capture_output=True,
    )
    if check.returncode != 0:
        print(f"不是有效 commit：{rev}", file=sys.stderr)
        return 2
    diff = subprocess.run(
        ["git", "-C", str(ROOT), "diff", "--name-only", f"{rev}..HEAD"],
        capture_output=True, text=True,
    )
    if diff.returncode != 0:
        print(diff.stderr.strip(), file=sys.stderr)
        return 1
    changed = {line.strip() for line in diff.stdout.splitlines() if line.strip()}
    hit = 0
    for u in data["units"]:
        if not u.get("frozen"):
            continue
        files = {r.split(":", 1)[0] for r in (u.get("anchor_refs") or [])}
        if files & changed:
            u["frozen"] = False
            u["frozen_at"] = None
            u["reopened"] = True
            u["stale"] = True
            if u["level"] == 3:
                u["t2_state"] = "pending"
            else:
                u["t1_state"] = "pending"
            hit += 1
    save_state(state_path, data)
    print(f"invalidate --from {rev}: {hit} 个冻结单元因锚点文件变化而重开")
    return 0


def cmd_baseline(doc: Path, state_path: Path, evidence: str | None) -> int:
    if not evidence or not Path(evidence).is_file():
        print("baseline 必须带 --evidence（整篇 survey 的 metrics/scan 产物）", file=sys.stderr)
        return 1
    if not evidence_replay_ok(Path(evidence)):
        print("baseline evidence 缺可重放 token（path:line / test::name / rg|grep / commit）", file=sys.stderr)
        return 1
    data = load_state(state_path)
    data["baseline_full_survey"] = True
    data["baseline_evidence"] = evidence
    save_state(state_path, data)
    print("baseline: 首个 edition 的整篇 survey 已记录")
    return 0


def cmd_gate(doc: Path, state_path: Path, ok: bool, evidence: str, ledger: str | None = None) -> int:
    data = load_state(state_path)
    errors, _ = validate(data, parse_units(doc))
    if ok and errors:
        for e in errors:
            print(f"[FAIL] {e}")
        print("[FAIL] 状态未通过 check，T3 不得通过", file=sys.stderr)
        return 1
    if ok and data.get("confirmation", {}).get("active"):
        print("[FAIL] 确认轮尚未结束，T3 不得通过", file=sys.stderr)
        return 1
    if ok and not evidence:
        print("[FAIL] gate --pass 必须带 --evidence 文件", file=sys.stderr)
        return 1
    if ok and not Path(evidence).is_file():
        print(f"[FAIL] evidence 文件不存在：{evidence}", file=sys.stderr)
        return 1
    if ok and not evidence_nonempty(Path(evidence)):
        print(f"[FAIL] evidence 文件为空或不可读：{evidence}", file=sys.stderr)
        return 1
    if ok and not evidence_replay_ok(Path(evidence)):
        print(f"[FAIL] evidence 缺可重放 token（path:line / test::name / rg|grep / commit）：{evidence}", file=sys.stderr)
        return 1
    if ok and not all_chapters_frozen(data):
        print("[FAIL] 仍有未冻结的章，T3 不得通过", file=sys.stderr)
        return 1
    if ok and any(u.get("reopened") or u.get("stale") for u in data["units"]):
        bad = [u["id"] for u in data["units"] if u.get("reopened") or u.get("stale")]
        print(f"[FAIL] 存在 reopened/stale 单元，T3 不得通过：{bad}", file=sys.stderr)
        return 1
    if ok:
        lpath = Path(ledger) if ledger else default_ledger(doc, data["stage"])
        counts, errs = ledger_open_by_unit(lpath)
        if errs:
            for e in errs:
                print(f"[FAIL] {e}")
            return 1
        if any(counts.get(u["id"], 0) > 0 for u in data["units"]):
            print("[FAIL] 仍有单元 open finding，T3 不得通过", file=sys.stderr)
            return 1
    data["t3"] = {"passed": ok, "evidence": evidence, "at_commit": head_commit()}
    save_state(state_path, data)
    print(f"gate T3: {'PASS' if ok else 'FAIL'}")
    return 0


def cmd_stats(doc: Path, state_path: Path) -> int:
    data = load_state(state_path)
    units = data["units"]
    chapters = [u for u in units if u["level"] == 2]
    sections = [u for u in units if u["level"] == 3]
    frozen = sum(1 for u in units if u.get("frozen"))
    print(f"chapters={len(chapters)} sections={len(sections)} frozen={frozen}/{len(units)}")
    print(f"t1_pending={sum(1 for u in chapters if u.get('t1_state')=='pending')} "
          f"t2_pending={sum(1 for u in sections if u.get('t2_state')=='pending')} "
          f"reopened={sum(1 for u in units if u.get('reopened'))} "
          f"stale={sum(1 for u in units if u.get('stale'))} "
          f"t3={'PASS' if data.get('t3',{}).get('passed') else 'pending'}")
    return 0


def _anchor_self_test(root: Path) -> None:
    """Nine anchor-invalidation cases (run inside a temp ROOT)."""
    global ROOT
    saved = ROOT
    try:
        root.mkdir(parents=True, exist_ok=True)
        ROOT = root
        (root / "os/kernel/src").mkdir(parents=True)
        (root / "minix3/minix/kernel").mkdir(parents=True)
        af = root / "os/kernel/src/anchors.rs"
        af.write_text(
            "pub fn alpha() -> u32 { 1 }\n\n"
            "pub struct Beta { pub x: u32 }\n\n"
            "impl Beta {\n    pub fn gamma(&self) -> u32 { self.x }\n}\n",
            encoding="utf-8",
        )
        (root / "minix3/minix/kernel/proc.c").write_text(
            "int proc_init(void) { return 0; }\n", encoding="utf-8"
        )
        doc = root / "01-stage-kernel/02-anchor.md"
        doc.parent.mkdir(parents=True)
        doc.write_text(
            "# anchor\n\n## 3. 锚点章\n\n"
            "### 3.1 直锚\n\n见 `os/kernel/src/anchors.rs:fn alpha`。\n\n"
            "### 3.2 缺失\n\n见 `os/kernel/src/anchors.rs:fn nope`。\n\n"
            "### 3.3 impl\n\n见 `os/kernel/src/anchors.rs:impl Beta::gamma`。\n\n"
            "### 3.4 C 锚\n\n见 `minix3/minix/kernel/proc.c:proc_init`。\n",
            encoding="utf-8",
        )
        sp = root / "anchor-state.yaml"
        ev = root / "ev.md"
        ev.write_text("rg -n 'alpha' os/kernel/src/anchors.rs\n", encoding="utf-8")
        ledger = root / "ledger.md"
        ledger.write_text("| 发现 ID | 单元 | 处置类 | 处置 | 复验 |\n|---|---|---|---|---|\n", encoding="utf-8")
        assert cmd_init(doc, sp, "01-stage-kernel", "V3.9", False) == 0
        assert cmd_baseline(doc, sp, str(ev)) == 0
        assert cmd_mark(doc, sp, "3", "t1", False, True, False, "", evidence=str(ev), ledger=str(ledger)) == 0
        # 1 direct freeze stores body hashes
        assert cmd_mark(doc, sp, "3.1", "t2", False, True, True, "", evidence=str(ev), ledger=str(ledger)) == 0
        d = load_state(sp)
        u = next(x for x in d["units"] if x["id"] == "3.1")
        assert u["anchor_status"] == "direct" and u["anchor_hashes"], u
        # 2 missing anchor refuses freeze
        assert cmd_mark(doc, sp, "3.2", "t2", False, True, True, "", evidence=str(ev), ledger=str(ledger)) == 1
        # 3 impl anchor refuses freeze
        assert cmd_mark(doc, sp, "3.3", "t2", False, True, True, "", evidence=str(ev), ledger=str(ledger)) == 1
        # 4 C anchor freezes as fallback
        assert cmd_mark(doc, sp, "3.4", "t2", False, True, True, "", evidence=str(ev), ledger=str(ledger)) == 0
        d = load_state(sp)
        assert next(x for x in d["units"] if x["id"] == "3.4")["anchor_status"] == "fallback"
        # 5 all-direct 10% vs fallback 20%
        d["units"] = [x for x in d["units"] if x["id"] == "3.1"]
        save_state(sp, d)
        assert cmd_sample(doc, sp, 0.1, "x") == 0
        d = load_state(sp)
        d["units"].append(new_unit({
            "id": "3.4", "heading": "3.4 C 锚", "level": 3, "parent": "3", "order": 9,
            "text_sha": "x", "_text": "",
        }))
        next(x for x in d["units"] if x["id"] == "3.4").update(
            anchor_status="fallback", frozen=True, t2_state="frozen",
        )
        save_state(sp, d)
        assert cmd_sample(doc, sp, 0.1, "x") == 2
        # 6 anchor body change reopens on sync
        af.write_text(af.read_text().replace("1 }", "2 }"), encoding="utf-8")
        assert cmd_sync(doc, sp, False, None) == 0
        d = load_state(sp)
        u = next(x for x in d["units"] if x["id"] == "3.1")
        assert u["reopened"] and u["stale"] and not u["frozen"], u
        # 7 old frozen without credential is force-reopened by check
        u["frozen"] = True
        u["t2_state"] = "frozen"
        u.pop("anchor_hashes", None)
        save_state(sp, d)
        assert cmd_check(doc, sp) == 0
        d = load_state(sp)
        assert next(x for x in d["units"] if x["id"] == "3.1")["reopened"]
        # 8 invalidate --from reopens anchors changed in git
        subprocess.run(["git", "init", "-q"], cwd=root, check=True)
        subprocess.run(["git", "config", "user.email", "t@t"], cwd=root, check=True)
        subprocess.run(["git", "config", "user.name", "t"], cwd=root, check=True)
        subprocess.run(["git", "add", "."], cwd=root, check=True)
        subprocess.run(["git", "commit", "-qm", "base"], cwd=root, check=True)
        base = subprocess.run(["git", "rev-parse", "HEAD"], cwd=root, capture_output=True, text=True, check=True).stdout.strip()
        d = load_state(sp)
        u = next(x for x in d["units"] if x["id"] == "3.1")
        u.update(frozen=True, t2_state="frozen", reopened=False, stale=False,
                 anchor_hashes=analyze_anchors("`os/kernel/src/anchors.rs:fn alpha`")[1],
                 anchor_refs=["os/kernel/src/anchors.rs:fn alpha"], anchor_status="direct")
        save_state(sp, d)
        af.write_text(af.read_text().replace("2 }", "3 }"), encoding="utf-8")
        subprocess.run(["git", "add", "os/kernel/src/anchors.rs"], cwd=root, check=True)
        subprocess.run(["git", "commit", "-qm", "change"], cwd=root, check=True)
        assert cmd_invalidate(doc, sp, base) == 0
        d = load_state(sp)
        assert next(x for x in d["units"] if x["id"] == "3.1")["stale"]
        # 9 T3 gate rejects stale/reopened
        assert cmd_gate(doc, sp, True, str(ev), str(ledger)) == 1
    finally:
        ROOT = saved


def self_test() -> int:
    with tempfile.TemporaryDirectory() as td:
        _anchor_self_test(Path(td) / "anchor-root")
    with tempfile.TemporaryDirectory() as td:
        root = Path(td)
        doc = root / "01-stage-kernel/01-demo.md"
        doc.parent.mkdir(parents=True)
        doc.write_text(
            "# demo\n\n"
            "## 1. 第一章\n\n引子。\n\n"
            "### 1.1 第一节\n\n内容一。\n\n"
            "### 1.2 第二节\n\n内容二。\n\n"
            "## 2. 第二章\n\n### 2.1 第三节\n\n内容三。\n",
            encoding="utf-8",
        )
        sp = root / "state.yaml"
        ev = root / "evidence.md"
        ev.write_text("rg -n 'fn demo' os/  # unit evidence\n", encoding="utf-8")
        ledger = root / "ledger.md"
        ledger.write_text(
            "| 发现 ID | 单元 | 处置类 | 处置 | 复验 |\n"
            "|---|---|---|---|---|\n",
            encoding="utf-8",
        )

        def mk(uid, track, fix=False, passed=False, freeze=False, note=""):
            return cmd_mark(doc, sp, uid, track, fix, passed, freeze, note,
                            evidence=str(ev), ledger=str(ledger))

        assert cmd_init(doc, sp, "01-stage-kernel", "V3.9", False) == 0
        data = load_state(sp)
        assert len(data["units"]) == 5, len(data["units"])
        assert cmd_check(doc, sp) == 0
        assert cmd_audit(doc, sp, str(ledger)) == 0
        ledger.write_text(
            "| 发现 ID | 单元 | 处置类 | 处置 | 复验 |\n"
            "|---|---|---|---|---|\n"
            "| F1 | 1.1 | ① | x | 未修 |\n",
            encoding="utf-8",
        )
        assert cmd_audit(doc, sp, str(ledger)) == 1
        ledger.write_text(
            "| 发现 ID | 单元 | 处置类 | 处置 | 复验 |\n"
            "|---|---|---|---|---|\n",
            encoding="utf-8",
        )
        assert cmd_mark(doc, sp, "1", "t1", False, True, False, "", ledger=str(ledger)) == 1  # 缺 evidence
        assert cmd_sample(doc, sp, 0.2, "x") == 1   # 首 edition 不抽查
        assert cmd_sample(doc, sp, 0.1, "x") == 2   # 比例下限
        assert cmd_baseline(doc, sp, str(ev)) == 0
        # 章 1 T1 通过；两节通过并冻结；章冻结
        assert mk("1", "t1", False, True, False, "") == 0
        assert mk("1.1", "t2", False, True, True, "") == 0
        assert mk("1.2", "t2", False, True, True, "") == 0
        assert mk("1", "t1", False, True, True, "") == 0
        assert mk("1.1", "t1", True, False, False, "") == 2  # t1 只用于章
        assert mk("2", "t2", False, True, True, "") == 2     # 章冻结只走 t1
        # 修改冻结节：check 失败，sync 无 --unfreeze 失败，带 --unfreeze 成功
        doc.write_text(doc.read_text(encoding="utf-8").replace("内容一。", "内容一改。"), encoding="utf-8")
        assert cmd_check(doc, sp) == 1
        assert cmd_gate(doc, sp, True, "") == 1
        assert cmd_sync(doc, sp, False, None) == 1
        assert cmd_sync(doc, sp, True, None) == 0
        assert cmd_check(doc, sp) == 0
        # 章标题改动：t1 回 pending，子节解冻
        txt = doc.read_text(encoding="utf-8").replace("## 1. 第一章", "## 1. 第一章改名")
        doc.write_text(txt, encoding="utf-8")
        assert cmd_sync(doc, sp, True, None) == 0
        d = load_state(sp)
        assert next(u for u in d["units"] if u["id"] == "1")["t1_state"] == "pending"
        assert all(not u.get("frozen") for u in d["units"])
        assert cmd_check(doc, sp) == 0
        # 预算：连续 fix 第 3 次被拦
        assert mk("1.1", "t2", True, False, False, "") == 0
        assert mk("1.1", "t2", True, False, False, "") == 0
        assert mk("1.1", "t2", True, False, False, "") == 1
        # 删除一节：sync --prune 收口
        txt = doc.read_text(encoding="utf-8")
        txt = txt.replace("### 1.2 第二节\n\n内容二。\n\n", "")
        doc.write_text(txt, encoding="utf-8")
        assert cmd_sync(doc, sp, True, None, True) == 1  # prune 必须带 --unit
        assert cmd_sync(doc, sp, True, "1.2", True) == 0
        assert cmd_check(doc, sp) == 0
        # 全部冻结后再抽样；首 edition 已 baseline
        assert mk("1", "t1", False, True, False, "") == 0
        assert mk("2", "t1", False, True, False, "") == 0
        assert mk("1.1", "t2", False, True, True, "") == 0
        assert mk("2.1", "t2", False, True, True, "") == 0
        assert mk("1", "t1", False, True, True, "") == 0
        assert mk("2", "t1", False, True, True, "") == 0
        # 抽样确定性
        out1 = cmd_sample(doc, sp, 0.2, "abc")
        out2 = cmd_sample(doc, sp, 0.2, "abc")
        assert out1 == 0 and out2 == 0
        # 新 edition：归档旧计数、保留冻结；确认轮修复不占修改轮
        assert cmd_new_edition(doc, sp, "V3.10", False) == 0
        d = load_state(sp)
        assert d["edition"] == "V3.10" and d["confirmation"]["active"] is False
        assert all(u["t2_rounds"] == 0 and u["t1_rounds"] == 0 for u in d["units"])
        assert any(u.get("frozen") for u in d["units"])
        assert cmd_begin_confirmation(doc, sp) == 0
        assert cmd_mark(doc, sp, "1.1", "t2", False, False, False, "",
                        confirm_fix=True, evidence=str(ev), ledger=str(ledger)) == 0
        assert cmd_end_confirmation(doc, sp) == 1  # 仍有未冻结子节
        assert mk("1.1", "t2", False, True, True, "") == 0
        assert cmd_end_confirmation(doc, sp) == 0
        assert cmd_gate(doc, sp, True, str(ev), str(ledger)) == 0
        # V3.9 单元列：全部 closed 的历史表不追溯；open 行缺单元必须报错
        legacy_ok = root / "legacy-closed.md"
        legacy_ok.write_text(
            "| 发现 ID | 处置类 | 处置 | 复验 |\n"
            "|---|---|---|---|\n"
            "| X-1 | ① | 已修 | grep -n x doc |\n",
            encoding="utf-8",
        )
        counts, errs = ledger_open_by_unit(legacy_ok)
        assert not errs and not counts
        legacy_open = root / "legacy-open.md"
        legacy_open.write_text(
            "## R99 测试轮（V3.9）\n\n"
            "| 发现 ID | 处置类 | 处置 | 复验 |\n"
            "|---|---|---|---|\n"
            "| X-2 | ⑤ | 取证不足 | 未修 |\n",
            encoding="utf-8",
        )
        counts, errs = ledger_open_by_unit(legacy_open)
        assert errs and not counts
        # CLI dispatch：mark 必须真正走 cmd_mark，不得再出现局部名缺失
        empty_ev = root / "empty-evidence.md"
        empty_ev.write_text("   \n", encoding="utf-8")
        assert cmd_gate(doc, sp, True, str(empty_ev), str(ledger)) == 1
        assert cmd_mark(doc, sp, "1", "t1", False, True, False, "",
                        evidence=str(empty_ev), ledger=str(ledger)) == 1
        no_token = root / "no-token.md"
        no_token.write_text("unit evidence\n", encoding="utf-8")
        assert cmd_gate(doc, sp, True, str(no_token), str(ledger)) == 1
        assert cmd_mark(doc, sp, "1", "t1", False, True, False, "",
                        evidence=str(no_token), ledger=str(ledger))
        dec_ok = root / "dec-ok.md"
        dec_ok.write_text("D-060\n", encoding="utf-8")
        assert evidence_replay_ok(dec_ok)
        dec_bad = root / "dec-bad.md"
        dec_bad.write_text("D-999\n", encoding="utf-8")
        assert not evidence_replay_ok(dec_bad)
        neg = root / "neg.md"
        neg.write_text("没有运行 rg 命令\n", encoding="utf-8")
        assert not evidence_replay_ok(neg) == 1
        src = Path(__file__).resolve().read_text(encoding="utf-8")
        assert "return cmd_mark(doc, sp, args.unit" in src
    print("self-test PASS: init/check/sync/unfreeze/budget/sample/audit/evidence/cli + 9 anchor-invalidation cases")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description="单元级 review 状态机（V3.9）")
    ap.add_argument("--self-test", action="store_true")
    sub = ap.add_subparsers(dest="cmd")
    for name in ("init", "check", "sync", "next", "mark", "sample", "gate", "stats",
                 "baseline", "new-edition", "begin-confirmation", "end-confirmation", "audit", "fingerprint", "invalidate"):
        p = sub.add_parser(name)
        p.add_argument("doc", nargs="?")
        p.add_argument("--doc", dest="doc_opt", help="文档路径（与位置参数二选一）")
    sub.choices["init"].add_argument("--stage")
    sub.choices["init"].add_argument("--edition", default="V3.9")
    sub.choices["init"].add_argument("--force", action="store_true")
    sub.choices["baseline"].add_argument("--evidence", default=None)
    sub.choices["invalidate"].add_argument("--from", dest="from_rev", required=True)
    sub.choices["new-edition"].add_argument("--edition", required=True)
    sub.choices["new-edition"].add_argument("--unfreeze-all", action="store_true")
    sub.choices["audit"].add_argument("--ledger", default=None)
    sub.choices["fingerprint"].add_argument("--code-list", default=None)
    sub.choices["fingerprint"].add_argument("--update", action="store_true")
    sub.choices["sync"].add_argument("--unfreeze", action="store_true")
    sub.choices["sync"].add_argument("--unit")
    sub.choices["sync"].add_argument("--prune", action="store_true")
    sub.choices["next"].add_argument("--track", choices=("t1", "t2", "t3"))
    m = sub.choices["mark"]
    m.add_argument("--unit", required=True)
    m.add_argument("--track", required=True, choices=("t1", "t2"))
    g = m.add_mutually_exclusive_group(required=False)
    g.add_argument("--fix", action="store_true")
    g.add_argument("--pass", dest="passed", action="store_true")
    m.add_argument("--freeze", action="store_true")
    m.add_argument("--rewrite", action="store_true")
    m.add_argument("--approve", action="store_true")
    m.add_argument("--deps", default=None, help="逗号分隔的上游单元 ID")
    m.add_argument("--confirm-fix", action="store_true", help="确认轮修复：解冻但不占修改轮")
    m.add_argument("--evidence", default=None, help="pass/freeze 的复核证据文件")
    m.add_argument("--ledger", default=None, help="defect-ledger 路径（默认按 run 目录推导）")
    m.add_argument("--note", default="")
    s = sub.choices["sample"]
    s.add_argument("--ratio", type=float, default=None)
    s.add_argument("--seed", default="")
    ga = sub.choices["gate"]
    gg = ga.add_mutually_exclusive_group(required=True)
    gg.add_argument("--pass", dest="ok", action="store_true")
    gg.add_argument("--fail", dest="ok", action="store_false")
    ga.add_argument("--evidence", default="")
    ga.add_argument("--ledger", default=None)

    args = ap.parse_args()
    if args.self_test:
        return self_test()
    if not args.cmd:
        ap.print_help()
        return 2
    raw_doc = args.doc or getattr(args, "doc_opt", None)
    if not raw_doc:
        print("缺少文档路径（位置参数或 --doc）", file=sys.stderr)
        return 2
    doc = Path(raw_doc)
    if not doc.is_absolute():
        doc = (Path.cwd() / doc).resolve()
    stage = getattr(args, "stage", None) or stage_of(doc)
    sp = review_state_path(stage, doc.stem)
    if args.cmd == "init":
        return cmd_init(doc, sp, stage, args.edition, args.force)
    if args.cmd == "check":
        return cmd_check(doc, sp)
    if args.cmd == "sync":
        return cmd_sync(doc, sp, args.unfreeze, args.unit, args.prune)
    if args.cmd == "next":
        return cmd_next(doc, sp, args.track)
    if args.cmd == "mark":
        return cmd_mark(doc, sp, args.unit, args.track, args.fix, args.passed, args.freeze, args.note, args.rewrite, args.approve, args.deps, args.confirm_fix, args.evidence, args.ledger)
    if args.cmd == "sample":
        return cmd_sample(doc, sp, args.ratio, args.seed)
    if args.cmd == "invalidate":
        return cmd_invalidate(doc, sp, args.from_rev)
    if args.cmd == "gate":
        return cmd_gate(doc, sp, args.ok, args.evidence, args.ledger)
    if args.cmd == "baseline":
        return cmd_baseline(doc, sp, args.evidence)
    if args.cmd == "new-edition":
        return cmd_new_edition(doc, sp, args.edition, args.unfreeze_all)
    if args.cmd == "begin-confirmation":
        return cmd_begin_confirmation(doc, sp)
    if args.cmd == "end-confirmation":
        return cmd_end_confirmation(doc, sp)
    if args.cmd == "audit":
        return cmd_audit(doc, sp, args.ledger)
    if args.cmd == "fingerprint":
        return cmd_fingerprint(doc, sp, args.code_list, args.update)
    if args.cmd == "stats":
        return cmd_stats(doc, sp)
    return 2


if __name__ == "__main__":
    sys.exit(main())
