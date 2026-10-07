#!/usr/bin/env bash
# lint-review-rules.sh — Review 规则集自检（2026-09-05 P6 落地）
#
# 把 2026-09-05 清淤时人工审计的项目固化为自动化检查，防止规则集再次腐化：
#   L1. 指向已删除目录的"活引用"检查（tmp_design_and_todo 必须带删除标注或为守卫规则）
#   L2. 旧 stage 目录名检查（03-stage-kernel → 01-stage-kernel，历史 .review/ 路径豁免）
#   L3. 残缺路径检查（prompt/../ 占位符残留）
#   L4. tools/ 下 .bak 备份文件检查
#   L5. 模式编号序列检查（无重复；断号仅限已记录的 61/62；总数与索引表宣称一致）
#   L6. Gate 定义唯一性（每个 Gate id 只有一个权威定义标题）
#   L7. cmd 薄壳 SKILL.md frontmatter（name=目录名、description ≤1024、双引号包裹）
#   L8. .agents/skills/ 软链解析检查
#   L9. 新增规则元规则锚点存在性（review-process.md 必含注册表与元规则）
#
# 用法：tools/lint-review-rules.sh          # 全部检查
#       tools/lint-review-rules.sh --check  # 同上（保留参数以兼容 CI 习惯）

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

errors=0
fail() { printf 'FAIL: %s\n' "$1" >&2; errors=$((errors + 1)); }
ok()   { printf '  OK: %s\n' "$1"; }

# ── 扫描域（2026-10-08 改造）：只扫受版本管理的规则与入口文件 ──
# 为什么不递归扫目录：独立审阅方会把审阅工作区（报告 + 捕获的门输出）写进 prompt/<agent>/。
# 未跟踪的第三方产物不该让门变红；更糟的是「门输出被证据文件捕获后再被门读到」会让命中清单
# 级联增长——三方审计在同一轮里全部撞上（清单从 3 处涨到 25 处，且互相引用对方的行号）。
# 语义上也更正确：本门守护的是「入库的规则内容」，判断基准应当是版本库里的文件。
LINT_ROOTS=(prompt .claude .codex .trae .agents AGENTS.md README.md)
LINT_TRACKED_FILES=${LINT_TRACKED_FILES:-$(git ls-files -- "${LINT_ROOTS[@]}" 2>/dev/null \
  | grep -E '\.(md|sh|py|json)$' | while read -r f; do [[ -f "$f" ]] && printf '%s\n' "$f"; done)}
export LINT_TRACKED_FILES
LINT_FILES=$(printf '%s\n' "$LINT_TRACKED_FILES" | tr '\n' ' ')
LINT_SELFTEST=${LINT_SELFTEST:-false}
echo "  扫描域 = 受版本管理的规则/入口文件 $(printf '%s\n' "$LINT_TRACKED_FILES" | grep -c .) 个（未跟踪的审阅工作区不在内）"


if [[ "${1:-}" == "--self-test" ]]; then
  ST="tools/.lint-rules-selftest"; rm -rf "$ST"; mkdir -p "$ST"
  trap 'rm -rf "$ST"' EXIT
  printf '引用 tools/ghost-probe-x.sh 完成该项检查。\n'            > "$ST/bad-ghost.md"
  printf '引用 tools/ghost-probe-y.sh（该脚本未实现，仅计划）。\n'   > "$ST/ok-ghost.md"
  printf '本文件里的 tmp_design_and_todo 属于禁止来源清单。\n'        > "$ST/ok-guard.md"
  st_out=""; st_rc=0
  # 正向：夹具含一处未标注幽灵引用 → 整套门必须红，且红在 bad-ghost.md
  if st_out=$(LINT_TRACKED_FILES="$ST/bad-ghost.md
$ST/ok-ghost.md
$ST/ok-guard.md" LINT_SELFTEST=1 bash "$0" 2>&1); then
    echo "SELF-TEST FAIL: 夹具含未标注的幽灵工具引用，整套门却判绿（门恒绿，最危险的一类失效）" >&2
    printf '%s\n' "$st_out" | tail -3 >&2; exit 1
  fi
  if ! printf '%s' "$st_out" | grep -q "bad-ghost.md"; then
    echo "SELF-TEST FAIL: 门红了，但没抓到 bad-ghost.md（判据未指向真问题）" >&2; exit 1
  fi
  if printf '%s' "$st_out" | grep -qE "ok-ghost.md|ok-guard.md"; then
    echo "SELF-TEST FAIL: 已标注/守卫语境的夹具被误报" >&2
    printf '%s\n' "$st_out" | grep -E "ok-ghost|ok-guard" >&2; exit 1
  fi
  # 反向：夹具全部合规 → 同一套门必须绿（证明红是因内容，不是因为门本身恒红）
  if ! st_out=$(LINT_TRACKED_FILES="$ST/ok-ghost.md
$ST/ok-guard.md" LINT_SELFTEST=1 bash "$0" > /dev/null 2>&1); then
    echo "SELF-TEST FAIL: 夹具全部合规时门仍红（不可复现的判据）" >&2
    printf '%s\n' "$st_out" | tail -3 >&2; exit 1
  fi
  echo "SELF-TEST PASS（正向：未标注幽灵引用被抓且不误伤已标注行；反向：全合规时门绿）"
  exit 0
fi
echo "== lint-review-rules =="

# L1. tmp_design_and_todo：活引用（不带"已删除/历史"标注且非守卫语境）应为 0
#     豁免机制是两层：① 下方词表（守卫语境的自然语言特征）；② 行内标记 `<!-- 守卫语境 -->`。
#     ② 是给「新写法被误判」留的出口——加标记比改门安全：词表一改就可能放行真违规，标记只放行那一行。
#     守卫规则（禁止来源清单、处置策略表）与 `AGENTS.md` 的中间产物目录约定条目允许保留。
l1=$(grep -n "tmp_design" $LINT_FILES 2>/dev/null \
     | grep -v "已删除\|历史形态\|历史案例\|历史输入\|历史均值" \
     | grep -v "禁止\|❌\|非定稿\|临时讨论池\|作为快照依据\|作为 design 依据\|design.md 不得引用\|当 design 依据" \
     | grep -v "Hidden Folder\|中间产物目录约定\|早期手动生成\|无 design/tmp_design 引用" \
     | grep -v "§〇.临时文档规则\|grep -rnE" | grep -v "守卫语境 -->" | wc -l || true)
if [ "$l1" -eq 0 ]; then ok "L1 tmp_design_and_todo 引用全部为守卫/已标注"; else
  fail "L1 发现 $l1 处未标注的 tmp_design_and_todo 活引用："; grep -n "tmp_design" $LINT_FILES 2>/dev/null \
    | grep -v "已删除\|历史形态\|历史案例\|历史输入\|历史均值" \
    | grep -v "禁止\|❌\|非定稿\|临时讨论池\|作为快照依据\|作为 design 依据\|design.md 不得引用\|当 design 依据" \
    | grep -v "Hidden Folder\|中间产物目录约定\|早期手动生成\|无 design/tmp_design 引用" \
    | grep -v "§〇.临时文档规则\|grep -rnE" | sed 's/^/     /' >&2
fi

# L2. 旧 stage 目录名：03-stage-kernel 出现在非 .review/ 路径中应为 0（历史 scan 路径豁免）
l2=$(grep -n "03-stage-kernel" $LINT_FILES 2>/dev/null \
     | grep -v "\.review/" | grep -v "历史\|现编号\|当时" | wc -l || true)
if [ "$l2" -eq 0 ]; then ok "L2 无 03-stage-kernel 旧名活引用"; else
  fail "L2 发现 $l2 处 03-stage-kernel 旧名活引用："; grep -n "03-stage-kernel" $LINT_FILES 2>/dev/null \
    | grep -v "\.review/" | grep -v "历史\|现编号\|当时\|守卫语境 -->" | sed 's/^/     /' >&2
fi

# L3. prompt/../ 残缺占位路径
l3=$(grep -n "prompt/\.\./" $LINT_FILES 2>/dev/null | wc -l || true)
if [ "$l3" -eq 0 ]; then ok "L3 无 prompt/../ 残缺路径"; else
  fail "L3 发现 $l3 处 prompt/../ 残缺路径"; grep -n "prompt/\.\./" $LINT_FILES 2>/dev/null | sed 's/^/     /' >&2
fi

# L4. tools/ 下 .bak 文件
l4=$(find tools -maxdepth 1 -name "*.bak*" 2>/dev/null | wc -l || true)
if [ "$l4" -eq 0 ]; then ok "L4 tools/ 无 .bak 备份文件"; else
  fail "L4 tools/ 存在 $l4 个 .bak 文件"; find tools -maxdepth 1 -name "*.bak*" | sed 's/^/     /' >&2
fi

# L5. 模式编号序列：无重复、断号仅限 61/62、总数与索引表宣称一致
nums=$(grep -oE "^### 模式 ?[0-9]+[:：]" prompt/review-rules/review-patterns.md | grep -oE "[0-9]+" | sort -n)
dups=$(echo "$nums" | uniq -d | tr '\n' ' ')
if [ -z "$dups" ]; then ok "L5a 模式编号无重复"; else fail "L5a 模式编号重复: $dups"; fi
# 注意：64 出现两次（模式 64 + "模式 64 扩展"复用编号），64 扩展标题格式不同，不会进 nums；61/62 是已记录空号
gaps=$(echo "$nums" | awk 'NR>1 && $1 != prev + 1 && !($1 == 63 && prev == 60) {print prev"->"$1} {prev=$1}')
l5b_ok="true"
for g in $gaps; do
  case "$g" in
    "60->63") : ;;  # 61/62 合并入 60（有记录的空号）
    *) l5b_ok="false"; fail "L5b 未记录的模式编号断裂: $g" ;;
  esac
done
[ "$l5b_ok" = "true" ] && ok "L5b 模式编号断裂仅限已记录的 61/62"
total=$(echo "$nums" | sort -un | wc -l)
# 宣称的数字从索引行读出来，不写死在脚本里：新增模式时只需改文档，不必同时改代码。
# grep 无命中时（宣称行被删或改写）必须显式报错，不能让 set -e 把整个脚本静默带走。
expect=$(grep -oE "[0-9]+ 个编号模式" prompt/review-rules/review-patterns.md | head -1 || true)
claim=$(printf '%s' "$expect" | grep -oE "^[0-9]+" || true)
if [ -z "$claim" ]; then
  fail "L5c 索引表缺少「N 个编号模式」宣称（实测编号模式 $total 个）——恢复 review-patterns.md 头部总量行"
elif [ "$total" -eq "$claim" ]; then ok "L5c 编号模式总数 $total 与索引表宣称（$claim）一致"
else
  fail "L5c 编号模式总数($total) 与索引表宣称($claim)不一致——更新 review-patterns.md 头部索引与各处宣称"
fi

# L10. 幽灵工具引用：规则文本里写到的 tools/*.sh|py 若文件不存在，同一行必须带状态标记
#      （未实现 / 未来实施 / 从未 / 不存在 / 计划中 / 已作废 / 拟 / 建议 / 曾写作 / 未落地）。
#      为什么要有这条：本仓曾把强制证据行「代码可读性增量」指向 tools/code-style-lint.sh，
#      而该脚本从未存在——执行 review 的 agent 拿不到证据，最自然的结局就是编一份输出出来。
L10_OUT=$(python3 - <<'PYEOF2'
import re, sys
from pathlib import Path
ROOT = Path(".")
SCAN = ["prompt", ".claude", ".codex", ".trae", ".agents", "AGENTS.md", "README.md"]
PAT = re.compile(r"tools/[A-Za-z0-9_-]+\.(?:sh|py)")
MARKERS = ("未实现", "未来实施", "从未", "不存在", "计划中", "已作废", "拟", "建议", "曾写作", "未落地", "作废")
bad = []
import os
files = [Path(x) for x in os.environ.get("LINT_TRACKED_FILES", "").splitlines() if x.strip()]
for f in files:
    try:
        text = f.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        continue
    for lineno, line in enumerate(text.splitlines(), 1):
        for m in PAT.finditer(line):
            target = ROOT / m.group(0)
            if target.exists():
                continue
            window = line[max(0, m.start() - 60): m.end() + 90]
            if not any(k in window for k in MARKERS):
                bad.append(f"{f}:{lineno}: {m.group(0)} :: {line.strip()[:70]}")
print("\n".join(bad))
PYEOF2
)
if [ -z "$L10_OUT" ]; then
  ok "L10 规则文本无未标注的幽灵工具引用"
else
  n=$(printf '%s\n' "$L10_OUT" | grep -c .)
  printf '%s\n' "$L10_OUT" | head -10 | sed 's/^/    ✗ /' >&2
  fail "L10 发现 $n 处幽灵工具引用（脚本不存在且同行未标注状态）"
fi

# L11. 幽灵模式号：规则文本引用的「模式 N / Pattern #N」必须在模式库里有定义。
#      与 L10 同一类缺陷（引用不存在的东西），只是对象从脚本换成条文编号：本仓历史上出现过
#      「通用强制门：锚点纪律门（模式 83）」这类引用，一旦编号写错或模式被合并，引用就悬空。
#      已登记空号 61/62（合并入 60）不算违规。
L11_OUT=$(python3 - <<'PYEOF2'
import re
from pathlib import Path
lib = Path("prompt/review-rules/review-patterns.md").read_text(encoding="utf-8")
defined = {int(n) for n in re.findall(r"^### 模式\s*([0-9]+)", lib, re.M)}
allowed = defined | {61, 62}
mx = max(defined)
SCAN = ["prompt", ".claude", ".codex", ".trae", ".agents", "AGENTS.md"]
bad = []
import os
for f in [Path(x) for x in os.environ.get("LINT_TRACKED_FILES", "").splitlines() if x.strip()]:
        if f.suffix != ".md": continue
        text = f.read_text(encoding="utf-8", errors="replace")
        for lineno, line in enumerate(text.splitlines(), 1):
            for m in re.finditer(r"(?:模式|Pattern ?#?)\s*([0-9]{1,3})", line):
                n = int(m.group(1))
                if n <= 60 or n > mx + 10 or n in allowed: continue
                bad.append(f"{f}:{lineno}: 模式 {n} :: {line.strip()[:64]}")
print("\n".join(sorted(set(bad))))
PYEOF2
)
if [ -z "$L11_OUT" ]; then
  ok "L11 无未定义的幽灵模式号引用"
else
  n=$(printf '%s\n' "$L11_OUT" | grep -c .)
  printf '%s\n' "$L11_OUT" | head -8 | sed 's/^/    ✗ /' >&2
  fail "L11 发现 $n 处幽灵模式号引用（模式库里无此编号，且不属于已登记空号 61/62）"
fi

# L12. 技能可达性：每个技能目录必须被入口、命令或规则引用至少一处（防「加了技能没人路由到」）。
#      触发式描述（L13）解决「会不会被读」，本条解决「有没有人指到它」——两者缺一技能就是死文件。
L12_OUT=$(python3 - <<'PYEOF2'
from pathlib import Path
dirs = []
for root in (".codex/skills", ".claude/skills", "prompt/skill/cmds"):
    q = Path(root)
    if q.is_dir():
        dirs += [d.name for d in q.iterdir() if d.is_dir()]
HUBS = ["prompt/review-rules", "prompt/skill", "prompt/README.md", "AGENTS.md", "opencode.json"]
corpus = ""
for h in HUBS:
    q = Path(h)
    if q.is_file():
        corpus += q.read_text(encoding="utf-8", errors="replace")
    elif q.is_dir():
        for f in q.rglob("*.md"):
            corpus += f.read_text(encoding="utf-8", errors="replace")
missing = [d for d in sorted(set(dirs)) if d not in corpus]
print("\n".join(missing))
PYEOF2
)
if [ -z "$L12_OUT" ]; then
  ok "L12 所有技能目录都能被入口或规则路由到"
else
  printf '%s\n' "$L12_OUT" | sed 's/^/    ✗ 无人引用的技能：/' >&2
  fail "L12 存在 $(printf '%s\n' "$L12_OUT" | grep -c .) 个技能未被任何入口/命令/规则引用"
fi

# L13. 模式库副本不落后于源：技能副本必须收录规则源里编号最新的模式。
#      为什么：模式库有三份用途不同的副本——规则源（全量权威）、技能源（review 时被调用的那份）、
#      Claude 编排器的 checks/patterns.md（按领域精选的高频子集，不承诺最新）。本轮实测踩坑：
#      新增模式 85 只写进了规则源，技能副本仍停在 84，而 --check 只比对「派生 vs 技能源」的差异，
#      根本看不出这种「源改了、副本没跟」的内容滞后。本条堵的就是这个缺口。
L13_OUT=$(python3 - <<'PYEOF2'
import re
from pathlib import Path
src = Path("prompt/review-rules/review-patterns.md").read_text(encoding="utf-8")
sk = Path("prompt/skill/review-patterns-skill.md").read_text(encoding="utf-8")
# 编号后不得紧跟字母数字：挡住「模式 85x」被当成已收录 85（审阅方指出的宽松度）。
# 用否定预查而非要求冒号，因为模式 64 有一条「模式 64 扩展」共用编号、编号后是空格。
nums = sorted({int(n) for n in re.findall(r"^### 模式\s*([0-9]+)(?![0-9A-Za-z])", src, re.M)})
have = {int(n) for n in re.findall(r"^### 模式\s*([0-9]+)(?![0-9A-Za-z])", sk, re.M)}
missing = [n for n in nums[-5:] if n not in have]
print(",".join(str(n) for n in missing))
PYEOF2
)
if [ -z "$L13_OUT" ]; then
  ok "L13 技能副本已收录规则源编号最新的 5 条模式"
else
  fail "L13 技能副本落后于规则源：缺模式 $L13_OUT（新增模式必须同步 prompt/skill/review-patterns-skill.md，再重派生）"
fi

# L14. 模式索引表的区间描述必须与实际编号一致（2026-10-08，独立审计 P2：L5c 只比总数，抓不到
#      「79-84」这类区间描述漏掉最新编号的漂移）。做法：取索引表里所有形如「a-b」的区间，
#      断言区间内的每个编号要么真存在、要么属于已登记空号。
L14_OUT=$(python3 - <<'PYEOF2'
import re
from pathlib import Path
lib = Path("prompt/review-rules/review-patterns.md").read_text(encoding="utf-8")
defined = {int(n) for n in re.findall(r"^### 模式\s*([0-9]+)(?![0-9A-Za-z])", lib, re.M)}
defined |= {int(n) for n in re.findall(r"^### 模式\s*([0-9]+)(?![0-9A-Za-z]) 扩展", lib, re.M)}
allowed_gap = {61, 62}
bad = []
for a, b in re.findall(r"\|\s*([0-9]{1,3})-([0-9]{1,3})\s*\|", lib):
    for n in range(int(a), int(b) + 1):
        if n not in defined and n not in allowed_gap and n > 60:
            bad.append(f"索引表区间 {a}-{b} 含不存在的模式 {n}")
print("\n".join(sorted(set(bad))))
PYEOF2
)
if [ -z "$L14_OUT" ]; then ok "L14 模式索引表的区间与实际编号一致"; else
  printf '%s\n' "$L14_OUT" | sed 's/^/    ✗ /' >&2
  fail "L14 模式索引表区间描述有 $(printf '%s\n' "$L14_OUT" | grep -c .) 处与实际编号不符"
fi

# L15. Agent Prompt 字符预算 + 文档不得抄录实测字符数（2026-10-08，独立审计 P1-4）
#      Trae 的 10,000 字符是硬上限（超出即截断），此前只写在 README 里、无任何机器检查；
#      而 README 同时抄录实测值，改一次文件就漂一次（本轮实测到 9,328 vs 真实 9,289）。
AGENT_FILE="prompt/skill/review-agent-ide.md"
agent_len=$(python3 -c "print(len(open('$AGENT_FILE',encoding='utf-8').read()))" 2>/dev/null || echo 0)
if [ "$agent_len" -le 10000 ]; then ok "L15 Agent Prompt 实测 $agent_len 字符，未越 10,000 硬上限"
else fail "L15 Agent Prompt 实测 $agent_len 字符，已超 Trae 硬上限 10,000（会被自动截断）"; fi
stale_claims=$(grep -n "9,328\|9,289[^0-9]*字符\|余量 672\|余量 711" prompt/README.md 2>/dev/null | grep -v "旧文本" | sed 's/^/    ✗ /' || true)
if [ -z "$stale_claims" ]; then ok "L15 README 未抄录 Agent Prompt 实测字符数（数字改由本门判定）"
else printf '%s\n' "$stale_claims" >&2; fail "L15 README 里仍有抄录的字符数/余量宣称，改为命令实测（抄一次漂一次）"; fi

# L16. 门的扫描域必须排除未跟踪的审阅工作区（本批 P1-1 的回归保护）：
#      若将来有人把审阅产物 add 进仓库，门的命中会包含别人写的门输出并级联增长——这条至少让这种情况显形。
untracked_in_scan=$(git ls-files --others --exclude-standard -- prompt 2>/dev/null | grep -E '\.(md|txt)$' | grep -vcE "prompt/(review-rules|skill|EXECUTION|README|todo_plan|agents-workflow)" || true)
if [ "${untracked_in_scan:-0}" -gt 0 ]; then
  echo "    ℹ L16 存在 ${untracked_in_scan} 个未跟踪的 prompt/ 审阅产物（按设计不在扫描域内；若入库请先归入规则域并复核 L1/L2/L10/L11）"
fi

# L17. Claude 运行时技能副本不落后于规范源（2026-10-08，独立审计 P2：`.claude/skills/review-implementation-skill/`
#      不在生成器目标内、只断言存在不比内容，是最后一处无守护的手抄件）。
#      归一化只抹掉两类正当差异：frontmatter 与相对链接深度（`.claude/skills/x/SKILL.md` 距 prompt/ 更远三级）。
L17_BAD=""
for pair in "review-implementation-skill:review-implementation-skill"; do
  src="prompt/skill/${pair%%:*}.md"; dst=".claude/skills/${pair##*:}/SKILL.md"
  [[ -f "$src" && -f "$dst" ]] || continue
  norm() { sed -E 's#]\((\.\./){1,3}prompt/#](P#g; s#]\((\.\./){1,3}#](P#g; s#\.claude/#RT/#g; /^---$/d; /^name:/d; /^description:/d; /^[[:space:]]*$/d' "$1"; }
  if ! diff -q <(norm "$src") <(norm "$dst") >/dev/null 2>&1; then
    L17_BAD="${L17_BAD}    ✗ ${dst} 与 ${src} 不一致（归一化后仍有差异，需同步或改为派生）\n"
  fi
done
if [ -z "$L17_BAD" ]; then ok "L17 Claude 技能副本与规范源一致（归一化后零差异）"
else printf "$L17_BAD" >&2; fail "L17 存在未同步的 Claude 运行时技能副本"; fi

# L6. Gate 权威注册表完整性：10 个 Gate 必须都在 review-process.md 的注册表中各占一行
registry=$(sed -n '/### Gate 权威注册表/,/^---$/p' prompt/review-rules/review-process.md)
for g in "Gate 0" "Gate A" "Gate B" "Gate C" "Gate D" "Gate D-6" "Gate D-Impl" "Gate E" "Gate G" "Gate H"; do
  n=$(echo "$registry" | grep -cE "^\| \*\*${g}\*\*" || true)
  if [ "$n" -eq 1 ]; then ok "L6 ${g} 注册表唯一收录"; else fail "L6 ${g} 在 Gate 权威注册表中出现 $n 次（应为 1）"; fi
done

# L7. cmd 薄壳 SKILL.md frontmatter
for f in prompt/skill/cmds/*/SKILL.md; do
  dir=$(basename "$(dirname "$f")")
  name=$(sed -n 's/^name: *"\([^"]*\)".*/\1/p' "$f" | head -1)
  desc=$(sed -n 's/^description: *"\(.*\)"$/\1/p' "$f" | head -1)
  [ "$name" = "$dir" ] || fail "L7 name≠dirname: $f"
  dlen=$(python3 -c 'import sys;print(len(sys.argv[1]))' "$desc")
  [ "$dlen" -le 1024 ] || fail "L7 description >1024: $f ($dlen)"
  grep -q '^description: "' "$f" || fail "L7 description 未加引号: $f"
done
ok "L7 cmd 薄壳 frontmatter 检查完成"

# L8. .agents/skills/ 软链解析
for name in full-review style-fix code-excellence test-audit todo-fix style-bible; do
  link=".agents/skills/$name"
  if [ -L "$link" ] && [ -f "$link/SKILL.md" ]; then
    : # OK
  else
    fail "L8 软链缺失或不可解析: $link"
  fi
done
ok "L8 .agents/skills/ 软链检查完成"

# L9. 元规则与注册表锚点（防再次膨胀的机制本体必须存在）
grep -q "⛔ 新增规则元规则" prompt/review-rules/review-process.md || fail "L9 review-process.md 缺新增规则元规则"
grep -q "### 检查项注册表" prompt/review-rules/review-process.md || fail "L9 review-process.md 缺检查项注册表"
grep -q "### Gate 权威注册表" prompt/review-rules/review-process.md || fail "L9 review-process.md 缺 Gate 权威注册表"
grep -q "现役入口变更" prompt/review-rules/review-profiles.md || fail "L9 review-profiles.md 缺 cmd 入口迁移声明"
ok "L9 元规则与注册表锚点检查完成"

echo "== lint-review-rules 完成：$errors 个失败 =="
if [ "$errors" -gt 0 ]; then exit 1; fi
exit 0
