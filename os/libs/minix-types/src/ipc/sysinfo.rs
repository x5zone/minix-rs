//! System-information request constants.
//!
//! Covers the three IS-adjacent wire vocabularies that had no home yet:
//! `SYS_GETINFO` sub-requests (`GET_*` — `minix3/minix/include/minix/com.h:
//! 315-345`), cross-service table requests (`SI_*` — `minix3/minix/include/
//! minix/sysinfo.h:11-17`), and `SYS_DIAGCTL` codes (`DIAGCTL_CODE_*` —
//! com.h:412-415). Companion call numbers (`SYS_GETINFO`, `SYS_DIAGCTL`,
//! `PM_GETSYSINFO`, `VFS_GETSYSINFO`) are included; the RS/DS/VM siblings
//! already live in their own `ipc` modules (`RS_GETSYSINFO`,
//! `DS_GETSYSINFO`, `VM_INFO`/`VMIW_*`) and are not duplicated here.
//!
//! Authority (§2.4g): single definition in minix-rs. `04-is-data-
//! acquisition.md` §2 is the doc counterpart; 05~10 import from here.

/// Kernel call number for sys_getinfo.
/// C: `SYS_GETINFO (KERNEL_CALL + 26)` — com.h:236 (`KERNEL_CALL 0x600`, :205).
pub const SYS_GETINFO: i32 = 0x600 + 26;

/// Kernel call number for sys_diagctl.
/// C: `SYS_DIAGCTL (KERNEL_CALL + 44)` — com.h:252.
pub const SYS_DIAGCTL: i32 = 0x600 + 44;

/// PM table request. C: `PM_GETSYSINFO (PM_BASE + 47)` — callnr.h:60.
pub const PM_GETSYSINFO: i32 = 47;

/// VFS table request. C: `VFS_GETSYSINFO (VFS_BASE + 48)` — callnr.h:120.
pub const VFS_GETSYSINFO: i32 = 0x100 + 48;

// SYS_GETINFO sub-requests. C: `minix3/minix/include/minix/com.h:315-345`.
// Numbers 7 and 22 are unassigned upstream (gaps, not omissions).
/// C: `GET_KINFO 0` — com.h:316.
pub const GET_KINFO: i32 = 0;
/// C: `GET_IMAGE 1` — com.h:317.
pub const GET_IMAGE: i32 = 1;
/// C: `GET_PROCTAB 2` — com.h:318.
pub const GET_PROCTAB: i32 = 2;
/// C: `GET_RANDOMNESS 3` — com.h:319.
pub const GET_RANDOMNESS: i32 = 3;
/// C: `GET_MONPARAMS 4` — com.h:320.
pub const GET_MONPARAMS: i32 = 4;
/// C: `GET_KENV 5` — com.h:321.
pub const GET_KENV: i32 = 5;
/// C: `GET_IRQHOOKS 6` — com.h:322.
pub const GET_IRQHOOKS: i32 = 6;
// 7: unassigned upstream.
/// C: `GET_PRIVTAB 8` — com.h:323.
pub const GET_PRIVTAB: i32 = 8;
/// C: `GET_KADDRESSES 9` — com.h:324.
pub const GET_KADDRESSES: i32 = 9;
/// C: `GET_SCHEDINFO 10` — com.h:325.
pub const GET_SCHEDINFO: i32 = 10;
/// C: `GET_PROC 11` — com.h:326.
pub const GET_PROC: i32 = 11;
/// C: `GET_MACHINE 12` — com.h:327.
pub const GET_MACHINE: i32 = 12;
/// C: `GET_LOCKTIMING 13` — com.h:328.
pub const GET_LOCKTIMING: i32 = 13;
/// C: `GET_BIOSBUFFER 14` — com.h:329.
pub const GET_BIOSBUFFER: i32 = 14;
/// C: `GET_LOADINFO 15` — com.h:330.
pub const GET_LOADINFO: i32 = 15;
/// C: `GET_IRQACTIDS 16` — com.h:331.
pub const GET_IRQACTIDS: i32 = 16;
/// C: `GET_PRIV 17` — com.h:332.
pub const GET_PRIV: i32 = 17;
/// C: `GET_HZ 18` — com.h:333.
pub const GET_HZ: i32 = 18;
/// C: `GET_WHOAMI 19` — com.h:334.
pub const GET_WHOAMI: i32 = 19;
/// C: `GET_RANDOMNESS_BIN 20` — com.h:335.
pub const GET_RANDOMNESS_BIN: i32 = 20;
/// C: `GET_IDLETSC 21` — com.h:336.
pub const GET_IDLETSC: i32 = 21;
// 22: unassigned upstream.
/// C: `GET_CPUINFO 23` — com.h:337.
pub const GET_CPUINFO: i32 = 23;
/// C: `GET_REGS 24` — com.h:338.
pub const GET_REGS: i32 = 24;
/// C: `GET_CPUTICKS 25` — com.h:339.
pub const GET_CPUTICKS: i32 = 25;

// Cross-service table requests. C: `minix3/minix/include/minix/sysinfo.h:11-17`.
/// C: `SI_PROC_TAB 2` — sysinfo.h:12.
pub const SI_PROC_TAB: i32 = 2;
/// C: `SI_DMAP_TAB 3` — sysinfo.h:13.
pub const SI_DMAP_TAB: i32 = 3;
/// C: `SI_DATA_STORE 5` — sysinfo.h:14.
pub const SI_DATA_STORE: i32 = 5;
/// C: `SI_CALL_STATS 9` — sysinfo.h:15.
pub const SI_CALL_STATS: i32 = 9;
/// C: `SI_PROCPUB_TAB 11` — sysinfo.h:16.
pub const SI_PROCPUB_TAB: i32 = 11;
/// C: `SI_PROCALL_TAB 12` — sysinfo.h:16 (`SI_PROCALL_TAB` shares the line).
pub const SI_PROCALL_TAB: i32 = 12;
/// C: `SI_PROCLIGHT_TAB 13` — sysinfo.h:17.
pub const SI_PROCLIGHT_TAB: i32 = 13;

// SYS_DIAGCTL codes. C: `minix3/minix/include/minix/com.h:412-415`.
/// C: `DIAGCTL_CODE_DIAG 1` — com.h:412.
pub const DIAGCTL_CODE_DIAG: i32 = 1;
/// C: `DIAGCTL_CODE_STACKTRACE 2` — com.h:413.
pub const DIAGCTL_CODE_STACKTRACE: i32 = 2;
/// C: `DIAGCTL_CODE_REGISTER 3` — com.h:414.
pub const DIAGCTL_CODE_REGISTER: i32 = 3;
/// C: `DIAGCTL_CODE_UNREGISTER 4` — com.h:415.
pub const DIAGCTL_CODE_UNREGISTER: i32 = 4;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_table_values_and_gaps() {
        // C: com.h:316-339. 7 and 22 unassigned (no constants by design).
        assert_eq!(
            (GET_KINFO, GET_IMAGE, GET_PROCTAB, GET_MONPARAMS, GET_KENV),
            (0, 1, 2, 4, 5)
        );
        assert_eq!(
            (GET_IRQHOOKS, GET_PRIVTAB, GET_MACHINE, GET_IRQACTIDS),
            (6, 8, 12, 16)
        );
        assert_eq!((GET_CPUINFO, GET_REGS, GET_CPUTICKS), (23, 24, 25));
        assert_eq!((SYS_GETINFO, SYS_DIAGCTL), (0x61A, 0x62C));
    }

    #[test]
    fn test_si_values() {
        // C: sysinfo.h:11-17.
        assert_eq!(
            (SI_PROC_TAB, SI_DMAP_TAB, SI_DATA_STORE, SI_CALL_STATS),
            (2, 3, 5, 9)
        );
        assert_eq!(
            (SI_PROCPUB_TAB, SI_PROCALL_TAB, SI_PROCLIGHT_TAB),
            (11, 12, 13)
        );
    }

    #[test]
    fn test_diagctl_and_getsysinfo_calls() {
        // C: com.h:412-415; callnr.h:60,120.
        assert_eq!(
            (
                DIAGCTL_CODE_DIAG,
                DIAGCTL_CODE_STACKTRACE,
                DIAGCTL_CODE_REGISTER,
                DIAGCTL_CODE_UNREGISTER
            ),
            (1, 2, 3, 4)
        );
        assert_eq!(PM_GETSYSINFO, 47);
        assert_eq!(VFS_GETSYSINFO, 0x130);
    }
}
