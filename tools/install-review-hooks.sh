#!/usr/bin/env bash
# install-review-hooks.sh — 安装 V3 review 的本地 pre-commit 门
#
# 用法：tools/install-review-hooks.sh [--force]
#
# 安装后，提交时自动：
#   1. 对暂存的 rewrite-notes/**/*.md 跑 doc-citation-check.py（引文缺行即拒绝提交）；
#   2. 跑 eval-trace-check.py / doc-citation-check.py / doc-symbol-drift.sh 的自测
#      （门自身坏了就拒绝提交，避免“门恒绿”）。
#
# 这是本地兜底；共享门仍在 .github/workflows/rules-consistency.yml。
# 已有非托管 pre-commit 时先备份为 pre-commit.pre-v3，除非 --force。

set -euo pipefail

force=0
[ "${1:-}" = "--force" ] && force=1

root="$(git rev-parse --show-toplevel)"
hook="$root/.git/hooks/pre-commit"
marker="# managed-by: install-review-hooks.sh (V3)"

if [ -f "$hook" ] && ! grep -qF "$marker" "$hook"; then
  if [ "$force" -eq 1 ]; then
    cp "$hook" "$hook.pre-v3"
    echo "已备份现有 pre-commit 到 $hook.pre-v3"
  else
    echo "已有非托管 pre-commit：$hook" >&2
    echo "用 --force 覆盖（会备份为 pre-commit.pre-v3）" >&2
    exit 1
  fi
fi

cat >"$hook" <<'HOOK'
#!/usr/bin/env bash
# managed-by: install-review-hooks.sh (V3)
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

staged=$(git diff --cached --name-only --diff-filter=ACM)
if [ -z "$staged" ]; then
  exit 0
fi

# 1. 暂存文档的引文对账
while IFS= read -r file; do
  case "$file" in
    rewrite-notes/*.md)
      if [ -f "$file" ]; then
        python3 tools/doc-citation-check.py "$file" --root .
      fi
      ;;
  esac
done <<<"$staged"

# 2. 门自测：门坏了就不许提交
python3 tools/eval-trace-check.py --self-test >/dev/null
python3 tools/doc-citation-check.py --self-test >/dev/null
bash tools/doc-symbol-drift.sh --self-test >/dev/null
HOOK

chmod +x "$hook"
echo "已安装 pre-commit 门：$hook"
