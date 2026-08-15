//! Boot image tables (ARCH A-13).
//!
//! Mirrors `minix3/minix/servers/rs/table.c` — the three static boot tables
//! consumed by the 4-step boot in `sef_cb_init_fresh` (main.c:158-494).
//!
//! # C ↔ Rust mapping
//!
//! | C (table.c) | Rust | Difference |
//! |-------------|------|------------|
//! | `boot_image_priv_table[]` (table.c:15-30) | [`BOOT_IMAGE_PRIV_TABLE`] | Sentinel `NULL_BOOT_NR` entry removed; slice end is the terminator |
//! | `boot_image_sys_table[]` (table.c:33-42) | [`BOOT_IMAGE_SYS_TABLE`] | `DEFAULT_BOOT_NR` entry becomes [`DEFAULT_SYS`] |
//! | `boot_image_dev_table[]` (table.c:45-50) | [`BOOT_IMAGE_DEV_TABLE`] | `DEFAULT_BOOT_NR` entry becomes [`DEFAULT_DEV`] |
//!
//! C uses magic sentinel values (`NULL_BOOT_NR`/`DEFAULT_BOOT_NR`, const.h:61-62)
//! to terminate or mark default entries. Rust expresses the same semantics with
//! slice length (priv table) and fallback values (sys/dev tables), so invalid
//! states are unrepresentable. The sentinel constants are still exported for
//! numeric alignment with the C protocol (99-rs-global-concepts.md).

use minix_types::Endpoint;

/// Marks a null boot entry. C: `NULL_BOOT_NR` — const.h:61.
pub const NULL_BOOT_NR: i32 = 17; // NR_BOOT_PROCS (minix-types boot.rs)

/// Marks the default boot entry. C: `DEFAULT_BOOT_NR` — const.h:62.
pub const DEFAULT_BOOT_NR: i32 = 17; // NR_BOOT_PROCS

/// Flags of a boot image priv entry.
///
/// C: `boot_image_priv.flags` (table.c) — values from
/// `minix3/minix/include/minix/priv.h:45-49` (`RSYS_F`/`VM_F`/`SRV_F`/`USR_F`).
/// Full priv-structure semantics: 03-rs-privilege.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootImagePriv {
    pub endpoint: Endpoint,
    /// Service label (e.g. `"rs"`, `"vm"`). C: `boot_image_priv.label` (table.c).
    pub label: &'static str,
    /// Priv flags (`SYS_PROC`/`ROOT_SYS_PROC`/...). C: `boot_image_priv.flags`.
    pub flags: u32,
}

/// Flags of a boot image sys entry.
///
/// C: `boot_image_sys.flags` — `SF_*` values (`minix3/minix/include/minix/rs.h:191-206`).
/// Full `SF_*` table: 99-rs-global-concepts.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootImageSys {
    pub endpoint: Endpoint,
    pub flags: u32,
}

/// Device properties of a boot image dev entry.
///
/// C: `boot_image_dev.dev_nr` — major device number (table.c:45-50).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootImageDev {
    pub endpoint: Endpoint,
    pub dev_nr: u32,
}

// ── Priv flags (priv.h:45-49) ──────────────────────────────────────────────

/// System services have their own priv structure. C: `SYS_PROC` — const.h:147.
pub const SYS_PROC: u32 = 0x010;
/// System services. C: `SRV_F` — priv.h:45.
pub const SRV_F: u32 = SYS_PROC | 0x004; // | PREEMPTIBLE
/// Dynamic system services. C: `DSRV_F` — priv.h:46.
pub const DSRV_F: u32 = SRV_F | 0x002; // | DYN_PRIV_ID
/// Root system processes. C: `RSYS_F` — priv.h:47.
pub const RSYS_F: u32 = SRV_F | 0x008; // | ROOT_SYS_PROC
/// VM. C: `VM_F` — priv.h:48.
pub const VM_F: u32 = SYS_PROC | 0x020; // | VM_SYS_PROC
/// User processes. C: `USR_F` — priv.h:49.
pub const USR_F: u32 = 0x004 | 0x001; // BILLABLE | PREEMPTIBLE

// ── Sys flags (rs.h) ────────────────────────────────────────────────────────

/// Core system services. C: `SF_CORE_SRV` — rs.h:191.
pub const SF_CORE_SRV: u32 = 0x001;
/// Service needs a replica to start. C: `SF_NEED_REPL` — rs.h:195.
pub const SF_NEED_REPL: u32 = 0x010;
/// Service needs synchronous boot init. C: `SF_SYNCH_BOOT` — rs.h:192.
pub const SF_SYNCH_BOOT: u32 = 0x002;

/// System services sys flags. C: `SRV_SF` — const.h:65.
pub const SRV_SF: u32 = SF_CORE_SRV;
/// Services needing a replica. C: `SRVR_SF` — const.h:66.
pub const SRVR_SF: u32 = SRV_SF | SF_NEED_REPL;
/// VM sys flags. C: `VM_SF` — const.h:68.
pub const VM_SF: u32 = SRVR_SF;

/// The boot image priv table (12 entries). C: `boot_image_priv_table` — table.c:15-30.
///
/// Order = boot order (RS → VM → PM → SCHED → VFS → DS → TTY → MEM → MIB → PFS → MFS → INIT).
/// The C `NULL_BOOT_NR` sentinel entry is not present: iteration over the slice
/// is the terminator.
pub static BOOT_IMAGE_PRIV_TABLE: &[BootImagePriv] = &[
    BootImagePriv {
        endpoint: Endpoint::RS,
        label: "rs",
        flags: RSYS_F,
    },
    BootImagePriv {
        endpoint: Endpoint::VM,
        label: "vm",
        flags: VM_F,
    },
    BootImagePriv {
        endpoint: Endpoint::PM,
        label: "pm",
        flags: SRV_F,
    },
    BootImagePriv {
        endpoint: Endpoint::SCHED,
        label: "sched",
        flags: SRV_F,
    },
    BootImagePriv {
        endpoint: Endpoint::VFS,
        label: "vfs",
        flags: SRV_F,
    },
    BootImagePriv {
        endpoint: Endpoint::DS,
        label: "ds",
        flags: SRV_F,
    },
    BootImagePriv {
        endpoint: Endpoint::TTY,
        label: "tty",
        flags: SRV_F,
    },
    BootImagePriv {
        endpoint: Endpoint::MEM,
        label: "memory",
        flags: SRV_F,
    },
    BootImagePriv {
        endpoint: Endpoint::MIB,
        label: "mib",
        flags: SRV_F,
    },
    BootImagePriv {
        endpoint: Endpoint::PFS,
        label: "pfs",
        flags: SRV_F,
    },
    BootImagePriv {
        endpoint: Endpoint::MFS,
        label: "fs_imgrd",
        flags: SRV_F,
    },
    BootImagePriv {
        endpoint: Endpoint::INIT,
        label: "init",
        flags: USR_F,
    },
];

/// Default sys entry. C: `boot_image_sys_table[].endpoint == DEFAULT_BOOT_NR` entry — table.c:41.
pub static DEFAULT_SYS: BootImageSys = BootImageSys {
    endpoint: Endpoint::NONE,
    flags: SRV_SF,
};

/// The boot image sys table (6 overrides + default). C: `boot_image_sys_table` — table.c:33-42.
pub static BOOT_IMAGE_SYS_TABLE: &[BootImageSys] = &[
    BootImageSys {
        endpoint: Endpoint::RS,
        flags: SRVR_SF,
    },
    BootImageSys {
        endpoint: Endpoint::VM,
        flags: VM_SF,
    },
    BootImageSys {
        endpoint: Endpoint::PM,
        flags: SRVR_SF,
    },
    BootImageSys {
        endpoint: Endpoint::SCHED,
        flags: SRVR_SF,
    },
    BootImageSys {
        endpoint: Endpoint::VFS,
        flags: SRVR_SF,
    },
    BootImageSys {
        endpoint: Endpoint::MFS,
        flags: 0,
    },
];

/// Default dev entry. C: `boot_image_dev_table[].endpoint == DEFAULT_BOOT_NR` entry — table.c:49.
pub static DEFAULT_DEV: BootImageDev = BootImageDev {
    endpoint: Endpoint::NONE,
    dev_nr: 0,
};

/// The boot image dev table (2 drivers + default). C: `boot_image_dev_table` — table.c:45-50.
pub static BOOT_IMAGE_DEV_TABLE: &[BootImageDev] = &[
    BootImageDev {
        endpoint: Endpoint::TTY,
        dev_nr: 4,
    }, // TTY_MAJOR (4)
    BootImageDev {
        endpoint: Endpoint::MEM,
        dev_nr: 1,
    }, // MEMORY_MAJOR (1)
];

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::Endpoint;

    #[test]
    fn test_priv_table_matches_c() {
        // C: table.c:17-28 — endpoint order + label + flags class.
        let expect: &[(Endpoint, &str)] = &[
            (Endpoint::RS, "rs"),
            (Endpoint::VM, "vm"),
            (Endpoint::PM, "pm"),
            (Endpoint::SCHED, "sched"),
            (Endpoint::VFS, "vfs"),
            (Endpoint::DS, "ds"),
            (Endpoint::TTY, "tty"),
            (Endpoint::MEM, "memory"),
            (Endpoint::MIB, "mib"),
            (Endpoint::PFS, "pfs"),
            (Endpoint::MFS, "fs_imgrd"),
            (Endpoint::INIT, "init"),
        ];
        assert_eq!(
            BOOT_IMAGE_PRIV_TABLE.len(),
            expect.len(),
            "12 boot services"
        );
        for (got, want) in BOOT_IMAGE_PRIV_TABLE.iter().zip(expect.iter()) {
            assert_eq!(got.endpoint, want.0, "endpoint order must match table.c");
            assert_eq!(got.label, want.1, "label must match table.c");
        }
        // Flags classes: first three are system-ish, INIT is a user process.
        assert!(
            BOOT_IMAGE_PRIV_TABLE[0].flags & SYS_PROC != 0,
            "RS is a sys proc"
        );
        assert!(
            BOOT_IMAGE_PRIV_TABLE[11].flags & SYS_PROC == 0,
            "INIT is a user proc"
        );
    }

    #[test]
    fn test_sys_table_has_default() {
        // C: table.c:35-41 — 6 overrides + DEFAULT_BOOT_NR default.
        assert_eq!(BOOT_IMAGE_SYS_TABLE.len(), 6);
        assert!(
            BOOT_IMAGE_SYS_TABLE
                .iter()
                .any(|e| e.endpoint == Endpoint::RS)
        );
        assert!(
            BOOT_IMAGE_SYS_TABLE
                .iter()
                .any(|e| e.endpoint == Endpoint::MFS)
        );
        assert_eq!(DEFAULT_SYS.flags, SRV_SF);
    }

    #[test]
    fn test_dev_table_has_default() {
        // C: table.c:47-49 — TTY + MEM + DEFAULT_BOOT_NR default.
        assert_eq!(BOOT_IMAGE_DEV_TABLE.len(), 2);
        assert!(
            BOOT_IMAGE_DEV_TABLE
                .iter()
                .any(|e| e.endpoint == Endpoint::TTY)
        );
        assert!(
            BOOT_IMAGE_DEV_TABLE
                .iter()
                .any(|e| e.endpoint == Endpoint::MEM)
        );
        assert_eq!(DEFAULT_DEV.dev_nr, 0);
    }
}
