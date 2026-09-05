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
#     守卫规则（禁止来源清单、处置策略表）与 CLAUDE.md 的 Hidden Folder Convention 文档化条目允许保留。
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
expect=$(grep -oE "81 个编号模式" prompt/review-rules/review-patterns.md | head -1)
if [ "$total" -eq 81 ] && [ -n "$expect" ]; then ok "L5c 编号模式总数 $total 与索引表宣称一致"; else
  fail "L5c 编号模式总数($total) 与索引表宣称($expect)不一致——更新 review-patterns.md 头部索引与各处宣称"
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
