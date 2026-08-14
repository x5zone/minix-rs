#!/usr/bin/env bash
# review-gate-check.sh — Minix-RS Review 综合门控检查工具（C 路径 Phase 1 C.1.3）
#
# 用法：
#   tools/review-gate-check.sh <tool> <module> <doc-stem> [agent] [--strict]
#
# 检查项（每次 review 启动时自动跑）：
#   1. design 快照完整（{NN}-design.v*.md 存在）        → Gate H.1
#   2. outline 快照完整（{NN}-outline.v*.md 存在）       → Gate H.6
#   3. STATE.md 存在                                       → State 预检
#   4. scan 文件存在                                       → 制品完整性
#   5. SYMBOLS.md 存在                                     → Gate A
#   6. structure.md 存在                                   → Gate D-6
#   7. VERIFY-CHECK.md 存在                                → Gate G
#
# 输出：
#   - 单行 PASS/FAIL
#   - 详细问题清单（每个缺失项）
#   - 退出码：0=PASS, 1=FAIL, 2=参数错误
#
# Session #15 (2026-07-16) 创建 — C 路径 Phase 1 C.1.3

set -euo pipefail

if [[ $# -lt 3 ]]; then
  echo "Usage: $0 <tool> <module> <doc-stem> [agent] [--strict]" >&2
  echo "  tool     = trae | claude | codex" >&2
  echo "  module   = notes/rewrite/<module>/ 下的第一级目录名（如 fork-syscall-rewrite）" >&2
  echo "  doc-stem = 目标文档去扩展名（如 03-kmain-cstart）" >&2
  echo "  agent    = AI 标识（默认 glm；只能放在 flag 之前）" >&2
  echo "  --strict = 任何缺失即 exit 2（默认 exit 1）" >&2
  exit 2
fi

# 先解析位置参数，再解析 flag 参数
POSITIONAL=()
STRICT_MODE="false"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --strict) STRICT_MODE="true"; shift ;;
    --agent) POSITIONAL+=("$2"); shift 2 ;;
    --*) echo "Unknown option: $1" >&2; exit 2 ;;
    *) POSITIONAL+=("$1"); shift ;;
  esac
done

if [[ ${#POSITIONAL[@]} -lt 3 ]]; then
  echo "⛔ 位置参数不足，需要 tool + module + doc-stem" >&2
  exit 2
fi

set -- "${POSITIONAL[@]}"
TOOL="$1"
MODULE="$2"
DOC_STEM="$3"
AGENT="${4:-glm}"

if [[ "$TOOL" != "trae" && "$TOOL" != "claude" && "$TOOL" != "codex" ]]; then
  echo "⛔ tool 必须是 trae、claude 或 codex: ${TOOL}" >&2
  exit 2
fi

# 解析 doc 编号（{NN}-xxx → NN）
if [[ "${DOC_STEM}" =~ ^([0-9]{2})- ]]; then
  NN="${BASH_REMATCH[1]}"
else
  echo "⛔ doc-stem 必须是 {NN}-xxx 格式: ${DOC_STEM}" >&2
  exit 2
fi

# 定位项目根
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

# 计算路径
REVIEW_BASE=".review/${TOOL}/${MODULE}"
SCANS_DIR="${REVIEW_BASE}/scans"

# === Session #16 改进（2026-07-16，兼容历史迭代后缀）：
# 历史 session 把 R12/R2/... 等迭代号嵌入文件名（如 06-proc-init-boot-proc-r12-glm-scan.md）。
# gate-check 默认查找 ${DOC_STEM}-${AGENT}-{type}.md（标准），找不到时回退 glob {DOC_STEM}-*-${AGENT}-{type}.md（兼容）。
# 用 resolve_compat() 在 check() 前把路径替换为兼容通配结果。
if [[ "${TOOL}" == "trae" ]]; then
  STATE_FILE="${REVIEW_BASE}/STATE.md"
  SCAN_FILE="${SCANS_DIR}/${DOC_STEM}-${AGENT}-scan.md"
  STRUCTURE_FILE="${SCANS_DIR}/${DOC_STEM}-${AGENT}-structure.md"
  SYMBOLS_FILE="${SCANS_DIR}/${DOC_STEM}-${AGENT}-SYMBOLS.md"
  VERIFY_FILE="${REVIEW_BASE}/VERIFY-CHECK.md"
else
  STATE_FILE="${REVIEW_BASE}/STATE.md"
  DOC_DIR="${REVIEW_BASE}/${DOC_STEM}"
  SCAN_FILE="${DOC_DIR}/scan.md"
  STRUCTURE_FILE="${DOC_DIR}/structure.md"
  SYMBOLS_FILE="${DOC_DIR}/SYMBOLS.md"
  VERIFY_FILE="${REVIEW_BASE}/VERIFY-CHECK.md"
fi
# 历史 fallback：早期 session 用 VERIFY-CHECK-{NN}.md（带 doc 编号后缀）；canonical 是无后缀的 VERIFY-CHECK.md（与 review-init.sh / README 一致）。
VERIFY_NN_FILE="${REVIEW_BASE}/VERIFY-CHECK-${NN}.md"

# design 目录路径：尝试推断 stage（从 notes/rewrite/{module}/ 下所有 stage 找 doc 文件）
DESIGN_FILE=""
OUTLINE_FILE=""
OUTLINE_REVIEW_FILE=""
MODULE_DIR="notes/rewrite/${MODULE}"

latest_snapshot() {
  local pattern="$1"
  local best=""
  local best_version=-1
  local path version
  while IFS= read -r path; do
    [[ -f "$path" ]] || continue
    if [[ "$path" =~ \.v([0-9]+)\.md$ ]]; then
      version="${BASH_REMATCH[1]}"
      if (( version > best_version )); then
        best_version="$version"
        best="$path"
      fi
    fi
  done < <(compgen -G "$pattern" || true)
  printf '%s' "$best"
}

for stage_dir in "${MODULE_DIR}"/*/; do
  [[ ! -d "${stage_dir}" ]] && continue
  doc_file="${stage_dir}/${DOC_STEM}.md"
  if [[ -f "${doc_file}" ]]; then
    DESIGN_DIR="${stage_dir}.design"
    DESIGN_FILE="$(latest_snapshot "${DESIGN_DIR}/${NN}-design.v*.md")"
    OUTLINE_FILE="$(latest_snapshot "${DESIGN_DIR}/${NN}-outline.v*.md")"
    OUTLINE_REVIEW_FILE="$(latest_snapshot "${DESIGN_DIR}/${NN}-outline-review.v*.md")"
    break
  fi
done

# ===== 执行检查 =====
CHECKS_TOTAL=0
CHECKS_PASS=0
FAIL_LIST=()

# Session #16 新增：resolve_compat <type>  -> 如果 <SCANS_DIR>/<DOC_STEM>-<AGENT>-<type>.md 不存在，
# 但 <SCANS_DIR>/<DOC_STEM>-*-<AGENT>-<type>.md 存在（兼容历史迭代后缀如 -r12），回退到 glob 第一个结果。
# 必须把结果同时写到调用者变量名（用间接引用）
resolve_compat() {
  local type="$1"
  local standard="${SCANS_DIR}/${DOC_STEM}-${AGENT}-${type}.md"
  if [[ -f "${standard}" ]]; then
    echo "${standard}"
    return
  fi
  # glob：容忍 -r12 / -r2 / -new 等中段
  local matched
  matched=$(ls "${SCANS_DIR}/${DOC_STEM}"-*-"${AGENT}-${type}.md" 2>/dev/null | head -1 || true)
  if [[ -n "${matched}" && -f "${matched}" ]]; then
    echo "${matched}"
    return
  fi
  echo "${standard}"   # 都失败 → 返回标准名（让 check() 输出 MISSING）
}

check() {
  local name="$1"
  local path="$2"
  local gate="$3"
  CHECKS_TOTAL=$((CHECKS_TOTAL + 1))
  if [[ -f "${path}" ]]; then
    CHECKS_PASS=$((CHECKS_PASS + 1))
    echo "  ✅ ${name} → ${path}"
  else
    FAIL_LIST+=("${gate} | ${name} | ${path}")
    echo "  ❌ ${name} MISSING → ${path}"
  fi
}

echo "=== Review Gate Check ==="
echo "tool=${TOOL} module=${MODULE} doc-stem=${DOC_STEM} agent=${AGENT} strict=${STRICT_MODE}"
echo ""

echo "[Gate H.1] design 快照:"
check "design.md" "${DESIGN_FILE}" "H.1"
echo ""

echo "[Gate H.6] outline 快照:"
check "outline.md" "${OUTLINE_FILE}" "H.6"
check "outline-review.md" "${OUTLINE_REVIEW_FILE}" "H.6"
echo ""

echo "[State 预检] STATE.md:"
check "STATE.md" "${STATE_FILE}" "STATE"
echo ""

echo "[制品完整性] scan / structure / SYMBOLS:"
# Session #16：Trae 兼容历史 -r12 后缀；Claude/Codex 使用各自的标准 doc 目录。
if [[ "${TOOL}" == "trae" ]]; then
  SCAN_FILE="$(resolve_compat scan)"
  STRUCTURE_FILE="$(resolve_compat structure)"
  SYMBOLS_FILE="$(resolve_compat SYMBOLS)"
fi

check "scan.md" "${SCAN_FILE}" "0"
check "structure.md" "${STRUCTURE_FILE}" "D-6"
check "SYMBOLS.md" "${SYMBOLS_FILE}" "A"
echo ""

echo "[Gate G] VERIFY-CHECK:"
# canonical: VERIFY-CHECK.md（与 review-init.sh / README 一致）。fallback: VERIFY-CHECK-{NN}.md（历史后缀）→ scans/ 子目录。
SCANS_VERIFY_FILE="${SCANS_DIR}/${DOC_STEM}-${AGENT}-VERIFY-CHECK.md"
if [[ -f "${VERIFY_FILE}" ]]; then
  CHECKS_TOTAL=$((CHECKS_TOTAL + 1))
  CHECKS_PASS=$((CHECKS_PASS + 1))
  echo "  ✅ VERIFY-CHECK.md → ${VERIFY_FILE}"
elif [[ -f "${VERIFY_NN_FILE}" ]]; then
  CHECKS_TOTAL=$((CHECKS_TOTAL + 1))
  CHECKS_PASS=$((CHECKS_PASS + 1))
  echo "  ✅ VERIFY-CHECK.md → ${VERIFY_NN_FILE}（历史 {NN} 后缀）"
elif [[ -f "${SCANS_VERIFY_FILE}" ]]; then
  CHECKS_TOTAL=$((CHECKS_TOTAL + 1))
  CHECKS_PASS=$((CHECKS_PASS + 1))
  echo "  ✅ VERIFY-CHECK.md → ${SCANS_VERIFY_FILE}（scans/ 子目录兼容）"
else
  CHECKS_TOTAL=$((CHECKS_TOTAL + 1))
  FAIL_LIST+=("G | VERIFY-CHECK.md | ${VERIFY_FILE} (也尝试 ${VERIFY_NN_FILE} / ${SCANS_VERIFY_FILE})")
  echo "  ❌ VERIFY-CHECK.md MISSING"
fi
echo ""

# ===== 汇总 =====
CHECKS_FAIL=$((CHECKS_TOTAL - CHECKS_PASS))
echo "=== Summary ==="
echo "PASS: ${CHECKS_PASS}/${CHECKS_TOTAL}"
echo "FAIL: ${CHECKS_FAIL}"
echo ""

if [[ ${CHECKS_FAIL} -eq 0 ]]; then
  echo "✅ ALL GATES PASS"
  exit 0
fi

echo "❌ FAILED CHECKS:"
for fail in "${FAIL_LIST[@]}"; do
  echo "  - ${fail}"
done
echo ""
echo "⛔ Review gate-check FAIL（exit $(${STRICT_MODE} && echo 2 || echo 1)）— 2026-07-16 Session #15 (C 路径 Phase 1 C.1.3)"
echo "   触发条件：.design/outline/outline-review 缺失 → Step 0.3 嵌入生成（不中断 review）"
echo "   触发条件：scan/structure/SYMBOLS 缺失 → 重跑 review Step 1.5/2/0.5"
echo "   触发条件：STATE/VERIFY 缺失 → 补充 state 文件"

if [[ "${STRICT_MODE}" == "true" ]]; then
  exit 2
fi
exit 1
