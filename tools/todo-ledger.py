#!/usr/bin/env python3
"""todo-ledger.py — TODO 单一账本（V3.7 / V3.8-r2 迁移扩展）

背景：此前 TODO 散在 50+ 个 markdown（旧 stage `todo.md`（已迁移删除）、OPEN-DEBT、
suggestions、follow-ups），彼此不引用、无门读取。收敛判据只读 OPEN-DEBT，于是出现
「CONVERGED 但旧 todo 文档还有 24 条待实施」的假收敛。

用户裁决：
  - 规范源 = `rewrite-notes/meta/coordination/todo-ledger/<stage>.yaml`（每 stage 一个，入库）。
  - 旧 todo 文件在迁移后删除；closed 历史靠 git 历史 + origin_commit 追溯。
  - 零欠账：CONVERGED 时当前篇 `status: open` 必须为 0。
  - 待裁项必须 `blocked: true` + `blocked_reason`，阻断收敛。

用法：
  python3 tools/todo-ledger.py check [--doc DOC] [--fail-on-open]
  python3 tools/todo-ledger.py list  [--doc DOC]
  python3 tools/todo-ledger.py import-md <todo.md> --stage S --doc D --out YAML
  python3 tools/todo-ledger.py import-suggestions <suggestions.md> --stage S --doc D --out YAML
  python3 tools/todo-ledger.py --self-test
"""
from __future__ import annotations

import argparse
import re
import subprocess
import sys
from datetime import date
from pathlib import Path

try:
    import yaml
except Exception:  # pragma: no cover
    print("需要 PyYAML（python3 -m pip install pyyaml）", file=sys.stderr)
    raise SystemExit(2)

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(Path(__file__).resolve().parent))
from notes_layout import TODO_LEDGER_DIR  # noqa: E402

LEDGER_DIR = TODO_LEDGER_DIR
STATUSES = {"open", "closed", "rejected", "defer-approved", "historical"}
SEVERITIES = {"P0", "P1", "P2", "P3"}
KINDS = {"code", "doc", "design", "arch"}
SCOPES = {"doc", "stage"}
REQUIRED = ("id", "stage", "doc", "title", "status", "severity", "kind")
OPEN_MARKERS = re.compile(r"待实施|未实施|登记不实施|不实施|待裁决|待用户批准|开放调查|未修|待补|待办")
CLOSED_MARKERS = re.compile(r"✅|已实施|已关闭|已修|完成")
ID_RE = re.compile(r"\b((?:I|T|N|D)-\d+[a-z]?|D-\d{3})\b")
SEV_RE = re.compile(r"\bP([0-3])\b")
ORIGIN_RE = re.compile(r"来源\s*[:：]\s*([^\n；;]{0,120})")
DATE_RE = re.compile(r"^20\d{2}-\d{2}-\d{2}$")
COMMIT_RE = re.compile(r"^[0-9a-f]{7,40}$")


def canon_doc(value: str | None) -> str | None:
    """`--doc` 归一：去掉目录前缀与 `.md`，只比 basename。"""
    if not value:
        return None
    name = Path(str(value)).name
    return name[:-3] if name.endswith(".md") else name


def doc_stage(doc: str | None) -> str | None:
    """从正式篇目路径反查 stage 目录名（用于 scope: stage 的行）。"""
    if not doc:
        return None
    name = canon_doc(doc)
    for p in ROOT.glob("rewrite-notes/*-stage-*/**/*.md"):
        if p.stem == name:
            for part in p.parts:
                if part.endswith("-stage-"):
                    return part
                if re.match(r"^\d+-stage-", part):
                    return part
    return None


def load(path: Path) -> list[dict]:
    data = yaml.safe_load(path.read_text(encoding="utf-8")) or []
    if not isinstance(data, list):
        raise SystemExit(f"{path}: 顶层必须是列表")
    return data


def save(path: Path, rows: list[dict]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    header = (
        "# TODO 单一账本（V3.7 / v2 refs 扩展）——规范源；01/02 旧 todo 已迁移，其余 stage 迁移前仍是输入。\n"
        "# v2 新增：refs{code,docs,tests,decisions} / relations{blocked_by,duplicates,supersedes}\n"
        "#          / lifecycle{created,updated,closed_by}；不存行号。\n"
        "# 字段：id/stage/doc/title/status/severity/kind/origin/evidence/defer/\n"
        "#       merged_into/legacy_status/origin_file/origin_line/origin_commit/\n"
        "#       verified_by/verified_at/aliases/scope/blocked/blocked_reason\n"
        "# status: open | closed | rejected | defer-approved | historical\n"
        "# scope: doc（默认）| stage；blocked=true 必须带 blocked_reason。\n"
        "# defer-approved 必须带 decision/approved/until/consequence 四项。\n"
    )
    body = yaml.safe_dump(rows, allow_unicode=True, sort_keys=False, width=120)
    path.write_text(header + body, encoding="utf-8")


def _row_in_scope(r: dict, doc: str | None, scope: str = "doc") -> bool:
    if scope == "all":
        return True
    if scope == "coordination":
        return r.get("stage") == "coordination"
    if scope == "stage":
        return r.get("scope") == "stage"
    return _row_matches_doc(r, doc)


def _row_matches_doc(r: dict, doc: str | None) -> bool:
    if doc is None:
        return True
    return canon_doc(r.get("doc")) == canon_doc(doc)


def check_rows(
    rows: list[dict],
    doc: str | None,
    all_ids: set[str] | None = None,
    global_seen: set[str] | None = None,
    *,
    strict_refs: bool = False,
    decisions: set[str] | None = None,
    rows_by_id: dict[str, dict] | None = None,
    stats: dict | None = None,
    scope: str = "doc",
) -> tuple[list[str], list[str], int, int]:
    errors: list[str] = []
    warnings: list[str] = []
    seen: set[str] = global_seen if global_seen is not None else set()
    open_count = 0
    blocked_count = 0
    for i, r in enumerate(rows, 1):
        where = f"#{i} {r.get('id', '<no id>')}"
        for k in REQUIRED:
            if k not in r or r[k] in (None, ""):
                errors.append(f"{where}: 缺字段 {k}")
        rid = str(r.get("id", ""))
        if rid in seen:
            errors.append(f"{where}: ID 重复（跨文件也计）")
        seen.add(rid)
        if r.get("status") not in STATUSES:
            errors.append(f"{where}: status 非法 `{r.get('status')}`")
        if r.get("severity") not in SEVERITIES:
            errors.append(f"{where}: severity 非法 `{r.get('severity')}`")
        if r.get("kind") not in KINDS:
            errors.append(f"{where}: kind 非法 `{r.get('kind')}`")
        if r.get("scope", "doc") not in SCOPES:
            errors.append(f"{where}: scope 非法 `{r.get('scope')}`")
        if r.get("status") == "closed" and not str(r.get("evidence", "")).strip():
            errors.append(f"{where}: closed 缺 evidence（commit / path:line / test:: / dup-of）")
        if r.get("status") == "rejected":
            ev = str(r.get("evidence", ""))
            if not re.search(r"D-\d{3}", ev):
                errors.append(f"{where}: rejected 必须引用用户裁决 D-xxx")
        if r.get("status") == "defer-approved":
            d = r.get("defer") or {}
            for k in ("decision", "approved", "until", "consequence"):
                if not d.get(k):
                    errors.append(f"{where}: defer-approved 缺 defer.{k}")
            if d.get("decision") and not re.search(r"D-\d{3}", str(d["decision"])):
                errors.append(f"{where}: defer.decision 必须是 D-xxx")
        # V3.7-r2：待裁项必须显式 blocked，且必须有理由。
        if r.get("blocked"):
            if not str(r.get("blocked_reason", "")).strip():
                errors.append(f"{where}: blocked=true 缺 blocked_reason")
            if r.get("status") not in ("open", "historical"):
                errors.append(f"{where}: blocked=true 的 status 必须是 open 或 historical")
            if _row_in_scope(r, doc, scope):
                blocked_count += 1
        # ---- V3.8-r2 迁移扩展字段（全部可选）----
        merged = r.get("merged_into")
        if merged is not None:
            if not str(merged).strip():
                errors.append(f"{where}: merged_into 为空")
            elif str(merged) == rid:
                errors.append(f"{where}: merged_into 不能指向自己")
            elif all_ids is not None and str(merged) not in all_ids:
                errors.append(f"{where}: merged_into 目标 `{merged}` 不存在")
            if r.get("status") not in ("closed", "historical"):
                errors.append(f"{where}: merged_into 的 status 必须是 closed 或 historical")
        if "legacy_status" in r and not str(r.get("legacy_status", "")).strip():
            errors.append(f"{where}: legacy_status 为空")
        of = r.get("origin_file")
        ol = r.get("origin_line")
        oc = r.get("origin_commit")
        if of is not None:
            if not str(of).strip():
                errors.append(f"{where}: origin_file 为空")
            elif not oc and not (ROOT / str(of)).is_file():
                errors.append(f"{where}: origin_file 不存在且无 origin_commit: {of}")
        if ol is not None:
            if not of:
                errors.append(f"{where}: origin_line 需要 origin_file")
            elif not isinstance(ol, int) or ol < 1:
                errors.append(f"{where}: origin_line 必须是 >=1 的整数")
            elif of and (ROOT / str(of)).is_file():
                n = len((ROOT / str(of)).read_text(encoding="utf-8").splitlines())
                if ol > n:
                    warnings.append(f"{where}: origin_line {ol} 超出文件行数 {n}")
        if oc is not None and not COMMIT_RE.match(str(oc)):
            errors.append(f"{where}: origin_commit 不是 7–40 位十六进制")
        vb, va = r.get("verified_by"), r.get("verified_at")
        if (vb is None) != (va is None):
            errors.append(f"{where}: verified_by/verified_at 必须成对出现")
        if va is not None and not _valid_date(va):
            errors.append(f"{where}: verified_at 必须是合法日期 YYYY-MM-DD")
        if va is not None and not str(r.get("evidence", "")).strip():
            errors.append(f"{where}: 有 verified_at 时 evidence 不能为空")
        aliases = r.get("aliases")
        if aliases is not None:
            if not isinstance(aliases, list):
                errors.append(f"{where}: aliases 必须是列表")
            else:
                if len(aliases) != len(set(map(str, aliases))):
                    errors.append(f"{where}: aliases 有重复")
                if rid in {str(a) for a in aliases}:
                    errors.append(f"{where}: aliases 不能包含自身 ID")
                for a in aliases:
                    if not str(a).strip():
                        errors.append(f"{where}: aliases 含空值")
        v2_errors, v2_warnings, has_ref = _check_v2_row(
            r, where, strict_refs=strict_refs, decisions=decisions, rows_by_id=rows_by_id
        )
        errors += v2_errors
        warnings += v2_warnings
        if stats is not None:
            if "refs" in r:
                stats["v2_rows"] = stats.get("v2_rows", 0) + 1
            if r.get("status") == "open" and _row_in_scope(r, doc, scope):
                stats["open_total"] = stats.get("open_total", 0) + 1
                kind = _ref_class(r)
                kd = stats.setdefault("by_kind", {}).setdefault(kind, [0, 0])
                kd[0] += 1
                if has_ref:
                    stats["open_with_refs"] = stats.get("open_with_refs", 0) + 1
                    kd[1] += 1
                else:
                    stats.setdefault("no_ref_ids", []).append(str(r.get("id")))
        if r.get("status") == "open" and _row_in_scope(r, doc, scope):
            open_count += 1
    return errors, warnings, open_count, blocked_count


_DOC_UNIT_CACHE: dict[str, set[str] | None] = {}
KIND_WORDS = ("fn", "struct", "enum", "trait", "const", "static", "type", "mod")


def _as_list(value) -> list:
    if value is None:
        return []
    return value if isinstance(value, list) else [value]


def _strip_comments(text: str) -> str:
    text = re.sub(r"/\*.*?\*/", "", text, flags=re.S)
    return re.sub(r"//[^\n]*", "", text)


def _rg_files(pattern: str, base: str) -> list[str]:
    try:
        proc = subprocess.run(["rg", "-l", pattern, base], cwd=ROOT, capture_output=True, text=True)
    except FileNotFoundError:
        proc = subprocess.run(["grep", "-rlE", pattern, base], cwd=ROOT, capture_output=True, text=True)
    return [x for x in proc.stdout.splitlines() if x.strip()]


def _split_code_ref(value):
    if isinstance(value, dict):
        return str(value.get("path", "")).strip(), (str(value.get("symbol")).strip() if value.get("symbol") else None)
    text = str(value).strip()
    m = re.match(r"^(.+?\.(?:rs|c|h|S|asm))(?:\s*:\s*(.+))?$", text)
    if not m:
        return text, None
    return m.group(1), (m.group(2).strip() if m.group(2) else None)


def _split_doc_ref(value):
    if isinstance(value, dict):
        return str(value.get("path", "")).strip(), (str(value.get("unit")).strip() if value.get("unit") else None)
    text = str(value).strip()
    m = re.match(r"^(.+?\.md)(?:\s*[#:]\s*(?:ch\s*)?(\d+(?:\.\d+)*))?$", text)
    if not m:
        return text, None
    return m.group(1), (m.group(2) if m.group(2) else None)


def _repo_path(path_text: str) -> Path | None:
    p = Path(path_text)
    if p.is_absolute():
        return None
    resolved = (ROOT / p).resolve()
    try:
        resolved.relative_to(ROOT.resolve())
    except ValueError:
        return None
    return resolved


def _valid_date(value) -> bool:
    try:
        date.fromisoformat(str(value))
        return True
    except ValueError:
        return False


def _doc_units(path: Path) -> set[str] | None:
    key = str(path)
    if key in _DOC_UNIT_CACHE:
        return _DOC_UNIT_CACHE[key]
    if not path.is_file():
        _DOC_UNIT_CACHE[key] = None
        return None
    units: set[str] = set()
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        m = re.match(r"^#{2,3}\s+(\d+(?:\.\d+)*)\.?\s", line)
        if m:
            units.add(m.group(1))
    _DOC_UNIT_CACHE[key] = units
    return units


def _symbol_exists(path: Path, symbol: str) -> bool:
    if not path.is_file():
        return False
    text = _strip_comments(path.read_text(encoding="utf-8", errors="replace"))
    m = re.match(r"^(fn|struct|enum|trait|const|static|type|mod)\s+([A-Za-z_]\w*)$", symbol.strip())
    if m:
        return re.search(rf"\b{m.group(1)}\s+{re.escape(m.group(2))}\b", text) is not None
    name = symbol.strip()
    if not re.match(r"^[A-Za-z_]\w*$", name):
        return False
    return re.search(rf"\b{re.escape(name)}\s*[(:]", text) is not None


def load_decision_ids() -> set[str] | None:
    path = ROOT / "rewrite-notes/meta/coordination/review-eval/DECISIONS.md"
    if not path.is_file():
        return None
    ids: set[str] = set()
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        m = re.match(r"^##\s+(D-\d{3})\b", line) or re.match(r"^\|\s*(D-\d{3})\b", line)
        if m:
            ids.add(m.group(1))
    return ids


def _check_provenance(r: dict, where: str, decisions: set[str] | None) -> list[str]:
    errors: list[str] = []
    status = r.get("status")
    if status == "historical":
        oc = str(r.get("origin_commit", ""))
        of = str(r.get("origin_file", ""))
        hr = str(r.get("historical_reason", ""))
        ok = bool(COMMIT_RE.match(oc)) if oc else False
        ok = ok or (bool(of) and (ROOT / of).is_file())
        ok = ok or bool(str(r.get("legacy_status", "")).strip())
        ok = ok or bool(hr and (re.search(r"\b[0-9a-f]{7,40}\b", hr) or re.search(r"\bD-\d{3}\b", hr)))
        if not ok:
            errors.append(f"{where}: historical 必须有可用来源（origin_commit / 存在的 origin_file / "
                          "legacy_status / 含 commit 或 D-xxx 的 historical_reason）")
    if status == "rejected":
        ids = re.findall(r"D-\d{3}", str(r.get("evidence", "")))
        if not ids:
            errors.append(f"{where}: rejected 必须引用用户裁决 D-xxx")
        elif decisions is not None and not any(d in decisions for d in ids):
            errors.append(f"{where}: rejected 引用的判决行不存在：{', '.join(ids)}")
    return errors


def _ref_class(r: dict) -> str:
    if r.get("stage") == "coordination" or r.get("scope") == "stage":
        return "coordination"
    return str(r.get("kind", "?"))


def _strict_requirement(r: dict) -> str:
    if r.get("stage") == "coordination" or r.get("scope") == "stage":
        return "decision 或 blocked_reason"
    if r.get("kind") in ("design", "arch"):
        return "docs#unit 或 decision"
    return "symbol/unit/test/decision"


def _check_v2_row(
    r: dict,
    where: str,
    *,
    strict_refs: bool,
    decisions: set[str] | None,
    rows_by_id: dict[str, dict] | None,
) -> tuple[list[str], list[str], bool]:
    errors: list[str] = []
    warnings: list[str] = []
    rid = str(r.get("id", ""))
    status = r.get("status")
    refs = r.get("refs")
    resolved = 0
    strong = 0
    errors += _check_provenance(r, where, decisions)
    if refs is None:
        if strict_refs and status in ("open",):
            if (r.get("stage") == "coordination" or r.get("scope") == "stage") and str(r.get("blocked_reason", "")).strip():
                return errors, warnings, True
            errors.append(f"{where}: open 项缺少 {_strict_requirement(r)}")
        return errors, warnings, False
    if not isinstance(refs, dict):
        errors.append(f"{where}: refs 必须是映射（code/docs/tests/decisions）")
        return errors, warnings, False

    for raw in _as_list(refs.get("code")):
        path_text, symbol = _split_code_ref(raw)
        path = _repo_path(path_text)
        if path is None or not path.is_file():
            msg = f"{where}: refs.code 文件不存在或在仓库外：{path_text}"
            (errors if status == "open" else warnings).append(msg)
            continue
        if symbol and not _symbol_exists(path, symbol):
            msg = f"{where}: refs.code 符号未解析：{path_text}:{symbol}"
            (errors if status == "open" else warnings).append(msg)
            continue
        resolved += 1
        if symbol:
            strong += 1

    for raw in _as_list(refs.get("docs")):
        path_text, unit = _split_doc_ref(raw)
        path = _repo_path(path_text)
        if path is None or not path.is_file():
            msg = f"{where}: refs.docs 文件不存在或在仓库外：{path_text}"
            (errors if status == "open" else warnings).append(msg)
            continue
        if unit:
            units = _doc_units(path)
            if units is None or unit not in units:
                msg = f"{where}: refs.docs 单元不存在：{path_text}#{unit}"
                (errors if status == "open" else warnings).append(msg)
                continue
            strong += 1
        resolved += 1

    for raw in _as_list(refs.get("tests")):
        name = str(raw).strip()
        if not re.match(r"^[A-Za-z_]\w*$", name):
            errors.append(f"{where}: refs.tests 名称非法：{name}")
            continue
        hit = False
        for rel in _rg_files(rf"\bfn\s+{re.escape(name)}\b", "os") + _rg_files(rf"\bfn\s+{re.escape(name)}\b", "rewrite-notes"):
            fp = ROOT / rel
            if not fp.is_file():
                continue
            clean = _strip_comments(fp.read_text(encoding="utf-8", errors="replace"))
            m = re.search(rf"\bfn\s+{re.escape(name)}\b", clean)
            if not m:
                continue
            prefix = clean[:m.start()]
            if "#[test]" in prefix or "#[cfg(test)]" in prefix:
                hit = True
                break
        if hit:
            resolved += 1
            strong += 1
        else:
            (errors if status == "open" else warnings).append(
                f"{where}: refs.tests 未命中 #[test] fn {name}")

    for raw in _as_list(refs.get("decisions")):
        did = str(raw).strip()
        if not re.fullmatch(r"D-\d{3}", did):
            errors.append(f"{where}: refs.decisions 格式非法：{did}")
            continue
        if decisions is not None and did not in decisions:
            errors.append(f"{where}: refs.decisions 指向不存在的判决行：{did}")
            continue
        resolved += 1
        strong += 1

    rel = r.get("relations")
    if rel is not None:
        if not isinstance(rel, dict):
            errors.append(f"{where}: relations 必须是映射（blocked_by/duplicates/supersedes）")
        else:
            for key in ("blocked_by", "duplicates", "supersedes"):
                for target in _as_list(rel.get(key)):
                    tid = str(target).strip()
                    if not tid:
                        errors.append(f"{where}: relations.{key} 含空值")
                    elif tid == rid:
                        errors.append(f"{where}: relations.{key} 不能指向自己")
                    elif rows_by_id is not None and tid not in rows_by_id:
                        errors.append(f"{where}: relations.{key} 指向不存在的 ID：{tid}")

    lc = r.get("lifecycle")
    if lc is not None:
        if not isinstance(lc, dict):
            errors.append(f"{where}: lifecycle 必须是映射")
        else:
            for key in ("created", "updated"):
                if lc.get(key) is not None and not _valid_date(lc.get(key)):
                    errors.append(f"{where}: lifecycle.{key} 必须是合法日期 YYYY-MM-DD")
            cb = lc.get("closed_by")
            if cb is not None and status != "closed":
                errors.append(f"{where}: lifecycle.closed_by 只能出现在 closed 行")
            if cb is not None:
                if not re.fullmatch(r"D-\d{3}", str(cb)):
                    errors.append(f"{where}: lifecycle.closed_by 必须是 D-xxx")
                elif decisions is not None and str(cb) not in decisions:
                    errors.append(f"{where}: lifecycle.closed_by 指向不存在的判决行：{cb}")

    if strict_refs and status == "open" and strong == 0:
        if (r.get("stage") == "coordination" or r.get("scope") == "stage") and str(r.get("blocked_reason", "")).strip():
            strong = 1
        else:
            errors.append(f"{where}: open 项只有弱 ref；strict 要求 {_strict_requirement(r)}")
    return errors, warnings, (strong > 0 if strict_refs else resolved > 0)


def _check_relation_cycles(rows_by_id: dict[str, dict]) -> list[str]:
    errors: list[str] = []
    for key in ("blocked_by", "supersedes", "duplicates"):
        graph: dict[str, list[str]] = {}
        for rid, r in rows_by_id.items():
            rel = r.get("relations") or {}
            graph[rid] = [str(x) for x in _as_list(rel.get(key)) if str(x).strip()]
        state: dict[str, int] = {}
        stack: list[str] = []

        def walk(node: str) -> list[str] | None:
            state[node] = 1
            stack.append(node)
            for nxt in graph.get(node, []):
                if nxt not in graph:
                    continue
                if state.get(nxt) == 1:
                    return stack[stack.index(nxt):] + [nxt]
                if state.get(nxt, 0) == 0:
                    found = walk(nxt)
                    if found:
                        return found
            stack.pop()
            state[node] = 2
            return None

        for node in graph:
            if state.get(node, 0) == 0:
                found = walk(node)
                if found:
                    # duplicates 的 A<->B 对称是正常形态，只报 >=3 节点的环
                    if key == "duplicates" and len(found) == 3:
                        continue
                    errors.append(f"relations.{key} 存在环路：{' -> '.join(found)}")
    return errors


def cmd_check(args) -> int:
    files = sorted(LEDGER_DIR.glob("*.yaml"))
    if not files:
        print(f"todo-ledger: {LEDGER_DIR} 下没有 .yaml")
        return 2
    all_rows: list[tuple[Path, list[dict]]] = [(f, load(f)) for f in files]
    all_ids = {str(r.get("id")) for _, rows in all_rows for r in rows}
    rows_by_id = {str(r.get("id")): r for _, rows in all_rows for r in rows if r.get("id")}
    decisions = load_decision_ids()
    stats = {"open_total": 0, "open_with_refs": 0, "v2_rows": 0}
    global_seen: set[str] = set()
    all_errors: list[str] = []
    all_warnings: list[str] = []
    all_open = 0
    all_blocked = 0
    matched = 0
    for f, rows in all_rows:
        errors, warnings, open_count, blocked_count = check_rows(
            rows, args.doc, all_ids, global_seen,
            strict_refs=getattr(args, "strict_refs", False),
            decisions=decisions, rows_by_id=rows_by_id, stats=stats,
            scope=getattr(args, "scope", "doc"),
        )
        all_errors += [f"{f.name}: {e}" for e in errors]
        all_warnings += [f"{f.name}: {w}" for w in warnings]
        all_open += open_count
        all_blocked += blocked_count
        matched += sum(1 for r in rows if _row_in_scope(r, args.doc, getattr(args, 'scope', 'doc')))
        print(f"[账本] {f.name}: {len(rows)} 条，open={open_count}，blocked={blocked_count}"
              + (f"（doc={args.doc}）" if args.doc else ""))
    all_errors += _check_relation_cycles(rows_by_id)
    for w in all_warnings:
        print(f"[WARN] {w}")
    for e in all_errors:
        print(f"[FAIL] {e}")
    if args.doc and getattr(args, "scope", "doc") == "doc" and matched == 0:
        print(f"todo-ledger check: --doc `{args.doc}` 匹配 0 行（doc 名或路径写错？）")
        return 1
    ot = stats.get("open_total", 0)
    ow = stats.get("open_with_refs", 0)
    pct = (100.0 * ow / ot) if ot else 100.0
    print(f"[覆盖] open={ot}，带可解析 ref={ow}（{pct:.1f}%），v2 行={stats.get('v2_rows', 0)}"
          + ("（--strict-refs）" if getattr(args, "strict_refs", False) else ""))
    bk = stats.get("by_kind", {})
    if bk:
        tag = "strong" if getattr(args, "strict_refs", False) else "weak"
        print(f"[覆盖:class:{tag}] " + "，".join(
            f"{k} {v[1]}/{v[0]}" for k, v in sorted(bk.items())))
    if getattr(args, "strict_refs", False) and ow < ot:
        missing = stats.get("no_ref_ids", [])
        print(f"todo-ledger check: {ot - ow} 条 open 缺 refs：{', '.join(missing[:12])}"
              + (" …" if len(missing) > 12 else ""))
        return 1
    if all_errors:
        print(f"todo-ledger check: {len(all_errors)} 处错误")
        return 1
    if args.fail_on_open and all_blocked:
        print(f"todo-ledger check: 当前范围有 {all_blocked} 条 blocked 待裁项——"
              "绝不允许收敛（即使代裁授权也不得自行决定）")
        return 1
    if args.fail_on_open and all_open:
        print(f"todo-ledger check: 当前范围仍有 {all_open} 条 open（零欠账要求 0）")
        return 1
    print("todo-ledger check: OK")
    return 0


def cmd_list(args) -> int:
    for f in sorted(LEDGER_DIR.glob("*.yaml")):
        for r in load(f):
            if not _row_matches_doc(r, args.doc):
                continue
            print(f"{r['id']:22} {r['status']:14} {r['severity']:3} {r['kind']:7} {r['title'][:70]}")
    return 0


def _section_open(title: str, body: str) -> bool:
    head = title + "\n" + body[:400]
    return bool(OPEN_MARKERS.search(head)) and not (
        CLOSED_MARKERS.search(title) and not OPEN_MARKERS.search(title)
    )


def _merge(out: Path, new_rows: list[dict]) -> list[dict]:
    rows = load(out) if out.is_file() else []
    by_id = {r["id"]: r for r in rows}
    for r in new_rows:
        by_id.setdefault(r["id"], r)
    return sorted(by_id.values(), key=lambda r: (r["stage"], r["doc"], r["id"]))


def cmd_import_md(args) -> int:
    text = Path(args.file).read_text(encoding="utf-8")
    parts = re.split(r"^###\s+", text, flags=re.MULTILINE)[1:]
    new_rows: list[dict] = []
    for part in parts:
        title, _, body = part.partition("\n")
        title = title.strip()
        if not _section_open(title, body):
            continue
        m = ID_RE.search(title)
        rid = m.group(1) if m else re.sub(r"[^A-Za-z0-9]+", "-", title)[:40]
        sev = SEV_RE.search(body) or SEV_RE.search(title)
        origin = ORIGIN_RE.search(body)
        new_rows.append({
            "id": f"{args.prefix}{rid}" if args.prefix else rid,
            "stage": args.stage,
            "doc": args.doc,
            "title": re.sub(r"（[^）]*）|\([^)]*\)", "", title).strip()[:160],
            "status": "open",
            "severity": f"P{sev.group(1)}" if sev else "P2",
            "kind": "design" if "设计" in body[:300] else "code",
            "origin": (origin.group(1).strip() if origin else ""),
            "evidence": "",
        })
    out = Path(args.out)
    rows = _merge(out, new_rows)
    save(out, rows)
    print(f"import-md: 从 {args.file} 迁入 {len(new_rows)} 条 open；账本共 {len(rows)} 条 -> {out}")
    return 0


def cmd_import_suggestions(args) -> int:
    text = Path(args.file).read_text(encoding="utf-8")
    new_rows: list[dict] = []
    for line in text.splitlines():
        if not line.strip().startswith("|"):
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) < 2 or "不实施" not in " ".join(cells):
            continue
        rid = cells[0].strip("`* ")
        if not re.match(r"[A-Za-z]", rid):
            continue
        new_rows.append({
            "id": f"{args.prefix}{rid}" if args.prefix else rid,
            "stage": args.stage,
            "doc": args.doc,
            "title": f"面板建议：{rid}",
            "status": "open",
            "severity": "P3",
            "kind": "design",
            "origin": args.file,
            "evidence": "",
        })
    out = Path(args.out)
    rows = _merge(out, new_rows)
    save(out, rows)
    print(f"import-suggestions: 迁入 {len(new_rows)} 条；账本共 {len(rows)} 条 -> {out}")
    return 0


def _iter_rows():
    for f in sorted(LEDGER_DIR.glob("*.yaml")):
        yield f, load(f)


def _find_row(rid: str):
    for f, rows in _iter_rows():
        for r in rows:
            if str(r.get("id")) == rid:
                return f, rows, r
    return None


def cmd_query(args) -> int:
    n = 0
    for _f, rows in _iter_rows():
        for r in rows:
            refs = r.get("refs") or {}
            code = [_split_code_ref(x) for x in _as_list(refs.get("code"))]
            docs = [_split_doc_ref(x) for x in _as_list(refs.get("docs"))]
            if args.file:
                want = Path(args.file).as_posix()
                if not any(Path(p).as_posix() == want for p, _ in code + docs):
                    continue
            if args.symbol:
                if not any(s == args.symbol for _, s in code):
                    continue
            if args.doc_unit and not any(u == args.doc_unit for _, u in docs):
                continue
            if args.status == "all":
                pass
            elif args.status:
                if r.get("status") != args.status:
                    continue
            elif r.get("status") != "open":
                continue
            if args.stage and r.get("stage") != args.stage:
                continue
            print(f"{r['id']:22} {r['status']:14} {r['severity']:3} {r['kind']:7} {r['title'][:70]}")
            n += 1
    print(f"# {n} 条", file=sys.stderr)
    return 0


def cmd_link(args) -> int:
    hit = _find_row(args.id)
    if not hit:
        print(f"link: 找不到 TODO {args.id}", file=sys.stderr)
        return 1
    f, rows, r = hit
    refs = r.setdefault("refs", {})
    for key in ("code", "docs", "tests", "decisions"):
        refs.setdefault(key, [])
    for value, key in ((args.code, "code"), (args.doc, "docs"),
                       (args.test, "tests"), (args.decision, "decisions")):
        for item in value or []:
            if item not in refs[key]:
                refs[key].append(item)
    save(f, rows)
    print(f"link: {args.id} 已更新 -> {f.name}")
    return 0


def cmd_close(args) -> int:
    hit = _find_row(args.id)
    if not hit:
        print(f"close: 找不到 TODO {args.id}", file=sys.stderr)
        return 1
    f, rows, r = hit
    if r.get("status") == "closed":
        print(f"close: {args.id} 已是 closed；不覆盖既有 evidence（确需改请手工编辑）", file=sys.stderr)
        return 1
    if r.get("blocked"):
        print(f"close: {args.id} 是 blocked 待裁项；先解除 blocked 再关闭", file=sys.stderr)
        return 1
    r["status"] = "closed"
    r["evidence"] = args.evidence
    lc = r.setdefault("lifecycle", {})
    lc["updated"] = date.today().isoformat()
    if args.by:
        lc["closed_by"] = args.by
    save(f, rows)
    print(f"close: {args.id} 已关闭 -> {f.name}")
    return 0


def _infer_refs(r: dict) -> dict:
    refs = {"code": [], "docs": [], "tests": [], "decisions": []}
    of = r.get("origin_file")
    if of and (ROOT / str(of)).is_file():
        if str(of).endswith((".rs", ".c", ".h", ".sh", ".py")):
            refs["code"].append({"path": str(of)})
        elif str(of).endswith(".md"):
            refs["docs"].append({"path": str(of)})
    doc = r.get("doc")
    if doc:
        text_doc = str(doc)
        cands = [text_doc] if text_doc.endswith(".md") else [
            str(p.relative_to(ROOT)) for p in ROOT.glob(f"rewrite-notes/*/{text_doc}.md")
        ]
        doc_path = None
        for cand in cands[:1]:
            if (ROOT / cand).is_file():
                doc_path = cand
                refs["docs"].append({"path": cand})
    text = " ".join(str(r.get(k, "")) for k in ("origin", "evidence", "note", "title"))
    if doc_path:
        units = _doc_units(ROOT / doc_path) or set()
        for m in re.finditer(r"§\s*(\d+(?:\.\d+)*)", text):
            unit = m.group(1)
            if unit in units:
                refs["docs"].append({"path": doc_path, "unit": unit})
    for m in re.finditer(r"((?:os|rewrite-notes|prompt|tools|minix3)/[\w./-]+\.(?:rs|md|c|h|sh|py))", text):
        path = m.group(1)
        if not (ROOT / path).is_file():
            continue
        tail = text[m.end():m.end() + 80]
        sm = re.match(r"\s*:\s*((?:%s)\s+[A-Za-z_]\w*|[A-Za-z_]\w*)" % "|".join(KIND_WORDS), tail)
        if path.endswith((".rs", ".c", ".h", ".sh", ".py")):
            entry = {"path": path}
            if sm:
                entry["symbol"] = sm.group(1)
            refs["code"].append(entry)
        elif path.endswith(".md"):
            refs["docs"].append({"path": path})
    for m in re.finditer(r"\btest_[a-z0-9_]+\b", text):
        refs["tests"].append(m.group(0))
    for m in re.finditer(r"\bD-\d{3}\b", text):
        refs["decisions"].append(m.group(0))
    for key in refs:
        seen = []
        for item in refs[key]:
            if item not in seen:
                seen.append(item)
        refs[key] = seen
    return refs


def cmd_migrate_v2(args) -> int:
    targets = [LEDGER_DIR / name for name in args.file] if args.file else sorted(LEDGER_DIR.glob("*.yaml"))
    for f in targets:
        rows = load(f)
        changed = 0
        for r in rows:
            inferred = _infer_refs(r)
            if not any(inferred.values()):
                continue
            cur = r.get("refs") or {}
            added = 0
            for key in ("code", "docs", "tests", "decisions"):
                cur.setdefault(key, [])
                for item in inferred[key]:
                    if item not in cur[key]:
                        cur[key].append(item)
                        added += 1
            if added:
                r["refs"] = cur
                changed += 1
        print(f"[migrate-v2] {f.name}: {changed}/{len(rows)} 行补 refs"
              + ("" if args.write else "（dry-run，未写）"))
        if args.write and changed:
            save(f, rows)
    return 0


def self_test() -> int:
    import tempfile

    good = [
        {"id": "D01-I-31", "stage": "01-stage-kernel", "doc": "01", "title": "t",
         "status": "closed", "severity": "P1", "kind": "code",
         "evidence": "abc1234 path:1"},
        {"id": "D01-I-32", "stage": "01-stage-kernel", "doc": "01", "title": "t2",
         "status": "defer-approved", "severity": "P2", "kind": "code",
         "defer": {"decision": "D-107", "approved": "2026-11-21",
                   "until": "出现第三调用者", "consequence": "…"}},
        {"id": "D01-ARCH-1", "stage": "01-stage-kernel", "doc": "01-todo",
         "title": "archive", "status": "historical", "severity": "P3", "kind": "doc",
         "legacy_status": "V11 archive", "origin_file": "rewrite-notes/MIGRATION.md",
         "origin_line": 1, "aliases": ["V11-1", "P0-1"]},
        {"id": "D01-DUP-1", "stage": "01-stage-kernel", "doc": "01", "title": "dup",
         "status": "closed", "severity": "P3", "kind": "doc",
         "evidence": "dup-of D01-I-31", "merged_into": "D01-I-31"},
    ]
    ids = {r["id"] for r in good}
    errors, _, _, _ = check_rows(good, None, ids)
    if errors:
        print(f"self-test FAIL: 合规账本被判错 {errors}")
        return 1
    bad = [
        {"id": "X-1", "stage": "s", "doc": "d", "title": "t", "status": "closed",
         "severity": "P1", "kind": "code"},
        {"id": "X-1", "stage": "s", "doc": "d", "title": "dup", "status": "open",
         "severity": "P9", "kind": "unknown"},
        {"id": "X-2", "stage": "s", "doc": "d", "title": "t", "status": "defer-approved",
         "severity": "P2", "kind": "code", "defer": {"decision": "D-1"}},
        {"id": "X-3", "stage": "s", "doc": "d", "title": "m", "status": "open",
         "severity": "P2", "kind": "code", "merged_into": "NOPE"},
        {"id": "X-4", "stage": "s", "doc": "d", "title": "v", "status": "open",
         "severity": "P2", "kind": "code", "verified_by": "a"},
        {"id": "X-5", "stage": "s", "doc": "d", "title": "a", "status": "open",
         "severity": "P2", "kind": "code", "aliases": ["X-5"]},
        {"id": "X-6", "stage": "s", "doc": "d", "title": "o", "status": "open",
         "severity": "P2", "kind": "code", "origin_file": "no/such/file.md"},
    ]
    errors, _, open_count, _bc = check_rows(bad, "d", {r["id"] for r in bad})
    need = ["缺 evidence", "ID 重复", "severity 非法", "kind 非法", "defer-approved 缺",
            "merged_into 目标", "verified_by/verified_at 必须成对", "aliases 不能包含自身",
            "origin_file 不存在"]
    miss = [n for n in need if not any(n in e for e in errors)]
    if miss:
        print(f"self-test FAIL: 漏检 {miss}")
        return 1
    if open_count != 5:
        print(f"self-test FAIL: open 计数错（{open_count}，应 5）")
        return 1
    blocked = [{"id": "B-1", "stage": "s", "doc": "d", "title": "待裁",
                "status": "open", "severity": "P1", "kind": "arch",
                "blocked": True, "blocked_reason": "用户未定"}]
    e2, _, o2, b2 = check_rows(blocked, "d", {"B-1"})
    if e2 or o2 != 1 or b2 != 1:
        print(f"self-test FAIL: blocked 计数错（errors={e2} open={o2} blocked={b2}）")
        return 1
    e3, _, _, _ = check_rows(
        [{"id": "B-2", "stage": "s", "doc": "d", "title": "x", "status": "open",
          "severity": "P1", "kind": "arch", "blocked": True}], "d", {"B-2"})
    if not any("blocked_reason" in e for e in e3):
        print("self-test FAIL: blocked 缺理由未拦截")
        return 1
    # --doc 归一：路径/`.md` 与 basename 等价
    if canon_doc("rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md") != "01-boot-shim-bootstrap":
        print("self-test FAIL: canon_doc 未归一")
        return 1
    # ---- v2 refs/relations/lifecycle ----
    v2 = {
        "id": "V2-1", "stage": "01-stage-kernel", "doc": "01-boot-shim-bootstrap",
        "title": "v2", "status": "open", "severity": "P2", "kind": "code",
        "refs": {
            "code": [{"path": "os/kernel/src/lib.rs"}],
            "docs": [{"path": "rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md", "unit": "1"}],
            "tests": ["test_clamp_covers_eof_and_shortening"],
            "decisions": ["D-060"],
        },
        "relations": {"blocked_by": [], "duplicates": [], "supersedes": []},
        "lifecycle": {"created": "2026-11-21", "updated": "2026-11-21"},
    }
    e4, w4, has4 = _check_v2_row(v2, "#v2", strict_refs=True,
                                 decisions={"D-060"}, rows_by_id={"V2-1": v2})
    if e4 or not has4:
        print(f"self-test FAIL: 合规 v2 行被判错 {e4} {w4}")
        return 1
    bad_refs = {
        **v2,
        "refs": {
            "code": [{"path": "no/such.rs"}],
            "docs": [{"path": "rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md", "unit": "99.99"}],
            "tests": [],
            "decisions": ["D-999"],
        },
        "relations": {"blocked_by": ["NOPE"], "duplicates": [], "supersedes": []},
    }
    e5, _, _ = _check_v2_row(bad_refs, "#bad", strict_refs=True,
                             decisions={"D-060"}, rows_by_id={"V2-1": v2})
    need5 = ["文件不存在", "单元不存在", "不存在的判决行", "不存在的 ID"]
    miss5 = [n for n in need5 if not any(n in e for e in e5)]
    if miss5:
        print(f"self-test FAIL: v2 漏检 {miss5}")
        return 1
    no_refs = {"id": "V2-2", "stage": "s", "doc": "d", "title": "x",
               "status": "open", "severity": "P2", "kind": "code"}
    e6, _, has6 = _check_v2_row(no_refs, "#norefs", strict_refs=True,
                                decisions=set(), rows_by_id={"V2-2": no_refs})
    if not e6 or has6:
        print("self-test FAIL: strict-refs 未拦截无 refs 的 open 行")
        return 1
    cyc = {
        "A": {"id": "A", "relations": {"blocked_by": ["B"]}},
        "B": {"id": "B", "relations": {"blocked_by": ["A"]}},
    }
    if not _check_relation_cycles(cyc):
        print("self-test FAIL: blocked_by 环路未拦截")
        return 1
    design_ok = {"id": "D-1", "stage": "s", "doc": "d", "title": "x", "status": "open",
                 "severity": "P2", "kind": "design", "scope": "doc",
                 "refs": {"code": [], "docs": [{"path": "rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md", "unit": "1"}],
                          "tests": [], "decisions": []}}
    e7, _, s7 = _check_v2_row(design_ok, "#design", strict_refs=True, decisions={"D-060"}, rows_by_id={"D-1": design_ok})
    if e7 or not s7:
        print(f"self-test FAIL: design 的 docs#unit 未被接受 {e7}")
        return 1
    coord_ok = {"id": "C-1", "stage": "coordination", "doc": "d", "title": "x", "status": "open",
                "severity": "P2", "kind": "arch", "scope": "stage",
                "refs": {"code": [], "docs": [], "tests": [], "decisions": ["D-060"]}}
    e8, _, s8 = _check_v2_row(coord_ok, "#coord", strict_refs=True, decisions={"D-060"}, rows_by_id={"C-1": coord_ok})
    if e8 or not s8:
        print(f"self-test FAIL: coordination 的 decision 未被接受 {e8}")
        return 1
    coord_blocked = {"id": "C-2", "stage": "coordination", "doc": "d", "title": "x", "status": "open",
                     "severity": "P2", "kind": "arch", "scope": "stage", "blocked": True,
                     "blocked_reason": "等待用户裁决"}
    e9, _, s9 = _check_v2_row(coord_blocked, "#coord", strict_refs=True, decisions={"D-060"}, rows_by_id={"C-2": coord_blocked})
    if e9 or not s9:
        print(f"self-test FAIL: coordination 的 blocked_reason 未被接受 {e9}")
        return 1
    code_weak = {"id": "K-1", "stage": "s", "doc": "d", "title": "x", "status": "open",
                 "severity": "P2", "kind": "code", "scope": "doc",
                 "refs": {"code": [{"path": "os/kernel/src/lib.rs"}], "docs": [], "tests": [], "decisions": []}}
    e10, _, s10 = _check_v2_row(code_weak, "#code", strict_refs=True, decisions={"D-060"}, rows_by_id={"K-1": code_weak})
    if not e10 or s10:
        print("self-test FAIL: code 的 path-only ref 未被 strict 拦截")
        return 1
    print("self-test PASS: 字段/跨文件重复/closed 证据/rejected D 行/defer 四要素/"
          "blocked/merged_into/verified 成对/aliases/origin/doc 归一/v2 refs/relations/lifecycle 均正确")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description="TODO 单一账本（V3.7 / V3.8-r2）")
    sub = ap.add_subparsers(dest="cmd", required=False)
    c = sub.add_parser("check")
    c.add_argument("--doc")
    c.add_argument("--fail-on-open", action="store_true")
    c.add_argument("--strict-refs", action="store_true",
                   help="open 项必须至少有一个强 ref（symbol/unit/test/decision）；缺失即 FAIL")
    c.add_argument("--scope", choices=("doc", "stage", "coordination", "all"), default="doc",
                   help="统计范围：doc=当前篇（默认）/coordination=跨阶段账本/all=全部")
    l = sub.add_parser("list")
    l.add_argument("--doc")
    im = sub.add_parser("import-md")
    im.add_argument("file")
    im.add_argument("--stage", required=True)
    im.add_argument("--doc", required=True)
    im.add_argument("--out", required=True)
    im.add_argument("--prefix", default="")
    isg = sub.add_parser("import-suggestions")
    isg.add_argument("file")
    isg.add_argument("--stage", required=True)
    isg.add_argument("--doc", required=True)
    isg.add_argument("--out", required=True)
    isg.add_argument("--prefix", default="")
    q = sub.add_parser("query")
    q.add_argument("--file")
    q.add_argument("--symbol")
    q.add_argument("--doc-unit")
    q.add_argument("--status")
    q.add_argument("--stage")
    lk = sub.add_parser("link")
    lk.add_argument("id")
    lk.add_argument("--code", action="append")
    lk.add_argument("--doc", action="append")
    lk.add_argument("--test", action="append")
    lk.add_argument("--decision", action="append")
    cl = sub.add_parser("close")
    cl.add_argument("id")
    cl.add_argument("--evidence", required=True)
    cl.add_argument("--by")
    mv = sub.add_parser("migrate-v2")
    mv.add_argument("--write", action="store_true")
    mv.add_argument("--file", action="append")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        return self_test()
    if not args.cmd:
        ap.print_help()
        return 2
    if args.cmd == "check":
        return cmd_check(args)
    if args.cmd == "list":
        return cmd_list(args)
    if args.cmd == "import-md":
        return cmd_import_md(args)
    if args.cmd == "import-suggestions":
        return cmd_import_suggestions(args)
    if args.cmd == "query":
        return cmd_query(args)
    if args.cmd == "link":
        return cmd_link(args)
    if args.cmd == "close":
        return cmd_close(args)
    if args.cmd == "migrate-v2":
        return cmd_migrate_v2(args)
    return 2


if __name__ == "__main__":
    sys.exit(main())
