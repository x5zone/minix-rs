#!/usr/bin/env bash
# Build and run the x86_64 ATF startup-stub spawn fixture.
#
# The child is a freestanding static ELF: startup-minix.c provides `_start`
# (RBX -> RDI -> startup_c_main) and the fixture provides startup_c_main.
# The parent uses ptrace to install a synthetic ps_strings descriptor and set
# RBX at exec, exactly the kernel's x86_64 birth contract.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
CC="${CC:-cc}"
OUT="$(mktemp -d /tmp/x86_64-startup-fixture.XXXXXX)"
trap 'rm -rf "$OUT"' EXIT

"$CC" -O2 -static -nostdlib -no-pie -DSTARTUP_MINIX_TEST \
    -I "$ROOT/tools/atf-c-compat" \
    "$ROOT/tools/atf-c-compat/startup-minix.c" \
    "$ROOT/tools/atf-c-compat/tests/x86_64_startup_fixture.c" \
    -o "$OUT/child"

readelf -h "$OUT/child" | grep -q 'Type:.*EXEC' || {
    echo "FAIL: child is not a static ET_EXEC" >&2; exit 1; }
nm "$OUT/child" | grep -q ' T _start$' || {
    echo "FAIL: child has no _start" >&2; exit 1; }

"$CC" -O2 -Wall -Wextra \
    "$ROOT/tools/atf-c-compat/tests/x86_64_startup_spawn.c" \
    -o "$OUT/spawn"

"$OUT/spawn" "$OUT/child"
