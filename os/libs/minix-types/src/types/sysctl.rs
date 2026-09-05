//! sysctl wire vocabulary: names, types, flags, versions, meta-ids.
//!
//! Corresponds to Minix3's `<sys/sysctl.h>` (NetBSD VERS_1 heritage) plus
//! the MINIX3 extensions in `<minix/sysctl.h>` (`CTL_MINIX` and below).
//!
//! This module is the **wire table**: every constant here travels inside a
//! message or inside a `sysctlnode`/`sysctldesc` exchanged with userland,
//! so values are pinned by tests against the C headers. How the flags
//! *behave* on a node (parent/remote matrix, verify callbacks) is the
//! node model's business — see `tree::flag` in `minix-mib` (03), which
//! owns the `PARENT`/`VERIFY`/`REMOTE` reassignments of `ROOT`/`ALIAS`/
//! `MMAP` documented below.
//!
//! Decode thresholds `CTL_MAXNAME`/`CTL_SHORTNAME` are repeated here as
//! the table authority; the judging copies in `minix-mib` `dispatch.rs`
//! (01) must match — both crates pin the same literals in tests.

// ── Name length limits ──

/// Largest sysctl name, in components. C: `CTL_MAXNAME` — sys/sys/sysctl.h:75.
pub const CTL_MAXNAME: u32 = 12;
/// Longest node name. C: `SYSCTL_NAMELEN` — sys/sys/sysctl.h:76.
pub const SYSCTL_NAMELEN: usize = 32;
/// Longest name that fits inside the request message, in components.
/// C: `CTL_SHORTNAME` — minix/ipc.h:15.
pub const CTL_SHORTNAME: u32 = 8;
/// First dynamic node id. C: `CREATE_BASE` — sys/sys/sysctl.h:78.
pub const CREATE_BASE: i32 = 1024;
/// Initial child-set size. C: `SYSCTL_DEFSIZE` — sys/sys/sysctl.h:79.
pub const SYSCTL_DEFSIZE: u32 = 8;

// ── Top-level identifiers ──
//
// C: `CTL_*` — sys/sys/sysctl.h:169-183.

/// Unused. C: `CTL_UNSPEC` — sysctl.h:169.
pub const CTL_UNSPEC: i32 = 0;
/// High kernel. C: `CTL_KERN` — sysctl.h:170.
pub const CTL_KERN: i32 = 1;
/// Virtual memory. C: `CTL_VM` — sysctl.h:171.
pub const CTL_VM: i32 = 2;
/// File system. C: `CTL_VFS` — sysctl.h:172.
pub const CTL_VFS: i32 = 3;
/// Networking (remote-populated). C: `CTL_NET` — sysctl.h:173.
pub const CTL_NET: i32 = 4;
/// Debugging parameters. C: `CTL_DEBUG` — sysctl.h:174.
pub const CTL_DEBUG: i32 = 5;
/// Generic CPU/io. C: `CTL_HW` — sysctl.h:176.
pub const CTL_HW: i32 = 6;
/// Machine dependent. C: `CTL_MACHDEP` — sysctl.h:177.
pub const CTL_MACHDEP: i32 = 7;
/// User-level (handled inside libc, never reaches MIB). C: `CTL_USER` — sysctl.h:178.
pub const CTL_USER: i32 = 8;
/// In-kernel debugger. C: `CTL_DDB` — sysctl.h:179.
pub const CTL_DDB: i32 = 9;
/// Per-proc attributes. C: `CTL_PROC` — sysctl.h:180.
pub const CTL_PROC: i32 = 10;
/// Vendor-specific (writable scratch). C: `CTL_VENDOR` — sysctl.h:181.
pub const CTL_VENDOR: i32 = 11;
/// Emulation-specific. C: `CTL_EMUL` — sysctl.h:182.
pub const CTL_EMUL: i32 = 12;
/// Security. C: `CTL_SECURITY` — sysctl.h:183.
pub const CTL_SECURITY: i32 = 13;
/// Number of valid top-level ids. C: `CTL_MAXID` — sysctl.h:183.
pub const CTL_MAXID: i32 = 14;

/// MINIX3 top-level id, parked past NetBSD's range so future NetBSD ids
/// never collide (part of the MINIX3 ABI).
/// C: `CTL_MINIX` — minix/sysctl.h:17.
pub const CTL_MINIX: i32 = 32;

// Compile-time mirror of C's `#error "CTL_MAXID has grown too large!"`
// (minix/sysctl.h:19-21).
const _: () = assert!(CTL_MAXID <= CTL_MINIX);

// ── MINIX3 subtree identifiers ──
//
// C: `MINIX_*` — minix/sysctl.h:28-31.

/// Test subtree (gated by `MINIX_TEST_SUBTREE`). C: `MINIX_TEST`.
pub const MINIX_TEST: i32 = 0;
/// MIB statistics subtree. C: `MINIX_MIB`.
pub const MINIX_MIB: i32 = 1;
/// Process list/data subtree (ProcFS contract). C: `MINIX_PROC`.
pub const MINIX_PROC: i32 = 2;
/// LWIP subtree (remote-populated). C: `MINIX_LWIP`.
pub const MINIX_LWIP: i32 = 3;

// ── Test node identifiers (test87 contract) ──
//
// C: `TEST_*` — minix/sysctl.h:37-48.

/// Integer leaf. C: `TEST_INT`.
pub const TEST_INT: i32 = 0;
/// Bool leaf. C: `TEST_BOOL`.
pub const TEST_BOOL: i32 = 1;
/// Quad leaf. C: `TEST_QUAD`.
pub const TEST_QUAD: i32 = 2;
/// String leaf. C: `TEST_STRING`.
pub const TEST_STRING: i32 = 3;
/// Struct leaf. C: `TEST_STRUCT`.
pub const TEST_STRUCT: i32 = 4;
/// Private node. C: `TEST_PRIVATE`.
pub const TEST_PRIVATE: i32 = 5;
/// Any-write node. C: `TEST_ANYWRITE`.
pub const TEST_ANYWRITE: i32 = 6;
/// Dynamic node. C: `TEST_DYNAMIC`.
pub const TEST_DYNAMIC: i32 = 7;
/// Secret (private) node. C: `TEST_SECRET`.
pub const TEST_SECRET: i32 = 8;
/// Permission-matrix node. C: `TEST_PERM`.
pub const TEST_PERM: i32 = 9;
/// Destroy probe 1. C: `TEST_DESTROY1`.
pub const TEST_DESTROY1: i32 = 10;
/// Destroy probe 2. C: `TEST_DESTROY2`.
pub const TEST_DESTROY2: i32 = 11;
/// Value backing the secret node. C: `SECRET_VALUE` — minix/sysctl.h:50.
pub const SECRET_VALUE: i32 = 0;

// ── MINIX_MIB statistics identifiers ──
//
// C: `MIB_*` — minix/sysctl.h:53-55.

/// Live node count. C: `MIB_NODES`.
pub const MIB_NODES: i32 = 1;
/// Live object count. C: `MIB_OBJECTS`.
pub const MIB_OBJECTS: i32 = 2;
/// Live remote-mount count. C: `MIB_REMOTES`.
pub const MIB_REMOTES: i32 = 3;

// ── MINIX_PROC identifiers ──
//
// C: `PROC_*` — minix/sysctl.h:58-59. Layouts (`minix_proc_list`/
// `minix_proc_data`) are 20's domain; only the ids travel here.

/// Full table snapshot. C: `PROC_LIST`.
pub const PROC_LIST: i32 = 1;
/// Single-PID snapshot. C: `PROC_DATA`.
pub const PROC_DATA: i32 = 2;

// ── KERN_PROC_ARGS requests, PROC_LIST/DATA flags and layouts (19, 20) ──
//
// C: `KERN_PROC_ARGV/NARGV/ENV/NENV` — sys/sys/sysctl.h:677-680;
// `MPLF_*`/`MPDF_*` and both structures — minix/sysctl.h:61-88.
// Both structures say "Not part of the ABI. Used by ProcFS only."
// That sentence means NetBSD tools never read them; it does not mean
// the layout is free. ProcFS reads them field by field, so the field
// order and widths below must stay identical to the C structures.

/// Argument vector. C: `KERN_PROC_ARGV 1` — sysctl.h:677.
pub const KERN_PROC_ARGV: i32 = 1;
/// Number of argument strings. C: `KERN_PROC_NARGV 2` — sysctl.h:678.
pub const KERN_PROC_NARGV: i32 = 2;
/// Environment vector. C: `KERN_PROC_ENV 3` — sysctl.h:679.
pub const KERN_PROC_ENV: i32 = 3;
/// Number of environment strings. C: `KERN_PROC_NENV 4` — sysctl.h:680.
pub const KERN_PROC_NENV: i32 = 4;

/// Slot is in use. C: `MPLF_IN_USE 0x01` — minix/sysctl.h:68.
pub const MPLF_IN_USE: u32 = 0x01;
/// Slot holds a zombie. C: `MPLF_ZOMBIE 0x02` — minix/sysctl.h:69.
pub const MPLF_ZOMBIE: u32 = 0x02;

/// System service. C: `MPDF_SYSTEM 0x01` — minix/sysctl.h:85.
pub const MPDF_SYSTEM: u32 = 0x01;
/// Zombie. C: `MPDF_ZOMBIE 0x02` — minix/sysctl.h:86.
pub const MPDF_ZOMBIE: u32 = 0x02;
/// Runnable. C: `MPDF_RUNNABLE 0x04` — minix/sysctl.h:87.
pub const MPDF_RUNNABLE: u32 = 0x04;
/// Stopped. C: `MPDF_STOPPED 0x08` — minix/sysctl.h:88.
pub const MPDF_STOPPED: u32 = 0x08;

/// One row of the full-table snapshot.
///
/// C: `struct minix_proc_list` — minix/sysctl.h:62-67 (four fields,
/// 16 bytes). `pid_t`/`uid_t`/`gid_t` are 32-bit here.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MinixProcList {
    /// Slot flags (`MPLF_*`).
    pub mpl_flags: u32,
    /// Process id.
    pub mpl_pid: i32,
    /// Effective user id.
    pub mpl_uid: u32,
    /// Effective group id.
    pub mpl_gid: u32,
}

/// One single-process snapshot.
///
/// C: `struct minix_proc_data` — minix/sysctl.h:72-84 (eleven fields,
/// 72 bytes with trailing padding to the 8-byte alignment).
/// `endpoint_t` is 32-bit; the name is a 16-byte buffer.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MinixProcData {
    /// Process endpoint.
    pub mpd_endpoint: i32,
    /// Process flags (`MPDF_*`).
    pub mpd_flags: u32,
    /// Blocked on this endpoint, or none.
    pub mpd_blocked_on: i32,
    /// Current priority.
    pub mpd_priority: u32,
    /// User time in clock ticks.
    pub mpd_user_time: u32,
    /// System time in clock ticks.
    pub mpd_sys_time: u32,
    /// Cycles spent by the process.
    pub mpd_cycles: u64,
    /// Cycles spent on kernel IPC.
    pub mpd_kipc_cycles: u64,
    /// Cycles spent on kernel calls.
    pub mpd_kcall_cycles: u64,
    /// Nice value.
    pub mpd_nice: u32,
    /// Short process name.
    pub mpd_name: [u8; 16],
}

/// Function-driven node marker: a fake nonzero func address reporting
/// "do not descend, a handler owns this" without leaking the real
/// pointer (anti-ASR). C: `SYSCTL_NODE_FN` — minix/sysctl.h:10.
pub const SYSCTL_NODE_FN: u32 = 0x1;

// ── CTL_KERN identifiers ──
//
// C: `KERN_*` — sys/sys/sysctl.h:194-278. The full 84-slot wire table;
// which slots MIB populates (45) vs leaves empty (39, A-9) is 13's
// domain (`subtree::kern`), values travel here.

/// String: system version. C: `KERN_OSTYPE` — sysctl.h:194.
pub const KERN_OSTYPE: i32 = 1;
/// String: system release. C: `KERN_OSRELEASE`.
pub const KERN_OSRELEASE: i32 = 2;
/// Int: system revision. C: `KERN_OSREV`.
pub const KERN_OSREV: i32 = 3;
/// String: compile time info. C: `KERN_VERSION`.
pub const KERN_VERSION: i32 = 4;
/// Int: max vnodes. C: `KERN_MAXVNODES`.
pub const KERN_MAXVNODES: i32 = 5;
/// Int: max processes. C: `KERN_MAXPROC`.
pub const KERN_MAXPROC: i32 = 6;
/// Int: max open files. C: `KERN_MAXFILES`.
pub const KERN_MAXFILES: i32 = 7;
/// Int: max arguments to exec. C: `KERN_ARGMAX`.
pub const KERN_ARGMAX: i32 = 8;
/// Int: system security level. C: `KERN_SECURELVL`.
pub const KERN_SECURELVL: i32 = 9;
/// String: hostname. C: `KERN_HOSTNAME`.
pub const KERN_HOSTNAME: i32 = 10;
/// Int: host identifier. C: `KERN_HOSTID`.
pub const KERN_HOSTID: i32 = 11;
/// Struct: struct clockinfo. C: `KERN_CLOCKRATE`.
pub const KERN_CLOCKRATE: i32 = 12;
/// Struct: vnode structures (unimplemented). C: `KERN_VNODE`.
pub const KERN_VNODE: i32 = 13;
/// Struct: process entries (unimplemented). C: `KERN_PROC`.
pub const KERN_PROC: i32 = 14;
/// Struct: file entries (unimplemented). C: `KERN_FILE`.
pub const KERN_FILE: i32 = 15;
/// Node: kernel profiling info. C: `KERN_PROF`.
pub const KERN_PROF: i32 = 16;
/// Int: POSIX.1 version. C: `KERN_POSIX1`.
pub const KERN_POSIX1: i32 = 17;
/// Int: # of supplemental group ids. C: `KERN_NGROUPS`.
pub const KERN_NGROUPS: i32 = 18;
/// Int: is job control available. C: `KERN_JOB_CONTROL`.
pub const KERN_JOB_CONTROL: i32 = 19;
/// Int: saved set-user/group-ID. C: `KERN_SAVED_IDS`.
pub const KERN_SAVED_IDS: i32 = 20;
/// Struct: time kernel was booted (obsolete). C: `KERN_OBOOTTIME`.
pub const KERN_OBOOTTIME: i32 = 21;
/// String: (YP) domainname. C: `KERN_DOMAINNAME`.
pub const KERN_DOMAINNAME: i32 = 22;
/// Int: number of partitions/disk. C: `KERN_MAXPARTITIONS`.
pub const KERN_MAXPARTITIONS: i32 = 23;
/// Int: raw partition number (incompatible). C: `KERN_RAWPARTITION`.
pub const KERN_RAWPARTITION: i32 = 24;
/// Struct: extended-precision time (unimplemented). C: `KERN_NTPTIME`.
pub const KERN_NTPTIME: i32 = 25;
/// Struct: ntp timekeeping state (unimplemented). C: `KERN_TIMEX`.
pub const KERN_TIMEX: i32 = 26;
/// Int: proc time before autonice (unimplemented). C: `KERN_AUTONICETIME`.
pub const KERN_AUTONICETIME: i32 = 27;
/// Int: auto nice value (unimplemented). C: `KERN_AUTONICEVAL`.
pub const KERN_AUTONICEVAL: i32 = 28;
/// Int: offset of rtc from gmt. C: `KERN_RTC_OFFSET`.
pub const KERN_RTC_OFFSET: i32 = 29;
/// String: root device. C: `KERN_ROOT_DEVICE`.
pub const KERN_ROOT_DEVICE: i32 = 30;
/// Int: max # of chars in msg buffer. C: `KERN_MSGBUFSIZE`.
pub const KERN_MSGBUFSIZE: i32 = 31;
/// Int: file synchronization support. C: `KERN_FSYNC`.
pub const KERN_FSYNC: i32 = 32;
/// Old: SysV message queue support. C: `KERN_OLDSYSVMSG`.
pub const KERN_OLDSYSVMSG: i32 = 33;
/// Old: SysV semaphore support. C: `KERN_OLDSYSVSEM`.
pub const KERN_OLDSYSVSEM: i32 = 34;
/// Old: SysV shared memory support. C: `KERN_OLDSYSVSHM`.
pub const KERN_OLDSYSVSHM: i32 = 35;
/// Old, unimplemented. C: `KERN_OLDSHORTCORENAME`.
pub const KERN_OLDSHORTCORENAME: i32 = 36;
/// Int: POSIX synchronized I/O. C: `KERN_SYNCHRONIZED_IO`.
pub const KERN_SYNCHRONIZED_IO: i32 = 37;
/// Int: max iovec's for readv(2) etc. C: `KERN_IOV_MAX`.
pub const KERN_IOV_MAX: i32 = 38;
/// Node: mbuf parameters (unimplemented). C: `KERN_MBUF`.
pub const KERN_MBUF: i32 = 39;
/// Int: POSIX memory mapped files. C: `KERN_MAPPED_FILES`.
pub const KERN_MAPPED_FILES: i32 = 40;
/// Int: POSIX memory locking. C: `KERN_MEMLOCK`.
pub const KERN_MEMLOCK: i32 = 41;
/// Int: POSIX memory range locking. C: `KERN_MEMLOCK_RANGE`.
pub const KERN_MEMLOCK_RANGE: i32 = 42;
/// Int: POSIX memory protections. C: `KERN_MEMORY_PROTECTION`.
pub const KERN_MEMORY_PROTECTION: i32 = 43;
/// Int: max length login name + NUL (unimplemented). C: `KERN_LOGIN_NAME_MAX`.
pub const KERN_LOGIN_NAME_MAX: i32 = 44;
/// Old: sort core name format. C: `KERN_DEFCORENAME`.
pub const KERN_DEFCORENAME: i32 = 45;
/// Int: log signaled processes (unimplemented). C: `KERN_LOGSIGEXIT`.
pub const KERN_LOGSIGEXIT: i32 = 46;
/// Struct: process entries. C: `KERN_PROC2`.
pub const KERN_PROC2: i32 = 47;
/// Struct: process argv/env. C: `KERN_PROC_ARGS`.
pub const KERN_PROC_ARGS: i32 = 48;
/// Int: fixpt FSCALE. C: `KERN_FSCALE`.
pub const KERN_FSCALE: i32 = 49;
/// Old: fixpt ccpu. C: `KERN_CCPU`.
pub const KERN_CCPU: i32 = 50;
/// Struct: CPU time counters. C: `KERN_CP_TIME`.
pub const KERN_CP_TIME: i32 = 51;
/// Old: number of valid kern ids. C: `KERN_OLDSYSVIPC_INFO`.
pub const KERN_OLDSYSVIPC_INFO: i32 = 52;
/// Kernel message buffer (unimplemented). C: `KERN_MSGBUF`.
pub const KERN_MSGBUF: i32 = 53;
/// Dev_t: console terminal device. C: `KERN_CONSDEV`.
pub const KERN_CONSDEV: i32 = 54;
/// Int: maximum number of ptys. C: `KERN_MAXPTYS`.
pub const KERN_MAXPTYS: i32 = 55;
/// Node: pipe limits (unimplemented). C: `KERN_PIPE`.
pub const KERN_PIPE: i32 = 56;
/// Int: kernel value of MAXPHYS. C: `KERN_MAXPHYS`.
pub const KERN_MAXPHYS: i32 = 57;
/// Int: max socket buffer size (unimplemented). C: `KERN_SBMAX`.
pub const KERN_SBMAX: i32 = 58;
/// Tty in/out counters (unimplemented). C: `KERN_TKSTAT`.
pub const KERN_TKSTAT: i32 = 59;
/// Int: POSIX monotonic clock. C: `KERN_MONOTONIC_CLOCK`.
pub const KERN_MONOTONIC_CLOCK: i32 = 60;
/// Int: random integer from urandom (unimplemented). C: `KERN_URND`.
pub const KERN_URND: i32 = 61;
/// Int: disklabel sector (unimplemented). C: `KERN_LABELSECTOR`.
pub const KERN_LABELSECTOR: i32 = 62;
/// Int: offset of label within sector (unimplemented). C: `KERN_LABELOFFSET`.
pub const KERN_LABELOFFSET: i32 = 63;
/// Struct: lwp entries. C: `KERN_LWP`.
pub const KERN_LWP: i32 = 64;
/// Int: sleep length on failed fork. C: `KERN_FORKFSLEEP`.
pub const KERN_FORKFSLEEP: i32 = 65;
/// Int: POSIX Threads option (unimplemented). C: `KERN_POSIX_THREADS`.
pub const KERN_POSIX_THREADS: i32 = 66;
/// Int: POSIX Semaphores option (unimplemented). C: `KERN_POSIX_SEMAPHORES`.
pub const KERN_POSIX_SEMAPHORES: i32 = 67;
/// Int: POSIX Barriers option (unimplemented). C: `KERN_POSIX_BARRIERS`.
pub const KERN_POSIX_BARRIERS: i32 = 68;
/// Int: POSIX Timers option (unimplemented). C: `KERN_POSIX_TIMERS`.
pub const KERN_POSIX_TIMERS: i32 = 69;
/// Int: POSIX Spin Locks option (unimplemented). C: `KERN_POSIX_SPIN_LOCKS`.
pub const KERN_POSIX_SPIN_LOCKS: i32 = 70;
/// Int: POSIX R/W Locks option (unimplemented). C: `KERN_POSIX_READER_WRITER_LOCKS`.
pub const KERN_POSIX_READER_WRITER_LOCKS: i32 = 71;
/// Int: dump on panic. C: `KERN_DUMP_ON_PANIC`.
pub const KERN_DUMP_ON_PANIC: i32 = 72;
/// Int: max socket kernel virtual mem (unimplemented). C: `KERN_SOMAXKVA`.
pub const KERN_SOMAXKVA: i32 = 73;
/// Int: root partition (incompatible). C: `KERN_ROOT_PARTITION`.
pub const KERN_ROOT_PARTITION: i32 = 74;
/// Struct: driver names and majors #s. C: `KERN_DRIVERS`.
pub const KERN_DRIVERS: i32 = 75;
/// Struct: buffers (unimplemented). C: `KERN_BUF`.
pub const KERN_BUF: i32 = 76;
/// Struct: file entries (unimplemented). C: `KERN_FILE2`.
pub const KERN_FILE2: i32 = 77;
/// Node: verified exec (unimplemented). C: `KERN_VERIEXEC`.
pub const KERN_VERIEXEC: i32 = 78;
/// Struct: cpu id numbers (unimplemented). C: `KERN_CP_ID`.
pub const KERN_CP_ID: i32 = 79;
/// Int: number of hardclock ticks. C: `KERN_HARDCLOCK_TICKS`.
pub const KERN_HARDCLOCK_TICKS: i32 = 80;
/// Void *buf, size_t siz random (unimplemented). C: `KERN_ARND`.
pub const KERN_ARND: i32 = 81;
/// Node: SysV IPC parameters. C: `KERN_SYSVIPC`.
pub const KERN_SYSVIPC: i32 = 82;
/// Struct: time kernel was booted. C: `KERN_BOOTTIME`.
pub const KERN_BOOTTIME: i32 = 83;
/// Struct: evcnts (unimplemented). C: `KERN_EVCNT`.
pub const KERN_EVCNT: i32 = 84;
/// Number of valid kern ids. C: `KERN_MAXID` — sysctl.h:278.
pub const KERN_MAXID: i32 = 85;

// ── KERN_SYSVIPC subtypes ──
//
// C: `KERN_SYSVIPC_*` — sys/sys/sysctl.h:686-693. Only INFO..SHM are
// tabled (13's mock); the rest travel for completeness.

/// Struct: number of valid kern ids. C: `KERN_SYSVIPC_INFO`.
pub const KERN_SYSVIPC_INFO: i32 = 1;
/// Int: SysV message queue support. C: `KERN_SYSVIPC_MSG`.
pub const KERN_SYSVIPC_MSG: i32 = 2;
/// Int: SysV semaphore support. C: `KERN_SYSVIPC_SEM`.
pub const KERN_SYSVIPC_SEM: i32 = 3;
/// Int: SysV shared memory support. C: `KERN_SYSVIPC_SHM`.
pub const KERN_SYSVIPC_SHM: i32 = 4;
/// Int: max shared memory segment size. C: `KERN_SYSVIPC_SHMMAX`.
pub const KERN_SYSVIPC_SHMMAX: i32 = 5;
/// Int: max number of shared memory identifiers. C: `KERN_SYSVIPC_SHMMNI`.
pub const KERN_SYSVIPC_SHMMNI: i32 = 6;
/// Int: max shared memory segments per process. C: `KERN_SYSVIPC_SHMSEG`.
pub const KERN_SYSVIPC_SHMSEG: i32 = 7;
/// Int: max amount of shared memory (pages). C: `KERN_SYSVIPC_SHMMAXPGS`.
pub const KERN_SYSVIPC_SHMMAXPGS: i32 = 8;

// ── CTL_VM identifiers ──
//
// C: `VM_*` — sys/uvm/uvm_param.h:165-180. MIB populates 4 of 13
// (loadavg/uvmexp2/maxslp/uspace); the rest are A-9 (14's list).

/// Struct vmmeter (unimplemented). C: `VM_METER`.
pub const VM_METER: i32 = 1;
/// Struct loadavg. C: `VM_LOADAVG`.
pub const VM_LOADAVG: i32 = 2;
/// Struct uvmexp (unimplemented). C: `VM_UVMEXP`.
pub const VM_UVMEXP: i32 = 3;
/// Kmem_map pages (unimplemented). C: `VM_NKMEMPAGES`.
pub const VM_NKMEMPAGES: i32 = 4;
/// Struct uvmexp_sysctl. C: `VM_UVMEXP2`.
pub const VM_UVMEXP2: i32 = 5;
/// Unimplemented. C: `VM_ANONMIN`.
pub const VM_ANONMIN: i32 = 6;
/// Unimplemented. C: `VM_EXECMIN`.
pub const VM_EXECMIN: i32 = 7;
/// Unimplemented. C: `VM_FILEMIN`.
pub const VM_FILEMIN: i32 = 8;
/// Max sleep before swap. C: `VM_MAXSLP`.
pub const VM_MAXSLP: i32 = 9;
/// Kernel-stack bytes (always 0 on MINIX3). C: `VM_USPACE`.
pub const VM_USPACE: i32 = 10;
/// Unimplemented. C: `VM_ANONMAX`.
pub const VM_ANONMAX: i32 = 11;
/// Unimplemented. C: `VM_EXECMAX`.
pub const VM_EXECMAX: i32 = 12;
/// Unimplemented. C: `VM_FILEMAX`.
pub const VM_FILEMAX: i32 = 13;
/// Min address (NetBSD-only). C: `VM_MINADDRESS`.
pub const VM_MINADDRESS: i32 = 14;
/// Max address (NetBSD-only). C: `VM_MAXADDRESS`.
pub const VM_MAXADDRESS: i32 = 15;
/// Process information (NetBSD-only). C: `VM_PROC`.
pub const VM_PROC: i32 = 16;

// ── CTL_HW identifiers ──
//
// C: `HW_*` — sys/sys/sysctl.h:893-909. MIB populates 10 of 16;
// the rest are A-9 (14's list).

/// String: machine class. C: `HW_MACHINE`.
pub const HW_MACHINE: i32 = 1;
/// String: specific machine model (unimplemented). C: `HW_MODEL`.
pub const HW_MODEL: i32 = 2;
/// Int: number of cpus. C: `HW_NCPU`.
pub const HW_NCPU: i32 = 3;
/// Int: machine byte order. C: `HW_BYTEORDER`.
pub const HW_BYTEORDER: i32 = 4;
/// Int: total memory (bytes). C: `HW_PHYSMEM`.
pub const HW_PHYSMEM: i32 = 5;
/// Int: non-kernel memory (bytes). C: `HW_USERMEM`.
pub const HW_USERMEM: i32 = 6;
/// Int: software page size. C: `HW_PAGESIZE`.
pub const HW_PAGESIZE: i32 = 7;
/// String: disk drive names (unimplemented). C: `HW_DISKNAMES`.
pub const HW_DISKNAMES: i32 = 8;
/// Struct: iostats[] (unimplemented). C: `HW_IOSTATS`.
pub const HW_IOSTATS: i32 = 9;
/// String: machine architecture. C: `HW_MACHINE_ARCH`.
pub const HW_MACHINE_ARCH: i32 = 10;
/// Int: ALIGNBYTES (unimplemented). C: `HW_ALIGNBYTES`.
pub const HW_ALIGNBYTES: i32 = 11;
/// String: console magic (unimplemented). C: `HW_CNMAGIC`.
pub const HW_CNMAGIC: i32 = 12;
/// Quad: total memory (bytes). C: `HW_PHYSMEM64`.
pub const HW_PHYSMEM64: i32 = 13;
/// Quad: non-kernel memory (bytes). C: `HW_USERMEM64`.
pub const HW_USERMEM64: i32 = 14;
/// String: iostat names (unimplemented). C: `HW_IOSTATNAMES`.
pub const HW_IOSTATNAMES: i32 = 15;
/// Number of valid hw ids. C: `HW_MAXID` — sysctl.h:908.
pub const HW_MAXID: i32 = 15;
/// Number CPUs online. C: `HW_NCPUONLINE` — sysctl.h:909 (past MAXID).
pub const HW_NCPUONLINE: i32 = 16;

// ── LWP states and flags (kinfo wire ABI) ──
//
// C: `LS*` — sys/lwp.h:279-285; `L_*` — sys/sysctl.h:607-617 (`P_*`
// aliases share the values for proc2). MIB reports these to ps/top;
// 17 fills them, 18 reuses the flag word.

// NOTE: `LSIDL` (1) never leaves MIB (idle is reported per-CPU, not per
// process); `LSONPROC` (7) never leaves either (runnable is `LSRUN`).

/// Runnable, not yet running. C: `LSRUN 2` — lwp.h:279.
pub const LSRUN: i32 = 2;
/// Sleeping on an address. C: `LSSLEEP 3` — lwp.h:280.
pub const LSSLEEP: i32 = 3;
/// Debugging/suspension stop. C: `LSSTOP 4` — lwp.h:281.
pub const LSSTOP: i32 = 4;
/// Awaiting collection. C: `LSZOMB 5` — lwp.h:282.
pub const LSZOMB: i32 = 5;
/// Almost a zombie. C: `LSDEAD 6` — lwp.h:284.
pub const LSDEAD: i32 = 6;
/// On a CPU (never reported by MIB). C: `LSONPROC 7` — lwp.h:285.
pub const LSONPROC: i32 = 7;

/// In memory. C: `L_INMEM 0x4` — sysctl.h:607.
pub const L_INMEM: u32 = 0x0000_0004;
/// Interruptible sleep. C: `L_SINTR 0x80` — sysctl.h:614.
pub const L_SINTR: u32 = 0x0000_0080;
/// Kernel task. C: `L_SYSTEM 0x200` — sysctl.h:617.
pub const L_SYSTEM: u32 = 0x0000_0200;

// ── KERN_PROC2 filters and process flags (18) ──
//
// C: `KERN_PROC_*` — sys/sys/sysctl.h:383-391; `EPROC_*` — :494-495;
// `P_*` — :606-621; `S*` — sys/sys/proc.h:341-345; `KI_NGROUPS` — :464;
// `NZERO` — sys/sys/syslimits.h:93.

/// Everything. C: `KERN_PROC_ALL 0` — sysctl.h:383.
pub const KERN_PROC_ALL: i32 = 0;
/// By process id. C: `KERN_PROC_PID 1` — sysctl.h:384.
pub const KERN_PROC_PID: i32 = 1;
/// By process group id. C: `KERN_PROC_PGRP 2` — sysctl.h:385.
pub const KERN_PROC_PGRP: i32 = 2;
/// By session of pid. C: `KERN_PROC_SESSION 3` — sysctl.h:386.
pub const KERN_PROC_SESSION: i32 = 3;
/// By controlling tty. C: `KERN_PROC_TTY 4` — sysctl.h:387.
pub const KERN_PROC_TTY: i32 = 4;
/// By effective uid. C: `KERN_PROC_UID 5` — sysctl.h:388.
pub const KERN_PROC_UID: i32 = 5;
/// By real uid. C: `KERN_PROC_RUID 6` — sysctl.h:389.
pub const KERN_PROC_RUID: i32 = 6;
/// By effective gid. C: `KERN_PROC_GID 7` — sysctl.h:390.
pub const KERN_PROC_GID: i32 = 7;
/// By real gid. C: `KERN_PROC_RGID 8` — sysctl.h:391.
pub const KERN_PROC_RGID: i32 = 8;

/// Controlling terminal active. C: `EPROC_CTTY 0x01` — sysctl.h:494.
pub const EPROC_CTTY: u32 = 0x01;
/// Session leader. C: `EPROC_SLEADER 0x02` — sysctl.h:495.
pub const EPROC_SLEADER: u32 = 0x02;

/// In memory (process view, same value as `L_INMEM`). C: `P_INMEM` — sysctl.h:608.
pub const P_INMEM: u32 = 0x0000_0004;
/// Kernel task (process view, same value as `L_SYSTEM`). C: `P_SYSTEM` — sysctl.h:618.
pub const P_SYSTEM: u32 = 0x0000_0200;
/// Interruptible sleep (process view, same value as `L_SINTR`). C: `P_SINTR` — sysctl.h:615.
pub const P_SINTR: u32 = 0x0000_0080;
/// Set-uid/set-gid history. C: `P_SUGID 0x100` — sysctl.h:616.
pub const P_SUGID: u32 = 0x0000_0100;
/// Traced by a debugger. C: `P_TRACED 0x800` — sysctl.h:621.
pub const P_TRACED: u32 = 0x0000_0800;
/// Has a controlling terminal. C: `P_CONTROLT 0x02` — sysctl.h:606.
pub const P_CONTROLT: u32 = 0x0000_0002;

/// Running, not stopped. C: `SACTIVE 2` — sys/proc.h:341.
pub const SACTIVE: i32 = 2;
/// Stopped for debugging. C: `SSTOP 4` — sys/proc.h:343.
pub const SSTOP: i32 = 4;
/// Awaiting collection. C: `SZOMB 5` — sys/proc.h:344.
pub const SZOMB: i32 = 5;
/// Almost a zombie. C: `SDEAD 6` — sys/proc.h:345.
pub const SDEAD: i32 = 6;

/// Attachable group slots per process. C: `KI_NGROUPS 16` — sysctl.h:464.
pub const KI_NGROUPS: usize = 16;

/// Default niceness offset. C: `NZERO 20` — syslimits.h:93.
pub const NZERO: i32 = 20;

// ── Node types ──
//
// C: `CTLTYPE_*` — sys/sys/sysctl.h:92-103.

/// Name is a node. C: `CTLTYPE_NODE`.
pub const CTLTYPE_NODE: u32 = 1;
/// Name describes an integer. C: `CTLTYPE_INT`.
pub const CTLTYPE_INT: u32 = 2;
/// Name describes a string. C: `CTLTYPE_STRING`.
pub const CTLTYPE_STRING: u32 = 3;
/// Name describes a 64-bit number. C: `CTLTYPE_QUAD`.
pub const CTLTYPE_QUAD: u32 = 4;
/// Name describes a structure. C: `CTLTYPE_STRUCT`.
pub const CTLTYPE_STRUCT: u32 = 5;
/// Name describes a bool. C: `CTLTYPE_BOOL`.
pub const CTLTYPE_BOOL: u32 = 6;
/// Native long: quad on 64-bit (minix-rs is 64-bit-only).
/// C: `CTLTYPE_LONG` (`CTLTYPE_QUAD` under `_LP64`) — sysctl.h:99-103.
pub const CTLTYPE_LONG: u32 = CTLTYPE_QUAD;

// ── Node flags (NetBSD wire set) ──
//
// C: `CTLFLAG_*` — sys/sys/sysctl.h:108-125. `ROOT`/`ALIAS`/`MMAP` keep
// their NetBSD names here because they travel on the wire inside
// `sysctl_flags`; MIB reuses the *bits* internally as
// `PARENT`/`VERIFY`/`REMOTE` (mib.h:72-74) and never exposes those —
// see `tree::flag` (03) for the reassignment.

// NOTE: `CTLFLAG_READONLY` is 0x0 — "no write bits", not a bit.

/// Read-only (zero: absence of write bits). C: `CTLFLAG_READONLY`.
pub const CTLFLAG_READONLY: u32 = 0x0000_0000;
/// Read-write. C: `CTLFLAG_READWRITE` — sysctl.h:112.
pub const CTLFLAG_READWRITE: u32 = 0x0000_0070;
/// Any user may write. C: `CTLFLAG_ANYWRITE` — sysctl.h:113.
pub const CTLFLAG_ANYWRITE: u32 = 0x0000_0080;
/// Superuser-only. C: `CTLFLAG_PRIVATE` — sysctl.h:114.
pub const CTLFLAG_PRIVATE: u32 = 0x0000_0100;
/// Cannot be destroyed. C: `CTLFLAG_PERMANENT` — sysctl.h:115.
pub const CTLFLAG_PERMANENT: u32 = 0x0000_0200;
/// Node owns its data buffer. C: `CTLFLAG_OWNDATA` — sysctl.h:116.
pub const CTLFLAG_OWNDATA: u32 = 0x0000_0400;
/// Value stored inline in the node. C: `CTLFLAG_IMMEDIATE` — sysctl.h:117.
pub const CTLFLAG_IMMEDIATE: u32 = 0x0000_0800;
/// Display as hex. C: `CTLFLAG_HEX` — sysctl.h:118.
pub const CTLFLAG_HEX: u32 = 0x0000_1000;
/// NetBSD tree root (reused as `PARENT` in 03). C: `CTLFLAG_ROOT` — sysctl.h:119.
pub const CTLFLAG_ROOT: u32 = 0x0000_2000;
/// Users may invent children below. C: `CTLFLAG_ANYNUMBER` — sysctl.h:120.
pub const CTLFLAG_ANYNUMBER: u32 = 0x0000_4000;
/// Hidden from enumeration. C: `CTLFLAG_HIDDEN` — sysctl.h:121.
pub const CTLFLAG_HIDDEN: u32 = 0x0000_8000;
/// NetBSD alias (reused as `VERIFY` in 03). C: `CTLFLAG_ALIAS` — sysctl.h:122.
pub const CTLFLAG_ALIAS: u32 = 0x0001_0000;
/// NetBSD mmap-future (reused as `REMOTE` in 03). C: `CTLFLAG_MMAP` — sysctl.h:123.
pub const CTLFLAG_MMAP: u32 = 0x0002_0000;
/// Node owns its description. C: `CTLFLAG_OWNDESC` — sysctl.h:124.
pub const CTLFLAG_OWNDESC: u32 = 0x0004_0000;
/// Integer is unsigned. C: `CTLFLAG_UNSIGNED` — sysctl.h:125.
pub const CTLFLAG_UNSIGNED: u32 = 0x0008_0000;

/// Flags a create request may set. C: `SYSCTL_USERFLAGS` — sysctl.h:139-145.
pub const SYSCTL_USERFLAGS: u32 = CTLFLAG_READWRITE
    | CTLFLAG_ANYWRITE
    | CTLFLAG_PRIVATE
    | CTLFLAG_OWNDATA
    | CTLFLAG_IMMEDIATE
    | CTLFLAG_HEX
    | CTLFLAG_HIDDEN;

/// Type mask / accessor. C: `SYSCTL_TYPEMASK`/`SYSCTL_TYPE(x)` — sysctl.h:150-151.
pub const SYSCTL_TYPEMASK: u32 = 0x0000_000f;
/// Extract the node type from a flags word. C: `SYSCTL_TYPE(x)`.
#[inline(always)]
pub const fn sysctl_type(flags: u32) -> u32 {
    flags & SYSCTL_TYPEMASK
}

/// Flag mask / accessor. C: `SYSCTL_FLAGMASK`/`SYSCTL_FLAGS(x)` — sysctl.h:152-153.
pub const SYSCTL_FLAGMASK: u32 = 0x00ff_fff0;
/// Extract the flag bits from a flags word. C: `SYSCTL_FLAGS(x)`.
#[inline(always)]
pub const fn sysctl_flags(flags: u32) -> u32 {
    flags & SYSCTL_FLAGMASK
}

// ── API version ──
//
// C: `SYSCTL_VERS_*` — sys/sys/sysctl.h:130-134. The tree speaks VERS_1;
// mib.h:331-333 refuses to compile against anything else.

/// Version mask. C: `SYSCTL_VERS_MASK`.
pub const SYSCTL_VERS_MASK: u32 = 0xff00_0000;
/// Legacy version 0. C: `SYSCTL_VERS_0`.
pub const SYSCTL_VERS_0: u32 = 0x0000_0000;
/// Current version. C: `SYSCTL_VERS_1`.
pub const SYSCTL_VERS_1: u32 = 0x0100_0000;
/// Pinned API version. C: `SYSCTL_VERSION` (= VERS_1).
pub const SYSCTL_VERSION: u32 = SYSCTL_VERS_1;
/// Extract the version from a flags word. C: `SYSCTL_VERS(f)`.
#[inline(always)]
pub const fn sysctl_vers(flags: u32) -> u32 {
    flags & SYSCTL_VERS_MASK
}

// ── Meta-identifiers (negative path components) ──
//
// C: `CTL_*` — sys/sys/sysctl.h:158-164. Negative ids can never collide
// with real child ids (children are ≥ 0... in practice ≥ 1), so they
// double as in-band operation codes during dispatch (10).

/// End of a create/destroy vector. C: `CTL_EOL` (-1).
pub const CTL_EOL: i32 = -1;
/// Enumerate children. C: `CTL_QUERY` (-2).
pub const CTL_QUERY: i32 = -2;
/// Create a node. C: `CTL_CREATE` (-3).
pub const CTL_CREATE: i32 = -3;
/// Create with symbol (unimplemented → EOPNOTSUPP). C: `CTL_CREATESYM` (-4).
pub const CTL_CREATESYM: i32 = -4;
/// Destroy a node. C: `CTL_DESTROY` (-5).
pub const CTL_DESTROY: i32 = -5;
/// Mmap node data (unimplemented → EOPNOTSUPP). C: `CTL_MMAP` (-6).
pub const CTL_MMAP: i32 = -6;
/// Fetch node descriptions. C: `CTL_DESCRIBE` (-7).
pub const CTL_DESCRIBE: i32 = -7;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_top_level_ids() {
        // C: sys/sys/sysctl.h:169-183.
        assert_eq!(CTL_UNSPEC, 0);
        assert_eq!(CTL_KERN, 1);
        assert_eq!(CTL_VM, 2);
        assert_eq!(CTL_VFS, 3);
        assert_eq!(CTL_NET, 4);
        assert_eq!(CTL_DEBUG, 5);
        assert_eq!(CTL_HW, 6);
        assert_eq!(CTL_MACHDEP, 7);
        assert_eq!(CTL_USER, 8);
        assert_eq!(CTL_DDB, 9);
        assert_eq!(CTL_PROC, 10);
        assert_eq!(CTL_VENDOR, 11);
        assert_eq!(CTL_EMUL, 12);
        assert_eq!(CTL_SECURITY, 13);
        assert_eq!(CTL_MAXID, 14);
        // MINIX3 extension parks past NetBSD's range (minix/sysctl.h:17).
        assert_eq!(CTL_MINIX, 32);
        assert!(CTL_MAXID <= CTL_MINIX);
    }

    #[test]
    fn test_minix_subtree_ids() {
        // C: minix/sysctl.h:28-59.
        assert_eq!(
            (MINIX_TEST, MINIX_MIB, MINIX_PROC, MINIX_LWIP),
            (0, 1, 2, 3)
        );
        assert_eq!(
            (TEST_INT, TEST_BOOL, TEST_QUAD, TEST_STRING, TEST_STRUCT),
            (0, 1, 2, 3, 4)
        );
        assert_eq!(
            (TEST_PRIVATE, TEST_ANYWRITE, TEST_DYNAMIC, TEST_SECRET),
            (5, 6, 7, 8)
        );
        assert_eq!((TEST_PERM, TEST_DESTROY1, TEST_DESTROY2), (9, 10, 11));
        assert_eq!(SECRET_VALUE, 0);
        assert_eq!((MIB_NODES, MIB_OBJECTS, MIB_REMOTES), (1, 2, 3));
        assert_eq!((PROC_LIST, PROC_DATA), (1, 2));
        assert_eq!(SYSCTL_NODE_FN, 0x1);
    }

    #[test]
    fn test_kern_ids() {
        // C: sys/sys/sysctl.h:194-278. Spot-check the seams; the table
        // in 13 pins every populated slot individually.
        assert_eq!(
            (
                KERN_OSTYPE,
                KERN_CLOCKRATE,
                KERN_PROC2,
                KERN_PROC_ARGS,
                KERN_LWP
            ),
            (1, 12, 47, 48, 64)
        );
        assert_eq!(
            (
                KERN_FORKFSLEEP,
                KERN_DRIVERS,
                KERN_HARDCLOCK_TICKS,
                KERN_SYSVIPC,
                KERN_BOOTTIME
            ),
            (65, 75, 80, 82, 83)
        );
        assert_eq!(KERN_MAXID, 85);
        assert_eq!(
            (
                KERN_SYSVIPC_INFO,
                KERN_SYSVIPC_MSG,
                KERN_SYSVIPC_SEM,
                KERN_SYSVIPC_SHM
            ),
            (1, 2, 3, 4)
        );
    }

    #[test]
    fn test_vm_hw_ids() {
        // C: uvm_param.h:165-180 + sysctl.h:893-909. 14 pins each populated
        // slot individually; here the seams.
        assert_eq!((VM_METER, VM_LOADAVG, VM_UVMEXP, VM_UVMEXP2), (1, 2, 3, 5));
        assert_eq!((VM_MAXSLP, VM_USPACE, VM_FILEMAX), (9, 10, 13));
        assert_eq!(
            (
                HW_MACHINE,
                HW_NCPU,
                HW_BYTEORDER,
                HW_PHYSMEM,
                HW_USERMEM,
                HW_PAGESIZE
            ),
            (1, 3, 4, 5, 6, 7)
        );
        assert_eq!((HW_MACHINE_ARCH, HW_PHYSMEM64, HW_USERMEM64), (10, 13, 14));
        assert_eq!((HW_MAXID, HW_NCPUONLINE), (15, 16));
    }

    #[test]
    fn test_proc2_ids() {
        // C: sys/sys/sysctl.h:383-391 (filters), :494-495 (EPROC_*),
        // :606-621 (P_*), sys/sys/proc.h:341-345 (S*), :464 (KI_NGROUPS),
        // sys/sys/syslimits.h:93 (NZERO). 18 pins the behavior.
        assert_eq!(
            (
                KERN_PROC_ALL,
                KERN_PROC_PID,
                KERN_PROC_PGRP,
                KERN_PROC_SESSION,
                KERN_PROC_TTY
            ),
            (0, 1, 2, 3, 4)
        );
        assert_eq!(
            (KERN_PROC_UID, KERN_PROC_RUID, KERN_PROC_GID, KERN_PROC_RGID),
            (5, 6, 7, 8)
        );
        assert_eq!((EPROC_CTTY, EPROC_SLEADER), (0x01, 0x02));
        assert_eq!((P_INMEM, P_SYSTEM, P_SINTR), (0x04, 0x200, 0x80));
        assert_eq!((P_SUGID, P_TRACED, P_CONTROLT), (0x100, 0x800, 0x02));
        assert_eq!((SACTIVE, SSTOP, SZOMB, SDEAD), (2, 4, 5, 6));
        assert_eq!((KI_NGROUPS, NZERO), (16, 20));
    }

    #[test]
    fn test_proc_args_proc_ids() {
        // C: sys/sys/sysctl.h:677-680 (requests), minix/sysctl.h:61-88
        // (flags and layouts). 19 pins the walk, 20 pins the rows.
        assert_eq!(
            (
                KERN_PROC_ARGV,
                KERN_PROC_NARGV,
                KERN_PROC_ENV,
                KERN_PROC_NENV
            ),
            (1, 2, 3, 4)
        );
        assert_eq!((MPLF_IN_USE, MPLF_ZOMBIE), (0x01, 0x02));
        assert_eq!(
            (MPDF_SYSTEM, MPDF_ZOMBIE, MPDF_RUNNABLE, MPDF_STOPPED),
            (0x01, 0x02, 0x04, 0x08)
        );
        // Layouts ProcFS reads field by field (minix/sysctl.h:62-84).
        assert_eq!(core::mem::size_of::<MinixProcList>(), 16);
        assert_eq!(core::mem::size_of::<MinixProcData>(), 72);
    }

    #[test]
    fn test_types_versions_limits() {
        // C: sys/sys/sysctl.h:75-79,92-103,130-134.
        assert_eq!(CTL_MAXNAME, 12);
        assert_eq!(SYSCTL_NAMELEN, 32);
        assert_eq!(CTL_SHORTNAME, 8);
        assert_eq!(CREATE_BASE, 1024);
        assert_eq!(SYSCTL_DEFSIZE, 8);
        assert_eq!(
            (
                CTLTYPE_NODE,
                CTLTYPE_INT,
                CTLTYPE_STRING,
                CTLTYPE_QUAD,
                CTLTYPE_STRUCT,
                CTLTYPE_BOOL
            ),
            (1, 2, 3, 4, 5, 6)
        );
        assert_eq!(CTLTYPE_LONG, CTLTYPE_QUAD);
        assert_eq!(SYSCTL_VERSION, SYSCTL_VERS_1);
        assert_eq!(sysctl_vers(0x0100_0072), SYSCTL_VERS_1);
        assert_eq!(sysctl_type(0x0100_0072), CTLTYPE_INT);
        assert_eq!(sysctl_flags(0x0100_0072), 0x0000_0070);
    }

    #[test]
    fn test_flag_bits_and_userflags() {
        // C: sys/sys/sysctl.h:108-125,139-145.
        assert_eq!(CTLFLAG_READONLY, 0x0);
        assert_eq!(CTLFLAG_READWRITE, 0x70);
        assert_eq!(CTLFLAG_ANYWRITE, 0x80);
        assert_eq!(CTLFLAG_PRIVATE, 0x100);
        assert_eq!(CTLFLAG_PERMANENT, 0x200);
        assert_eq!(CTLFLAG_OWNDATA, 0x400);
        assert_eq!(CTLFLAG_IMMEDIATE, 0x800);
        assert_eq!(CTLFLAG_HEX, 0x1000);
        assert_eq!(CTLFLAG_ROOT, 0x2000);
        assert_eq!(CTLFLAG_ANYNUMBER, 0x4000);
        assert_eq!(CTLFLAG_HIDDEN, 0x8000);
        assert_eq!(CTLFLAG_ALIAS, 0x1_0000);
        assert_eq!(CTLFLAG_MMAP, 0x2_0000);
        assert_eq!(CTLFLAG_OWNDESC, 0x4_0000);
        assert_eq!(CTLFLAG_UNSIGNED, 0x8_0000);
        assert_eq!(
            SYSCTL_USERFLAGS,
            CTLFLAG_READWRITE
                | CTLFLAG_ANYWRITE
                | CTLFLAG_PRIVATE
                | CTLFLAG_OWNDATA
                | CTLFLAG_IMMEDIATE
                | CTLFLAG_HEX
                | CTLFLAG_HIDDEN
        );
        // Reassignment bits are disjoint from the type nibble (03's matrix).
        assert_eq!(
            SYSCTL_TYPEMASK & (CTLFLAG_ROOT | CTLFLAG_ALIAS | CTLFLAG_MMAP),
            0
        );
    }

    #[test]
    fn test_meta_identifiers() {
        // C: sys/sys/sysctl.h:158-164. All negative, all distinct.
        assert_eq!(
            (
                CTL_EOL,
                CTL_QUERY,
                CTL_CREATE,
                CTL_CREATESYM,
                CTL_DESTROY,
                CTL_MMAP,
                CTL_DESCRIBE
            ),
            (-1, -2, -3, -4, -5, -6, -7)
        );
    }
}
