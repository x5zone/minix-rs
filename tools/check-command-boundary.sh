#!/usr/bin/env bash
# check-command-boundary.sh - enforce the command-layer dependency contract
# (18-stage-commands/99-global-concepts.md §1).
#
# Hard rules checked here (source-level, not Cargo.toml edges — the ABI is
# SUPPOSED to flow to commands transitively through minix-sys/minix-rt):
#   1. No file under os/commands/ imports minix_types directly. Commands see
#      errno and friends only through the minix-sys re-exports.
#   2. No file under os/commands/ touches minix_sys::ipc (or constructs
#      messages through any other server protocol module). The libc face is
#      the minix-sys top level; message construction belongs to servers and
#      to the libsys modules themselves.
#
# Scope ruling (OQ-N5, 2026-09-21): this boundary guards PRODUCTION
# layering, so `#[cfg(test)]`-gated modules are stripped before the checks
# below — test code may import server protocol types to exercise them.
# Findings keep their original file line numbers.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

errors=0

fail() {
  printf 'FAIL: %s\n' "$1" >&2
  errors=$((errors + 1))
}

# Emit every line of the command tree that is NOT inside a
# `#[cfg(test)]`-gated module, as file:lineno:content (the shape the checks
# below parse). Brace counting strips the whole gated module; original line
# numbers are preserved so findings stay clickable.
strip_test_modules() {
  while IFS= read -r file; do
    awk -v file="$file" '
      /^#[[:space:]]*\[cfg\(test\)\][[:space:]]*$/ { skip=1; depth=0; next }
      skip {
        depth += gsub(/{/, "{") - gsub(/}/, "}");
        if (depth <= 0 && /}/) skip=0;
        next
      }
      { print file ":" FNR ":" $0 }
    ' "$file"
  done < <(find os/commands -name '*.rs')
}

# 1. Direct minix_types imports (server protocol types, message layouts).
while IFS=: read -r file line rest; do
  [ -n "$file" ] || continue
  fail "command file imports minix_types directly: $file:$line$rest"
done < <(strip_test_modules | grep "minix_types" || true)

# 2. Direct IPC / server-protocol reach-through from the command layer.
while IFS=: read -r file line rest; do
  [ -n "$file" ] || continue
  fail "command file reaches into the libsys/IPC layer: $file:$line$rest"
done < <(strip_test_modules | grep -E "minix_sys::ipc|minix_sys :: ipc|use minix_sys::\{[^}]*\bipc\b" || true)

if [ "$errors" -eq 0 ]; then
  printf 'OK: command layer dependency boundary clean (99-global-concepts.md §1).\n'
else
  printf '%d violation(s). Fix: reach for the minix-sys top-level wrappers;\n' "$errors"
  printf 'a missing wrapper is registered at 14-stage-runtime, never hand-rolled in a command.\n' >&2
  exit 1
fi
