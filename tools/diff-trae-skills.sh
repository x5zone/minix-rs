#!/usr/bin/env bash
# diff-trae-skills.sh
# 对比 .trae/skills/ 与 prompt/skill/ 同步状态
#
# 用法:
#   ./tools/diff-trae-skills.sh           # 完整对比 (推荐)
#   ./tools/diff-trae-skills.sh --only-diff   # 只显示有差异的
#   ./tools/diff-trae-skills.sh --raw        # 原始 diff (不做 frontmatter 规范化)
#   ./tools/diff-trae-skills.sh <name>      # 对比单个 skill (e.g. patterns)

set -uo pipefail

# 切换到仓库根
cd "$(git rev-parse --show-toplevel 2>/dev/null || echo ".")"

PROMPT_DIR="prompt/skill"
TRAE_DIR=".trae/skills"

# 解析参数
ONLY_DIFF=0
RAW=0
TARGET=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --only-diff) ONLY_DIFF=1; shift ;;
    --raw)       RAW=1; shift ;;
    -h|--help)
      sed -n '2,12p' "$0" | sed 's/^# //'
      exit 0 ;;
    *)           TARGET="$1"; shift ;;
  esac
done

# 工具函数
red()    { printf '\033[31m%s\033[0m' "$*"; }
green()  { printf '\033[32m%s\033[0m' "$*"; }
yellow() { printf '\033[33m%s\033[0m' "$*"; }
bold()   { printf '\033[1m%s\033[0m' "$*"; }

# 提取 frontmatter (--- 之间的内容)
get_fm() {
  awk '/^---$/{c++; next} c==1' "$1"
}

# 提取正文 (--- 之后的内容)
get_body() {
  awk '/^---$/{c++; next} c>=2' "$1"
}

# 规范化 frontmatter: 去掉引号 + 排序
normalize_fm() {
  sed -E 's/^(name|description):[[:space:]]*"?([^"]*)"?/\1: \2/'
}

# 规范化正文: trim 末尾空白
normalize_body() {
  sed -E 's/[[:space:]]+$//'
}

if [[ -n "$TARGET" ]]; then
  # 对比单个 skill
  PROMPT_FILE="$PROMPT_DIR/review-${TARGET}-skill.md"
  TRAE_FILE="$TRAE_DIR/review-${TARGET}-skill/SKILL.md"

  echo "$(bold "== 对比 $TARGET ==")"
  echo "源: $PROMPT_FILE"
  echo "派生: $TRAE_FILE"
  echo ""

  if [[ ! -f "$PROMPT_FILE" ]]; then
    echo "$(red "❌ 源文件不存在: $PROMPT_FILE")"
    exit 1
  fi
  if [[ ! -f "$TRAE_FILE" ]]; then
    echo "$(red "❌ 派生文件不存在: $TRAE_FILE")"
    exit 1
  fi

  if [[ $RAW -eq 1 ]]; then
    diff -u "$PROMPT_FILE" "$TRAE_FILE" | head -100
  else
    # 规范化对比
    P_FM=$(mktemp); P_BODY=$(mktemp)
    T_FM=$(mktemp); T_BODY=$(mktemp)
    normalize_fm < <(get_fm "$PROMPT_FILE") > "$P_FM"
    normalize_body < <(get_body "$PROMPT_FILE") > "$P_BODY"
    normalize_fm < <(get_fm "$TRAE_FILE") > "$T_FM"
    normalize_body < <(get_body "$TRAE_FILE") > "$T_BODY"

    echo "$(bold "--- Frontmatter (规范化后) ---")"
    if diff -q "$P_FM" "$T_FM" > /dev/null; then
      echo "$(green "✅ frontmatter 一致")"
    else
      echo "$(yellow "⚠️  frontmatter 仍有差异 (规范化后):")"
      diff -u "$P_FM" "$T_FM"
    fi
    echo ""
    echo "$(bold "--- 正文 ---")"
    if diff -q "$P_BODY" "$T_BODY" > /dev/null; then
      echo "$(green "✅ 正文完全一致")"
    else
      echo "$(red "❌ 正文有差异:")"
      diff -u "$P_BODY" "$T_BODY" | head -100
    fi

    rm -f "$P_FM" "$P_BODY" "$T_FM" "$T_BODY"
  fi
  exit 0
fi

# 完整对比
echo "$(bold "========================================")"
echo "$(bold "  .trae/skills/ vs prompt/skill/ 同步对比")"
echo "$(bold "========================================")"
echo ""

# 找出所有 skill
PROMPT_SKILLS=$(find "$PROMPT_DIR" -name "review-*-skill.md" -printf "%f\n" | sed 's/.md$//' | sort)
TRAE_SKILLS=$(find "$TRAE_DIR" -mindepth 1 -maxdepth 1 -type d -printf "%f\n" | sort)

P_COUNT=$(echo "$PROMPT_SKILLS" | wc -l)
T_COUNT=$(echo "$TRAE_SKILLS" | wc -l)
echo "源: $P_COUNT 个 skill  |  派生: $T_COUNT 个 skill"
echo ""

# 检查集合差
P_SET=$(echo "$PROMPT_SKILLS" | sort)
T_SET=$(echo "$TRAE_SKILLS" | sort)

MISSING_IN_TRAE=$(comm -23 <(echo "$P_SET") <(echo "$T_SET"))
MISSING_IN_PROMPT=$(comm -13 <(echo "$P_SET") <(echo "$T_SET"))

if [[ -n "$MISSING_IN_TRAE" ]]; then
  echo "$(red "❌ prompt/skill/ 有但 .trae/skills/ 缺失:")"
  echo "$MISSING_IN_TRAE" | sed 's/^/  - /'
  echo ""
fi
if [[ -n "$MISSING_IN_PROMPT" ]]; then
  echo "$(red "❌ .trae/skills/ 有但 prompt/skill/ 缺失:")"
  echo "$MISSING_IN_PROMPT" | sed 's/^/  - /'
  echo ""
fi

# 逐个对比
echo "$(bold "--- 逐个对比 ---")"
printf "%-35s | %-10s | %-10s | %s\n" "Skill" "字节差" "行数差" "状态"
echo "----------------------------------------------------------------------"

ALL_MATCH=1
for skill in $P_SET; do
  PROMPT_FILE="$PROMPT_DIR/${skill}.md"
  TRAE_FILE="$TRAE_DIR/${skill}/SKILL.md"

  if [[ ! -f "$TRAE_FILE" ]]; then
    printf "%-35s | %-10s | %-10s | %s\n" "$skill" "—" "—" "$(red '缺失')"
    ALL_MATCH=0
    continue
  fi

  P_BYTES=$(wc -c < "$PROMPT_FILE")
  T_BYTES=$(wc -c < "$TRAE_FILE")
  BYTE_DIFF=$((P_BYTES - T_BYTES))

  P_LINES=$(wc -l < "$PROMPT_FILE")
  T_LINES=$(wc -l < "$TRAE_FILE")
  LINE_DIFF=$((P_LINES - T_LINES))

  # 规范化对比
  P_FM=$(mktemp); P_BODY=$(mktemp)
  T_FM=$(mktemp); T_BODY=$(mktemp)
  normalize_fm < <(get_fm "$PROMPT_FILE") > "$P_FM"
  normalize_body < <(get_body "$PROMPT_FILE") > "$P_BODY"
  normalize_fm < <(get_fm "$TRAE_FILE") > "$T_FM"
  normalize_body < <(get_body "$TRAE_FILE") > "$T_BODY"

  if diff -q "$P_BODY" "$T_BODY" > /dev/null && diff -q "$P_FM" "$T_FM" > /dev/null; then
    if [[ $ONLY_DIFF -eq 1 ]]; then
      rm -f "$P_FM" "$P_BODY" "$T_FM" "$T_BODY"
      continue
    fi
    printf "%-35s | %-10s | %-10s | %s\n" "$skill" "$BYTE_DIFF" "$LINE_DIFF" "$(green '✅ 一致')"
  else
    ALL_MATCH=0
    printf "%-35s | %-10s | %-10s | %s\n" "$skill" "$BYTE_DIFF" "$LINE_DIFF" "$(red '❌ 内容差异')"
  fi

  rm -f "$P_FM" "$P_BODY" "$T_FM" "$T_BODY"
done

echo ""
if [[ $ALL_MATCH -eq 1 ]]; then
  echo "$(green "✅ 全部 8 个 skill 完全同步 (规范化对比)")"
else
  echo "$(red "❌ 存在内容差异，运行 '$(basename "$0") <skill>' 查看详情")"
fi
