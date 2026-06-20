#!/usr/bin/env bash
# review-init.sh — Minix-RS Review 初始化助手
#
# 用法: review-init.sh {tool} {doc-path} [agent]
# 例:   review-init.sh trae notes/rewrite/fork-syscall-rewrite/03-stage-kernel/03-kmain-cstart.md glm
#       review-init.sh claude notes/rewrite/fork-syscall-rewrite/03-stage-kernel/03-kmain-cstart.md
#
# 功能:
#   1. 从 doc-path 自动计算 {module} / {stage} / {doc-stem}
#   2. mkdir -p 对应工具的标准目录（.review/{tool}/{module}/scans/ 等）
#   3. 输出 Derived Paths 表格供 agent 在 Step 0 引用
#   4. 若 STATE.md 已存在，运行 review-state-validate.py 预检
#   5. 若 STATE.md 不存在，生成空 STATE.md 骨架
#
# 路径布局（见 improve-v2 §2.1.1）:
#   .review/trae/{module}/
#       ├── STATE.md
#       ├── VERIFY-CHECK.md
#       ├── session-plan.md
#       └── scans/
#           ├── {doc-stem}-{agent}-scan.md
#           ├── {doc-stem}-{agent}-structure.md
#           └── {doc-stem}-{agent}-SYMBOLS.md
#   .review/claude/{module}/
#       ├── STATE.md
#       ├── VERIFY-CHECK.md
#       └── {doc-stem}/{scan,structure,SYMBOLS}.md

set -euo pipefail

# ===== 参数校验 =====
if [[ $# -lt 2 ]]; then
    echo "用法: $0 {tool} {doc-path} [agent]" >&2
    echo "  tool     = trae | claude" >&2
    echo "  doc-path = 相对项目根的文档路径（如 notes/rewrite/{module}/{stage}/{doc}.md）" >&2
    echo "  agent    = AI 标识（Trae: glm/kimi/ds/qwen/seed；Claude 可省略）" >&2
    exit 1
fi

TOOL="$1"
DOC_PATH="$2"
AGENT="${3:-}"

if [[ "$TOOL" != "trae" && "$TOOL" != "claude" ]]; then
    echo "❌ tool 必须是 trae 或 claude，得到: $TOOL" >&2
    exit 1
fi

# ===== 定位项目根 =====
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

# ===== 解析 doc-path =====
if [[ ! -f "$DOC_PATH" ]]; then
    echo "❌ 文档不存在: $DOC_PATH (相对 $PROJECT_ROOT)" >&2
    exit 1
fi

# 期望路径形式: notes/rewrite/{module}/{stage}/{doc-stem}.md
# 或简化形式: {module}/{stage}/{doc-stem}.md（无 notes/rewrite 前缀）
DOC_PATH_NORMALIZED="$DOC_PATH"
# 去除 ./ 前缀
DOC_PATH_NORMALIZED="${DOC_PATH_NORMALIZED#./}"

# 提取 doc-stem（去扩展名）
DOC_FILENAME="$(basename "$DOC_PATH_NORMALIZED")"
DOC_STEM="${DOC_FILENAME%.md}"

# 提取 module 和 stage
# 策略：若路径含 notes/rewrite/，取其后第一级目录为 module
if [[ "$DOC_PATH_NORMALIZED" == notes/rewrite/* ]]; then
    # 形式: notes/rewrite/{module}/{stage}/{doc}.md
    REMAINING="${DOC_PATH_NORMALIZED#notes/rewrite/}"
    MODULE="${REMAINING%%/*}"
    # 去掉 module/ 后剩余
    REMAINING_AFTER_MODULE="${REMAINING#*/}"
    # stage 是剩余路径的第一级目录（若存在）
    if [[ "$REMAINING_AFTER_MODULE" == */* ]]; then
        STAGE="${REMAINING_AFTER_MODULE%%/*}"
    else
        STAGE=""
    fi
else
    # 简化形式：尝试用第一级目录作 module
    if [[ "$DOC_PATH_NORMALIZED" == */* ]]; then
        MODULE="${DOC_PATH_NORMALIZED%%/*}"
        REMAINING_AFTER_MODULE="${DOC_PATH_NORMALIZED#*/}"
        if [[ "$REMAINING_AFTER_MODULE" == */* ]]; then
            STAGE="${REMAINING_AFTER_MODULE%%/*}"
        else
            STAGE=""
        fi
    else
        MODULE="default"
        STAGE=""
    fi
fi

# ===== 构造标准路径 =====
REVIEW_BASE=".review/$TOOL/$MODULE"

if [[ "$TOOL" == "trae" ]]; then
    SCANS_DIR="$REVIEW_BASE/scans"
    if [[ -n "$AGENT" ]]; then
        SCAN_FILE="$SCANS_DIR/${DOC_STEM}-${AGENT}-scan.md"
        STRUCTURE_FILE="$SCANS_DIR/${DOC_STEM}-${AGENT}-structure.md"
        SYMBOLS_FILE="$SCANS_DIR/${DOC_STEM}-${AGENT}-SYMBOLS.md"
    else
        SCAN_FILE="$SCANS_DIR/${DOC_STEM}-scan.md"
        STRUCTURE_FILE="$SCANS_DIR/${DOC_STEM}-structure.md"
        SYMBOLS_FILE="$SCANS_DIR/${DOC_STEM}-SYMBOLS.md"
    fi
    STATE_FILE="$REVIEW_BASE/STATE.md"
    VERIFY_FILE="$REVIEW_BASE/VERIFY-CHECK.md"
    SESSION_PLAN_FILE="$REVIEW_BASE/session-plan.md"
else
    # claude
    DOC_DIR="$REVIEW_BASE/$DOC_STEM"
    SCAN_FILE="$DOC_DIR/scan.md"
    STRUCTURE_FILE="$DOC_DIR/structure.md"
    SYMBOLS_FILE="$DOC_DIR/SYMBOLS.md"
    STATE_FILE="$REVIEW_BASE/STATE.md"
    VERIFY_FILE="$REVIEW_BASE/VERIFY-CHECK.md"
    SESSION_PLAN_FILE=""  # Claude 通常单 session
fi

# ===== mkdir 标准目录 =====
if [[ "$TOOL" == "trae" ]]; then
    mkdir -p "$SCANS_DIR"
else
    mkdir -p "$DOC_DIR"
fi

# ===== 生成 STATE.md 骨架（若不存在）=====
generate_state_skeleton() {
    local state_file="$1"
    local tool="$2"
    local module="$3"
    cat > "$state_file" <<EOF
# Review State: $module ($tool)

- **Tool**: $tool
- **Module**: $module
- **Phase**: init
- **Last completed phase**: —
- **Open P0 issues**: 0
- **Open P1 issues**: 0
- **Open P2 issues**: 0
- **Convergence status**: NOT_CONVERGED
- **Next action**: Run Step 0 (scope + time budget)
- **Blocker Gates**: 0❌ A❌ B❌ C❌ D❌ D-6❌ E❌ G❌

## Phase Completion Log
| Phase | Date | Passes | P0 found | P1 found | P2 found |
|-------|------|--------|----------|----------|----------|

## Per-Doc Status
| Doc | Last scan | Agent | P0 | P1 | P2 | VERIFY? |
|-----|-----------|-------|----|----|----|---------|
| $DOC_STEM | — | ${AGENT:-(pending)} | — | — | — | ❌ |

## Session Status
| # | Session | 范围 | Status | Started | Completed | 产出 |
|---|---------|------|--------|---------|-----------|------|
| 1 | $DOC_STEM | (待填) | ⏳ PENDING | — | — | — |

## Convergence Checklist
- [ ] §2.1 概念准确性 — UNCHECKED
- [ ] §2.2 C引用验证 — UNCHECKED
- [ ] §2.3 数据结构覆盖 — UNCHECKED
- [ ] §2.8 源码覆盖完整性 — UNCHECKED
- [ ] §2.9 设计决策质量 — UNCHECKED
- [ ] §2.10 章节链路 — UNCHECKED
- [ ] Code §1-14 — UNCHECKED
- [ ] 跨文档联动 — UNCHECKED
- [ ] Claims-Evidence — UNCHECKED
- [ ] 独立验证 (Gate G) — UNCHECKED
EOF
}

# ===== STATE 预检或生成 =====
STATE_VALIDATE_SCRIPT="$SCRIPT_DIR/review-state-validate.py"
if [[ -f "$STATE_FILE" ]]; then
    echo "ℹ️ STATE.md 已存在，运行预检..."
    if [[ -f "$STATE_VALIDATE_SCRIPT" ]]; then
        python3 "$STATE_VALIDATE_SCRIPT" --state "$STATE_FILE" --project-root "$PROJECT_ROOT" || true
    else
        echo "⚠️ review-state-validate.py 不存在，跳过预检"
    fi
else
    echo "ℹ️ STATE.md 不存在，生成骨架..."
    generate_state_skeleton "$STATE_FILE" "$TOOL" "$MODULE"
    echo "✅ 已生成: $STATE_FILE"
fi

# ===== 输出 Derived Paths 表 =====
echo ""
echo "=== Derived Paths (Step 0 引用) ==="
echo ""
echo "| 变量 | 值 |"
echo "|------|---|"
echo "| tool | $TOOL |"
echo "| module | $MODULE |"
echo "| stage | ${STAGE:-(none)} |"
echo "| doc-stem | $DOC_STEM |"
echo "| agent | ${AGENT:-(none)} |"
echo "| doc-path | $DOC_PATH_NORMALIZED |"
echo ""
echo "| 产物 | 路径 |"
echo "|------|------|"
echo "| STATE.md | $STATE_FILE |"
echo "| VERIFY-CHECK.md | $VERIFY_FILE |"
if [[ -n "$SESSION_PLAN_FILE" ]]; then
    echo "| session-plan.md | $SESSION_PLAN_FILE |"
fi
echo "| scan.md | $SCAN_FILE |"
echo "| structure.md | $STRUCTURE_FILE |"
echo "| SYMBOLS.md | $SYMBOLS_FILE |"
echo ""

# ===== 双写路径提示 =====
if [[ "$TOOL" == "trae" ]]; then
    DOC_DIR_REL="$(dirname "$DOC_PATH_NORMALIZED")"
    DUAL_WRITE_FILE="$DOC_DIR_REL/${DOC_STEM}-trae-review.md"
else
    DOC_DIR_REL="$(dirname "$DOC_PATH_NORMALIZED")"
    DUAL_WRITE_FILE="$DOC_DIR_REL/${DOC_STEM}-claude-report.md"
fi
echo "双写交互式修复文档（可选）: $DUAL_WRITE_FILE"
echo ""

# ===== 覆盖率脚本命令提示 =====
COVERAGE_SCRIPT="$SCRIPT_DIR/coverage-extract/coverage-extract.py"
if [[ -f "$COVERAGE_SCRIPT" ]]; then
    echo "=== coverage-extract.py 命令模板（Gate A）==="
    echo ""
    # 推断 minix3 module：kernel / vm / pm / fs / ...
    MINIX3_MODULE="kernel"
    if [[ "$MODULE" == *"vm"* ]] || [[ "$STAGE" == *"vm"* ]]; then
        MINIX3_MODULE="vm"
    elif [[ "$MODULE" == *"pm"* ]] || [[ "$STAGE" == *"pm"* ]]; then
        MINIX3_MODULE="pm"
    elif [[ "$MODULE" == *"fs"* ]] || [[ "$STAGE" == *"fs"* ]]; then
        MINIX3_MODULE="fs"
    fi
    DOC_DIR_FOR_COVERAGE="$(dirname "$DOC_PATH_NORMALIZED")"
    echo "python3 $COVERAGE_SCRIPT $MINIX3_MODULE \\"
    echo "  $DOC_DIR_FOR_COVERAGE \\"
    echo "  --rust-dir os --c-dir minix3/minix/$MINIX3_MODULE \\"
    echo "  --doc-file $DOC_FILENAME \\"
    if [[ -f "$SCRIPT_DIR/coverage-extract/${MINIX3_MODULE}-semantic-map.json" ]]; then
        echo "  --semantic-map tools/coverage-extract/${MINIX3_MODULE}-semantic-map.json \\"
    fi
    echo "  --output $SYMBOLS_FILE"
    echo ""
fi

echo "✅ review-init 完成。下一步：在 Step 0 引用上述 Derived Paths。"
