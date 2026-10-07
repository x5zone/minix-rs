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

echo "== lint-review-rules =="

# L1. tmp_design_and_todo：活引用（不带"已删除/历史"标注且非守卫语境）应为 0
#     守卫规则（禁止来源清单、处置策略表）与 `AGENTS.md` 的中间产物目录约定条目允许保留。
l1=$(grep -rn "tmp_design" prompt/ .claude/rules .claude/skills .codex/skills .trae/skills 2>/dev/null \
     | grep -v "已删除\|历史形态\|历史案例\|历史输入\|历史均值" \
     | grep -v "禁止\|❌\|非定稿\|临时讨论池\|作为快照依据\|作为 design 依据\|design.md 不得引用\|当 design 依据" \
     | grep -v "Hidden Folder\|早期手动生成\|无 design/tmp_design 引用" \
     | grep -v "§〇.临时文档规则\|grep -rnE" | wc -l || true)
if [ "$l1" -eq 0 ]; then ok "L1 tmp_design_and_todo 引用全部为守卫/已标注"; else
  fail "L1 发现 $l1 处未标注的 tmp_design_and_todo 活引用："; grep -rn "tmp_design" prompt/ .claude/rules .claude/skills .codex/skills .trae/skills 2>/dev/null \
    | grep -v "已删除\|历史形态\|历史案例\|历史输入\|历史均值" \
    | grep -v "禁止\|❌\|非定稿\|临时讨论池\|作为快照依据\|作为 design 依据\|design.md 不得引用\|当 design 依据" \
    | grep -v "Hidden Folder\|早期手动生成\|无 design/tmp_design 引用" \
    | grep -v "§〇.临时文档规则\|grep -rnE" | sed 's/^/     /' >&2
fi

# L2. 旧 stage 目录名：03-stage-kernel 出现在非 .review/ 路径中应为 0（历史 scan 路径豁免）
l2=$(grep -rn "03-stage-kernel" prompt/ .claude/rules .claude/skills .codex/skills .trae/skills 2>/dev/null \
     | grep -v "\.review/" | grep -v "历史\|现编号\|当时" | wc -l || true)
if [ "$l2" -eq 0 ]; then ok "L2 无 03-stage-kernel 旧名活引用"; else
  fail "L2 发现 $l2 处 03-stage-kernel 旧名活引用："; grep -rn "03-stage-kernel" prompt/ .claude/rules .claude/skills .codex/skills .trae/skills 2>/dev/null \
    | grep -v "\.review/" | grep -v "历史\|现编号\|当时" | sed 's/^/     /' >&2
fi

# L3. prompt/../ 残缺占位路径
l3=$(grep -rn "prompt/\.\./" prompt/ .claude/ .codex/ .trae/ 2>/dev/null | wc -l || true)
if [ "$l3" -eq 0 ]; then ok "L3 无 prompt/../ 残缺路径"; else
  fail "L3 发现 $l3 处 prompt/../ 残缺路径"; grep -rn "prompt/\.\./" prompt/ .claude/ .codex/ .trae/ 2>/dev/null | sed 's/^/     /' >&2
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
files = []
for base in SCAN:
    q = ROOT / base
    if q.is_file():
        files.append(q)
    elif q.is_dir():
        files += [f for f in q.rglob("*") if f.is_file() and f.suffix in (".md", ".sh", ".py", ".json")]
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
for base in SCAN:
    q = Path(base)
    files = [q] if q.is_file() else list(q.rglob("*.md"))
    for f in files:
        if not f.is_file(): continue
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
