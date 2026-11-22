#!/usr/bin/env python3
"""v312-closure-check.py — V3.12 单轮关账检查（R1 full / ROUND-CLOSED）

不替代 eval-trace-check 的旧版全历史审计；V3.12 的账户、事件、单元冻结由
review-units.py 负责，本工具只检查“这一轮的记录是否自洽、可关账”：
  loop.md 的 V3.12 九节与 schema 标记；
  metrics.md 的 V3.12 键与轮结果；
  panel/round1 的角色报告（fact/reader-a/b/c/design-d2；高风险篇需 refute）；
  defect-ledger 对 panel FINDING 的逐 ID 覆盖与根因数；
  review-units check/audit/event verify 的当前读数。

用法：python3 tools/v312-closure-check.py <doc> [--round 1] [--run-dir DIR] [--ledger PATH]
退出码：0 通过；1 失败；2 用法错误。
"""
from __future__ import annotations
import argparse, re, subprocess, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
from notes_layout import REVIEW_EVAL_DIR, REVIEW_STATE_DIR  # noqa: E402

SECTIONS = ["§0 基线", "§1 面板", "§2 发现", "§3 归因", "§4 修复",
            "§5 复验", "§6 指标", "§7 状态", "§8 给 meta-review 的不确定点"]
METRIC_KEYS = ["发现总数", "唯一根因数", "本轮新准入", "certify 新准入",
               "重复合并", "自引入", "未闭环", "轮型", "轮结果", "edition",
               "ARCH 提案", "用户否决", "re-litigation", "adjacent 阻塞",
               "重构", "成本"]
ROLES = ["fact.md", "reader-a.md", "reader-b.md", "reader-c.md", "design-d2.md"]


def fail(errs: list[str], msg: str) -> None:
    errs.append(msg)
    print(f"[FAIL] {msg}")


def read(p: Path) -> str:
    return p.read_text(encoding="utf-8") if p.is_file() else ""


def panel_findings(panel: Path) -> set[str]:
    ids: set[str] = set()
    for f in sorted(panel.glob("*.md")):
        if f.name in ("INDEX.md", "refute.md"):
            continue
        for line in read(f).splitlines():
            m = re.match(r"^#{2,3}\s+FINDING\s+([A-Za-z][A-Za-z0-9_-]*)", line)
            if m:
                ids.add(m.group(1))
    return ids


def refute_findings(panel: Path) -> set[str]:
    ids: set[str] = set()
    f = panel / "refute.md"
    if not f.is_file():
        return ids
    for line in read(f).splitlines():
        m = re.match(r"^#{2,3}\s+FINDING\s+([A-Za-z][A-Za-z0-9_-]*)", line)
        if m:
            ids.add(m.group(1))
    return ids


def refute_ledger_ids(ledger: Path) -> set[str]:
    """只读 `## P-REFUTE 附加` 段，避免把普通缺陷行当反证回执。"""
    ids: set[str] = set()
    in_section = False
    for line in read(ledger).splitlines():
        if line.startswith("## "):
            in_section = "P-REFUTE 附加" in line
            continue
        if not in_section or not line.startswith("|") or line.startswith("|---"):
            continue
        cells = [c.strip() for c in line.strip("|").split("|")]
        if not cells or not cells[0] or "claim" in cells[0].lower():
            continue
        for tok in re.split(r"[、,，/\s]+", cells[0].strip("` ")):
            if re.fullmatch(r"REFUTE-\d+", tok):
                ids.add(tok)
    return ids


def ledger_ids(ledger: Path) -> tuple[set[str], set[str]]:
    ids: set[str] = set()
    roots: set[str] = set()
    for line in read(ledger).splitlines():
        if not line.startswith("|") or line.startswith("|---") or "发现 ID" in line:
            continue
        cells = [c.strip() for c in line.strip("|").split("|")]
        if not cells or not cells[0]:
            continue
        first = re.sub(r"[（(].*?[）)]", lambda m: "、" + m.group(0)[1:-1], cells[0])
        for tok in re.split(r"[、,，/\s]+", first):
            tok = tok.strip("` ")
            m = re.fullmatch(r"A-(\d+)", tok)
            if m:
                tok = f"READER-A-{int(m.group(1)):02d}"
            m = re.fullmatch(r"B-?(\d+)", tok)
            if m:
                tok = f"READER-B-{int(m.group(1)):02d}"
            m = re.fullmatch(r"C-(\d+)", tok)
            if m:
                tok = f"READER-C-{int(m.group(1)):02d}"
            if re.fullmatch(r"[A-Z][A-Z0-9]{1,}-[A-Z0-9-]+", tok):
                ids.add(tok)
        for cell in cells:
            if re.fullmatch(r"R\d+-\d+", cell):
                roots.add(cell)
                break
    return ids, roots


def metrics_block(text: str, round_n: int) -> str | None:
    m = re.search(rf"^##\s+R{round_n}\b.*?$(.*?)(?=^##\s|\Z)",
                  text, re.MULTILINE | re.DOTALL)
    return m.group(1) if m else None


def refute_ok(panel: Path, loop: str) -> bool:
    return (panel / "refute.md").is_file() or bool(
        re.search(r"P-REFUTE\s+VARIANCE\s*[:：]", loop))


def frozen_stats_ok(stats: str) -> bool:
    m = re.search(r"frozen=(\d+)/(\d+)", stats)
    if not m or m.group(1) != m.group(2):
        return False
    return all((x := re.search(rf"{key}=(\d+)", stats)) and int(x.group(1)) == 0
               for key in ("t1_pending", "t2_pending", "reopened", "stale"))


def self_test() -> int:
    import tempfile
    with tempfile.TemporaryDirectory() as td:
        panel = Path(td)
        (panel / "reader-a.md").write_text(
            "## FINDING READER-A-01（P1）\n## FINDING READER-A-02（P2）\n", encoding="utf-8")
        (panel / "design-d2.md").write_text("## FINDING D2-01（P1）\n", encoding="utf-8")
        ledger = panel / "ledger.md"
        ledger.write_text(
            "| 发现 ID | 单元 | 处置类 | 处置 | 复验 | 根因 ID | origin |\n"
            "|---|---|---|---|---|---|---|\n"
            "| B1/A-1/C-01/FACT-02/D2-01 | 1 | ① | x | y | R1-01 | R1 |\n",
            encoding="utf-8")
        ids, roots = ledger_ids(ledger)
        assert {"READER-B-01", "READER-A-01", "READER-C-01", "FACT-02", "D2-01"} <= ids, ids
        assert roots == {"R1-01"}, roots
        text = "## R0 账户初始化\n- 唯一根因数: 0\n\n## R1 full（V3.12）\n- 唯一根因数: 34\n"
        assert "34" in (metrics_block(text, 1) or ""), metrics_block(text, 1)
        assert "0" in (metrics_block(text, 0) or ""), metrics_block(text, 0)
        assert not refute_ok(panel, "进入 R2 前需补 P-REFUTE/VARIANCE。")
        assert refute_ok(panel, "P-REFUTE VARIANCE: 本轮不做，理由见 §8。")
        (panel / "refute.md").write_text("## FINDING REFUTE-01（P0）\n", encoding="utf-8")
        assert "REFUTE-01" not in panel_findings(panel)
        assert refute_findings(panel) == {"REFUTE-01"}
        assert refute_ledger_ids(ledger) == set()
        (panel / "refute.md").write_text("## FINDING REFUTE-01（P0）\n", encoding="utf-8")
        ledger.write_text(
            "| 发现 ID | 单元 | 处置类 | 处置 | 复验 | 根因 ID | origin |\n"
            "|---|---|---|---|---|---|---|\n"
            "## P-REFUTE 附加\n"
            "| claim ID | 结果 | 处置 |\n|---|---|---|\n"
            "| REFUTE-01 | 成立 | 已修 |\n",
            encoding="utf-8")
        assert refute_ledger_ids(ledger) == {"REFUTE-01"}
        assert not frozen_stats_ok("frozen=37/43 t1_pending=0 t2_pending=3 reopened=0 stale=0")
        assert frozen_stats_ok("frozen=43/43 t1_pending=0 t2_pending=0 reopened=0 stale=0")
    print("self-test PASS: slash IDs/缩写展开、R1 指标作用域、refute 严格标记、全冻结读数")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description="V3.12 单轮关账检查")
    ap.add_argument("doc", nargs="?")
    ap.add_argument("--round", type=int, default=1)
    ap.add_argument("--run-dir", default=None)
    ap.add_argument("--ledger", default=None)
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        return self_test()
    if not args.doc:
        ap.print_help()
        return 2
    doc = Path(args.doc)
    if not doc.is_file():
        print(f"文档不存在：{doc}", file=sys.stderr); return 2
    stage = next((p for p in doc.parts if re.fullmatch(r"\d{2}-stage-[^/]+", p)), "")
    if not stage:
        print("无法识别 stage", file=sys.stderr); return 2
    run = Path(args.run_dir) if args.run_dir else REVIEW_EVAL_DIR / "runs" / stage / doc.stem
    ledger = Path(args.ledger) if args.ledger else run / "defect-ledger.md"
    errs: list[str] = []

    loop = read(run / "loop.md")
    if f"schema: V3.12 from R{args.round}" not in loop:
        fail(errs, f"loop.md 缺 `schema: V3.12 from R{args.round}`")
    for s in SECTIONS:
        if s not in loop:
            fail(errs, f"loop.md 缺 `{s}`")
    metrics = read(run / "metrics.md")
    block = metrics_block(metrics, args.round)
    if block is None:
        fail(errs, f"metrics.md 缺 `## R{args.round}` 轮次小节")
        block = ""
    for k in METRIC_KEYS:
        if not re.search(rf"^-\s*{re.escape(k)}\s*[:：]", block, re.MULTILINE):
            fail(errs, f"metrics.md R{args.round} 缺 `- {k}:`")
    if "ROUND-CLOSED" not in block:
        fail(errs, f"metrics.md R{args.round} 未写 `ROUND-CLOSED`")
    # panel roles (round subdir)
    panel = run / "panel" / f"round{args.round}"
    missing = [r for r in ROLES if not (panel / r).is_file()]
    if missing:
        fail(errs, f"panel/round{args.round} 缺角色报告：{', '.join(missing)}")
    if not refute_ok(panel, loop):
        fail(errs, f"panel/round{args.round} 缺 refute.md（高风险篇）且 loop 无 `P-REFUTE VARIANCE:` 记录")
    pids = panel_findings(panel)
    lids, roots = ledger_ids(ledger)
    mt = re.search(r"^-\s*发现总数\s*[:：]\s*(\d+)", block, re.MULTILINE)
    if mt and int(mt.group(1)) != len(pids):
        fail(errs, f"metrics 发现总数 {mt.group(1)} != panel FINDING 数 {len(pids)}")
    missing_ids = sorted(pids - lids)
    if missing_ids:
        fail(errs, f"ledger 未覆盖 panel FINDING：{', '.join(missing_ids[:8])}…" if len(missing_ids) > 8 else f"ledger 未覆盖 panel FINDING：{', '.join(missing_ids)}")
    rids = refute_findings(panel)
    if rids:
        rlids = refute_ledger_ids(ledger)
        rmissing = sorted(rids - rlids)
        if rmissing:
            fail(errs, f"ledger `## P-REFUTE 附加` 未覆盖：{', '.join(rmissing)}")
    m = re.search(r"^-\s*唯一根因数\s*[:：]\s*(\d+)", block, re.MULTILINE)
    if m and int(m.group(1)) != len(roots):
        fail(errs, f"metrics 唯一根因数 {m.group(1)} != ledger 根因 ID {len(roots)}")
    # state/events
    cmds = (["check", "--doc", str(doc)],
            ["audit", "--doc", str(doc), "--ledger", str(ledger)],
            ["event", "verify", str(doc), "--fresh"])
    for cmd in cmds:
        r = subprocess.run([sys.executable, str(ROOT / "tools/review-units.py"), *cmd],
                           capture_output=True, text=True)
        if r.returncode != 0:
            fail(errs, f"review-units {cmd[0]} 失败：{(r.stderr or r.stdout).strip()[:200]}")
    st = subprocess.run([sys.executable, str(ROOT / "tools/review-units.py"),
                         "stats", str(doc)], capture_output=True, text=True)
    if not frozen_stats_ok(st.stdout):
        for key in ("frozen", "t1_pending", "t2_pending", "reopened", "stale"):
            m = re.search(rf"{key}=([\d/]+)", st.stdout)
            if key == "frozen":
                fm = re.search(r"frozen=(\d+)/(\d+)", st.stdout)
                if not fm or fm.group(1) != fm.group(2):
                    fail(errs, f"单元未全部冻结：{fm.group(0) if fm else '无读数'}")
            elif not m or int(m.group(1)) != 0:
                fail(errs, f"{key}={m.group(1) if m else '无读数'}（关账要求 0）")
    if errs:
        print(f"v312-closure-check: FAIL({len(errs)})")
        return 1
    print("v312-closure-check: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
