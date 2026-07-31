#!/usr/bin/env bash
# review-init.sh — Minix-RS Review 初始化助手
#
# 用法: review-init.sh {tool} {doc-path} [agent] [--design-policy strict|optional|required] [--require-multi-agent] [--size-adaptive]
# 例:   review-init.sh trae notes/rewrite/fork-syscall-rewrite/03-stage-kernel/03-kmain-cstart.md glm
#       review-init.sh trae notes/rewrite/fork-syscall-rewrite/04-platform-discovery/04-platform-discovery.md kimi --design-policy strict --require-multi-agent
#
# 功能:
#   1. 从 doc-path 自动计算 {module} / {stage} / {doc-stem}
#   2. mkdir -p 对应工具的标准目录（.review/{tool}/{module}/scans/ 等）
#   3. 输出 Derived Paths 表格供 agent 在 Step 0 引用
#   4. 若 STATE.md 已存在，运行 review-state-validate.py 预检
#   5. 若 STATE.md 不存在，生成空 STATE.md 骨架
#   6. **NEW 2026-07-16**: Design 存在性预检（按 --design-policy 决定 strict/optional/required）
#   7. **NEW 2026-07-16**: --require-multi-agent 强制验证 Gate G 必须 multi-agent
#   8. **NEW 2026-07-16**: --size-adaptive 输出 size-adaptive round 推荐
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

# ===== 默认值 =====
DESIGN_POLICY="strict"           # strict | optional | required
REQUIRE_MULTI_AGENT="false"       # true | false
SIZE_ADAPTIVE="false"             # true | false

# ===== 参数校验 + 解析 =====
if [[ $# -lt 2 ]]; then
    echo "用法: $0 {tool} {doc-path} [agent] [--design-policy strict|optional|required] [--require-multi-agent] [--size-adaptive]" >&2
    echo "  tool     = trae | claude" >&2
    echo "  doc-path = 相对项目根的文档路径（如 notes/rewrite/{module}/{stage}/{doc}.md）" >&2
    echo "  agent    = AI 标识（Trae: glm/kimi/ds/qwen/seed；Claude 可省略）" >&2
    echo "  --design-policy   = strict（默认；缺失 design 阻断 review 启动 / review 内部 Step 0.3 嵌入生成）" >&2
    echo "                      | optional（缺失 design 仅警告，不阻断；用于跨文档复用场景）" >&2
    echo "                      | required（缺失 design 则立即报错退出）" >&2
    echo "  --require-multi-agent = 报告 P0≥1 时强制多 agent 验证 Gate G" >&2
    echo "  --size-adaptive    = 输出 size-adaptive round 推荐（>1000 行文档分轮）" >&2
    exit 1
fi

# 先提取位置参数，再解析 flag 参数（支持混合顺序）
POSITIONAL=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        --design-policy)
            DESIGN_POLICY="$2"
            shift 2
            ;;
        --require-multi-agent)
            REQUIRE_MULTI_AGENT="true"
            shift
            ;;
        --size-adaptive)
            SIZE_ADAPTIVE="true"
            shift
            ;;
        --help|-h)
            echo "用法见 review-init.sh 顶部"
            exit 0
            ;;
        *)
            POSITIONAL+=("$1")
            shift
            ;;
    esac
done

if [[ ${#POSITIONAL[@]} -lt 2 ]]; then
    echo "❌ 位置参数不足，需要 tool + doc-path" >&2
    exit 1
fi

set -- "${POSITIONAL[@]}"
TOOL="$1"
DOC_PATH="$2"
AGENT="${3:-}"

if [[ "$TOOL" != "trae" && "$TOOL" != "claude" ]]; then
    echo "❌ tool 必须是 trae 或 claude，得到: $TOOL" >&2
    exit 1
fi

if [[ ! "$DESIGN_POLICY" =~ ^(strict|optional|required)$ ]]; then
    echo "❌ --design-policy 必须是 strict / optional / required，得到: $DESIGN_POLICY" >&2
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

## Resume Point（NEW 2026-07-16，跨 session 续审必填）
> 当 session 中断时强制填写，详细协议见 [review-process.md §一.附录 A.2](#)
- **Next Session Resume Point**: —（待首次 session 设置）
- **Last Session Status**: —
- **必读文件清单**: scan.md + design.md + 结构脚手架（按 Step 0.5.6 6维反查决定）

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

# ===== Design 存在性预检（NEW 2026-07-16）=====
# 触发条件：完整/深度/设计优先 review 模式；review-process.md §Step 0 必须做
echo ""
echo "=== Design 预检（NEW Step 0 requirement）==="
# 提取 doc 编号前两位（如 03-kmain-cstart → 03），用于匹配 {NN}-design.v{N}.md 模式
DOC_NN="$(echo "$DOC_STEM" | cut -d- -f1)"
DESIGN_DIR="$(dirname "$DOC_PATH_NORMALIZED")/.design"
if [[ -d "$DESIGN_DIR" ]]; then
    DESIGN_FILES=$(ls "$DESIGN_DIR"/"${DOC_NN}"-design*.md 2>/dev/null || true)
    if [[ -n "$DESIGN_FILES" ]]; then
        echo "✅ Design 已存在（policy=$DESIGN_POLICY）："
        echo "$DESIGN_FILES" | sed 's/^/   /'
    else
        case "$DESIGN_POLICY" in
            strict)
                echo "⛔ Design MISSING（policy=strict）：$DESIGN_DIR/${DOC_NN}-design.v*.md"
                echo "   下一步：启动 review 后，AI 将自动执行 Step 0.3 嵌入生成（design-structure → outline → outline-review → design）"
                echo "   详见：prompt/review-rules/review-process.md §Step 0.3 缺失即生成"
                echo "   阻断 review 启动（exit 2）— 改用 --design-policy optional 可跳过阻断，review 内部 Step 0.3 仍会生成"
                exit 2
                ;;
            optional)
                echo "⚠️  Design MISSING（policy=optional）：$DESIGN_DIR/${DOC_NN}-design.v*.md"
                echo "   将仅 WARN 不阻断 review；本 review 不要求专属 design 但建议通过引用复用相邻文档 design"
                ;;
            required)
                echo "⛔ Design REQUIRED（policy=required）但缺失：$DESIGN_DIR/${DOC_NN}-design.v*.md"
                echo "   立即退出。请先生成 design 后再 review。"
                exit 2
                ;;
        esac
    fi
else
    echo "⚠️ Design 目录不存在：$DESIGN_DIR"
    case "$DESIGN_POLICY" in
        strict)
            echo "   policy=strict：阻断 review 启动（exit 2）— 改用 --design-policy optional 跳过，review 内部 Step 0.3 仍会生成"
            exit 2
            ;;
        optional)
            echo "   policy=optional，仅警告不阻断"
            ;;
        required)
            echo "   立即退出。"
            exit 2
            ;;
    esac
fi

# ===== Multi-Agent 提示（NEW 2026-07-16）=====
if [[ "$REQUIRE_MULTI_AGENT" == "true" ]]; then
    echo ""
    echo "=== Multi-Agent 验证（Gate G 强化）==="
    echo "⛔ 已设置 --require-multi-agent："
    echo "   若本 review 报告 P0 ≥ 1，Gate G 必须由不同 agent 执行 VERIFY-CHECK"
    echo "   当前 agent: ${AGENT:-(未指定)}"
    if [[ -z "$AGENT" ]]; then
        echo "   ⚠️  警告：未指定 agent，同 agent 偏差风险上升"
    fi
fi

# ===== Size-Adaptive Round 推荐（NEW 2026-07-16）=====
if [[ "$SIZE_ADAPTIVE" == "true" ]]; then
    DOC_LINES=$(wc -l < "$DOC_PATH_NORMALIZED")
    echo ""
    echo "=== Size-Adaptive Round 推荐 ==="
    echo "文档行数: $DOC_LINES"
    echo ""
    echo "| 文档行数 | rounds 推荐 | 备注 |"
    echo "|---------|------------|------|"
    if [[ $DOC_LINES -lt 500 ]]; then
        echo "| < 500    | 1 round（Step 0-7 一轮完成） | 小文档单轮可完成，无需分阶段 |"
    elif [[ $DOC_LINES -lt 1501 ]]; then
        echo "| 500-1500 | 2 rounds | R1 正确性 + R2 卓越性 |"
    else
        echo "| > 1500   | 4 rounds | R1 正确性 + R2 卓越性 + R3 patterns + R4 跨文档 |"
    fi
    echo ""
    echo "⏸ 中断策略：若 session 内上下文 >80%，按 review-process.md §一.附录 A.2 写 Resume Point"
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
