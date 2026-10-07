#!/usr/bin/env bash
# check-staged-inflight.sh — 提交前断言：他人/维护者的在制文件不得出现在暂存区
#
# 为什么存在：notes 目录迁移期间，同一次误提交发生四轮——每次对 rewrite-notes/ 这类
# 含在制文件的目录跑 `git add -A` 或 `git add -u`，都会把维护者未提交的编辑一起暂存。
# 靠自觉不可靠，故固化为一条命令；任何批量 add 之后、commit 之前必须跑它。
#
# 用法：tools/check-staged-inflight.sh            # 检查全部暂存项
#       tools/check-staged-inflight.sh --list     # 只看当前登记的在制路径
#
# 退出码：0 = 暂存区干净；1 = 有在制文件被暂存（拒绝提交）；2 = 用法错误
#
# 在制清单来源：本脚本的 INFLIGHT 数组。维护者开工或收工后请增删这里的路径。

set -uo pipefail
cd "$(git rev-parse --show-toplevel)"

# 维护者明确「只搬路径、不代提交」的文件（notes 迁移期间登记）
# `2026-10-08` 待办合并与清理轮：旧结构债台账存根已删（全文已由 `STRUCTURAL-DEBT-REGISTER-20261008.md` 承担），
# 本清单同步摘除那一条；齐平待办清单仍在制，保留登记。
INFLIGHT=(
  "rewrite-notes/coordination/TODO-3ARCH-PARITY-20261006.md"
)

if [[ "${1:-}" == "--list" ]]; then
  printf '%s\n' "${INFLIGHT[@]}"
  exit 0
fi

bad=0
for path in "${INFLIGHT[@]}"; do
  if git diff --cached --name-only | grep -qxF "$path"; then
    echo "❌ 在制文件被暂存：$path" >&2
    echo "   退回索引到 HEAD 内容、保留工作树编辑：" >&2
    echo "   git update-index --cacheinfo 100644,$(git rev-parse "HEAD:$path" 2>/dev/null || echo '<blob>'),"$path"" >&2
    bad=1
  fi
done

if [[ $bad -eq 1 ]]; then
  echo "处置：先按上面的命令退回，再重新提交（勿用 git commit -a 绕过）" >&2
  exit 1
fi
echo "✅ 暂存区无登记的在制文件（检查 ${#INFLIGHT[@]} 条）"
