#!/usr/bin/env bash
# design-index-update.sh — 自动维护 notes/rewrite/{module}/{stage}/.design/DESIGN-INDEX.md
#
# 用法: design-index-update.sh {stage-dir}
# 例:   design-index-update.sh notes/rewrite/fork-syscall-rewrite/03-stage-kernel
#
# 功能:
#   1. 扫描 {stage-dir}/.design/ 下所有 *-design*.md / *-outline*.md
#   2. 自动生成/更新 DESIGN-INDEX.md（每 NN 一行 + 路径 + 最新版本 + 状态）
#   3. 提供 review 跨文档"找 design"统一入口

set -euo pipefail

if [[ $# -lt 1 ]]; then
    echo "用法: $0 {stage-dir}" >&2
    echo "  stage-dir = notes/rewrite/{module}/{stage}" >&2
    exit 1
fi

STAGE_DIR="$1"
DESIGN_DIR="$STAGE_DIR/.design"
INDEX_FILE="$DESIGN_DIR/DESIGN-INDEX.md"

if [[ ! -d "$DESIGN_DIR" ]]; then
    echo "❌ Design 目录不存在: $DESIGN_DIR" >&2
    exit 1
fi

# 提取 stage 名（路径最后一段）
STAGE_NAME="$(basename "$STAGE_DIR")"
MODULE_NAME="$(basename "$(dirname "$STAGE_DIR")")"

# 扫描所有 {NN}-*.md 文件
DESIGN_FILES=$(ls "$DESIGN_DIR"/*-design*.md 2>/dev/null || true)
OUTLINE_FILES=$(ls "$DESIGN_DIR"/*-outline*.md 2>/dev/null || true)

if [[ -z "$DESIGN_FILES" && -z "$OUTLINE_FILES" ]]; then
    echo "⚠️ Design / outline 文件都不存在: $DESIGN_DIR" >&2
    exit 0
fi

{
    echo "# DESIGN-INDEX.md — $STAGE_NAME Design 元数据索引（NEW 2026-07-16）"
    echo ""
    echo "> 自动生成（tools/design-index-update.sh）。Reviewer 跨文档查 design 的统一入口。"
    echo "> 路径: $DESIGN_DIR"
    echo "> 生成日期: $(date +%Y-%m-%d)"
    echo ""
    echo "## Design 文件清单"
    echo ""
    echo "| NN | 文档名 | design 存在 | outline 存在 | 最新版本 | 状态 |"
    echo "|----|--------|-------------|--------------|---------|------|"
    
    # 提取所有 NN 前缀（排除 DESIGN-INDEX.md 本身）
    NNS=$(ls "$DESIGN_DIR"/*.md 2>/dev/null | \
          while read f; do
              base="$(basename "$f")"
              if [[ "$base" == "DESIGN-INDEX.md" ]]; then
                  continue
              fi
              nn="$(echo "$base" | cut -d- -f1)"
              echo "$nn"
          done | sort -u)
    
    for nn in $NNS; do
        # 提取 NN 对应的标题（从 outline.v1.md 标题推断，因设计章节命名约定 NN-title）
        doc_title=$(echo "$(ls "$DESIGN_DIR/${nn}"-outline*.md 2>/dev/null | head -1)" | xargs -I{} grep -E "^# " {} 2>/dev/null | head -1 | sed 's/# //')
        if [[ -z "$doc_title" ]]; then
            doc_title="(无标题文件)"
        fi
        
        # check design + outline
        design_count=$(ls "$DESIGN_DIR/${nn}"-design*.md 2>/dev/null | wc -l)
        outline_count=$(ls "$DESIGN_DIR/${nn}"-outline*.md 2>/dev/null | wc -l)
        
        design_str="$design_count"
        outline_str="$outline_count"
        
        # 最新版本
        latest_design=$(ls -t "$DESIGN_DIR/${nn}"-design*.md 2>/dev/null | head -1)
        latest_design_str="❌"
        if [[ -n "$latest_design" ]]; then
            latest_design_str="$(basename "$latest_design")"
            # 检查是否在 v1 以后
            version=$(echo "$latest_design_str" | grep -oE "v[0-9]+\.md$" | sed 's/v\(.*\)\.md/\1/')
            if [[ -n "$version" && "$version" -gt 1 ]]; then
                latest_design_str="$latest_design_str (v$version)"
            fi
        fi
        
        # 状态推断：v1 → PASS；>v1 → UNDER_REVIEW
        status="✅ PASS"
        if [[ -z "$latest_design" ]]; then
            status="❌ MISSING"
        elif [[ -n "$(echo "$latest_design_str" | grep -E "v[2-9]")" ]]; then
            status="🔄 REVIEW"
        fi
        
        printf "| %s | %s | %s | %s | %s | %s |\n" "$nn" "$doc_title" "$design_str" "$outline_str" "$latest_design_str" "$status"
    done
    
    echo ""
    echo "## 版本历史"
    echo ""
    echo "| NN | 设计决策版本数 | 最新修改日期 |"
    echo "|----|--------------|-------------|"
    for nn in $NNS; do
        latest=$(ls -t "$DESIGN_DIR/${nn}"-design*.md 2>/dev/null | head -1)
        if [[ -z "$latest" ]]; then
            continue
        fi
        count=$(ls "$DESIGN_DIR/${nn}"-design*.md 2>/dev/null | wc -l)
        mtime=$(stat -c %y "$latest" 2>/dev/null | cut -d' ' -f1)
        printf "| %s | %d | %s |\n" "$nn" "$count" "$mtime"
    done
    
    echo ""
    echo "## 使用说明"
    echo ""
    echo "- **查最新 design**: 看本表格 \"最新版本\" 列"
    echo "- **追溯演进**: 看 \"## 版本历史\" 段（v{N} → v{N+1} 差异可看对应 outline-review.md）"
    echo "- **重新生成**: 用 \`tools/design-index-update.sh $STAGE_DIR\`"
    echo ""
} > "$INDEX_FILE"

echo "✅ DESIGN-INDEX.md 已生成: $INDEX_FILE"
ls -la "$INDEX_FILE"
