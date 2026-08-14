#!/usr/bin/env bash
# generate-derived-skills.sh — 从 prompt/skill/ 自动生成 .trae/skills/ + .codex/skills/
#
# 用法：
#   tools/generate-derived-skills.sh           # 生成两套派生集
#   tools/generate-derived-skills.sh trae      # 仅生成 .trae/
#   tools/generate-derived-skills.sh codex     # 仅生成 .codex/
#   tools/generate-derived-skills.sh --check   # 不写文件，只报告漂移
#
# 派生关系：
#   prompt/skill/{name}.md  →  .trae/skills/{name}/SKILL.md   (sed 去引号)
#   prompt/skill/{name}.md  →  .codex/skills/{name}/SKILL.md  (sed 去引号 + 路径适配)
#
# 注意：Codex 有少量上下文改写（如 bagging 说明）无法用 sed 机械替换，
#       这些由 tools/check-review-rules.sh 的 H3 计数检查兜底。
#       生成后必跑 check-review-rules.sh 确认无漂移。
#
# 2026-08-14 创建

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

# 9 个领域 Skill（review-scan 编排器由 .claude/ 独立维护，不在此生成）
SKILLS=(
  review-code-skill
  review-doc-skill
  review-patterns-skill
  review-process-skill
  review-core-semantics-skill
  review-coverage-skill
  review-excellence-skill
  review-implementation-skill
  review-socratic-skill
)

CHECK_ONLY="false"
TARGET="all"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --check) CHECK_ONLY="true"; shift ;;
    trae|codex|all) TARGET="$1"; shift ;;
    *) echo "Usage: $0 [--check|trae|codex|all]" >&2; exit 2 ;;
  esac
done

generate_trae() {
  local src="$1"
  local dst="$2"
  # Trae: name/description 去引号
  sed -E '1,/^---$/ {
    s/^name: "([^"]+)"/name: \1/
    s/^description: "([^"]+)"/description: \1/
  }' "$src" > "$dst"
}

generate_codex() {
  local src="$1"
  local dst="$2"
  # Codex: name 去引号，description 保留双引号（Codex 硬限制）
  # 路径/命名/工具名适配
  sed -E '
    # frontmatter: name 去引号
    1,/^---$/ {
      s/^name: "([^"]+)"/name: \1/
    }
    # 路径替换：.review/trae → .review/codex
    s|\.review/trae/|\.review/codex/|g
    # 文件命名：{doc-stem}-{agent}-scan.md → {doc-stem}/scan.md（Codex 单 session，无 agent 后缀）
    s|\{doc-stem\}-\{agent\}-scan\.md|{doc-stem}/scan.md|g
    s|\{doc-stem\}-\{agent\}-structure\.md|{doc-stem}/structure.md|g
    s|\{doc-stem\}-\{agent\}-SYMBOLS\.md|{doc-stem}/SYMBOLS.md|g
    # 工具名：Trae IDE → Codex CLI
    s|Trae IDE|Codex CLI|g
    # 工具名（运行文本）：Trae →/专属/内： → Codex
    s|Trae →|Codex →|g
    s|Trae 专属|Codex 专属|g
    s|Trae 内：|Codex 内：|g
    s|Trae 单文档|Codex 单文档|g
    # bagging 上下文改写
    s|Bagging 聚合只发生在 Trae 内（多 AI 的 scan 聚合）|Codex 单 session 不使用 bagging 聚合|g
    s|Trae 内可通过多 AI bagging 互为验证；Claude 内部需独立会话验证|Codex 内部需独立会话验证（单 session）|g
    s|（独立会话 / Trae 内跨 AI 聚合）|（独立会话）|g
    # 双写文档：-trae-review.md → -codex-review.md
    s|-trae-review\.md|-codex-review.md|g
    # review-init.sh 参数：trae → codex
    s|review-init\.sh trae |review-init.sh codex |g
  ' "$src" > "$dst"
}

report_drift() {
  local name="$1"
  local existing="$2"
  local generated_tmp="$3"
  if ! diff -q "$existing" "$generated_tmp" >/dev/null 2>&1; then
    local diff_lines
    diff_lines=$(diff "$existing" "$generated_tmp" | wc -l)
    echo "  DRIFT $name: $diff_lines diff lines"
    return 1
  else
    echo "  OK    $name"
    return 0
  fi
}

main() {
  local drift_count=0

  echo "=== Generate derived skills (target=$TARGET, check_only=$CHECK_ONLY) ==="

  # Trae Agent files (no frontmatter conversion, direct copy to .trae/)
  if [[ "$TARGET" == "all" || "$TARGET" == "trae" ]]; then
    for agent in review-agent-ide review-agent-trigger; do
      local src="prompt/skill/${agent}.md"
      local dst=".trae/skills/${agent}/SKILL.md"
      [[ -f "$src" ]] || continue
      mkdir -p "$(dirname "$dst")"
      if [[ "$CHECK_ONLY" == "true" ]]; then
        report_drift "trae/$agent" "$dst" "$src" || drift_count=$((drift_count + 1))
      else
        cp "$src" "$dst"
        echo "  GEN   trae/$agent"
      fi
    done
  fi

  for skill in "${SKILLS[@]}"; do
    local src="prompt/skill/${skill}.md"
    [[ -f "$src" ]] || { echo "  SKIP $skill (source not found)"; continue; }

    # Trae
    if [[ "$TARGET" == "all" || "$TARGET" == "trae" ]]; then
      local trae_dst=".trae/skills/${skill}/SKILL.md"
      mkdir -p "$(dirname "$trae_dst")"
      if [[ "$CHECK_ONLY" == "true" ]]; then
        local tmp
        tmp=$(mktemp)
        generate_trae "$src" "$tmp"
        report_drift "trae/$skill" "$trae_dst" "$tmp" || drift_count=$((drift_count + 1))
        rm -f "$tmp"
      else
        generate_trae "$src" "$trae_dst"
        echo "  GEN   trae/$skill"
      fi
    fi

    # Codex
    if [[ "$TARGET" == "all" || "$TARGET" == "codex" ]]; then
      local codex_dst=".codex/skills/${skill}/SKILL.md"
      mkdir -p "$(dirname "$codex_dst")"
      if [[ "$CHECK_ONLY" == "true" ]]; then
        local tmp
        tmp=$(mktemp)
        generate_codex "$src" "$tmp"
        report_drift "codex/$skill" "$codex_dst" "$tmp" || drift_count=$((drift_count + 1))
        rm -f "$tmp"
      else
        generate_codex "$src" "$codex_dst"
        echo "  GEN   codex/$skill"
      fi
    fi
  done

  echo ""
  if [[ "$CHECK_ONLY" == "true" ]]; then
    if [[ "$drift_count" -eq 0 ]]; then
      echo "✅ No drift detected. All derived skills match source."
      exit 0
    else
      echo "❌ $drift_count skill(s) drifted. Run without --check to regenerate."
      exit 1
    fi
  else
    echo "✅ Generated. Run 'tools/check-review-rules.sh' to validate."
    echo "   Note: Codex has ~5% contextual rewrites (bagging/agent lines) that"
    echo "   sed cannot handle. check-review-rules.sh H3-count check catches these."
  fi
}

main
