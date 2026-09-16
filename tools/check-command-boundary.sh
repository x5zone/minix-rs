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

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

errors=0

fail() {
  printf 'FAIL: %s\n' "$1" >&2
  errors=$((errors + 1))
}

# 1. Direct minix_types imports (server protocol types, message layouts).
while IFS=: read -r file line rest; do
  [ -n "$file" ] || continue
  fail "command file imports minix_types directly: $file:$line$rest"
done < <(grep -rn "minix_types" os/commands/ --include='*.rs' || true)

# 2. Direct IPC / server-protocol reach-through from the command layer.
while IFS=: read -r file line rest; do
  [ -n "$file" ] || continue
  fail "command file reaches into the libsys/IPC layer: $file:$line$rest"
done < <(grep -rnE "minix_sys::ipc|minix_sys :: ipc|use minix_sys::\{[^}]*\bipc\b" os/commands/ --include='*.rs' || true)

if [ "$errors" -eq 0 ]; then
  printf 'OK: command layer dependency boundary clean (99-global-concepts.md §1).\n'
else
  printf '%d violation(s). Fix: reach for the minix-sys top-level wrappers;\n' "$errors"
  printf 'a missing wrapper is registered at 14-stage-runtime, never hand-rolled in a command.\n' >&2
  exit 1
fi
