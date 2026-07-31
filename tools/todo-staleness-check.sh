#!/usr/bin/env bash
# todo-staleness-check.sh — 扫描 TODO 清单，检测跨轮状态陈旧（模式 70 CTOS 配套）
#
# 功能：
#   1. 解析 TODO 文件中的 `### TODO-XX-N` 条目
#   2. 检测"原文标 P0 + 后文否定"不一致（staleness 核心信号）
#   3. 检测已删除/否定的 TODO 是否在原文添加了内联否定标记
#   4. 输出 staleness 报告
#
# 用法：
#   tools/todo-staleness-check.sh <todo-file>
#
# 退出码：
#   - 0：无 staleness 问题（或所有否定 TODO 都有内联标记）
#   - 1：发现 staleness 问题（否定 TODO 缺内联标记）
#   - 2：参数错误 / 文件不存在
#
# 示例：
#   tools/todo-staleness-check.sh tmp_design_and_todo/0108-todo-final.md

set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "Usage: $0 <todo-file>" >&2
  exit 2
fi

TODO_FILE="$1"

if [[ ! -f "${TODO_FILE}" ]]; then
  echo "❌ TODO file not found: ${TODO_FILE}" >&2
  exit 2
fi

echo "# TODO Staleness Report (模式 70 CTOS)"
echo ""
echo "**File**: \`${TODO_FILE}\`"
echo "**Generated**: $(date '+%Y-%m-%d %H:%M:%S')"
echo ""

# Step 1: 提取"已删除/否定的 TODO"段中的 TODO 编号
# 匹配模式：| ~~TODO-XX-N~~ | 或 | TODO-XX-N | ... | 否定/删除 |
NEGATED_SECTION=$(awk '
  /^### 已删除\/否定的 TODO/ { in_section=1; next }
  /^### / && in_section { in_section=0 }
  in_section && /^\| ~/ { print }
' "${TODO_FILE}")

NEGATED_TODOS=$(echo "${NEGATED_SECTION}" | grep -oE 'TODO-[0-9]+-[0-9]+' || true)

if [[ -z "${NEGATED_TODOS}" ]]; then
  echo "✅ No negated/deleted TODOs found in summary section."
  echo ""
  echo "**Result**: PASS (0 staleness issues)"
  exit 0
fi

NEGATED_COUNT=$(echo "${NEGATED_TODOS}" | wc -l | tr -d ' ')
echo "## Negated/Deleted TODOs (from summary section)"
echo ""
echo "Found **${NEGATED_COUNT}** negated/deleted TODOs in summary section."
echo ""
echo "| TODO | Original Entry Has Inline Negation Mark? | Status |"
echo "|------|------------------------------------------|--------|"

ISSUES=0
CHECKED=0

while IFS= read -r todo_id; do
  [[ -z "${todo_id}" ]] && continue
  CHECKED=$((CHECKED + 1))

  # 检查原文条目（### TODO-XX-N）是否已添加内联否定标记
  # 内联标记信号：~~TODO-XX-N~~ 或 ❌ 已否定 / ❌ 已删除
  ORIGINAL_HEADER=$(grep -n "^### .*${todo_id}" "${TODO_FILE}" | head -1 || true)

  if [[ -z "${ORIGINAL_HEADER}" ]]; then
    # 原文找不到——可能在其他文件提出，本文件只有汇总
    echo "| ${todo_id} | N/A (原文不在本文件) | ➖ SKIP |"
    continue
  fi

  HEADER_LINE=$(echo "${ORIGINAL_HEADER}" | cut -d: -f2-)

  # 检测内联否定标记
  if echo "${HEADER_LINE}" | grep -qE '~~|❌|已否定|已删除|STALENESS'; then
    echo "| ${todo_id} | ✅ Yes (\`${HEADER_LINE:0:80}\`) | ✅ PASS |"
  else
    echo "| ${todo_id} | ❌ No (\`${HEADER_LINE:0:80}\`) | ❌ FAIL |"
    ISSUES=$((ISSUES + 1))
  fi
done <<< "${NEGATED_TODOS}"

echo ""
echo "## Summary"
echo ""
echo "- Checked: ${CHECKED} negated/deleted TODOs"
echo "- PASS: $((CHECKED - ISSUES))"
echo "- FAIL: ${ISSUES}"
echo ""

if [[ ${ISSUES} -gt 0 ]]; then
  echo "## Required Actions"
  echo ""
  echo "以下 TODO 在汇总段已否定/删除，但原文条目缺少内联否定标记。"
  echo "AI 拿到本文件时可能误把已否定的 P0 当作有效 TODO。"
  echo ""
  echo "修复方法：在原文 \`### TODO-XX-N\` 标题后添加："
  echo ""
  echo '```markdown'
  echo "### ~~TODO-XX-N~~: ...【P0】❌ 已否定"
  echo ""
  echo "> **⚠️ STALENESS 警告（模式 70 CTOS）**：本 TODO 已在下方汇总段否定。"
  echo "> 原始条目保留仅作历史记录。"
  echo '```'
  echo ""
  echo "**Result**: FAIL (${ISSUES} staleness issues)"
  exit 1
fi

echo "**Result**: PASS (0 staleness issues)"
exit 0
