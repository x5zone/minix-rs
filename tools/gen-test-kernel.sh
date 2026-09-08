#!/usr/bin/env bash
# gen-test-kernel.sh — single source of truth for test-kernel Cargo.toml layout.
#
# The 23 bootstrap test-kernels under os/qemu-tests/test-kernels/kernel/
# bootstrap/ each carry a Cargo.toml that is one of two arch families
# (uefi / opensbi) with a few per-kernel dependency toggles. Historically
# every configuration evolution (e.g. the 2026-08-14 `[[bin]] test = false`
# batch fix) had to touch all files by hand — the duplication was the bug.
#
# Modes:
#   tools/gen-test-kernel.sh --new <name> <x86_64|aarch64|riscv64> [flags]
#       Create a new test-kernel directory with a templated Cargo.toml.
#       Flags: --qemu-test --no-kernel --no-arch --with-platform
#              --uefi-no-alloc
#       src/main.rs is NOT generated: each main.rs is the test's own
#       scenario logic, not configuration — copy the closest sibling
#       test-kernel's src/main.rs as the starting point.
#   tools/gen-test-kernel.sh --apply
#       Regenerate every existing Cargo.toml from the template: canonical
#       dependency order, no dead [profile] blocks (workspace members'
#       profiles are ignored by cargo — only the workspace root's count),
#       unified [[bin]] comment.
#   tools/gen-test-kernel.sh --check
#       Dry-run of --apply: exit 1 with a diff if any file would change
#       (review/CI gate against configuration drift).
#
# The template deliberately does NOT emit [profile.dev]/[profile.release]:
# every test-kernel is a workspace member built through the workspace root
# (run_all.sh: cargo build --manifest-path $OS_ROOT/Cargo.toml -p $pkg), so
# member-level profiles are dead configuration — cargo even warns about it
# ("profiles for the non root package will be ignored").

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BOOTSTRAP_DIR="$ROOT/os/qemu-tests/test-kernels/kernel/bootstrap"

# ── Template ─────────────────────────────────────────────────────────────
# Emit a Cargo.toml for one test-kernel. Arguments:
#   $1 name, $2 uefi_alloc, $3 opensbi, $4 qemu_test, $5 with_platform,
#   $6 no_kernel, $7 no_arch, $8 uefi_no_alloc   (flags are "1"/"0")
emit_cargo_toml() {
    local name="$1" uefi_alloc="$2" opensbi="$3" qemu_test="$4" \
          with_platform="$5" no_kernel="$6" no_arch="$7" uefi_no_alloc="$8" no_shim="$9"

    echo '[package]'
    echo "name = \"$name\""
    echo 'version = "0.1.0"'
    echo 'edition = "2024"'
    echo
    echo '[dependencies]'
    if [ "$opensbi" = "1" ]; then
        echo 'minix-arch = { workspace = true, default-features = false }'
        echo 'minix-plat = { workspace = true }'
        echo 'minix-types = { workspace = true }'
        echo 'minix-boot = { workspace = true }'
        if [ "$qemu_test" = "1" ]; then
            echo 'minix-kernel = { workspace = true, default-features = false, features = ["qemu_test"] }'
        elif [ "$no_kernel" = "0" ]; then
            echo 'minix-kernel = { workspace = true, default-features = false }'
        fi
        if [ "$no_shim" = "0" ]; then
            echo 'boot-shim = { workspace = true, default-features = false, features = ["opensbi"] }'
        fi
    else
        if [ "$uefi_no_alloc" = "1" ]; then
            echo 'uefi = { version = "0.33" }'
        elif [ "$uefi_alloc" = "1" ]; then
            echo 'uefi = { version = "0.33", features = ["alloc"] }'
            echo '# uefi "global_allocator" feature deliberately NOT enabled — this'
            echo '# test-kernel defines its own #[global_allocator]. Enabling uefi'"'"'s'
            echo '# would conflict under feature unification in host `cargo test`'
            echo '# (boot-shim lib test).'
        fi
        if [ "$no_arch" = "0" ]; then
            echo 'minix-arch = { workspace = true, default-features = false }'
        fi
        echo 'minix-plat = { workspace = true }'
        if [ "$with_platform" = "1" ]; then
            echo 'minix-platform = { workspace = true }'
        fi
        echo 'minix-types = { workspace = true }'
        echo 'minix-boot = { workspace = true }'
        if [ "$qemu_test" = "1" ]; then
            echo 'minix-kernel = { workspace = true, default-features = false, features = ["qemu_test"] }'
        elif [ "$no_kernel" = "0" ]; then
            echo 'minix-kernel = { workspace = true, default-features = false }'
        fi
        if [ "$no_shim" = "0" ]; then
            echo 'boot-shim = { workspace = true, features = ["uefi"] }'
        fi
    fi
    echo
    echo '[[bin]]'
    echo "name = \"$name\""
    echo 'path = "src/main.rs"'
    if [ "$opensbi" = "1" ]; then
        echo '# Bare-metal firmware binary: never compile a host test harness'
        echo '# (host `cargo test` would link std → duplicate panic_impl /'
        echo '# global_allocator conflicts with the custom allocator).'
    else
        echo '# Bare-metal firmware binary: never compile a host test harness'
        echo '# (host `cargo test` would link std → duplicate panic_impl /'
        echo '# global_allocator conflicts with uefi).'
    fi
    echo 'test = false'
}

# ── Flag detection from an existing Cargo.toml ───────────────────────────
# Sets the flag variables (globals) for one file.
detect_flags() {
    local file="$1"
    UEFI_ALLOC=0; OPENSBI=0; QEMU_TEST=0; WITH_PLATFORM=0
    NO_KERNEL=1; NO_ARCH=1; UEFI_NO_ALLOC=0; NO_SHIM=1
    grep -q 'uefi = { version = "0.33", features = \["alloc"\] }' "$file" && UEFI_ALLOC=1 || true
    grep -q 'uefi = { version = "0.33" }' "$file" && UEFI_NO_ALLOC=1 || true
    grep -q 'features = \["opensbi"\]' "$file" && OPENSBI=1 || true
    grep -q 'features = \["qemu_test"\]' "$file" && QEMU_TEST=1 || true
    grep -q '^minix-platform = ' "$file" && WITH_PLATFORM=1 || true
    grep -q '^minix-kernel = ' "$file" && NO_KERNEL=0 || true
    grep -q '^minix-arch = ' "$file" && NO_ARCH=0 || true
    grep -q '^boot-shim = ' "$file" && NO_SHIM=0 || true
}

usage() {
    sed -n '2,26p' "${BASH_SOURCE[0]}" | sed 's/^#\{1,2\} \{0,1\}//'
    exit 2
}

cmd_new() {
    local name="$1" arch="$2"; shift 2
    local opensbi=0 uefi_alloc=1 uefi_no_alloc=0 qemu_test=0 with_platform=0 no_kernel=0 no_arch=0
    case "$arch" in
        x86_64|aarch64) opensbi=0 ;;
        riscv64) opensbi=1; uefi_alloc=0 ;;
        *) echo "unknown arch: $arch (use x86_64|aarch64|riscv64)" >&2; exit 2 ;;
    esac
    while [ $# -gt 0 ]; do
        case "$1" in
            --qemu-test) qemu_test=1 ;;
            --no-kernel) no_kernel=1 ;;
            --no-arch) no_arch=1 ;;
            --with-platform) with_platform=1 ;;
            --uefi-no-alloc) uefi_no_alloc=1; uefi_alloc=0 ;;
            *) echo "unknown flag: $1" >&2; exit 2 ;;
        esac
        shift
    done
    local dir="$BOOTSTRAP_DIR/$name"
    if [ -e "$dir" ]; then echo "already exists: $dir" >&2; exit 2; fi
    mkdir -p "$dir/src"
    emit_cargo_toml "$name" "$uefi_alloc" "$opensbi" "$qemu_test" \
        "$with_platform" "$no_kernel" "$no_arch" "$uefi_no_alloc" 0 \
        > "$dir/Cargo.toml"
    echo "created $dir/Cargo.toml"
    echo "src/main.rs is test scenario logic — copy the closest sibling"
    echo "test-kernel's src/main.rs as the starting point."
    echo "next: register the crate path in os/Cargo.toml [workspace] members"
    echo "      and wire a QEMU run step in run_all.sh."
}

cmd_apply_or_check() {
    local mode="$1" rc=0
    local file name regenerated
    for file in "$BOOTSTRAP_DIR"/*/Cargo.toml; do
        name="$(basename "$(dirname "$file")")"
        detect_flags "$file"
        regenerated="$(emit_cargo_toml "$name" "$UEFI_ALLOC" "$OPENSBI" \
            "$QEMU_TEST" "$WITH_PLATFORM" "$NO_KERNEL" "$NO_ARCH" "$UEFI_NO_ALLOC" "$NO_SHIM")"
        if [ "$mode" = "--check" ]; then
            if ! diff -u "$file" <(printf '%s\n' "$regenerated") > /tmp/gtk_check.diff 2>&1; then
                echo "DRIFT: $file"
                cat /tmp/gtk_check.diff
                rc=1
            fi
        else
            printf '%s\n' "$regenerated" > "$file"
            echo "normalized: $file"
        fi
    done
    return $rc
}

cmd_apply() { cmd_apply_or_check --apply; }
cmd_check() { cmd_apply_or_check --check; }

case "${1:-}" in
    --new) shift; [ $# -ge 2 ] || usage; cmd_new "$@" ;;
    --apply) cmd_apply ;;
    --check) cmd_check ;;
    *) usage ;;
esac
