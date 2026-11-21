#!/usr/bin/env bash
# publication-changed-docs.sh — 选出变更的正式篇目（publication-gates 用）。
# 用法：tools/publication-changed-docs.sh <base> <sha>
#       tools/publication-changed-docs.sh --self-test
set -euo pipefail

filter_docs() {
  grep -E '^rewrite-notes/[0-9]+-stage-[^/]+/[0-9]{2}-[^/]+\.md$' \
    | grep -vE '/(draft|archive)/' \
    | grep -vE '/([^/]*(design|evidence|review|forensics|brief|note|learn|todo|archive)[^/]*\.md)$'
}

if [ "${1:-}" = "--self-test" ]; then
  good='rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md'
  draft='rewrite-notes/01-stage-kernel/draft/16-smp.md'
  design='rewrite-notes/01-stage-kernel/04-design-backlog-round28.md'
  out=$(printf '%s\n%s\n%s\n' "$good" "$draft" "$design" | filter_docs || true)
  if [ "$out" != "$good" ]; then
    echo "publication-changed-docs --self-test FAIL：$out" >&2
    exit 1
  fi
  echo "publication-changed-docs --self-test PASS"
  exit 0
fi

base=${1:?base}; sha=${2:?sha}
git diff --name-only "$base" "$sha" -- 'rewrite-notes/*-stage-*/*.md' | filter_docs || true
