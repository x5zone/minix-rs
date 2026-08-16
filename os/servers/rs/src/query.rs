//! Query and diagnostic requests (pure slice).
//!
//! Mirrors `minix3/minix/servers/rs/request.c:1095-1263` (`do_getsysinfo` —
//! 1095, `do_lookup` — 1144, `do_sysctl` — 1181, `do_fi` — 1229) and
//! `utility.c:69-77` (`fi_service`). 14-rs-query-requests.md.
//!
//! The IPC-coupled steps (`sys_datacopy`, the `COMMON_REQ_FI_CTL` message,
//! the `print_*` output) are wired through 19-rs-external-interfaces.md; the
//! `RS_SYSCTL_UPD_*` actions are 16-rs-live-update.md mechanisms. This
//! module owns the pure classification: which table a `SI_*` request names,
//! the `do_lookup` name-length gate, and the `RS_SYSCTL_*` sub-type mapping.

use minix_types::Errno;

/// C: `SI_PROC_TAB` — sysinfo.h:11.
pub const SI_PROC_TAB: i32 = 2;
/// C: `SI_PROCPUB_TAB` — sysinfo.h:15.
pub const SI_PROCPUB_TAB: i32 = 11;
/// C: `SI_PROCALL_TAB` — sysinfo.h:16.
pub const SI_PROCALL_TAB: i32 = 12;

/// Which process-table the `do_getsysinfo` request names.
///
/// C: `do_getsysinfo` — request.c:1112-1128. `SI_PROCALL_TAB` copies both
/// tables back to back (the first `sys_datacopy` happens inside the switch,
/// request.c:1118-1122); the Rust decision only selects the table kind —
/// the C-layout sizes (`sizeof(struct rproc)`/`sizeof(struct rprocpub)`,
/// request.c:1113/1125) and the copies are wired at 19 (Rust's layout is
/// not C-layout compatible) (ARCH A-2/A-3 — 14-rs-query-requests.md §3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GetsysinfoTable {
    /// C: `SI_PROC_TAB` — the private `rproc[]` table.
    ProcTab,
    /// C: `SI_PROCPUB_TAB` — the public `rprocpub[]` table.
    ProcPubTab,
    /// C: `SI_PROCALL_TAB` — both tables, private first.
    ProcAllTab,
}

/// Maps a `SI_*` request to the table kind.
///
/// C: `do_getsysinfo` — request.c:1112-1128; unknown `what` → `EINVAL`
/// (request.c:1129).
pub fn getsysinfo_table(what: i32) -> Result<GetsysinfoTable, Errno> {
    match what {
        SI_PROC_TAB => Ok(GetsysinfoTable::ProcTab),
        SI_PROCPUB_TAB => Ok(GetsysinfoTable::ProcPubTab),
        SI_PROCALL_TAB => Ok(GetsysinfoTable::ProcAllTab),
        _ => Err(Errno::EINVAL),
    }
}

/// Maximum lookup name length.
///
/// C: `static char namebuf[100]` — request.c:1147.
pub const NAME_BUF_LEN: usize = 100;

/// The `do_lookup` name-length gate.
///
/// C: `do_lookup` — request.c:1152-1156: `len < 2 || len >= sizeof(namebuf)`
/// → `EINVAL` (a service label is at least 2 chars, e.g. "rs").
pub fn lookup_name_len(len: usize) -> Result<(), Errno> {
    if !(2..NAME_BUF_LEN).contains(&len) {
        Err(Errno::EINVAL)
    } else {
        Ok(())
    }
}

/// C: `RS_SYSCTL_SRV_STATUS` — com.h:485.
pub const RS_SYSCTL_SRV_STATUS: i32 = 1;
/// C: `RS_SYSCTL_UPD_START` — com.h:486.
pub const RS_SYSCTL_UPD_START: i32 = 2;
/// C: `RS_SYSCTL_UPD_RUN` — com.h:487.
pub const RS_SYSCTL_UPD_RUN: i32 = 3;
/// C: `RS_SYSCTL_UPD_STOP` — com.h:488.
pub const RS_SYSCTL_UPD_STOP: i32 = 4;
/// C: `RS_SYSCTL_UPD_STATUS` — com.h:489.
pub const RS_SYSCTL_UPD_STATUS: i32 = 5;

/// The `RS_SYSCTL_*` sub-request kind.
///
/// C: `do_sysctl` — request.c:1181-1228. The `UPD_*` actions are
/// 16-rs-live-update.md mechanisms (`start_update_prepare`,
/// `abort_update_proc`); only the classification is pure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SysctlAction {
    /// Print the service table. C: `RS_SYSCTL_SRV_STATUS` — request.c:1186-1188.
    PrintServices,
    /// Prepare an update. C: `RS_SYSCTL_UPD_START` — request.c:1189-1211.
    UpdateStart,
    /// Run a prepared update. C: `RS_SYSCTL_UPD_RUN` — request.c:1189-1217.
    UpdateRun,
    /// Abort a scheduled update. C: `RS_SYSCTL_UPD_STOP` — request.c:1212-1215.
    UpdateStop,
    /// Print the update status. C: `RS_SYSCTL_UPD_STATUS` — request.c:1216-1218.
    UpdateStatus,
}

/// Classifies an `RS_SYSCTL_*` sub-type.
///
/// C: `do_sysctl` — request.c:1183-1222; unknown sub-type → `EINVAL`.
pub fn classify_sysctl(request_type: i32) -> Result<SysctlAction, Errno> {
    match request_type {
        RS_SYSCTL_SRV_STATUS => Ok(SysctlAction::PrintServices),
        RS_SYSCTL_UPD_START => Ok(SysctlAction::UpdateStart),
        RS_SYSCTL_UPD_RUN => Ok(SysctlAction::UpdateRun),
        RS_SYSCTL_UPD_STOP => Ok(SysctlAction::UpdateStop),
        RS_SYSCTL_UPD_STATUS => Ok(SysctlAction::UpdateStatus),
        _ => Err(Errno::EINVAL),
    }
}

/// C: `RS_FI_CRASH` — com.h:492 (fault-injection sub-type).
pub const RS_FI_CRASH: i32 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_getsysinfo_table_mapping() {
        // C: request.c:1112-1128 — SI_* → table kind.
        assert_eq!(getsysinfo_table(SI_PROC_TAB), Ok(GetsysinfoTable::ProcTab));
        assert_eq!(
            getsysinfo_table(SI_PROCPUB_TAB),
            Ok(GetsysinfoTable::ProcPubTab)
        );
        assert_eq!(
            getsysinfo_table(SI_PROCALL_TAB),
            Ok(GetsysinfoTable::ProcAllTab)
        );
        assert_eq!(getsysinfo_table(99), Err(Errno::EINVAL));
    }

    #[test]
    fn test_lookup_name_len_gate() {
        // C: request.c:1152-1156 — <2 or >= sizeof(namebuf) → EINVAL.
        assert_eq!(lookup_name_len(0), Err(Errno::EINVAL));
        assert_eq!(lookup_name_len(1), Err(Errno::EINVAL));
        assert_eq!(lookup_name_len(2), Ok(()));
        assert_eq!(lookup_name_len(99), Ok(()));
        assert_eq!(lookup_name_len(100), Err(Errno::EINVAL));
        assert_eq!(lookup_name_len(101), Err(Errno::EINVAL));
    }

    #[test]
    fn test_classify_sysctl() {
        // C: request.c:1183-1222 — 5 sub-types + default EINVAL.
        assert_eq!(
            classify_sysctl(RS_SYSCTL_SRV_STATUS),
            Ok(SysctlAction::PrintServices)
        );
        assert_eq!(
            classify_sysctl(RS_SYSCTL_UPD_START),
            Ok(SysctlAction::UpdateStart)
        );
        assert_eq!(
            classify_sysctl(RS_SYSCTL_UPD_RUN),
            Ok(SysctlAction::UpdateRun)
        );
        assert_eq!(
            classify_sysctl(RS_SYSCTL_UPD_STOP),
            Ok(SysctlAction::UpdateStop)
        );
        assert_eq!(
            classify_sysctl(RS_SYSCTL_UPD_STATUS),
            Ok(SysctlAction::UpdateStatus)
        );
        assert_eq!(classify_sysctl(0), Err(Errno::EINVAL));
        assert_eq!(classify_sysctl(6), Err(Errno::EINVAL));
    }

    #[test]
    fn test_fi_crash_constant() {
        // C: com.h:492 — RS_FI_CRASH = 1.
        assert_eq!(RS_FI_CRASH, 1);
    }
}
