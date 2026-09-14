//! The sysctl(2) exchange formats: the byte layouts userland parses.
//!
//! These are the A-4 structures — the shapes that cross the address
//! space between MIB and its clients (`sysctlnode`/`sysctldesc`) and
//! the process-information records `ps`/`top` consume
//! (`kinfo_lwp`/`kinfo_proc2`). They are contracts, not internals:
//! `SYSCTL_VERSION` is pinned to NetBSD VERS_1 (02), so every field
//! sits where a NetBSD-era client expects it.
//!
//! The C headers engineered these layouts to be **data-model stable**:
//! `__sysc_pad` wraps every pointer in a `uint64_t` union
//! (sys/sys/sysctl.h:1367-1373) and `kinfo_proc2` hand-pads its tail
//! ("use 8 byte alignment", sysctl.h:459), so the ILP32 reference build
//! and an LP64 consumer agree on every offset. The Rust transcriptions
//! use fixed-width fields in C order and pin the result with
//! `offset_of!`/`size_of` assertions — the exchange face never drifts
//! with the platform. `[ARCH: ...]` minix-rs userland is self-consistently
//! 64-bit; the C i386 ABI remains the ground truth these assertions are
//! checked against. 02-mib-message-contract.md (A-4); 17/18/20 篇消费。
//!
//! C sources: `minix3/sys/sys/sysctl.h:1382-1410`（sysctlnode）、
//! `:1444-1450`（sysctldesc）、`:647-673`（kinfo_lwp）、`:473-640`
//! （kinfo_proc2）。

// ── KI_* sizing constants ──
// C: sys/sys/sysctl.h:458-470.

/// Groups array width. C: `KI_NGROUPS` — sysctl.h:458.
pub const KI_NGROUPS: usize = 16;
/// Command name buffer. C: `KI_MAXCOMLEN` — sysctl.h:465.
pub const KI_MAXCOMLEN: usize = 24;
/// Wchan message buffer. C: `KI_WMESGLEN` — sysctl.h:466.
pub const KI_WMESGLEN: usize = 8;
/// Login name buffer. C: `KI_MAXLOGNAME` — sysctl.h:467.
pub const KI_MAXLOGNAME: usize = 24;
/// Emulation name buffer. C: `KI_MAXEMULLEN` — sysctl.h:468.
pub const KI_MAXEMULLEN: usize = 16;
/// LWP name buffer. C: `KI_LNAMELEN` — sysctl.h:469.
pub const KI_LNAMELEN: usize = 20;
/// "No CPU" sentinel. C: `KI_NOCPU` — sysctl.h:471.
pub const KI_NOCPU: u64 = u64::MAX;

// ── sysctlnode ──

/// The child-window arm of [`SysctlNodeUn`].
///
/// C: `scu_child` — the padded child-array pointer rides last so the
/// arm is 16 bytes in both data models.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SysctlNodeChild {
    /// Size of the child node array. C: `suc_csize`.
    pub suc_csize: u32,
    /// Number of valid children. C: `suc_clen`.
    pub suc_clen: u32,
    /// `__sysc_pad(struct sysctlnode*)` — 8 bytes either model.
    pub _suc_child: u64,
}

/// The external-data arm of [`SysctlNodeUn`].
///
/// C: `scu_data` — two padded words (pointer + offset), 16 bytes.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SysctlNodeData {
    /// `__sysc_pad(void*)` — pointer to external data.
    pub _sud_data: u64,
    /// `__sysc_pad(size_t)` — offset to data.
    pub _sud_offset: u64,
}

/// The `sysctl_un` union: one node's value, five ways.
///
/// C: tree.c reads the arm that matches `CTLTYPE`/flags (immediate
/// data, child window, alias…); which arm is live is a flag decision,
/// not a tag — the union carries no discriminator, exactly like C.
#[repr(C)]
#[derive(Clone, Copy)]
pub union SysctlNodeUn {
    /// Parent-shaped: child window plus array pointer.
    pub scu_child: SysctlNodeChild,
    /// Data-pointer shaped: external data plus offset.
    pub scu_data: SysctlNodeData,
    /// Alias: the node this node refers to. C: `scu_alias`.
    pub scu_alias: i32,
    /// Immediate `int`. C: `scu_idata`.
    pub scu_idata: i32,
    /// Immediate `quad`. C: `scu_qdata`.
    pub scu_qdata: u64,
    /// Immediate `bool`. C: `scu_bdata`.
    pub scu_bdata: u8,
}

/// One node of the sysctl tree, as userland sees it.
///
/// C: `struct sysctlnode` — sys/sys/sysctl.h:1382-1410. 96 bytes in
/// both data models (the C header's own design goal); every pointer
/// field is `__sysc_pad`ded to a `uint64_t`-wide union slot.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SysctlNode {
    /// Flags and type. C: `sysctl_flags`.
    pub sysctl_flags: u32,
    /// MIB number. C: `sysctl_num`.
    pub sysctl_num: i32,
    /// Node name, NUL-padded. C: `sysctl_name[SYSCTL_NAMELEN]`.
    pub sysctl_name: [u8; 32],
    /// Node's version vs. rest of tree. C: `sysctl_ver`.
    pub sysctl_ver: u32,
    /// Reserved; travels as zero. C: `__rsvd`.
    pub __rsvd: u32,
    /// The value union. C: `sysctl_un`.
    pub sysctl_un: SysctlNodeUn,
    /// Size of instrumented data. C: `_sysctl_size`.
    pub _sysctl_size: u64,
    /// Access helper function. C: `_sysctl_func`.
    pub _sysctl_func: u64,
    /// Parent of this node. C: `_sysctl_parent`.
    pub _sysctl_parent: u64,
    /// Description of node. C: `_sysctl_desc`.
    pub _sysctl_desc: u64,
}

// ── sysctldesc ──

/// One node's description, as `CTL_DESCRIBE` returns it.
///
/// C: `struct sysctldesc` — sys/sys/sysctl.h:1444-1450. The header is
/// 12 bytes; `descr_str` is "not really 1" — a variable-length string
/// whose true length is `descr_len` (terminator included), packed to
/// the next 4-byte boundary (`__sysc_desc_roundup`, :1449).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SysctlDesc {
    /// MIB number of node. C: `descr_num`.
    pub descr_num: i32,
    /// Version of node. C: `descr_ver`.
    pub descr_ver: u32,
    /// Length of description string (terminator included). C: `descr_len`.
    pub descr_len: u32,
    /// Variable-length string start. C: `descr_str[1]`.
    pub descr_str: [u8; 1],
}

// ── kinfo_lwp ──

/// One LWP, as `KERN_LWP` returns it.
///
/// C: `struct kinfo_lwp` — sys/sys/sysctl.h:647-673. Fixed-width
/// fields, hand-padded to 8-byte boundaries (`l_pad1`/`l_pad2`), so
/// the record is data-model stable by construction.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KinfoLwp {
    /// PTR: linked run/sleep queue. C: `l_forw`.
    pub l_forw: u64,
    /// PTR: linked run/sleep queue. C: `l_back`.
    pub l_back: u64,
    /// PTR: address of LWP. C: `l_laddr`.
    pub l_laddr: u64,
    /// PTR: kernel virtual addr of u-area. C: `l_addr`.
    pub l_addr: u64,
    /// LWP identifier. C: `l_lid`.
    pub l_lid: i32,
    /// `L_*` flags. C: `l_flag`.
    pub l_flag: i32,
    /// Time swapped in or out. C: `l_swtime`.
    pub l_swtime: u32,
    /// Time since last blocked. C: `l_slptime`.
    pub l_slptime: u32,
    /// `PSCHED_*` flags. C: `l_schedflags`.
    pub l_schedflags: i32,
    /// If non-zero, don't swap. C: `l_holdcnt`.
    pub l_holdcnt: i32,
    /// Process priority. C: `l_priority`.
    pub l_priority: u8,
    /// User priority (from `l_cpu` and `p_nice`). C: `l_usrpri`.
    pub l_usrpri: u8,
    /// `S*` process status. C: `l_stat`.
    pub l_stat: i8,
    /// Fill to 4-byte boundary. C: `l_pad1`.
    pub l_pad1: i8,
    /// Fill to 8-byte boundary. C: `l_pad2`.
    pub l_pad2: i32,
    /// Wchan message. C: `l_wmesg[KI_WMESGLEN]`.
    pub l_wmesg: [u8; KI_WMESGLEN],
    /// PTR: sleep address. C: `l_wchan`.
    pub l_wchan: u64,
    /// CPU id. C: `l_cpuid`.
    pub l_cpuid: u64,
    /// Real time (seconds). C: `l_rtime_sec`.
    pub l_rtime_sec: u32,
    /// Real time (microseconds). C: `l_rtime_usec`.
    pub l_rtime_usec: u32,
    /// Ticks during `l_swtime`. C: `l_cpticks`.
    pub l_cpticks: u32,
    /// CPU usage for ps. C: `l_pctcpu`.
    pub l_pctcpu: u32,
    /// Process identifier. C: `l_pid`.
    pub l_pid: u32,
    /// Name, may be empty. C: `l_name[KI_LNAMELEN]`.
    pub l_name: [u8; KI_LNAMELEN],
}

// ── kinfo_proc2 ──

/// Signal set as it travels on the wire. C: `ki_sigset_t` — 4 words
/// (sysctl.h:471-473).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KiSigset {
    /// Bit words, little-endian order. C: `__bits[4]`.
    pub __bits: [u32; 4],
}

/// One process, as `KERN_PROC2` returns it.
///
/// C: `struct kinfo_proc2` — sys/sys/sysctl.h:473-640. "Relatively
/// fixed size structures … use 8 byte alignment, and new elements
/// should only be added to the end" (:459-462) — the record is the
/// compat story itself: every field is fixed-width, pointers travel as
/// `uint64_t`, and growth appends.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct KinfoProc2 {
    /// PTR: linked run/sleep queue. C: `p_forw`.
    pub p_forw: u64,
    /// PTR: linked run/sleep queue. C: `p_back`.
    pub p_back: u64,
    /// PTR: address of proc. C: `p_paddr`.
    pub p_paddr: u64,
    /// PTR: kernel virtual addr of u-area. C: `p_addr`.
    pub p_addr: u64,
    /// PTR: open files structure. C: `p_fd`.
    pub p_fd: u64,
    /// PTR: cdir/rdir/cmask info. C: `p_cwdi`.
    pub p_cwdi: u64,
    /// PTR: accounting/statistics. C: `p_stats`.
    pub p_stats: u64,
    /// PTR: process limits. C: `p_limit`.
    pub p_limit: u64,
    /// PTR: address space. C: `p_vmspace`.
    pub p_vmspace: u64,
    /// PTR: signal actions, state. C: `p_sigacts`.
    pub p_sigacts: u64,
    /// PTR: session pointer. C: `p_sess`.
    pub p_sess: u64,
    /// PTR: tty session pointer. C: `p_tsess`.
    pub p_tsess: u64,
    /// PTR: exit information. C: `p_ru`.
    pub p_ru: u64,
    /// Extra `kinfo_proc2` flags (`EPROC_*`). C: `p_eflag`.
    pub p_eflag: i32,
    /// Signal to send to parent on exit. C: `p_exitsig`.
    pub p_exitsig: i32,
    /// `P_*` flags. C: `p_flag`.
    pub p_flag: i32,
    /// Process identifier. C: `p_pid`.
    pub p_pid: i32,
    /// Parent process id. C: `p_ppid`.
    pub p_ppid: i32,
    /// Session id. C: `p_sid`.
    pub p_sid: i32,
    /// Process group id. C spells it `p__pgid` (double underscore) only
    /// because `<sys/proc.h>` hijacks `p_pgid`; Rust has no such
    /// collision, so the field takes the clean name.
    pub p_pgid: i32,
    /// TTY process group id. C: `p_tpgid`.
    pub p_tpgid: i32,
    /// Effective user id. C: `p_uid`.
    pub p_uid: u32,
    /// Real user id. C: `p_ruid`.
    pub p_ruid: u32,
    /// Effective group id. C: `p_gid`.
    pub p_gid: u32,
    /// Real group id. C: `p_rgid`.
    pub p_rgid: u32,
    /// Groups. C: `p_groups[KI_NGROUPS]`.
    pub p_groups: [u32; KI_NGROUPS],
    /// Number of groups. C: `p_ngroups`.
    pub p_ngroups: i16,
    /// Job control counter. C: `p_jobc`.
    pub p_jobc: i16,
    /// Controlling tty dev. C: `p_tdev`.
    pub p_tdev: u32,
    /// Time averaged value of `p_cpticks`. C: `p_estcpu`.
    pub p_estcpu: u32,
    /// Real time (seconds). C: `p_rtime_sec`.
    pub p_rtime_sec: u32,
    /// Real time (microseconds). C: `p_rtime_usec`.
    pub p_rtime_usec: u32,
    /// Ticks of CPU time. C: `p_cpticks`.
    pub p_cpticks: i32,
    /// %CPU during `p_swtime`. C: `p_pctcpu`.
    pub p_pctcpu: u32,
    /// Time swapped in or out. C: `p_swtime`.
    pub p_swtime: u32,
    /// Time since last blocked. C: `p_slptime`.
    pub p_slptime: u32,
    /// `PSCHED_*` flags. C: `p_schedflags`.
    pub p_schedflags: i32,
    /// Statclock hits in user mode. C: `p_uticks`.
    pub p_uticks: u64,
    /// Statclock hits in system mode. C: `p_sticks`.
    pub p_sticks: u64,
    /// Statclock hits processing intr. C: `p_iticks`.
    pub p_iticks: u64,
    /// PTR: trace to vnode or file. C: `p_tracep`.
    pub p_tracep: u64,
    /// Kernel trace points. C: `p_traceflag`.
    pub p_traceflag: i32,
    /// If non-zero, don't swap. C: `p_holdcnt`.
    pub p_holdcnt: i32,
    /// Signals arrived but not delivered. C: `p_siglist`.
    pub p_siglist: KiSigset,
    /// Current signal mask. C: `p_sigmask`.
    pub p_sigmask: KiSigset,
    /// Signals being ignored. C: `p_sigignore`.
    pub p_sigignore: KiSigset,
    /// Signals being caught by user. C: `p_sigcatch`.
    pub p_sigcatch: KiSigset,
    /// `S*` process status (from LWP). C: `p_stat`.
    pub p_stat: i8,
    /// Process priority. C: `p_priority`.
    pub p_priority: u8,
    /// User priority (from `p_cpu` and `p_nice`). C: `p_usrpri`.
    pub p_usrpri: u8,
    /// Process "nice" value. C: `p_nice`.
    pub p_nice: u8,
    /// Exit status for wait; also stop signal. C: `p_xstat`.
    pub p_xstat: u16,
    /// Accounting flags. C: `p_acflag`.
    pub p_acflag: u16,
    /// Command name. C: `p_comm[KI_MAXCOMLEN]`.
    pub p_comm: [u8; KI_MAXCOMLEN],
    /// Wchan message. C: `p_wmesg[KI_WMESGLEN]`.
    pub p_wmesg: [u8; KI_WMESGLEN],
    /// PTR: sleep address. C: `p_wchan`.
    pub p_wchan: u64,
    /// `setlogin()` name. C: `p_login[KI_MAXLOGNAME]`.
    pub p_login: [u8; KI_MAXLOGNAME],
    /// Current resident set size in pages. C: `p_vm_rssize`.
    pub p_vm_rssize: i32,
    /// Text size (pages). C: `p_vm_tsize`.
    pub p_vm_tsize: i32,
    /// Data size (pages). C: `p_vm_dsize`.
    pub p_vm_dsize: i32,
    /// Stack size (pages). C: `p_vm_ssize`.
    pub p_vm_ssize: i32,
    /// Following `p_u*` fields are valid. C: `p_uvalid` (64 for
    /// alignment).
    pub p_uvalid: i64,
    /// Starting time (seconds). C: `p_ustart_sec`.
    pub p_ustart_sec: u32,
    /// Starting time (microseconds). C: `p_ustart_usec`.
    pub p_ustart_usec: u32,
    /// User time (seconds). C: `p_uutime_sec`.
    pub p_uutime_sec: u32,
    /// User time (microseconds). C: `p_uutime_usec`.
    pub p_uutime_usec: u32,
    /// System time (seconds). C: `p_ustime_sec`.
    pub p_ustime_sec: u32,
    /// System time (microseconds). C: `p_ustime_usec`.
    pub p_ustime_usec: u32,
    /// Max resident set size. C: `p_uru_maxrss`.
    pub p_uru_maxrss: u64,
    /// Integral shared memory size. C: `p_uru_ixrss`.
    pub p_uru_ixrss: u64,
    /// Integral unshared data. C: `p_uru_idrss`.
    pub p_uru_idrss: u64,
    /// Integral unshared stack. C: `p_uru_isrss`.
    pub p_uru_isrss: u64,
    /// Page reclaims. C: `p_uru_minflt`.
    pub p_uru_minflt: u64,
    /// Page faults. C: `p_uru_majflt`.
    pub p_uru_majflt: u64,
    /// Swaps. C: `p_uru_nswap`.
    pub p_uru_nswap: u64,
    /// Block input operations. C: `p_uru_inblock`.
    pub p_uru_inblock: u64,
    /// Block output operations. C: `p_uru_oublock`.
    pub p_uru_oublock: u64,
    /// Messages sent. C: `p_uru_msgsnd`.
    pub p_uru_msgsnd: u64,
    /// Messages received. C: `p_uru_msgrcv`.
    pub p_uru_msgrcv: u64,
    /// Signals received. C: `p_uru_nsignals`.
    pub p_uru_nsignals: u64,
    /// Voluntary context switches. C: `p_uru_nvcsw`.
    pub p_uru_nvcsw: u64,
    /// Involuntary context switches. C: `p_uru_nivcsw`.
    pub p_uru_nivcsw: u64,
    /// Child u+s time (seconds). C: `p_uctime_sec`.
    pub p_uctime_sec: u32,
    /// Child u+s time (microseconds). C: `p_uctime_usec`.
    pub p_uctime_usec: u32,
    /// CPU id. C: `p_cpuid`.
    pub p_cpuid: u64,
    /// `P_*` flags not including LWPs. C: `p_realflag`.
    pub p_realflag: u64,
    /// Number of LWPs. C: `p_nlwps`.
    pub p_nlwps: u64,
    /// Number of running LWPs. C: `p_nrlwps`.
    pub p_nrlwps: u64,
    /// Non-LWP process status. C: `p_realstat`.
    pub p_realstat: u64,
    /// Saved user id. C: `p_svuid`.
    pub p_svuid: u32,
    /// Saved group id. C: `p_svgid`.
    pub p_svgid: u32,
    /// Emulation name. C: `p_ename[KI_MAXEMULLEN]`.
    pub p_ename: [u8; KI_MAXEMULLEN],
    /// Total map size (pages). C: `p_vm_vsize`.
    pub p_vm_vsize: i64,
    /// Stack-adjusted map size (pages). C: `p_vm_msize`.
    pub p_vm_msize: i64,
}


// ── Handler payload structs (A-4 trailing items, pinned at first
//    consumption — the walker's scalar function handlers are that
//    first consumer) ──

/// Kernel clock rates. C: `struct clockinfo` — sys/sys/sysctl.h:206-212
/// (five `int`s; data-model stable).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Clockinfo {
    /// Ticks per second. C: `hz`.
    pub hz: i32,
    /// Microseconds per tick. C: `tick`.
    pub tick: i32,
    /// Microseconds of tick adjustment. C: `tickadj`.
    pub tickadj: i32,
    /// Statistics clock frequency. C: `stathz`.
    pub stathz: i32,
    /// Profiling clock frequency. C: `profhz`.
    pub profhz: i32,
}

/// Broken-down time. C: `struct timeval` — `long tv_sec/tv_usec`, so
/// this record is LP64 (8+8); the i386 reference build packs it in 8.
/// Pinned as part of the self-consistent-64-bit A-4 decision.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeval {
    /// Seconds. C: `tv_sec`.
    pub tv_sec: i64,
    /// Microseconds. C: `tv_usec`.
    pub tv_usec: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{align_of, offset_of, size_of};

    /// Handler payload pins (A-4 trailing items).
    #[test]
    fn test_payload_layouts() {
        assert_eq!(size_of::<Clockinfo>(), 20);
        assert_eq!(offset_of!(Clockinfo, profhz), 16);
        assert_eq!(size_of::<Timeval>(), 16);
        assert_eq!(offset_of!(Timeval, tv_usec), 8);
    }

    /// The exchange face is 96 bytes in *both* C data models (the
    /// `__sysc_pad` design goal) — the Rust transcription must agree
    /// exactly, and the union must sit where the C union sits.
    #[test]
    fn test_sysctl_node_layout() {
        assert_eq!(size_of::<SysctlNode>(), 96);
        assert_eq!(offset_of!(SysctlNode, sysctl_flags), 0);
        assert_eq!(offset_of!(SysctlNode, sysctl_num), 4);
        assert_eq!(offset_of!(SysctlNode, sysctl_name), 8);
        assert_eq!(offset_of!(SysctlNode, sysctl_ver), 40);
        assert_eq!(offset_of!(SysctlNode, __rsvd), 44);
        assert_eq!(offset_of!(SysctlNode, sysctl_un), 48);
        assert_eq!(size_of::<SysctlNodeUn>(), 16);
        assert_eq!(offset_of!(SysctlNode, _sysctl_size), 64);
        assert_eq!(offset_of!(SysctlNode, _sysctl_func), 72);
        assert_eq!(offset_of!(SysctlNode, _sysctl_parent), 80);
        assert_eq!(offset_of!(SysctlNode, _sysctl_desc), 88);
        // The child arm: two lanes then the padded pointer at +8.
        assert_eq!(offset_of!(SysctlNodeChild, suc_csize), 0);
        assert_eq!(offset_of!(SysctlNodeChild, suc_clen), 4);
        assert_eq!(offset_of!(SysctlNodeChild, _suc_child), 8);
        assert_eq!(size_of::<SysctlNodeChild>(), 16);
        // Union arms all start at the union base.
        assert_eq!(offset_of!(SysctlNodeData, _sud_offset), 8);
    }

    /// The description header is 12 bytes (`describe.rs` derives its
    /// constants from this struct) and the whole record is 16.
    #[test]
    fn test_sysctl_desc_layout() {
        assert_eq!(offset_of!(SysctlDesc, descr_num), 0);
        assert_eq!(offset_of!(SysctlDesc, descr_ver), 4);
        assert_eq!(offset_of!(SysctlDesc, descr_len), 8);
        assert_eq!(offset_of!(SysctlDesc, descr_str), 12);
        assert_eq!(size_of::<SysctlDesc>(), 16);
        assert_eq!(align_of::<SysctlDesc>(), 4);
    }

    /// Hand-checked against sysctl.h:647-673: pads land the tail so
    /// the record closes on an 8-byte boundary at 128.
    #[test]
    fn test_kinfo_lwp_layout() {
        assert_eq!(size_of::<KinfoLwp>(), 128);
        assert_eq!(offset_of!(KinfoLwp, l_lid), 32);
        assert_eq!(offset_of!(KinfoLwp, l_priority), 56);
        assert_eq!(offset_of!(KinfoLwp, l_pad2), 60);
        assert_eq!(offset_of!(KinfoLwp, l_wmesg), 64);
        assert_eq!(offset_of!(KinfoLwp, l_wchan), 72);
        assert_eq!(offset_of!(KinfoLwp, l_cpuid), 80);
        assert_eq!(offset_of!(KinfoLwp, l_rtime_sec), 88);
        assert_eq!(offset_of!(KinfoLwp, l_pid), 104);
        assert_eq!(offset_of!(KinfoLwp, l_name), 108);
    }

    /// Hand-checked against sysctl.h:473-640: 13 pointer words, then
    /// the scalar block, four sigsets, and the growth-only tail.
    #[test]
    fn test_kinfo_proc2_layout() {
        assert_eq!(size_of::<KinfoProc2>(), 680);
        assert_eq!(offset_of!(KinfoProc2, p_eflag), 104);
        assert_eq!(offset_of!(KinfoProc2, p_pid), 116);
        assert_eq!(offset_of!(KinfoProc2, p_uid), 136);
        assert_eq!(offset_of!(KinfoProc2, p_groups), 152);
        assert_eq!(offset_of!(KinfoProc2, p_ngroups), 216);
        assert_eq!(offset_of!(KinfoProc2, p_tdev), 220);
        assert_eq!(offset_of!(KinfoProc2, p_uticks), 256);
        assert_eq!(offset_of!(KinfoProc2, p_tracep), 280);
        assert_eq!(offset_of!(KinfoProc2, p_siglist), 296);
        assert_eq!(size_of::<KiSigset>(), 16);
        assert_eq!(offset_of!(KinfoProc2, p_stat), 360);
        assert_eq!(offset_of!(KinfoProc2, p_xstat), 364);
        assert_eq!(offset_of!(KinfoProc2, p_comm), 368);
        assert_eq!(offset_of!(KinfoProc2, p_wchan), 400);
        assert_eq!(offset_of!(KinfoProc2, p_login), 408);
        assert_eq!(offset_of!(KinfoProc2, p_vm_rssize), 432);
        assert_eq!(offset_of!(KinfoProc2, p_uvalid), 448);
        assert_eq!(offset_of!(KinfoProc2, p_ustart_sec), 456);
        assert_eq!(offset_of!(KinfoProc2, p_uru_maxrss), 480);
        assert_eq!(offset_of!(KinfoProc2, p_uctime_sec), 592);
        assert_eq!(offset_of!(KinfoProc2, p_cpuid), 600);
        assert_eq!(offset_of!(KinfoProc2, p_svuid), 640);
        assert_eq!(offset_of!(KinfoProc2, p_ename), 648);
        assert_eq!(offset_of!(KinfoProc2, p_vm_vsize), 664);
        assert_eq!(offset_of!(KinfoProc2, p_vm_msize), 672);
        // The compat sentinel is all-ones.
        assert_eq!(KI_NOCPU, u64::MAX);
    }
}
