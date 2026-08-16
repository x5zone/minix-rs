#!/usr/bin/env bash
# check-rs-unwired.sh — T7 门禁：RS 服务生产代码（非测试）中的接线占位符审计
#
# 背景：
#   RS 是 root system process（RSYS_F），内核不重启 RS——一条未接线的路径若以
#   `unimplemented!()`/`todo!()` 静默混入生产代码，接线遗漏 = 整机不可用。
#   本脚本强制：**生产代码中的每个占位符必须带 fail-closed 文档契约**——
#   即所在注释引用其归属文档（`{NN}-rs-*.md`，06/12/18/19 等）。
#
# 检查规则：
#   1. 排除 `#[cfg(test)]` 测试模块（mock 用 `unimplemented!()` 表示"本测试不触达"是合法的）
#   2. 生产代码中每个 `unimplemented!()`/`todo!()`：
#      - 必须能在同一行或前 5 行注释中找到 `NN-rs-*.md` 文档引用
#      - 否则 FAIL（新增未接线路径缺 fail-closed 契约）
#
# 用法： tools/check-rs-unwired.sh [rs-src-dir]
# 退出码：0=PASS, 1=FAIL
#
# Session 2026-08-15 创建（T7）。

set -euo pipefail

RS_SRC="${1:-$(cd "$(dirname "$0")/.." && pwd)/os/servers/rs/src}"

if [[ ! -d "${RS_SRC}" ]]; then
  echo "❌ RS source dir not found: ${RS_SRC}" >&2
  exit 2
fi

# 1) 收集非测试代码中的占位符（brace-counting 排除 `#[cfg(test)] mod tests {}`）
MAPFILE_OCC=()
while IFS=: read -r file line text; do
  MAPFILE_OCC+=("${file}:${line}:${text}")
done < <(
  awk '
    /^#\[cfg\(test\)\]/ { in_test = 1 }
    in_test {
      for (i = 1; i <= length($0); i++) {
        c = substr($0, i, 1)
        if (c == "{") depth++
        if (c == "}") { depth--; if (depth == 0) { in_test = 0 } }
      }
      next
    }
    /unimplemented!|todo!/ { print FILENAME ":" FNR ":" $0 }
  ' "${RS_SRC}"/*.rs
)

if [[ ${#MAPFILE_OCC[@]} -eq 0 ]]; then
  echo "✅ PASS: no unwired markers in production RS code."
  exit 0
fi

# 2) 每个占位符必须有归属文档契约（前 5 行内 `NN-rs-*.md`）
FAILED=0
for occ in "${MAPFILE_OCC[@]}"; do
  file="${occ%%:*}"
  rest="${occ#*:}"
  line="${rest%%:*}"
  text="${rest#*:}"

  # 前 5 行 + 本行拼成上下文
  start=$((line - 5))
  [[ ${start} -lt 1 ]] && start=1
  ctx=$(sed -n "${start},${line}p" "${file}")

  if grep -qE '(0[0-9]|1[0-9])-rs-[a-z0-9-]+\.md' <<<"${ctx}"; then
    echo "✅ ${file}:${line} — ${text} (doc contract present)"
  else
    echo "❌ ${file}:${line} — ${text} (missing NN-rs-*.md contract in preceding comment)"
    FAILED=$((FAILED + 1))
  fi
done

if [[ ${FAILED} -gt 0 ]]; then
  echo ""
  echo "**Result**: FAIL (${FAILED} unwired marker(s) without a fail-closed doc contract)"
  echo "Fix: annotate each production placeholder with its owning document"
  echo "(06-rs-main-loop.md / 12-rs-init-run.md / 18-rs-self-lifecycle.md / 19-rs-external-interfaces.md)."
  exit 1
fi

echo ""
echo "**Result**: PASS (all production unwired markers carry a doc contract)"
exit 0
