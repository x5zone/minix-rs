//! Reply encoding — the wire-format half of the VM's reply path.
//!
//! V12-P2-1: this module owns the pure "VmReply → IPC message" translation
//! that used to live in `vm_server.rs` (the server-orchestration file). The
//! run loop's job is receive → dispatch → send; how a `VmReply` maps onto
//! the M1/M2/m_vmmcp_reply message layouts is IPC concern, and this module
//! is its single home (C keeps the equivalent scatter in main.c's reply
//! tail and the per-service do_* functions).

use minix_types::{EncodeToM1, Message, VmReply};
#[cfg(test)] // VirBytes appears only in the test assertions below
use minix_types::VirBytes;

/// A statically-filtered view of [`VmReply`] that excludes [`VmReply::Suspend`].
///
/// `reply_to_errno` and `encode_reply_data` both require a `VmReply` that
/// will actually be encoded into an IPC reply message — i.e. *not* a
/// `Suspend` (which means "no reply now, resume later"). By constructing
/// `VmReplyForIpc` at the boundary, we make this requirement a compile-time
/// property: `VmReplyForIpc::new(VmReply::Suspend)` returns `None`, and the
/// only way to obtain a `VmReplyForIpc` is via `new()` which statically
/// rejects `Suspend`.
///
/// # Why not just `match` everywhere?
///
/// Before this wrapper, `reply_to_errno` carried an `unreachable!()` arm for
/// `VmReply::Suspend`. If a future refactor accidentally routed a
/// `VmReply::Suspend` through `DispatchAction::Reply(...)` (e.g. by adding
/// a new call site in `dispatch_on_msg`), the kernel would panic deep in
/// the reply path with the cryptic message "Suspend filtered before
/// reply_to_errno". The wrapper moves the check to the *only* place where
/// the boundary is crossed, and produces a much more useful diagnostic
/// ("DispatchAction::Reply carries VmReply::Suspend; dispatch_on_msg should
/// translate to DispatchAction::Suspend") with a clear remediation hint.
///
/// # C-Rust parity
///
/// This is purely a Rust-side type-system improvement; the C code uses a
/// `result` integer (SUSPEND vs others) without a typed wrapper. The
/// wrapper exists only to encode the Rust type-level invariant.
#[derive(Debug, Clone)]
pub(crate) struct VmReplyForIpc {
    inner: VmReply,
}

impl VmReplyForIpc {
    /// Construct a `VmReplyForIpc` from a `VmReply`. Returns `None` if
    /// `reply` is `VmReply::Suspend` — caller must use `DispatchAction::Suspend`
    /// for that case instead of `DispatchAction::Reply(...)`.
    pub(crate) fn new(reply: VmReply) -> Option<Self> {
        match reply {
            VmReply::Suspend => None,
            inner => Some(Self { inner }),
        }
    }

    /// Returns a clone of the underlying `VmReply`. By construction this
    /// is never `VmReply::Suspend`. Use this when the caller still needs
    /// to inspect the reply after encoding (e.g. for logging).
    pub(crate) fn payload(&self) -> VmReply {
        self.inner
    }

    /// Consumes the wrapper and returns the owned `VmReply`. By construction
    /// this is never `VmReply::Suspend`.
    pub(crate) fn into_payload(self) -> VmReply {
        self.inner
    }
}

/// Map a reply to its reply-message errno (`msg.m_type` on the wire).
///
/// # DEFERRED
///
/// Previously contained an `unreachable!("Suspend filtered before
/// reply_to_errno")` arm reachable from any code path that called
/// `reply_to_errno` with `VmReply::Suspend`. The fix introduces
/// [`VmReplyForIpc`], a typed wrapper whose `new()` constructor statically
/// rejects `VmReply::Suspend` (returns `None`), so this function never
/// receives `Suspend` *through the wrapper path*. The `unreachable!()`
/// arm is retained as a defense-in-depth runtime assertion: if a future
/// refactor calls this function directly (bypassing the wrapper) with
/// `VmReply::Suspend`, we panic immediately rather than silently emitting
/// a bogus errno. This is the Rust idiom "make illegal states
/// unrepresentable where possible, but keep assertions as a backstop".
/// Convert VmReply to raw errno for IPC reply. C: result != SUSPEND branch.
///
/// All variants are explicitly listed — adding a new `VmReply` variant
/// will produce a compile error here, forcing the author to decide the
/// correct errno value.
pub(crate) fn reply_to_errno(reply: VmReply) -> i32 {
    match reply {
        VmReply::Ok => 0,
        VmReply::Error(e) => e.to_errno(),
        // All success replies return 0 (OK) to the caller.
        VmReply::Fork(_) => 0,
        VmReply::Brk(_) => 0,
        VmReply::Mmap(_) => 0,
        VmReply::MapPhys(_) => 0,
        VmReply::Exit => 0,
        VmReply::Willexit => 0,
        VmReply::Munmap => 0,
        VmReply::MapCache { .. } => 0,
        VmReply::VfsMmap(_) => 0,
        VmReply::GetPhys { .. } => 0,
        VmReply::GetRefcount { .. } => 0,
        VmReply::InfoStats { .. } => 0,
        VmReply::InfoUsage { .. } => 0,
        VmReply::InfoRegion { .. } => 0,
        VmReply::Getrusage { .. } => 0,
        VmReply::RsMemctlAddrLen { .. } => 0,
        // Compile-time guarantee: VmReplyForIpc::new() returns None for
        // VmReply::Suspend, so this function never receives it via the
        // wrapper path. If you see this panic, someone bypassed the
        // wrapper — fix the caller, do not silence this assertion.
        VmReply::Suspend => unreachable!(
            "VmReply::Suspend reached reply_to_errno; \
             caller must wrap in VmReplyForIpc or use DispatchAction::Suspend"
        ),
    }
}

/// Encode VmReply per-service output data into the reply message fields.
///
/// All variants are explicitly listed — adding a new `VmReply` variant
/// will produce a compile error here, forcing the author to encode the
/// output data correctly.
///
/// # Defense-in-depth
///
/// The `VmReply::Suspend` arm is unreachable in practice because the
/// [`VmReplyForIpc`] wrapper statically filters it out at the call site
/// (the only call site passes `reply_for_ipc.into_payload()`). However,
/// this function accepts `VmReply` directly, so a future refactor that
/// bypasses the wrapper could pass `Suspend` here. The `unreachable!()`
/// assertion catches that bug immediately, consistent with `reply_to_errno`.
pub(crate) fn encode_reply_data(reply: VmReply, msg: &mut Message) {
    // SAFETY: All VM replies use the M1 message format.
    let m1 = unsafe { &mut msg.m_u.m_m1 };
    match reply {
        VmReply::Fork(out) => out.encode(m1),
        VmReply::Brk(out) => out.encode(m1),
        VmReply::Mmap(out) => out.encode(m1),
        VmReply::MapPhys(out) => out.encode(m1),
        VmReply::MapCache { addr } => {
            // C: msg->m_vmmcp_reply.addr = vr->vaddr (mem_cache.c:170);
            // libminixfs reads it back in vm_map_cacheblock (libsys/vm_cache.c:47-54).
            // The addr field is u64 on the minix-rs x86_64 wire (edge
            // E-VMMCPWIRE): MMAP-window VAs are ≥4 GiB and a u32 would
            // truncate every reply.
            // SAFETY: cache replies use the m_vmmcp_reply format.
            let reply = unsafe { &mut msg.m_u.m_vmmcp_reply };
            reply.addr = addr.0;
        }
        VmReply::VfsMmap(out) => out.encode(m1),
        VmReply::GetPhys { phys_addr } => { m1.m1p1 = phys_addr.0; }
        VmReply::GetRefcount { count } => { m1.m1i1 = count as i32; }
        VmReply::InfoStats { page_size, total_pages, free_pages, largest_contiguous, cached_pages, .. } => {
            // C: struct vm_stats_info (vm.h:39-44) — pagesize/total/free/
            // largest/cached. M1 slots: p1=pagesize, i1=total, i2=free,
            // i3=largest, p2=cached (u64 page count; no integer slots left).
            // `dropped_messages`/`pagefault_errors` (V10-P2-4) are a
            // minix-rs extension with no C wire slot — dropped here, like
            // InfoUsage's minflt/majflt (see below).
            m1.m1p1 = page_size;
            m1.m1i1 = total_pages as i32;
            m1.m1i2 = free_pages as i32;
            m1.m1i3 = largest_contiguous as i32;
            m1.m1p2 = cached_pages;
        }
        VmReply::InfoUsage { total, common, shared, virtual_total, mvirtual, max_rss_kb, minor_faults, major_faults } => {
            // Minix3 C uses sys_datacopy to copy a `struct vm_usage_info`
            // (5 VirBytes fields + 3 u64 fields) into the caller's address
            // space (utility.c — do_info → get_usage_info).
            // Rust M1 layout has 3 pointer slots (m1p1..m1p3) and 3 integer
            // slots (m1i1..m1i3). Encode the 5 VirBytes fields: 3 in pointer
            // slots, 2 as page counts (saturated i32) in integer slots, and
            // vui_maxrss (KB) in the last integer slot.
            //
            // Field mapping (aligned with C's struct vm_usage_info):
            //   m1p1 = vui_total, m1p2 = vui_common, m1p3 = vui_shared
            //   m1i1 = vui_virtual (page count), m1i2 = vui_mvirtual (page count)
            //   m1i3 = vui_maxrss (KB, saturated)
            // vui_minflt / vui_majflt have no M1 slot — judgment (V11/T31):
            // C's vm_stats_info (minix/vm.h:41-45) carries no fault fields,
            // so the VM_INFO wire stays C-parity; the counters' observable
            // exit is Getrusage (minor/major wired at dispatch_pagefault).
            // to the sys_datacopy path (VMI-3 follow-up; MIB gets them via
            // the full reply once transport lands).
            m1.m1p1 = total.0;
            m1.m1p2 = common.0;
            m1.m1p3 = shared.0;
            // SAFETY: `as i32` is a truncating cast. Region sizes in bytes fit
            // in u32 when divided by PAGE_SIZE (PAGE_SIZE = 4096 ⇒ 1 TiB of
            // memory ≈ 2^28 pages, well within i32::MAX = 2^31). Documented
            // for reviewer (see review-patterns-skill §模式19 — `as` 截断
            // 必须有 SAFETY 注释).
            let pages = |bytes: u64| -> i32 {
                (bytes / crate::region::page_state::PAGE_SIZE).min(i32::MAX as u64) as i32
            };
            m1.m1i1 = pages(virtual_total.0);
            m1.m1i2 = pages(mvirtual.0);
            // SAFETY: `as i32` saturates — maxrss in KB is bounded by
            // total physical memory / 1024, far below i32::MAX.
            m1.m1i3 = max_rss_kb.min(i32::MAX as u64) as i32;
            let _ = minor_faults;
            let _ = major_faults;
        }
        VmReply::RsMemctlAddrLen { addr, len } => {
            // C message layout (com.h:738-741): VM_RS_CTL_ADDR == m2_p1,
            // VM_RS_CTL_LEN == m2_i3. In C's `mess` union, m2_p1 is at
            // offset 40 while m1_p1/m2_l1 are at offset 24 — the minix-rs
            // MessageM1/M2 layouts are offset-shifted vs C (see
            // minix-types message.rs), so within this model addr→m1p1 and
            // len→m1i3 alias the slots the request decode reads back
            // (m2l1/m2i3). The C wire offsets differ and need a dedicated
            // minix-types overlay when a real C RS is on the wire (A-8:
            // transport DEFERRED). (FIX 25-R2: len was written to m1i1,
            // i.e. the request's endpoint slot — vm_memctl would read a
            // stale len.)
            m1.m1p1 = addr.0;
            m1.m1i3 = len as i32;
        }
        VmReply::InfoRegion { regions, count, next } => {
            // Minix3 C uses `sys_datacopy(VM_PROC_NR, regions_addr,
            // caller, call_addr, count*sizeof(vm_region_info))` to copy
            // the region array (utility.c). M1 layout has only 1 pointer
            // and 3 integer slots — insufficient to ship the array inline.
            //
            // FIX (VMI-2): Previously `let _ = regions;` discarded the
            // entire region list, leaving PM unable to enumerate regions
            // (m1.m1i1=count, m1.m1i2=next but no array payload). Encoding
            // is unchanged for now (count + next in integer slots), but we
            // expose the source length in m1.m1p1 so the caller can detect
            // "VM stub returned N regions but no sys_datacopy happened" vs
            // "VM really has 0 regions". When `IpcTransport::send` lands,
            // replace this with a real sys_datacopy call.
            let len_u32 = u32::try_from(regions.len()).unwrap_or(u32::MAX);
            m1.m1p1 = u64::from(len_u32); // sentinel: source-side length
            m1.m1i1 = count as i32;
            // SAFETY: `as i32` truncates the vaddr cursor. Region addresses
            // live in the low 4 GiB user range (VM_MMAPTOP = 0x80000000),
            // so the cursor fits — documented per §模式19.
            m1.m1i2 = next.0 as i32;
            // m1.m1i3 deliberately left as 0 — reserved for caller-side
            // buffer capacity once sys_datacopy is wired.
        }
        VmReply::Getrusage { max_rss_kb, minor_faults, major_faults } => {
            // Minix3 C uses sys_datacopy to copy a `struct rusage`
            // (≥15 fields: utime, stime, maxrss, ixrss, idrss, isrss,
            // minflt, majflt, nswap, inblock, oublock, msgsnd, msgrcv,
            // nvcsw, nivcsw) into the caller's address space (utility.c).
            // M1 layout has 3 pointer slots + 3 integer slots — pick the
            // 4 most diagnostic fields. The remaining 11 are DEFERRED to
            // the sys_datacopy path (VMI-3 follow-up).
            //
            // FIX (VMI-3): Previously only 3 fields were encoded
            // (max_rss/min_flt/maj_flt) with `m1p2..m1p3`/`m1i3` unused.
            // Encoding now uses remaining slots: m1p2=minor_faults, m1p3=
            // major_faults (both fit in u64 for any realistic process),
            // freeing m1i1/m1i2 for future in_use_time fields.
            m1.m1p1 = max_rss_kb;
            // SAFETY: `as i32` truncates fault counters. i32::MAX = 2^31 ≈
            // 2.1B faults per measurement window; saturation at i32::MAX
            // signals "very high fault rate" rather than overflow in
            // practice. Documented per review-patterns-skill §模式19.
            let faults_to_i32 = |n: u64| -> i32 { n.min(i32::MAX as u64) as i32 };
            m1.m1i1 = faults_to_i32(minor_faults);
            m1.m1i2 = faults_to_i32(major_faults);
            // m1.m1i3 reserved for future ru_inblock (block-input ops).
        }
        // Variants with no output data to encode
        VmReply::Ok | VmReply::Error(_)
        | VmReply::Exit | VmReply::Willexit | VmReply::Munmap => {}
        // Defense-in-depth: Suspend is statically excluded by VmReplyForIpc
        // at the only call site. If this fires, someone bypassed the wrapper.
        VmReply::Suspend => unreachable!(
            "VmReply::Suspend reached encode_reply_data; \
             caller must wrap in VmReplyForIpc or use DispatchAction::Suspend"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::Message;

    #[test]
    fn test_encode_mapcache_reply_preserves_high_addr_bits() {
        // E-VMMCPWIRE regression: m_vmmcp_reply.addr is u64 on the minix-rs
        // x86_64 wire. The former u32 field truncated every MapCache reply,
        // because MMAP-window VAs are always ≥4 GiB (mmap.rs MMAP_BASE).
        let addr = VirBytes(0x0000_0123_4567_89AB);
        let mut msg = Message::default();
        encode_reply_data(VmReply::MapCache { addr }, &mut msg);
        // SAFETY: MapCache replies use the m_vmmcp_reply overlay.
        let reply = unsafe { msg.m_u.m_vmmcp_reply };
        assert_eq!(reply.addr, 0x0000_0123_4567_89AB);
        assert_eq!(reply.flags, 0);
    }

    #[test]
    fn test_encode_reply_rs_memctl_addr_len_slots() {
        // 25-R2 regression: len must be written to m1i3 (the VM_RS_CTL_LEN
        // slot), not m1i1 (the VM_RS_CTL_ENDPT slot). C: com.h:746-747 —
        // VM_RS_CTL_ADDR=m2_p1, VM_RS_CTL_LEN=m2_i3; vm_memctl reads both
        // back after the call.
        let mut msg = Message::default();
        encode_reply_data(
            VmReply::RsMemctlAddrLen {
                addr: VirBytes(0x1_2345_6000),
                len: 0x3000,
            },
            &mut msg,
        );
        let m1 = unsafe { &msg.m_u.m_m1 };
        assert_eq!(m1.m1p1, 0x1_2345_6000);
        assert_eq!(m1.m1i3, 0x3000);
        // The endpoint slot must not be clobbered by the len write (25-R2).
        assert_eq!(m1.m1i1, 0);
    }

    /// V11/T23 (V11-P2-5): the wrapper is the compile-time gate — wrapping a
    /// `VmReply::Suspend` must refuse it (`None` → the run loop's `.expect`
    /// panics with the remediation hint) instead of a cryptic `unreachable!`
    /// deep in `reply_to_errno`.
    #[test]
    #[should_panic(expected = "DispatchAction::Reply carries VmReply::Suspend")]
    fn test_vmreplyforipc_rejects_suspend() {
        // Drive the invariant directly: wrap a Suspend in the Reply arm's
        // constructor — the wrapper must refuse it (None → expect panic).
        let reply = VmReply::Suspend;
        let _ = VmReplyForIpc::new(reply)
            .expect("DispatchAction::Reply carries VmReply::Suspend;                          dispatch_on_msg should translate to DispatchAction::Suspend");
    }
}
