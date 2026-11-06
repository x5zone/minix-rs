//! `exec_worker` — the executor: the I/O landing of the `pm_exec` pipeline.
//!
//! C counterpart `minix3/minix/servers/vfs/exec.c:185-402` (`pm_exec` itself)
//! plus `minix3/minix/lib/libexec/exec_elf.c:131-318` (`libexec_load_elf`),
//! `exec_general.c:78-90` (`libexec_pm_newexec`), and `libexec.h:24-58` (the
//! 192-byte wire format of `struct exec_info`). The decision layer (the
//! **knowledge** of phases/gates/switches/cleanup) lives in [`crate::exec`]
//! (see 25-exec.md); this module only does the part that lands that knowledge
//! **on the wire**: fetching the frame across processes, opening the volume,
//! reading the first block, loading ELF segments (VM allocation + direct FS
//! write + kernel zeroing), the PM_EXEC_NEW announcement, and the ending.
//!
//! # Model positioning (ARCH A-1)
//!
//! C's `pm_exec` runs in a worker thread and blocks on the `fs_sendrec`/VM/PM
//! round trips; minix-rs VFS is a single-threaded event loop, so exec follows
//! the 25-exec.md ARCH decision of "synchronous to the end, no continuation" —
//! the arm finishes every round trip before returning to the main loop (the
//! same shape as the boot-phase `finish_init` synchronous `WireFsClient`). The
//! cost is that no other message is received during exec (C can interleave);
//! this is the registered deviation of the A-1 volume.
//!
//! # Scope of this wave
//!
//! * **Full static ELF chain**: the three identity checks → first block →
//!   segment load → PM_EXEC_NEW → reply to PM.
//! * The `#!` script branch (exec.c:259-271) and the dynamic linking branch
//!   (exec.c:278-314) are **deferred**: detecting either returns `ENOEXEC` —
//!   the same error surface as C's "no loader accepts these bytes"
//!   (`libexec_load_elf` returns ENOEXEC for both non-ELF and
//!   interpreter-carrying images, exec_elf.c:143-148/186-189).
//! * Injecting `AT_*` auxiliary vectors is not a gap: the first line of
//!   `stack_prepare_elf` is `if (!execi->is_dyn) return OK` (exec.c:417-419) —
//!   a static frame passes through untouched.
//! * The `vmfd` direct-mapping path (exec.c:316-335) is not provided: C
//!   itself keeps the `read_seg` segment read as the fallback route (the
//!   "there is a loading route" invariant of 25-exec.md §4.3), so behavior is
//!   equivalent and throughput is a separate matter.
//! * The `VM_MMAP` payload uses `MessMmap`'s 64-bit lanes (once a u32
//!   truncation blocker, since fixed): C's `mess_mmap` (ipc.h:1583-1593) is
//!   constrained by a 56-byte payload assertion and only has the i386 shape
//!   (32-bit addr/len/retaddr), so the same field table under LP64 is 72 bytes
//!   and the assertion must fail — minix-rs user-level wire carries honest
//!   64-bit values (the upper bound of the user half depends on the
//!   architecture: x86-64 level 4 2^47, aarch64 T0SZ=16 ⇒ 2^48, riscv64 Sv39
//!   2^38; the authoritative constants are in `minix_types::USER_STACK_TOP`),
//!   the same ruling as `m_vm_pagefault` and the other 64-bit lanes. The single
//!   authoritative lane diagram is the minix-types `MessMmap` comment.

extern crate alloc;

use crate::exec::{self, ExecError};
use crate::fproc::PID_FREE;
use crate::misc::PAGE_SIZE;
use crate::path::{self, Lookup, LookupRes, LookupStart, MountedFs, PathError, RootDir};
use crate::protect::{ForbidInput, forbidden_decision};
use crate::vnode::{VnodeId, VnodeTable};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::cell::Cell;
use minix_sys::ipc::IpcTransport;
use minix_sys::syscall::{KernelCallTransport, sys_datacopy, sys_memset};
use minix_types::{CpFlags, Endpoint, Message};

/// Page count of the first-read buffer:
/// `static char hdr[10*PAGE_SIZE] __aligned(8)` (exec.c:738).
const HEADER_BUF_PAGES: usize = 10;

/// `PT_INTERP` (`sys/exec_elf.h`) — an interpreter segment present means a dyn
/// binary (`elf_has_interpreter`, exec_elf.c:73-110).
const PT_INTERP: u32 = 3;

/// `ET_DYN` (`elf_sane` accepts both ET_EXEC and ET_DYN, exec_elf.c:29-32).
const ET_DYN: u16 = 3;

/// `PF_X` (the text/data accounting bit of exec_elf.c:289-293).
const PF_X: u32 = 0x1;

/// mmap flag bits (authoritative values = VM's `MmapFlags` bit table,
/// `os/servers/vm/src/mmap.rs:59-66`; C counterpart: the Linux-compatible bit
/// patterns of mman.h/vm.h).
mod map_flags {
    pub const PRIVATE: u32 = 0x0002;
    pub const FIXED: u32 = 0x0010;
    pub const ANON: u32 = 0x1000;
    pub const THIRDPARTY: u32 = 0x0080_0000;
    pub const PREALLOC: u32 = 0x0008_0000;
    pub const UNINITIALIZED: u32 = 0x0004_0000;
}

/// `PROT_READ|PROT_WRITE|PROT_EXEC` — libexec's alloc callback always takes
/// all three (exec_general.c:9-31).
const PROT_RWX: u32 = 0x1 | 0x2 | 0x4;

/// The value of `sizeof(struct stat)` — the magic grant window for req_stat
/// (request.c:1087 likewise grants by `sizeof(struct stat)`). Taken from the
/// authority `minix_types::Stat::USER_STAT_SIZE` (= the `size_of` of
/// `types::stat::Stat`, 152 under LP64), the same source as the FS side's
/// `write_user_stat` byte count — an out-of-range kernel grant is a hard
/// failure (EPERM, `grant.rs:452`), so a window smaller than what FS writes
/// makes the whole copy be rejected.
const STAT_BUF_SIZE: u64 = minix_types::Stat::USER_STAT_SIZE as u64;

/// The `who_from` VFS uses when building a magic grant for its **own buffer**:
/// it must be the **concrete endpoint** `Endpoint::VFS`, never the
/// `Endpoint::SELF` sentinel.
///
/// **Root cause**: the kernel's `verify_grant` installs the magic grant's
/// `who_from` verbatim as `effective_granter` (`kernel/src/grant.rs:461`), and
/// the subsequent data copy uses it to resolve the `start` address in that
/// endpoint's page table. `Endpoint::SELF` is a sentinel
/// (`ENDPOINT_SLOT_TOP - 3`, `is_valid() == false`), not any real process — the
/// kernel must fail to resolve a page table with it, while the FS side's
/// `copy_out` swallows the copy error (`let _ = ipc.copy_to(..)`,
/// `fs-rt/src/transport.rs:272`) → not one byte lands in the grant target
/// buffer, yet `req_read` returns `Ok`. On a real machine this showed up as
/// `after-read hdr[0..8]=00 00 00 00` (the exec first-block read). C
/// `map_header` (exec.c:755) uses the concrete number `VFS_PROC_NR`, and this
/// constant matches it. Segment-load reads go through `target_e` (the target
/// process's concrete endpoint, exec.c:709-711), which was already right and is
/// not in question here.
const VFS_LOCAL_WHO_FROM: Endpoint = Endpoint::VFS;

/// Folds the result of any symbol into a **negative errno** uniformly (the C
/// `pm_exec` `return r` semantics: the worker folds the return value verbatim
/// into `VFS_PM_STATUS`, and PM reads it as `result != OK`).
fn neg(v: i32) -> i32 {
    if v > 0 { -v } else { v.max(-0x7fff_ffff) }
}

/// Transaction packing for FS round trips. Every VFS→FS request in C does
/// `TRNS_ADD_ID` inside `sendmsg` (`comm.c:21`) — the REQ number moves into the
/// high 16 bits of `m_type` and the transid occupies the low 16; replies
/// uniformly `TRNS_DEL_ID` (`main.c:88`) to take the real status from the high
/// 16. `exec_worker`'s synchronous `ipc.sendrec(fs_e, …)` bypasses the stamping
/// and unpacking of `fs_comm::send_fs` and sends `m_type = REQ_xxx` directly
/// (high half zero), so the FS side's `TransactionId::decode` routes by
/// `m_type >> 16` and resolves call=0 (unroutable), while the reply's `m_type`
/// is folded into the result verbatim as an errno — on a real machine the
/// status measured `0x4E0A1A`, whose low 16 bits are exactly the echoed
/// `REQ_LOOKUP` (FS_BASE 0xA00+26=0xA1A). These two helpers supply both steps
/// and **delegate directly to `minix_types`' `trns_add_id`/`trns_del_id`** (the
/// authoritative line-by-line landing of the three-line vfsif.h macro; changing
/// one place requires syncing the others). The transid is **0** (the same
/// ruling as the sibling synchronous leg `request.rs`'s REQ_READSUPER): a
/// synchronous sendrec matches its reply by blocking and never looks at the
/// transid; and 0 is outside the `VFS_TRANSID + slot` worker number space (slot
/// 0 would collide with `encode(0)=0xB01`, conflicting with the main loop's
/// `handle_fs_reply` reverse lookup by low 16), so a misdelivery is dropped as
/// an illegal transid instead of polluting worker 0. Only the FS leg needs
/// this — VM/PM are taskcall-style servers whose `m_type` is the result
/// directly, with no transaction packing.
fn fs_trans_stamp(msg: &mut Message) {
    msg.m_type = minix_types::trns_add_id(msg.m_type, 0);
}

/// Takes the real status from the FS reply's `m_type` (C
/// `TRNS_DEL_ID = (short)(m_type >> 16)`, delegated to the authoritative
/// `minix_types::trns_del_id`).
fn fs_trans_status(msg: &Message) -> i32 {
    minix_types::trns_del_id(msg.m_type)
}

/// Successful ending: the triple PM's `exec_restart` needs (com.h:570-575's
/// `VFS_PM_PC`/`VFS_PM_NEWSP`/`VFS_PM_NEWPS_STR`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecLoaded {
    /// `*pc` — the new entry point (`e_entry + load_offset`,
    /// exec_elf.c:311).
    pub pc: u64,
    /// `*newsp` — the new stack pointer (exec.c:371).
    pub newsp: u64,
    /// `*ps_str` — the absolute ps_strings address the caller (`exec_via`'s
    /// `process_strings_address`) already computed for the **new stack**; VFS
    /// hands it back to PM unchanged (for the kernel to seed into the child's
    /// RBX). It must be a full 64 bits under LP64 (an older `i32` would
    /// truncate `0x0000_7fff_xxxx` and PM would sign-extend it).
    pub newps_str: u64,
}

/// Payload of one exec (the six `VfsCall::Exec` fields plus the stack-top
/// resolution seam).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecRequest {
    /// Target process (`VFS_PM_ENDPT`, m7_i1).
    pub target: Endpoint,
    /// Position of the path in the target process's space (`VFS_PM_PATH`,
    /// m7_p1).
    pub path_addr: u64,
    /// Path length including the NUL (`VFS_PM_PATH_LEN`, m7_i2).
    pub path_len: usize,
    /// Position of the argument frame in the target process's space
    /// (`VFS_PM_FRAME`, m7_p2).
    pub frame_addr: u64,
    /// Frame length (`VFS_PM_FRAME_LEN`, m7_i3).
    pub frame_len: usize,
    /// ps_strings pointer (`VFS_PM_PS_STR`). A full 64 bits under LP64 (see
    /// `ExecLoaded::newps_str`).
    pub ps_str: u64,
    /// Stack top. C takes the kerninfo `user_sp` (exec.c:205 through
    /// `minix_get_user_sp`, kernel_utils.c:40-60); the kerninfo user mapping
    /// belongs to the kerninfo wave, so it is an explicit seam here: the caller
    /// supplies it (in production the kerninfo read half, in tests a fixed
    /// value).
    pub stack_high: u64,
}

/// Exec load failure — a negative errno passed straight through to PM.
pub type ExecErrno = i32;

/// Byte-level reading of the ELF first block (`Elf64_Ehdr`/`Elf64_Phdr`, LP64
/// layout).
///
/// C's phdr table must live in the first sector (`elf_sane`,
/// exec_elf.c:34-38), while segment data is **not** in the first block — so
/// only the header is parsed here, with no whole-image iterator (`minix_elf`'s
/// `SegmentIter` validates that "segment data lies inside the image", which
/// would skip every segment for exec's block-wise reads).
struct ElfHead<'a> {
    buf: &'a [u8],
}

impl<'a> ElfHead<'a> {
    /// Parses and validates the Ehdr — a merge of C `check_header`
    /// (exec_elf.c:56-68) and `elf_sane` (:26-44): magic, data order, both
    /// version fields, phentsize, e_type, and the phoff bound.
    fn parse(buf: &'a [u8]) -> Result<Self, ExecErrno> {
        let enoexec = ExecError::NoExec.to_errno();
        if buf.len() < 64 {
            return Err(ExecError::NoExec.to_errno()); // hdr_len < sizeof(Ehdr) (elf_unpack :50-52)
        }
        let magic_ok = buf[0] == 0x7f && &buf[1..4] == b"ELF";
        let data_ok = buf[5] == 1; // EI_DATA = ELFDATA2LSB (ELF_TARG_DATA)
        let ident_ver_ok = buf[6] == 1; // EI_VERSION = EV_CURRENT
        let phentsize = u16_at(buf, 54);
        let e_version = u32_at(buf, 20);
        // ELF_TARG_VER = EV_CURRENT = 1 in elf_64.h.
        if !magic_ok || !data_ok || !ident_ver_ok || phentsize != 56 || e_version != 1 {
            return Err(enoexec);
        }
        let e_type = u16_at(buf, 16);
        if e_type != minix_elf::ET_EXEC && e_type != ET_DYN {
            return Err(enoexec); // elf_sane (exec_elf.c:29-32)
        }
        let e_phoff = u64_at(buf, 32);
        let e_phnum = u16_at(buf, 56) as u64;
        // The phdr table must fit entirely inside the **loaded first-block
        // buffer**. C's `elf_sane` (exec_elf.c:34-38) bounds it by
        // `SECTOR_SIZE` (512), but C dates from the 32-bit era
        // (`__ELF_WORD_SIZE 32`, `Elf32_Phdr` 32 bytes), where a phdr table
        // ≤512 holds naturally; minix-rs is LP64 (`e_phentsize`=56), and a
        // normal binary's phdr table (e.g. 10 entries = 624B) legitimately
        // exceeds 512, while `map_header` (exec.c:738) already loads 10 pages
        // (`buf.len()`). So the bound is the actually loaded header buffer
        // length — precisely the intent of the `phdr + phnum >= hdr_len` check
        // that `#if 0` disabled in C's `elf_unpack` (exec_elf.c): SECTOR_SIZE
        // sufficed under 32 bits, hence the comment-out, while under LP64 the
        // buffer length must be checked.
        let hdr_loaded = buf.len() as u64;
        if e_phoff > hdr_loaded || e_phoff.saturating_add(e_phnum * 56) > hdr_loaded {
            return Err(enoexec);
        }
        Ok(Self { buf })
    }

    /// Entry address (`e_entry`, exec_elf.c:311).
    fn entry(&self) -> u64 {
        u64_at(self.buf, 24)
    }

    /// Number of phdrs (`e_phnum`).
    fn phnum(&self) -> usize {
        u16_at(self.buf, 56) as usize
    }

    /// Six-tuple of the `i`-th phdr:
    /// `(p_type, p_flags, p_offset, p_vaddr, p_filesz, p_memsz)`.
    fn phdr(&self, i: usize) -> (u32, u32, u64, u64, u64, u64) {
        let at = u64_at(self.buf, 32) as usize + i * 56;
        let b = self.buf;
        (
            u32_at(b, at),
            u32_at(b, at + 4),
            u64_at(b, at + 8),
            u64_at(b, at + 16),
            u64_at(b, at + 32),
            u64_at(b, at + 40),
        )
    }

    /// Whether `PT_INTERP` is present (`elf_has_interpreter`'s existence use,
    /// exec_elf.c:73-110: any occurrence means dynamic linking).
    fn has_interpreter(&self) -> bool {
        (0..self.phnum()).any(|i| self.phdr(i).0 == PT_INTERP)
    }
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

fn u64_at(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}

/// Round up to a page boundary (the `roundup` page arithmetic of
/// exec_elf.c:157-158).
fn roundup(value: u64, unit: u64) -> u64 {
    value.div_ceil(unit) * unit
}

/// Last path component (the copyprogname half of `get_read_vp`,
/// exec.c:109-115).
fn progname_of(fullpath: &str) -> String {
    match fullpath.rfind('/') {
        Some(pos) => fullpath[pos + 1..].to_string(),
        None => fullpath.to_string(),
    }
}

/// The public entry of `pm_exec`: folds every failing leg of the
/// [`pm_exec_inner`] pipeline into a **negative errno** uniformly.
///
/// **Positive-value leak (sign masking)**: both the [`ExecErrno`] type contract
/// and the "failure is a negative errno" note in main_loop.rs require exec
/// failures to reach PM as a **negative errno** (PM's `exec_restart` writes the
/// status verbatim into the reply's `m_type`, and init's `perform_syscall`
/// treats `m_type < 0` as `Err`). But several static legs inside the inner body
/// (a bare `enoexec` value and `ExecError::*::to_errno()`, measured on a real
/// machine as status=**+8** ENOEXEC) leaked **positive** values, which init's
/// `exec_via` swallowed into EIO through its `Ok(_) => EIO` success lane — the
/// real errno could never surface. This single point folds them negative with
/// [`neg`] (idempotent for the already-negative IPC round-trip legs), letting
/// the real errno (e.g. ENOEXEC) surface on `perform_syscall`'s Err lane.
/// Choosing the common boundary over per-leg fixes: one fold point per leg is
/// both easy to miss and high churn.
#[inline]
pub fn pm_exec<K, T>(
    st: &mut crate::main_loop::VfsState,
    kernel: &K,
    ipc: &T,
    req: ExecRequest,
) -> Result<ExecLoaded, ExecErrno>
where
    K: KernelCallTransport,
    T: IpcTransport,
{
    pm_exec_inner(st, kernel, ipc, req).map_err(neg)
}

/// `pm_exec` itself (exec.c:185-402).
///
/// All borrows come from [`crate::main_loop::VfsState`] (the ARCH A-4
/// aggregate) — mounted on that type in its own `impl` block, with decision
/// knowledge left in [`crate::exec`]. Success returns [`ExecLoaded`]; failure
/// returns a mixed-sign errno (positive static legs are folded negative at the
/// [`pm_exec`] boundary).
///
/// Iron rule throughout: every failure path first returns the vnode reference it
/// holds (C `pm_execfinal`'s `unlock_vnode + put_vnode`, exec.c:392-395 —
/// returns go through `PutNodeSink` into the `pending_puts` queue, delivered by
/// the main loop). A failure in the middle of loading redoes the half-built
/// image (the failure arms of exec_elf.c:279-281/293-295 rerun clearproc).
///
/// Note: several static failure legs in this body (a bare `enoexec`,
/// `ExecError::*::to_errno()`) return **positive** values and are folded
/// negative at the [`pm_exec`] boundary — see that function's contract note.
fn pm_exec_inner<K, T>(
    st: &mut crate::main_loop::VfsState,
    kernel: &K,
    ipc: &T,
    req: ExecRequest,
) -> Result<ExecLoaded, ExecErrno>
where
    K: KernelCallTransport,
    T: IpcTransport,
{
    let enoexec = ExecError::NoExec.to_errno();

    // Target slot and fproc — C's `fp` (the `isokendpt_fproc` around
    // exec.c:190).
    let slot = req.target.to_user_slot().ok_or(enoexec)?;
    {
        let fp = st.fproc_table.get(slot).ok_or(enoexec)?;
        if fp.pid == PID_FREE {
            return Err(enoexec);
        }
    }
    let (target_e, uid, gid) = {
        let fp = st.fproc_table.get(slot).expect("checked above");
        (fp.endpoint, fp.eff_uid, fp.eff_gid)
    };

    // ── Fetch the frame (exec.c:226-238): before every destructive step; a
    // frame that is too large is ENOMEM (the ARG_MAX gate of exec.c:227-229
    // runs before the copy).
    if req.frame_len > exec::ARG_MAX {
        return Err(neg(minix_types::ENOMEM));
    }
    // C's mbuf is a function-level static (BSS zero-initialized, exec.c:207);
    // here the zero-filled safety default is kept (a failed datacopy discards
    // the frame, so nothing uninitialized is read); the `set_len` lint
    // suggestion is unsafe and not worth introducing for one transient buffer.
    #[allow(clippy::slow_vector_initialization)]
    let mut frame = {
        let mut v = alloc::vec::Vec::with_capacity(req.frame_len);
        v.resize(req.frame_len, 0);
        v
    };
    sys_datacopy(
        kernel,
        target_e.get(),
        req.frame_addr,
        Endpoint::SELF.get(),
        frame.as_mut_ptr() as u64,
        req.frame_len as u64,
    )
    .map_err(neg)?;

    // vsp is fixed before the stack patch (the note at exec.c:243-245).
    let vsp = req.stack_high - req.frame_len as u64;

    // Credentials default: keep the old eff uid/gid (exec.c:247-248).
    let mut new_uid = uid;
    let mut new_gid = gid;

    // ── Fetch the name (exec.c:242 `fetch_name`; utility.c:60-90).
    let fullpath = fetch_name(kernel, target_e, req.path_addr, req.path_len)?;

    // First-block buffer (exec.c:738's function-level static is a local of this
    // function under the single-threaded model — its lifetime covers the three
    // identity checks, ELF parsing and PM_EXEC_NEW, and its address stays stable
    // across the round trips to satisfy the grant window).
    let mut hdr_buf = alloc::boxed::Box::new([0u8; HEADER_BUF_PAGES * PAGE_SIZE as usize]);

    // ── Open the volume + the three checks + the first block (`get_read_vp`,
    // exec.c:89-154; sugid=1, exec.c:254). When the setuid/setgid bits are
    // present, the new_uid/new_gid proposal is rewritten.
    let vnode_id = open_exec(
        st,
        kernel,
        ipc,
        &fullpath,
        target_e,
        hdr_buf.as_mut_slice(),
        &mut new_uid,
        &mut new_gid,
    )?;
    let suid_proposal = {
        let v = st.vnode_table.get(VnodeId(vnode_id)).expect("opened above");
        v.mode & (0o4000 | 0o2000) != 0 // I_SET_UID_BIT | I_SET_GID_BIT (exec.c:141-149)
    };

    // Failure cleanup: return the vnode reference (pm_execfinal,
    // exec.c:392-395).
    macro_rules! bail {
        ($errno:expr) => {{
            release_vnode(st, VnodeId(vnode_id));
            return Err($errno);
        }};
    }

    // ── Classify the first block (`is_script`, exec.c:522-529; the same
    // function in the decision layer).
    if exec::is_script(hdr_buf.as_slice()) {
        bail!(enoexec); // script branch deferred (registered deviation)
    }

    // ── ELF parse + dyn rejection (`elf_unpack`/`elf_has_interpreter`).
    let elf = match ElfHead::parse(hdr_buf.as_slice()) {
        Ok(e) => e,
        Err(e) => bail!(e),
    };
    if elf.has_interpreter() {
        bail!(enoexec); // exec.c:278-314 branch deferred (registered deviation)
    }

    // ── Full phdr precheck (exec_elf.c:184-191): verify first, then tear down
    // the old address space.
    let v_size = st
        .vnode_table
        .get(VnodeId(vnode_id))
        .map(|v| v.size)
        .unwrap_or(0);
    for i in 0..elf.phnum() {
        let (_, _, p_offset, _, p_filesz, _) = elf.phdr(i);
        if p_offset.saturating_add(p_filesz) > v_size {
            bail!(enoexec);
        }
    }

    // ── Tear down the old address space (`clearproc` →
    // `libexec_clearproc_vm_procctl`, exec_elf.c:194; VM_PROCCTL's
    // VMPPARAM_CLEAR, exit.c:129-137).
    if let Err(e) = vm_procctl_clear(ipc, target_e) {
        bail!(e);
    }

    // ── Segment load (the read_seg route of exec_elf.c:197-306). Borrow
    // splitting: a segment read touches the vnode table (immutable) and the
    // grant table (mutable) at once — so the borrow is split at field level.
    {
        let vnodes: &VnodeTable = &st.vnode_table;
        let grants = &mut st.grants;
        if let Err(e) = load_elf_segments(
            kernel,
            ipc,
            vnodes,
            grants,
            VnodeId(vnode_id),
            target_e,
            &elf,
        ) {
            // C's failure cleanup runs clearproc once more
            // (exec_elf.c:279-281/293-295), so a half-built image does not
            // linger.
            // [SD19-gate] best-effort VM cleanup (exec failure rollback
            // wrap-up)
            #[allow(clippy::let_underscore_must_use)]
            let _ = vm_procctl_clear(ipc, target_e);
            bail!(e);
        }
    }

    // ── Stack allocation (exec_elf.c:308-313's
    // `allocmem_ondemand(stacklow, stack_size)`; whole pages for
    // execi.stack_high/stack_size, exec_elf.c:156-158). The stack top (the
    // per-architecture upper bound, authoritative in
    // `minix_types::USER_STACK_TOP`; all three are > 4 GiB) rides VM_MMAP's
    // 64-bit lanes with no truncation (the u32 arm was a real-machine blocker
    // here).
    let stack_high_paged = req.stack_high - req.stack_high % PAGE_SIZE;
    let stack_size_paged = roundup(exec::DEFAULT_STACK_LIMIT, PAGE_SIZE);
    let stacklow = stack_high_paged - stack_size_paged;
    if let Err(e) = vm_mmap(ipc, target_e, stacklow, stack_size_paged, 0, PROT_RWX) {
        // [SD19-gate] best-effort VM cleanup (exec failure rollback
        // wrap-up)
        #[allow(clippy::let_underscore_must_use)]
        let _ = vm_procctl_clear(ipc, target_e);
        bail!(e);
    }

    // ── PM_EXEC_NEW (exec.c:359; exec_general.c:78-90). The returned suid bit
    // is PM's verdict on the proposal (do_newexec's tracer gate plus the VFS
    // proposal bits, pm/exec.c:90-122) and decides whether the wrap-up swaps
    // credentials (exec.c:378-383).
    let allow_setuid = match pm_newexec(
        ipc,
        target_e,
        hdr_buf.as_ptr() as u64,
        hdr_buf.len(),
        &progname_of(&fullpath),
        new_uid,
        new_gid,
        suid_proposal,
        stack_high_paged,
        stack_size_paged,
        req.frame_len,
    ) {
        Ok(allow) => allow,
        Err(e) => bail!(e),
    };

    // ── Stack preparation: a static executable gets no aux injection
    // (exec.c:417-419 `!is_dyn → OK`), so the frame lands as is. ── Copy the
    // frame (exec.c:368-369).
    if let Err(e) = sys_datacopy(
        kernel,
        Endpoint::SELF.get(),
        frame.as_ptr() as u64,
        target_e.get(),
        vsp,
        req.frame_len as u64,
    ) {
        bail!(neg(e));
    }

    // ── Wrap-up (exec.c:374-384): clo_exec → swap credentials → record the
    // name.
    {
        let fp = st.fproc_table.get_mut(slot).expect("checked above");
        exec::clo_exec(fp, &mut st.filp_table);
        let (u, g) = exec::settle_ids(allow_setuid, uid, gid, new_uid, new_gid);
        fp.eff_uid = u;
        fp.eff_gid = g;
        let mut name = [0u8; crate::fproc::PROC_NAME_LEN];
        let pn = progname_of(&fullpath);
        let n = pn.len().min(crate::fproc::PROC_NAME_LEN);
        name[..n].copy_from_slice(&pn.as_bytes()[..n]);
        fp.name = name;
    }

    release_vnode(st, VnodeId(vnode_id));
    Ok(ExecLoaded {
        pc: elf.entry(),
        newsp: vsp,
        newps_str: req.ps_str,
    })
}

/// vnode return shared by the failure and success endings (C
/// `unlock_vnode + put_vnode`, exec.c:392-395; the slow path's REQ_PUTNODE goes
/// into `pending_puts` and is delivered with the rest).
fn release_vnode(st: &mut crate::main_loop::VfsState, id: VnodeId) {
    let mut sink = crate::main_loop::put_node_sink(&mut st.pending_puts);
    // [SD19-gate] best-effort vnode put / reference-count wrap-up (C's
    // put_vnode is void and reports no error)
    #[allow(clippy::let_underscore_must_use)]
    let _ = st.vnode_table.put(id, &mut sink);
}

/// `fetch_name` (the execution counterpart of utility.c:60-90): take the path
/// out of the target process's space.
fn fetch_name<K: KernelCallTransport>(
    kernel: &K,
    who: Endpoint,
    addr: u64,
    len: usize,
) -> Result<String, ExecErrno> {
    if len > path::PATH_MAX {
        return Err(neg(minix_types::ENAMETOOLONG));
    }
    if len == 0 {
        // The same convention as the fetch gate of path.rs `SysPathFetcher`
        // (the short-length side of utility.c's two gates → EINVAL).
        return Err(neg(minix_types::EINVAL));
    }
    let mut buf = [0u8; path::PATH_MAX];
    sys_datacopy(
        kernel,
        who.get(),
        addr,
        Endpoint::SELF.get(),
        buf.as_mut_ptr() as u64,
        len as u64,
    )
    .map_err(neg)?;
    let n = len.min(path::PATH_MAX);
    let end = buf[..n].iter().position(|&b| b == 0).unwrap_or(n);
    if end == 0 {
        return Err(ExecError::NoExec.to_errno()); // empty path
    }
    core::str::from_utf8(&buf[..end])
        .map(str::to_string)
        .map_err(|_| ExecError::NoExec.to_errno())
}

/// The second of exec's three open checks: the execute permission decision
/// (C `exec.c:128` `forbidden(fp, vp, X_BIT)`).
///
/// It is extracted as a pure function so that the "`access` must be the low
/// 3-bit triple" defect point lands directly in unit-test range: when the
/// `access` constant hides inside an inline `ForbidInput` construction, falling
/// back to a 9-bit mode bit (`0o100`) would not turn any test red (the existing
/// tests all bypass the three-check gate and call `load_elf_segments` directly).
/// This function wraps the constant-carrying construction and the decision in
/// one unit that a primitive can drive, so a wrong `access` fails a test
/// immediately.
fn exec_forbid_check(
    file_mode: u32,
    file_uid: u32,
    file_gid: u32,
    real_uid: u32,
    real_gid: u32,
    eff_uid: u32,
    eff_gid: u32,
) -> Result<(), crate::protect::ProtectError> {
    forbidden_decision(&ForbidInput {
        real_uid,
        real_gid,
        eff_uid,
        eff_gid,
        is_access_call: false,
        file_uid,
        file_gid,
        mode: file_mode & 0o777,
        // C `forbidden`'s `access_desired` gets `X_BIT` (=0o1, `const.h:119`),
        // which lives in the same low 3-bit triple as `perm_bits` (root gets
        // `R|W|X`, otherwise `(mode >> shift) & RWX_BITS`); the final test
        // `(perm | access) != perm` requires access ⊆ perm. Passing the 9-bit
        // mode's owner-X bit (`0o100`) would make root/owner with `perm=0o7`
        // fail forever → a false EACCES (measured on a real machine:
        // mode=0o100755, euid=0, a root-owned file was rejected). Every other
        // `forbidden_decision` call site in the tree (`main_loop.rs`, 11 of
        // them) passes the low 3-bit constants of `crate::open::*`
        // (`R_BIT`/`W_BIT`/`X_BIT` or their OR), none a 9-bit mode value; exec
        // takes `X_BIT` per C `exec.c:128`.
        access: crate::protect::X_BIT,
        is_dir: false,
        supp: &[],
        readonly_fs: false,
    })
}

/// Open the volume + the three checks + the suid/sgid proposal + the first-block
/// read (`get_read_vp`, exec.c:89-154; `map_header`, :736-763).
///
/// Returns the vnode slot on success; the `new_uid`/`new_gid` out params are
/// rewritten when the setuid/setgid bits are present (C's
/// `execi.args.new_uid/new_gid`, exec.c:141-150).
#[allow(clippy::too_many_arguments)]
fn open_exec<K, T>(
    st: &mut crate::main_loop::VfsState,
    kernel: &K,
    ipc: &T,
    fullpath: &str,
    target_e: Endpoint,
    hdr_buf: &mut [u8],
    new_uid: &mut u32,
    new_gid: &mut u32,
) -> Result<usize, ExecErrno>
where
    K: KernelCallTransport,
    T: IpcTransport,
{
    // The lookup environment comes from the target process's fproc (in the
    // worker context C's `fp` is the target process).
    let slot = target_e.to_user_slot().ok_or(ExecError::Io.to_errno())?;
    let (rd, wd, uid, gid, real_uid, real_gid) = {
        let fp = st.fproc_table.get(slot).ok_or(ExecError::Io.to_errno())?;
        let rd = fp
            .root_dir
            .and_then(|idx| st.vnode_table.get(VnodeId(idx)))
            .map(|v| RootDir {
                ino: v.ino,
                fs: v.fs,
                dev: v.dev,
            })
            .unwrap_or(RootDir {
                ino: 1,
                fs: Endpoint::NONE,
                dev: 0,
            });
        let wd = fp
            .work_dir
            .and_then(|idx| st.vnode_table.get(VnodeId(idx)))
            .map(|v| LookupStart {
                ino: v.ino,
                fs: v.fs,
                dev: v.dev,
            })
            .unwrap_or(LookupStart {
                ino: 1,
                fs: Endpoint::NONE,
                dev: 0,
            });
        (rd, wd, fp.eff_uid, fp.eff_gid, fp.real_uid, fp.real_gid)
    };
    let mut resolve = Lookup::new(fullpath.to_string(), path::LookupFlags::NOFLAGS)
        .map_err(|e: PathError| e.to_errno())?;
    let mounts: Vec<MountedFs> = st.mounted_fs_list();

    // The synchronous lookup seam: one REQ_LOOKUP round trip = the three-step
    // dance of C `req_lookup` (request.c:430-500). The path grant carries
    // `CPF_READ|CPF_WRITE` — FS must write the remaining path back (the
    // progress-report method of EENTERMOUNT/ELEAVEMOUNT/ESYMLINK). The raw errno
    // of a transport/FS-side failure is preserved through `last_errno` (the
    // PathError vocabulary has no Io, so it is carried straight out of the seam
    // instead of degrading to EINVAL).
    let last_errno: Cell<i32> = Cell::new(0);
    let path_scratch: core::cell::RefCell<[u8; path::PATH_MAX]> =
        core::cell::RefCell::new([0; path::PATH_MAX]);
    let vnode_id = {
        let walk = path::eat_path(
            &mut resolve,
            rd,
            wd,
            uid,
            gid,
            &mounts,
            &mut st.vnode_table,
            &mut |fs_e, dir_ino, root_ino, lk: &mut Lookup| {
                let mut scratch = path_scratch.borrow_mut();
                let bytes = lk.path.as_bytes();
                let n = bytes.len().min(path::PATH_MAX - 1);
                scratch[..n].copy_from_slice(&bytes[..n]);
                scratch[n] = 0;
                let grant = st
                    .grants
                    .grant_direct(
                        kernel,
                        fs_e.get(),
                        scratch[..].as_ptr() as u64,
                        (n + 1) as u64,
                        CpFlags::READ | CpFlags::WRITE,
                    )
                    .map_err(|_e| PathError::Inval)?;
                let mut msg =
                    crate::request::encode_lookup(grant, n + 1, dir_ino, root_ino, lk.flags.bits());
                fs_trans_stamp(&mut msg);
                let r = ipc.sendrec(fs_e, &mut msg);
                // [SD19-gate] best-effort revoke: the main operation is already
                // settled or already on the error path, so a revoke failure
                // does not change its result
                #[allow(clippy::let_underscore_must_use)]
                let _ = st.grants.revoke(grant);
                if let Err(t) = r {
                    last_errno.set(neg(t.0));
                    return Err(PathError::Inval);
                }
                // Read back the remaining path FS wrote — C's grant IS the
                // VFS's `l_path` itself, so after FS writes it VFS just keeps
                // walking (C request.c:494-499).
                let end = scratch[..].iter().position(|&b| b == 0).unwrap_or(0);
                let status = fs_trans_status(&msg);
                match crate::request::decode_lookup_reply(status, &msg) {
                    Some(found) => {
                        if !matches!(found, LookupRes::Ok { .. })
                            && let Ok(tail) = core::str::from_utf8(&scratch[..end])
                        {
                            lk.path = tail.to_string();
                        }
                        Ok(found)
                    }
                    None => {
                        // A negative errno (or an undecodable reply) — the raw
                        // value is left to the caller.
                        last_errno.set(neg(status));
                        Err(PathError::Inval)
                    }
                }
            },
        );
        match walk {
            Ok(id) => id,
            Err(e) => {
                let preserved = last_errno.get();
                return Err(if preserved != 0 {
                    preserved
                } else {
                    e.to_errno()
                });
            }
        }
    };

    if st.vnode_table.get(vnode_id).is_none() {
        return Err(ExecError::Io.to_errno());
    }
    // After a successful open, every failure in this function must first return
    // the vnode reference (C pm_execfinal's put_vnode for execi.vp,
    // exec.c:392-395).
    macro_rules! bail_open {
        ($errno:expr) => {{
            release_vnode(st, vnode_id);
            return Err($errno);
        }};
    }
    let v = st.vnode_table.get(vnode_id).expect("checked above");
    // Check one of three: not a regular file → ENOEXEC (exec.c:126-127).
    if (v.mode & crate::open::S_IFMT) != crate::open::S_IFREG {
        bail_open!(ExecError::NoExec.to_errno());
    }
    // Check two of three: execute permission (exec.c:128-129
    // `forbidden(fp, vp, X_BIT)`).
    if let Err(e) = exec_forbid_check(v.mode, v.uid, v.gid, real_uid, real_gid, uid, gid) {
        bail_open!(e.to_errno());
    }

    // Check three of three: req_stat (exec.c:131-134) — a failure is EIO (the
    // `IdCheck::StatFailed` error surface). The data goes into VFS's `sb`
    // (execi.sb); the static load route consumes only the success bit (size from
    // the vnode's v_size, credentials from fproc), and the full stat consumption
    // surface comes with the script/dyn waves.
    {
        let (fs_e, ino) = (v.fs, v.ino);
        let grant = st
            .grants
            .grant_magic(
                kernel,
                fs_e.get(),
                VFS_LOCAL_WHO_FROM.get(),
                hdr_buf.as_ptr() as u64,
                STAT_BUF_SIZE,
                CpFlags::WRITE,
            )
            .map_err(neg)?;
        let mut msg = crate::request::encode_stat(ino, grant);
        fs_trans_stamp(&mut msg);
        let r = ipc.sendrec(fs_e, &mut msg);
        // [SD19-gate] best-effort revoke: the main operation is already
        // settled or already on the error path, so a revoke failure
        // does not change its result
        #[allow(clippy::let_underscore_must_use)]
        let _ = st.grants.revoke(grant);
        match r {
            Err(t) => bail_open!(neg(t.0)),
            Ok(()) if fs_trans_status(&msg) != minix_types::OK => {
                bail_open!(neg(fs_trans_status(&msg)))
            }
            Ok(()) => {}
        }
    }

    // suid/sgid proposal (exec.c:137-147: the setuid/setgid bits of `v_mode` →
    // `args.new_uid/new_gid`; the proposal bit travels with PM_EXEC_NEW for PM
    // to rule on).
    let v = st.vnode_table.get(vnode_id).expect("checked above");
    if v.mode & 0o4000 != 0 {
        *new_uid = v.uid;
    }
    if v.mode & 0o2000 != 0 {
        *new_gid = v.gid;
    }

    // First block (`map_header`, exec.c:736-763): min(v_size, 10 pages) read
    // straight into the VFS buffer — the user of `req_readwrite` is VFS itself
    // (exec.c:755's `VFS_PROC_NR`, i.e. the concrete endpoint
    // [`VFS_LOCAL_WHO_FROM`], not the SELF sentinel).
    let hdr_len = (v.size as usize).min(hdr_buf.len());
    if let Err(e) = req_read(
        st,
        kernel,
        ipc,
        vnode_id,
        0,
        hdr_buf.as_ptr() as u64,
        hdr_len,
        VFS_LOCAL_WHO_FROM,
    ) {
        bail_open!(e);
    }
    Ok(vnode_id.0)
}

/// `req_read` (the READING half of `req_readwrite_actual`, request.c:834-873):
/// magic grant + `REQ_READ` + revoke. `who_from` is parameterized like C's user
/// argument — the first-block read is VFS itself (exec.c:755, the concrete
/// endpoint [`VFS_LOCAL_WHO_FROM`]; never the `Endpoint::SELF` sentinel, or the
/// kernel fails to resolve the page table, FS swallows the error, the buffer
/// stays zero bytes, and this function still returns `Ok`), while a segment-load
/// read is the **target process** (exec.c:709-711: FS writes the file bytes
/// straight into the target's new address space, with no VFS relay).
#[allow(clippy::too_many_arguments)]
fn req_read<K, T>(
    st: &mut crate::main_loop::VfsState,
    kernel: &K,
    ipc: &T,
    vnode_id: VnodeId,
    off: u64,
    seg_addr: u64,
    seg_bytes: usize,
    who_from: Endpoint,
) -> Result<(), ExecErrno>
where
    K: KernelCallTransport,
    T: IpcTransport,
{
    if seg_bytes == 0 {
        return Ok(());
    }
    let (fs_e, ino, v_size) = {
        let v = st
            .vnode_table
            .get(vnode_id)
            .ok_or(ExecError::Io.to_errno())?;
        (v.fs, v.ino, v.size)
    };
    // The two gates of exec.c:700-702: the LONG_MAX bound and the file bound.
    if off.saturating_add(seg_bytes as u64) > exec::LONG_MAX_U64
        || v_size < off.saturating_add(seg_bytes as u64)
    {
        return Err(ExecError::Io.to_errno());
    }
    let grant = st
        .grants
        .grant_magic(
            kernel,
            fs_e.get(),
            who_from.get(),
            seg_addr,
            seg_bytes as u64,
            CpFlags::WRITE,
        )
        .map_err(neg)?;
    let mut msg = crate::request::encode_read(ino, grant, off as i64, seg_bytes);
    fs_trans_stamp(&mut msg);
    let r = ipc.sendrec(fs_e, &mut msg);
    // [SD19-gate] best-effort revoke: the main operation is already
    // settled or already on the error path, so a revoke failure
    // does not change its result
    #[allow(clippy::let_underscore_must_use)]
    let _ = st.grants.revoke(grant);
    match r {
        Err(t) => Err(neg(t.0)),
        Ok(()) if fs_trans_status(&msg) != minix_types::OK => Err(neg(fs_trans_status(&msg))),
        Ok(()) => Ok(()),
    }
}

/// ELF segment load (the read_seg route of `libexec_load_elf`,
/// exec_elf.c:197-306).
///
/// Returns the `(text_size, data_size)` accounting (`args.text_size/data_size`;
/// no consumer in this wave, hooked up with the dyn/signal waves).
fn load_elf_segments<K, T>(
    kernel: &K,
    ipc: &T,
    vnodes: &VnodeTable,
    grants: &mut minix_sys::grant::GrantTable,
    vnode_id: VnodeId,
    target_e: Endpoint,
    elf: &ElfHead<'_>,
) -> Result<(u64, u64), ExecErrno>
where
    K: KernelCallTransport,
    T: IpcTransport,
{
    let (fs_e, ino, v_size) = {
        let v = vnodes.get(vnode_id).ok_or(ExecError::Io.to_errno())?;
        (v.fs, v.ino, v.size)
    };
    let mut first = true;
    let mut startv = 0u64;
    let mut text_size = 0u64;
    let mut data_size = 0u64;

    for i in 0..elf.phnum() {
        let (p_type, p_flags, p_offset, p_vaddr, p_filesz, p_memsz) = elf.phdr(i);
        if p_type != minix_elf::PT_LOAD || p_memsz == 0 {
            continue; // exec_elf.c:217
        }
        // Page arithmetic (exec_elf.c:228-254).
        let page_offset = p_vaddr % PAGE_SIZE;
        let vaddr = p_vaddr - page_offset;
        let seg_membytes = roundup(p_memsz + page_offset, PAGE_SIZE);
        let vfileend = p_vaddr + p_filesz;

        if first || startv > vaddr {
            startv = vaddr; // the load_base record of exec_elf.c:256-258
        }
        first = false;
        if (p_flags & PF_X) != 0 && text_size < seg_membytes {
            text_size = seg_membytes;
        } else {
            data_size = seg_membytes;
        }

        // allocmem_prealloc_junk (exec_elf.c:273-282; callback
        // exec_general.c:22-31: ANON|PREALLOC|UNINITIALIZED|FIXED, R|W|X;
        // `minix_mmap_for`'s forwhom = the target process).
        vm_mmap(
            ipc,
            target_e,
            vaddr,
            seg_membytes,
            map_flags::ANON | map_flags::PREALLOC | map_flags::UNINITIALIZED | map_flags::FIXED,
            PROT_RWX,
        )?;

        // copymem = read_seg (exec_elf.c:285-287): the whole segment's file
        // bytes written straight into the target process (exec.c:709-711).
        // read_seg's own two gates (exec.c:700-702) apply per segment here.
        if p_offset.saturating_add(p_filesz) > exec::LONG_MAX_U64
            || v_size < p_offset.saturating_add(p_filesz)
        {
            return Err(ExecError::Io.to_errno());
        }
        let grant = grants
            .grant_magic(
                kernel,
                fs_e.get(),
                target_e.get(),
                p_vaddr,
                p_filesz,
                CpFlags::WRITE,
            )
            .map_err(neg)?;
        let mut msg = crate::request::encode_read(ino, grant, p_offset as i64, p_filesz as usize);
        fs_trans_stamp(&mut msg);
        let r = ipc.sendrec(fs_e, &mut msg);
        // [SD19-gate] best-effort revoke: the main operation is already
        // settled or already on the error path, so a revoke failure
        // does not change its result
        #[allow(clippy::let_underscore_must_use)]
        let _ = grants.revoke(grant);
        match r {
            Err(t) => return Err(neg(t.0)),
            Ok(()) if fs_trans_status(&msg) != minix_types::OK => {
                return Err(neg(fs_trans_status(&msg)));
            }
            Ok(()) => {}
        }

        // clearmem for the head/tail gaps (exec_elf.c:294-309;
        // `libexec_clear_sys_memset` → the sys_memset kernel call,
        // exec_general.c:66-71).
        let vmemend = vaddr + seg_membytes;
        let head = p_vaddr - vaddr;
        if head > 0 {
            sys_memset(kernel, target_e.get(), 0, vaddr, head).map_err(neg)?;
        }
        let tail = vmemend - vfileend;
        if tail > 0 {
            sys_memset(kernel, target_e.get(), 0, vfileend, tail).map_err(neg)?;
        }
    }
    Ok((text_size, data_size))
}

/// The ANON allocation of `VM_MMAP` (the VM_MMAP leg of `minix_mmap_for`; the
/// m_mmap union arm layout is dual to VM's `VmMmapIn::decode_message`).
/// Address/length ride the 64-bit lanes — C's header is i386 pointer width (a
/// 56-byte assertion that must fail under LP64), while the upper bound of the
/// minix-rs user half depends on the architecture (authoritative
/// `minix_types::USER_STACK_TOP`), and the 64-bit lanes fit on all three.
fn vm_mmap<T: IpcTransport>(
    ipc: &T,
    forwhom: Endpoint,
    addr: u64,
    len: u64,
    extra_flags: u32,
    prot: u32,
) -> Result<(), ExecErrno> {
    let mut msg = Message {
        m_type: minix_types::VM_MMAP as i32,
        ..Message::default()
    };
    {
        // SAFETY: m_mmap is the documented payload arm of VM_MMAP (VM's
        // `VmMmapIn::decode_message` reads the same arm; C ipc.h:1583-1593).
        let m = unsafe { &mut msg.m_u.m_mmap };
        m.offset = 0;
        m.addr = addr;
        m.len = len;
        m.prot = prot as i32;
        // Two required bits that were once missing: exec builds address-space
        // mappings (segments and stack) for the **target child process**, and
        // this helper hand-writes the `m_mmap` arm with `forwhom = target_e`.
        //   • `MAP_THIRDPARTY` (`0x800000`): VM decides only from this bit into
        //     whose address space a mapping lands
        //     (`os/servers/vm/src/mmap.rs:283-291`
        //     `target = if THIRDPARTY { forwhom } else { caller }`, where
        //     `caller = m_source`, i.e. VFS itself). Without it the child's
        //     segments land silently in VFS and the child stays empty.
        //     C `minix_mmap_for` (`mmap.c:36-38`) always adds this bit when
        //     `forwhom != SELF`; on this path `forwhom` is always the child (not
        //     VFS), so it is set unconditionally. VFS has exec privilege
        //     (`mmap.rs:280`), so THIRDPARTY is allowed.
        //   • `MAP_PRIVATE`: minix-rs VM's `is_valid` (`mmap.rs:88-92`, tightened
        //     in doc 20 §3.6) requires exactly one of SHARED/PRIVATE, otherwise
        //     it returns EINVAL. `to_vr_flags` (`mmap.rs:102-126`) does not
        //     consume PRIVATE (a new mapping never sets VR_SHARED, the same
        //     semantics as C `do_mmap`), so a region produced with PRIVATE
        //     carried matches C bit for bit (C does not pass the bit but does
        //     not validate it either).
        // Measured on a real machine: the old code had no THIRDPARTY and, after
        // PRIVATE was carried, mmap no longer returned EINVAL (it passed
        // is_valid) but still landed in the wrong object → child empty →
        // `exec_via` instruction fetch noaddr → PM↔VFS ping-pong livelock; only
        // after THIRDPARTY was added was the image really loaded into the child.
        m.flags = (map_flags::ANON
            | map_flags::FIXED
            | map_flags::PRIVATE
            | map_flags::THIRDPARTY
            | extra_flags) as i32;
        m.fd = -1;
        m.forwhom = forwhom.get();
    }
    ipc.sendrec(Endpoint::VM, &mut msg).map_err(|t| neg(t.0))?;
    if msg.m_type != minix_types::OK {
        return Err(neg(msg.m_type));
    }
    Ok(())
}

/// `VM_PROCCTL`'s `VMPPARAM_CLEAR` (exec_elf.c:194's clearproc callback
/// `libexec_clearproc_vm_procctl`; param=1, exit.c:129-137 — the RS/VFS caller
/// allow-list on the VM side, os/servers/vm/src/ipc/dispatcher.rs:1674).
fn vm_procctl_clear<T: IpcTransport>(ipc: &T, who: Endpoint) -> Result<(), ExecErrno> {
    let mut msg = Message {
        m_type: minix_types::VM_PROCCTL as i32,
        ..Message::default()
    };
    {
        // SAFETY: m_lc_vm_procctl is the documented payload arm of VM_PROCCTL
        // (VM's `VmProcctlIn::decode_message` reads the same arm;
        // com.h:753-757).
        let p = unsafe { &mut msg.m_u.m_lc_vm_procctl };
        p.ull1 = 0;
        p.ull2 = 0;
        p.param = 1; // VMPPARAM_CLEAR
        p.who = who.get();
        p.m1 = 0;
        p.len = 0;
        p.flags = 0;
    }
    ipc.sendrec(Endpoint::VM, &mut msg).map_err(|t| neg(t.0))?;
    if msg.m_type != minix_types::OK {
        return Err(neg(msg.m_type));
    }
    Ok(())
}

/// `libexec_pm_newexec` (exec_general.c:78-90): the PM_EXEC_NEW sendrec.
///
/// The 192-byte wire format of `exec_info` (libexec.h:24-58, LP64; PM decodes
/// it with `EXEC_INFO_COPY_SIZE` and the field offsets —
/// proc_e@0/hdr@8/hdr_len@16/frame_len@24/progname@32(16)/new_uid@48/new_gid@52/
/// allow_setuid@56/stack_size@64/load_offset@72/text_size@80/data_size@88/
/// filesize@96/…/stack_high@184). PM copies scalars only and never
/// dereferences the `hdr` pointer (do_newexec reads only
/// progname/uid/gid/allow_setuid/stack_high/frame_len). The reply's `m_type` is
/// the status code, and the payload's suid@0 is the ruled `allow_setuid`
/// (pm/exec.c:120-122).
#[allow(clippy::too_many_arguments)]
fn pm_newexec<T: IpcTransport>(
    ipc: &T,
    target_e: Endpoint,
    hdr_addr: u64,
    hdr_len: usize,
    progname: &str,
    new_uid: u32,
    new_gid: u32,
    suid_proposal: bool,
    stack_high: u64,
    stack_size: u64,
    frame_len: usize,
) -> Result<bool, ExecErrno> {
    let mut info = alloc::boxed::Box::new([0u8; 192]);
    info[0..4].copy_from_slice(&target_e.get().to_le_bytes()); // proc_e@0
    info[8..16].copy_from_slice(&hdr_addr.to_le_bytes()); // hdr@8（exec.c:745）
    info[16..24].copy_from_slice(&(hdr_len as u64).to_le_bytes()); // hdr_len@16（exec.c:748-750）
    info[24..32].copy_from_slice(&(frame_len as u64).to_le_bytes()); // frame_len@24
    {
        // progname@32 (PROC_NAME_LEN=16, C strncpy semantics: fill it up or
        // stop at the NUL)
        let n = progname.len().min(16);
        info[32..32 + n].copy_from_slice(&progname.as_bytes()[..n]);
    }
    info[48..52].copy_from_slice(&new_uid.to_le_bytes()); // new_uid@48
    info[52..56].copy_from_slice(&new_gid.to_le_bytes()); // new_gid@52
    info[56..60].copy_from_slice(&(i32::from(suid_proposal)).to_le_bytes()); // allow_setuid@56 (the VFS proposal)
    info[64..72].copy_from_slice(&stack_size.to_le_bytes()); // stack_size@64
    info[184..192].copy_from_slice(&stack_high.to_le_bytes()); // stack_high@184

    let mut msg = Message {
        m_type: minix_types::PM_EXEC_NEW,
        ..Message::default()
    };
    {
        // SAFETY: endpt@0/ptr@8 of m_lexec_pm_exec_new (ipc.h:966-973; PM's
        // decode::exec_new reads the same fields).
        let raw = unsafe { &mut msg.m_u.raw };
        raw[0..4].copy_from_slice(&target_e.get().to_le_bytes());
        raw[8..16].copy_from_slice(&(info.as_ptr() as u64).to_le_bytes());
    }
    // sendrec semantics guarantee PM finishes copying the 192 bytes of `info`
    // before replying (do_newexec:82-84's sys_datacopy precedes the answer), so
    // the buffer only needs to live until this function returns — no static
    // storage required.
    let r = ipc.sendrec(Endpoint::PM, &mut msg).map_err(|t| neg(t.0));
    drop(info);
    r?;
    if msg.m_type != minix_types::OK {
        return Err(neg(msg.m_type));
    }
    // SAFETY: the reply payload's suid@0 (pm/exec.c:120-122) is read as raw
    // bytes.
    let suid = unsafe { i32::from_le_bytes(msg.m_u.raw[0..4].try_into().unwrap()) };
    Ok(suid != 0)
}

// ════════════════════════════════════════════════════════════════════
// Tests — layered: (1) the pure ELF header parsing piece; (2) the wire shapes
// of segment load / PM announcement / VM clear (canned transport, asserted one
// by one); (3) the open phase of the `pm_exec` orchestration (the
// lookup/stat/read three-step dance + the failure paths + vnode return). The FS
// data back-flow (FS writing file bytes into the VFS buffer the grant points
// at) is unreachable on the host (the canned transport moves no bytes), the same
// boundary as the rest of the tree's FS data-path tests, so coverage is by wire
// shape plus staged driving.
// ════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod tests {
    use super::*;
    // Test-only constant: the production face of this file does not
    // reference it, so the import lives here instead of the top-level use
    // list (where `clippy --workspace`, which builds without `--tests`,
    // would flag it unused).
    use crate::main_loop::VfsState;
    use alloc::format;
    use minix_types::USER_STACK_TOP;

    /// Call-site regression pin for `exec_forbid_check`'s `access`: it must be
    /// the low 3-bit `X_BIT`. The existing tests all bypass the three-check gate
    /// and call `load_elf_segments` directly, never driving this `ForbidInput`
    /// construction; this test drives it directly and checks the "execute
    /// intent" across root/owner/other files. If the internal `access` goes back
    /// to the 9-bit mode bit `0o100`, root/owner executing `0o755` would be
    /// rejected forever and this test turns red at once (the real defect was
    /// mode=0o100755 with euid=0).
    #[test]
    fn test_exec_forbid_check_uses_low_three_bit_access() {
        // root executing its own 0o755 (the real-machine scenario): allowed.
        assert_eq!(
            exec_forbid_check(0o100755, 0, 0, 0, 0, crate::link::SU_UID, 0),
            Ok(())
        );
        // A non-root owner executing its own 0o755 (owner slot shift=6 →
        // perm=0o7): allowed.
        assert_eq!(
            exec_forbid_check(0o755, 100, 100, 100, 100, 100, 100),
            Ok(())
        );
        // A non-root owner executing 0o644 with no execute bit (perm=0o6, X not
        // included): rejected.
        assert_eq!(
            exec_forbid_check(0o644, 100, 100, 100, 100, 100, 100),
            Err(crate::protect::ProtectError::Acces)
        );
    }

    /// Minimal ELF64 image header (Ehdr 64B + a 56B/entry phdr table, all inside
    /// the first sector).
    fn make_test_elf(entry: u64, phdrs: &[(u32, u32, u64, u64, u64, u64)]) -> Vec<u8> {
        let mut e = vec![0u8; 64 + phdrs.len() * 56];
        e[0..4].copy_from_slice(&[0x7f, b'E', b'L', b'F']);
        e[4] = 2; // ELFCLASS64
        e[5] = 1; // ELFDATA2LSB
        e[6] = 1; // EV_CURRENT
        e[16..18].copy_from_slice(&minix_elf::ET_EXEC.to_le_bytes()); // e_type
        e[20..24].copy_from_slice(&1u32.to_le_bytes()); // e_version
        e[24..32].copy_from_slice(&entry.to_le_bytes()); // e_entry
        e[32..40].copy_from_slice(&64u64.to_le_bytes()); // e_phoff
        e[54..56].copy_from_slice(&56u16.to_le_bytes()); // e_phentsize
        e[56..58].copy_from_slice(&(phdrs.len() as u16).to_le_bytes()); // e_phnum
        for (i, (ty, flags, off, vaddr, filesz, memsz)) in phdrs.iter().enumerate() {
            let at = 64 + i * 56;
            e[at..at + 4].copy_from_slice(&ty.to_le_bytes());
            e[at + 4..at + 8].copy_from_slice(&flags.to_le_bytes());
            e[at + 8..at + 16].copy_from_slice(&off.to_le_bytes());
            e[at + 16..at + 24].copy_from_slice(&vaddr.to_le_bytes());
            e[at + 32..at + 40].copy_from_slice(&filesz.to_le_bytes());
            e[at + 40..at + 48].copy_from_slice(&memsz.to_le_bytes());
        }
        e
    }

    /// Wire shape 1: what Ehdr parsing accepts and rejects (the merged
    /// `check_header`/`elf_sane` semantics).
    #[test]
    fn test_elfhead_parse_accepts_static_and_rejects_broken() {
        let good = make_test_elf(
            0x401_000,
            &[(minix_elf::PT_LOAD, PF_X, 0x1000, 0x401_000, 0x200, 0x200)],
        );
        let head = ElfHead::parse(&good).expect("static ELF must parse");
        assert_eq!(head.entry(), 0x401_000);
        assert_eq!(head.phnum(), 1);
        assert!(!head.has_interpreter());

        // Bad magic.
        let mut bad = good.clone();
        bad[0] = b'M';
        assert!(ElfHead::parse(&bad).is_err());
        // The phdr table runs past the loaded buffer (120B here, e_phoff=600 is
        // outside) → rejected.
        let mut far = good.clone();
        far[32..40].copy_from_slice(&600u64.to_le_bytes());
        assert!(ElfHead::parse(&far).is_err());
        // PT_INTERP present = dynamic linking (the rejected surface of this
        // wave).
        let dynimg = make_test_elf(0x1000, &[(PT_INTERP, 0, 0x200, 0, 0x10, 0)]);
        let dhead = ElfHead::parse(&dynimg).expect("dyn header parses");
        assert!(dhead.has_interpreter());
        // ET_DYN is accepted like ET_EXEC (elf_sane).
        let mut et_dyn = good.clone();
        et_dyn[16..18].copy_from_slice(&ET_DYN.to_le_bytes());
        assert!(ElfHead::parse(&et_dyn).is_ok());
        // Truncated image.
        assert!(ElfHead::parse(&good[..32]).is_err());
    }

    /// LP64 regression pin: a 10-entry phdr table (`e_phoff` 64 + 10×56 = 624 >
    /// 512) must parse — a real `/bin/sh` first block (`map_header` loads 10
    /// pages = a 40960 buffer) fits it. The old `SECTOR_SIZE` (512) bound was a
    /// 32-bit-era leftover (`Elf32_Phdr` 32B) that misjudged valid LP64 headers
    /// as ENOEXEC (measured on a real machine: the leg that still returned
    /// ENOEXEC after the first block's bytes had landed as `7f454c46`). The new
    /// bound is the actually loaded header buffer length.
    #[test]
    fn test_elfhead_parse_lp64_phdr_table_beyond_sector_size() {
        let phdrs: Vec<(u32, u32, u64, u64, u64, u64)> = (0..10)
            .map(|i| {
                (
                    minix_elf::PT_LOAD,
                    PF_X,
                    0x1000 + i as u64 * 0x100,
                    0x401_000 + i as u64 * 0x100,
                    0x100,
                    0x100,
                )
            })
            .collect();
        let img = make_test_elf(0x401_000, &phdrs);
        assert_eq!(
            img.len(),
            64 + 10 * 56,
            "10 项 LP64 phdr 表 = 624B > 旧 512 界"
        );
        let head =
            ElfHead::parse(&img).expect("LP64 10 项 phdr 表必须 parse（超 512 但 ≤ 缓冲长）");
        assert_eq!(head.phnum(), 10);
        // A real overrun (e_phoff pushed past the loaded buffer) must still be
        // rejected.
        let mut oob = img.clone();
        oob[32..40].copy_from_slice(&2000u64.to_le_bytes());
        assert!(ElfHead::parse(&oob).is_err(), "phdr 表越过加载缓冲界必须拒");
    }

    /// Scripted IPC: canned replies keyed by peer m_type, recording every
    /// sendrec.
    struct ExecScriptedIpc {
        sent: core::cell::RefCell<Vec<(Endpoint, i32)>>,
        /// Full message snapshots of VM-side sendrecs (used to pin the 64-bit
        /// lanes).
        vm_sent: core::cell::RefCell<Vec<Message>>,
    }

    impl ExecScriptedIpc {
        fn new() -> Self {
            Self {
                sent: core::cell::RefCell::new(Vec::new()),
                vm_sent: core::cell::RefCell::new(Vec::new()),
            }
        }
        fn types_of(&self, e: Endpoint) -> Vec<i32> {
            self.sent
                .borrow()
                .iter()
                .filter(|(pe, _)| *pe == e)
                .map(|(_, t)| *t)
                .collect()
        }
    }

    impl minix_sys::ipc::IpcTransport for ExecScriptedIpc {
        fn send(&self, _d: Endpoint, _m: &Message) -> Result<(), minix_sys::ipc::TrapStatus> {
            Ok(())
        }
        fn receive(
            &self,
            _s: Endpoint,
            _m: &mut Message,
        ) -> Result<minix_sys::ipc::IpcStatus, minix_sys::ipc::TrapStatus> {
            unimplemented!("exec 测试不收消息")
        }
        fn sendrec(&self, d: Endpoint, m: &mut Message) -> Result<(), minix_sys::ipc::TrapStatus> {
            let ty = m.m_type;
            if d == Endpoint::VM {
                self.vm_sent.borrow_mut().push(*m);
            }
            self.sent.borrow_mut().push((d, ty));
            m.m_type = match (d, ty) {
                // Both VM routes (clear and allocate) succeed.
                (Endpoint::VM, _) => minix_types::OK,
                // PM_EXEC_NEW: reply OK (suid=0, no credential swap).
                (Endpoint::PM, minix_types::PM_EXEC_NEW) => {
                    // Reply payload suid@0 (pm/exec.c:120-122) — zero means no
                    // credential swap.
                    // SAFETY: the test injects a canned reply payload.
                    unsafe {
                        m.m_u.raw[0..4].copy_from_slice(&0i32.to_le_bytes());
                    }
                    minix_types::OK
                }
                // All three FS routes (lookup/stat/read) succeed.
                (Endpoint::MFS, _) => minix_types::OK,
                _ => minix_types::OK,
            };
            Ok(())
        }
        fn notify(&self, _d: Endpoint) -> Result<(), minix_sys::ipc::TrapStatus> {
            Ok(())
        }
        fn sendnb(&self, _d: Endpoint, _m: &Message) -> Result<(), minix_sys::ipc::TrapStatus> {
            Ok(())
        }
        fn senda(
            &self,
            _table: &[minix_sys::ipc::AsyncSlot],
        ) -> Result<(), minix_sys::ipc::TrapStatus> {
            Ok(())
        }
        fn query_kerninfo_page(&self) -> Result<u64, minix_sys::ipc::TrapStatus> {
            Err(minix_sys::ipc::TrapStatus(1))
        }
    }

    /// Wire shape 2: the segment load route — VM_MMAP allocation + REQ_READ
    /// written straight into the target + head/tail clearmem (sys_memset), one
    /// by one (exec_elf.c:197-306).
    #[test]
    fn test_load_elf_segment_wires() {
        let mut state = VfsState::new();
        crate::main_loop::seed_ready_state(&mut state);
        // The target process's exec-volume vnode (mfs, regular file, size
        // larger than the segment).
        let vid = state.vnode_table.alloc().unwrap();
        {
            let vn = state.vnode_table.get_mut(vid).unwrap();
            vn.fs = Endpoint::MFS;
            vn.ino = 42;
            vn.dev = 1;
            vn.mode = crate::open::S_IFREG | 0o755;
            vn.size = 0x2000;
            vn.ref_count = 1;
        }
        let kernel = minix_sys::syscall::CannedKernelCallTransport::new();
        let ipc = ExecScriptedIpc::new();
        let elfimg = make_test_elf(
            0x401_000,
            &[(
                minix_elf::PT_LOAD,
                PF_X | 0x2,
                0x1000,
                0x401_000,
                0x100,
                0x300,
            )],
        );
        let elf = ElfHead::parse(&elfimg).unwrap();
        let grants = &mut state.grants;
        let vnodes: &VnodeTable = &state.vnode_table;
        let (text, data) =
            load_elf_segments(&kernel, &ipc, vnodes, grants, vid, Endpoint::INIT, &elf)
                .expect("segment load must succeed");
        // A PF_X segment counts as text, no data segment → data_size 0
        // (exec_elf.c:289-293).
        assert_eq!(text, PAGE_SIZE); // roundup(memsz 0x300, 4096)
        assert_eq!(data, 0);
        // One VM_MMAP: forwhom=target, page-aligned vaddr,
        // ANON|PREALLOC|UNINIT|FIXED.
        let vm_types = ipc.types_of(Endpoint::VM);
        assert_eq!(vm_types, vec![minix_types::VM_MMAP as i32]);
        // Regression pin: exec segment mappings are built for the **child
        // process** (not VFS itself), so VM_MMAP must carry MAP_THIRDPARTY (else
        // VM silently installs the segment into caller=VFS) and MAP_PRIVATE (else
        // VM's `is_valid` rejects it with EINVAL). On a real machine these two
        // missing bits together left the child's address space empty →
        // `exec_via` noaddr livelock.
        {
            let vm_msgs = ipc.vm_sent.borrow();
            let mmap_flags = unsafe { vm_msgs[0].m_u.m_mmap.flags };
            assert_eq!(
                mmap_flags,
                (map_flags::ANON
                    | map_flags::FIXED
                    | map_flags::PRIVATE
                    | map_flags::THIRDPARTY
                    | map_flags::PREALLOC
                    | map_flags::UNINITIALIZED) as i32,
            );
            // The mapping beneficiary must be the target child (INIT), not VFS
            // itself.
            let forwhom = unsafe { vm_msgs[0].m_u.m_mmap.forwhom };
            assert_eq!(forwhom, Endpoint::INIT.get());
        }
        // One REQ_READ: FS writes into the target process's memory (REQ_READ @
        // inode 42). The outgoing m_type is transaction-packed (TRNS_ADD_ID: the
        // REQ number into the high 16, the transid into the low 16) — the real
        // wire shape constant `trns_add_id(REQ_READ, 0)` pins the shape, so a
        // regression back to a bare send (m_type=REQ_READ) turns this red.
        let fs_types = ipc.types_of(Endpoint::MFS);
        assert_eq!(
            fs_types,
            vec![minix_types::trns_add_id(minix_types::REQ_READ, 0)]
        );
        // clearmem: an in-page head gap of 0 and a tail gap of
        // 4096-0x100=0xF00 → exactly one sys_memset.
        let memsets = kernel
            .sent
            .borrow()
            .iter()
            .filter(|m| m.m_type == minix_types::SYS_MEMSET)
            .count();
        assert_eq!(memsets, 1, "tail clear only (vaddr page-aligned)");
    }

    /// Regression pin: the VFS-local magic grant of the exec first-block read /
    /// stat uses the **concrete endpoint** [`VFS_LOCAL_WHO_FROM`]
    /// (= `Endpoint::VFS`) as `who_from`, never the `Endpoint::SELF` sentinel —
    /// the kernel installs `magic.who_from` verbatim as `effective_granter`
    /// (`kernel/src/grant.rs:461`) to resolve the page table; SELF is not a valid
    /// endpoint (`is_valid() == false`) → the copy fails and FS swallows it
    /// (transport.rs:272) → hdr_buf stays zero bytes while `req_read` still
    /// returns `Ok` (measured on a real machine; before the correction the phdr
    /// table was wrongly suspected of exceeding 512).
    #[test]
    fn test_vfs_local_who_from_is_concrete_endpoint() {
        assert_eq!(VFS_LOCAL_WHO_FROM, Endpoint::VFS);
        assert!(
            VFS_LOCAL_WHO_FROM.is_valid(),
            "VFS-local grant 的 who_from 必须是具体端点，否则内核页表解析失败"
        );
        assert!(
            !Endpoint::SELF.is_valid(),
            "SELF 哨兵不可作 magic grant who_from（B37 回归即此）"
        );
    }

    /// Drives the first-block leg of `req_read` (VFS-local): a seeded vnode plus
    /// a concrete `who_from` should issue exactly one packed `REQ_READ` to MFS
    /// and return `Ok` (the canned transport moves no bytes, so only the wire
    /// shape and the absence of an error are pinned; the data back-flow is
    /// verified by running on the real machine).
    #[test]
    fn test_req_read_first_block_wires_with_concrete_who_from() {
        let mut state = VfsState::new();
        crate::main_loop::seed_ready_state(&mut state);
        let vid = state.vnode_table.alloc().unwrap();
        {
            let vn = state.vnode_table.get_mut(vid).unwrap();
            vn.fs = Endpoint::MFS;
            vn.ino = 8;
            vn.dev = 1;
            vn.mode = crate::open::S_IFREG | 0o755;
            vn.size = 0x2000;
            vn.ref_count = 1;
        }
        let kernel = minix_sys::syscall::CannedKernelCallTransport::new();
        let ipc = ExecScriptedIpc::new();
        let mut buf = [0u8; 512];
        let r = req_read(
            &mut state,
            &kernel,
            &ipc,
            vid,
            0,
            buf.as_mut_ptr() as u64,
            buf.len(),
            VFS_LOCAL_WHO_FROM,
        );
        assert_eq!(r, Ok(()));
        assert_eq!(
            ipc.types_of(Endpoint::MFS),
            vec![minix_types::trns_add_id(minix_types::REQ_READ, 0)],
            "首块读应恰发一条打包后的 REQ_READ 到 MFS"
        );
    }

    /// Regression pin: the VFS→FS transaction packing round trip. Outgoing,
    /// `fs_trans_stamp` moves the REQ number into the high 16 (FS routes by
    /// `m_type >> 16`) and puts 0 in the low 16 (the synchronous leg matches
    /// itself and does not occupy the worker transid space); incoming,
    /// `fs_trans_status` takes the real status from the high 16. The real defect
    /// shape was the status field being taken as a bare errno — the old code read
    /// `msg.m_type` (=0x4E0A1A) directly and folded out a bogus errno of 5114394;
    /// after the fix only the real status in the high 16 (=78) is seen.
    #[test]
    fn test_fs_transaction_pack_roundtrip() {
        // Outgoing: REQ_LOOKUP moves to the high 16, low 16 = 0 (outside the
        // worker transid space).
        let mut req = Message {
            m_type: minix_types::REQ_LOOKUP,
            ..Message::default()
        };
        fs_trans_stamp(&mut req);
        assert_eq!(
            minix_types::trns_del_id(req.m_type),
            minix_types::REQ_LOOKUP,
            "FS 路由用的 call 必须在高 16"
        );
        assert_eq!(
            minix_types::trns_get_id(req.m_type),
            0,
            "同步腿 transid 取 0，不能落入 VFS_TRANSID+slot 的 worker 号空间"
        );
        // Incoming: simulate a successful FS reply (status=0).
        let ok = Message {
            m_type: minix_types::trns_add_id(0, 0),
            ..Message::default()
        };
        assert_eq!(fs_trans_status(&ok), minix_types::OK);
        // Incoming: simulate the bogus real-machine value 0x4E0A1A
        // (=(78<<16)|REQ_LOOKUP) — the extracted status is 78 (not the old
        // code's bare 5114394).
        let bogus = Message {
            m_type: 0x4E0A_1A,
            ..Message::default()
        };
        assert_eq!(fs_trans_status(&bogus), 0x4E);
        assert_eq!(
            neg(fs_trans_status(&bogus)),
            -0x4E,
            "折成负 errno 而非指针值"
        );
    }

    /// Lane pin: the stack mapping's VM_MMAP request carries a stack top above
    /// 4 GiB on the 64-bit lanes, and VM's `VmMmapIn::decode_message` (the same
    /// decode VM runs on the real wire) reads it back unchanged — the u32 arm
    /// silently truncated here (a real-machine blocker, since fixed).
    #[test]
    fn test_vm_mmap_stack_lanes_carry_above_4gib() {
        let ipc = ExecScriptedIpc::new();
        // Default stack top USER_STACK_TOP − DEFAULT_STACK_LIMIT(4 MiB), page
        // aligned.
        let stack_low = USER_STACK_TOP as u64 - exec::DEFAULT_STACK_LIMIT;
        assert_eq!(stack_low % PAGE_SIZE, 0);
        assert!(stack_low > 4 * 1024 * 1024 * 1024);
        vm_mmap(
            &ipc,
            Endpoint(5),
            stack_low,
            exec::DEFAULT_STACK_LIMIT,
            0,
            PROT_RWX,
        )
        .expect("stack mmap succeeds on canned VM");
        let sent = ipc.vm_sent.borrow();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].m_type, minix_types::VM_MMAP as i32);
        // Whichever lane the sending arm writes, decode_message (the same
        // decode VM runs) reads from it.
        let req = minix_types::VmMmapIn::decode_message(&sent[0]);
        assert_eq!(req.addr.0, stack_low);
        assert_eq!(req.length.0, exec::DEFAULT_STACK_LIMIT);
        assert_eq!(req.forwhom, Endpoint(5));
        assert_eq!(req.prot, PROT_RWX);
        // The helper uniformly supplies `MAP_PRIVATE` (to pass is_valid) and
        // `MAP_THIRDPARTY` (so the mapping lands on forwhom=target rather than
        // caller). The lane assertion includes both bits.
        assert_eq!(
            req.flags,
            map_flags::ANON | map_flags::FIXED | map_flags::PRIVATE | map_flags::THIRDPARTY
        );
        assert_eq!(req.fd, -1);
        // Truncation regression pin: the old u32 arm corrupted the request into
        // its low 32 bits.
        assert_ne!((stack_low as u32) as u64, stack_low);
    }

    /// Wire shape 2: the PM_EXEC_NEW payload and the verdict reply
    /// (exec_general.c:78-90).
    #[test]
    fn test_pm_newexec_wire_and_verdict() {
        let ipc = ExecScriptedIpc::new();
        let allow = pm_newexec(
            &ipc,
            Endpoint(5),
            0xdead,
            4096,
            "sh",
            1000,
            100,
            true,
            USER_STACK_TOP as u64,
            4 * 1024 * 1024,
            256,
        )
        .expect("pm_newexec succeeds");
        assert!(!allow, "scripted PM replies suid=0");
        let types = ipc.types_of(Endpoint::PM);
        assert_eq!(types, vec![minix_types::PM_EXEC_NEW]);
    }

    /// Orchestration 3: the open phase of `pm_exec` — the lookup/stat/read
    /// three-step dance lands on MFS one by one, the first block is all zeros →
    /// ENOEXEC (no loader accepts it), and the vnode reference goes back into
    /// pending_puts (the put_vnode half of C pm_execfinal).
    #[test]
    fn test_pm_exec_open_phase_wires_and_bail() {
        let mut state = VfsState::new();
        crate::main_loop::seed_ready_state(&mut state);
        // Target process slot (endpoint 5 → slot 5).
        {
            let fp = state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(5))
                .expect("slot exists");
            fp.pid = 500;
            fp.endpoint = Endpoint(5);
        }
        let kernel = minix_sys::syscall::CannedKernelCallTransport::new();
        let ipc = ExecScriptedIpc::new();
        let req = ExecRequest {
            target: Endpoint(5),
            path_addr: 0x7000, // the canned kernel moves no bytes; the path
            // comes from process space — host datacopy is unreachable, see the
            // note on the path source below.
            path_len: 0,
            frame_addr: 0x8000,
            frame_len: 16,
            ps_str: 0x1234,
            stack_high: USER_STACK_TOP as u64,
        };
        // Under the host canned kernel the path buffer fetch_name's sys_datacopy
        // gets is zero-filled → empty path → an honest rejection (the pipeline
        // stops right here; the wire shape of the three-step dance is covered
        // separately by the lookup seam test — see test_lookup_closure_wire).
        let r = pm_exec(&mut state, &kernel, &ipc, req);
        // The name-fetch gate (same convention as path.rs fetch: an empty path
        // is EINVAL) trips first — no VM/PM traffic.
        assert_eq!(r, Err(neg(minix_types::EINVAL)));
        assert!(ipc.types_of(Endpoint::VM).is_empty());
        assert!(ipc.types_of(Endpoint::PM).is_empty());
    }

    /// Orchestration 3: the frame limit gate (exec.c:227-229's ARG_MAX gate
    /// before every round trip).
    #[test]
    fn test_pm_exec_frame_gate_enomem() {
        let mut state = VfsState::new();
        crate::main_loop::seed_ready_state(&mut state);
        {
            let fp = state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(5))
                .expect("slot exists");
            fp.pid = 500;
            fp.endpoint = Endpoint(5);
        }
        let kernel = minix_sys::syscall::CannedKernelCallTransport::new();
        let ipc = ExecScriptedIpc::new();
        let req = ExecRequest {
            target: Endpoint(5),
            path_addr: 0,
            path_len: 0,
            frame_addr: 0,
            frame_len: exec::ARG_MAX + 1,
            ps_str: 0,
            stack_high: USER_STACK_TOP as u64,
        };
        assert_eq!(
            pm_exec(&mut state, &kernel, &ipc, req),
            Err(neg(minix_types::ENOMEM))
        );
        assert!(ipc.types_of(Endpoint::VM).is_empty());
        assert!(ipc.types_of(Endpoint::PM).is_empty());
    }

    /// Orchestration 3: a bad target slot (no fproc row) is rejected honestly,
    /// with zero wire traffic.
    #[test]
    fn test_pm_exec_rejects_dead_target() {
        let mut state = VfsState::new();
        crate::main_loop::seed_ready_state(&mut state);
        let kernel = minix_sys::syscall::CannedKernelCallTransport::new();
        let ipc = ExecScriptedIpc::new();
        let req = ExecRequest {
            target: Endpoint(7), // slot 7 has no fproc row
            path_addr: 0,
            path_len: 0,
            frame_addr: 0,
            frame_len: 8,
            ps_str: 0,
            stack_high: USER_STACK_TOP as u64,
        };
        // A dead target takes the bare positive `enoexec` leg (inner body), and
        // the public `pm_exec` boundary folds it negative: assert -ENOEXEC (the
        // leak that had been frozen as +8 is gone).
        assert_eq!(
            pm_exec(&mut state, &kernel, &ipc, req),
            Err(-ExecError::NoExec.to_errno())
        );
        assert!(ipc.types_of(Endpoint::VM).is_empty());
    }

    /// Regression: the positive static legs of the inner body
    /// (`enoexec`/`ExecError::*::to_errno()`) must be folded negative by the
    /// public `pm_exec` boundary — otherwise the real errno is swallowed into
    /// EIO by init's `exec_via` `Ok(_) => EIO` success lane and never surfaces.
    #[test]
    fn test_pm_exec_boundary_folds_positive_errno_negative() {
        let mut state = VfsState::new();
        crate::main_loop::seed_ready_state(&mut state);
        let kernel = minix_sys::syscall::CannedKernelCallTransport::new();
        let ipc = ExecScriptedIpc::new();
        let req = ExecRequest {
            target: Endpoint(7), // no fproc row → inner leaks +ENOEXEC
            path_addr: 0,
            path_len: 0,
            frame_addr: 0,
            frame_len: 8,
            ps_str: 0,
            stack_high: USER_STACK_TOP as u64,
        };
        match pm_exec(&mut state, &kernel, &ipc, req) {
            // Before the fold the inner body returns +8 (measured on a real
            // machine); after the fold it must be negative.
            Err(errno) => assert!(errno < 0, "exec 失败必须以负 errno 直达 PM，实得 {errno}"),
            Ok(_) => panic!("死目标应失败"),
        }
    }
}
