//! Host-shape pin test for riscv64 instruction-cache synchronization.
//!
//! `os/arch/src/riscv64/cache_sync.rs` is gated wholesale behind
//! `#[cfg(target_arch = "riscv64")]`, so the host never compiles it; this
//! file pins the load-bearing shape with `include_str!`:
//!
//! 1. the local hart's synchronization action must be `FENCE.I` (a local
//!    hart's earlier writes become visible to its own later instruction
//!    fetches — the loader scenario is exactly "this hart writes, the same
//!    hart fetches afterwards");
//! 2. a hart that did not perform the stores needs its own fence, so the
//!    implementor must issue the SBI RFENCE `remote_fence_i` broadcast after
//!    the local fence. `hart_mask_base = -1` addresses every available hart,
//!    which is the portable form the arch layer can use without a kernel
//!    hart-mask table; a nonzero SBI status fails fast;
//! 3. the file documentation must record the RFENCE platform prerequisite
//!    and the broadcast shape, so a later reader cannot mistake it for an
//!    optional optimization or "half-wired" remote arm.
//!
//! The real-hardware judgment boundary matches `arm64_cache_sync_pin.rs`:
//! QEMU does not model this class of cache mismatch, so a green gate only
//! proves "nothing was written wrong".

/// Full source text of the file (comments and docs included).
fn src() -> &'static str {
    include_str!("../src/riscv64/cache_sync.rs")
}

/// Code skeleton with comments stripped (same as `arm64_cache_sync_pin.rs::code`).
fn code() -> String {
    let mut out = String::new();
    for line in src().lines() {
        let cut = match line.find("//") {
            Some(i) => &line[..i],
            None => line,
        };
        out.push_str(cut);
        out.push(' ');
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn local_fence_i_is_the_synchronization_action() {
    let c = code();
    assert!(c.contains(r#"asm!("fence.i")"#), "local sync action is not FENCE.I");
}

#[test]
fn remote_fence_i_broadcasts_to_all_harts() {
    let c = code();
    for needle in [
        "ecall",
        "SBI_RFENCE_EID",
        "SBI_RFENCE_REMOTE_FENCE_I_FID",
        "SBI_HART_MASK_ALL",
        "0x5246_4E43",
    ] {
        assert!(
            c.contains(needle),
            "remote-fence shape missing: {needle}"
        );
    }
    assert!(
        c.contains("remote_fence_i"),
        "the SBI failure path lost its remote_fence_i diagnostic"
    );
}

#[test]
fn smp_binding_is_documented_in_file() {
    let s = src();
    assert!(
        s.contains("remote_fence_i") && s.contains("RFENCE"),
        "file does not record the SBI RFENCE remote-fence contract"
    );
    assert!(
        s.contains("all available harts") || s.contains("hart_mask_base"),
        "file does not document the broadcast form (hart_mask_base = -1)"
    );
    assert!(
        !s.contains("local fence is complete for the current execution model"),
        "stale BSP-only rationale survived: the remote arm is now implemented"
    );
}
