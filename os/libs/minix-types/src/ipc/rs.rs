//! Reincarnation Server (RS) IPC message types and payloads.
//!
//! C: `<minix/com.h>`:463-492 — the `RS_*` message `m_type` values. The typed
//! message payloads (`mess_rs_*`/`mess_lsys_*`, ipc.h:1048-1072,1420-1428,
//! 1466-1474,1858-1906) are the ARCH A-2 item (19-rs-external-interfaces.md /
//! 99-rs-global-concepts.md): the raw call numbers consumed by access control
//! (04), control requests (13), query requests (14) and live update (16), plus
//! semantic payload views modeled after the vm.rs In/Out convention.

use crate::{Endpoint, Gid, Pid, Uid, VirBytes};

/// Base for RS messages. C: `RS_RQ_BASE` — com.h:463.
pub const RS_RQ_BASE: i32 = 0x700;

/// Start system service. C: `RS_UP` — com.h:465.
pub const RS_UP: i32 = RS_RQ_BASE + 0;
/// Stop system service. C: `RS_DOWN` — com.h:466.
pub const RS_DOWN: i32 = RS_RQ_BASE + 1;
/// Refresh system service. C: `RS_REFRESH` — com.h:467.
pub const RS_REFRESH: i32 = RS_RQ_BASE + 2;
/// Restart system service. C: `RS_RESTART` — com.h:468.
pub const RS_RESTART: i32 = RS_RQ_BASE + 3;
/// Alert about shutdown. C: `RS_SHUTDOWN` — com.h:469.
pub const RS_SHUTDOWN: i32 = RS_RQ_BASE + 4;
/// Update system service. C: `RS_UPDATE` — com.h:470.
pub const RS_UPDATE: i32 = RS_RQ_BASE + 5;
/// Clone system service. C: `RS_CLONE` — com.h:471.
pub const RS_CLONE: i32 = RS_RQ_BASE + 6;
/// Unclone system service. C: `RS_UNCLONE` — com.h:472.
pub const RS_UNCLONE: i32 = RS_RQ_BASE + 7;
/// Lookup server name. C: `RS_LOOKUP` — com.h:474.
pub const RS_LOOKUP: i32 = RS_RQ_BASE + 8;
/// Get system information. C: `RS_GETSYSINFO` — com.h:476.
pub const RS_GETSYSINFO: i32 = RS_RQ_BASE + 9;
/// Service init message. C: `RS_INIT` — com.h:478.
pub const RS_INIT: i32 = RS_RQ_BASE + 20;
/// Prepare to update message. C: `RS_LU_PREPARE` — com.h:479.
pub const RS_LU_PREPARE: i32 = RS_RQ_BASE + 21;
/// Edit system service. C: `RS_EDIT` — com.h:480.
pub const RS_EDIT: i32 = RS_RQ_BASE + 22;
/// Perform system ctl action. C: `RS_SYSCTL` — com.h:481.
pub const RS_SYSCTL: i32 = RS_RQ_BASE + 23;
/// Inject fault into service. C: `RS_FI` — com.h:482.
pub const RS_FI: i32 = RS_RQ_BASE + 24;

/// Subfunctions for `RS_SYSCTL`. C: com.h:485-489.
pub mod sysctl {
    /// C: `RS_SYSCTL_SRV_STATUS` — com.h:485.
    pub const SRV_STATUS: i32 = 1;
    /// C: `RS_SYSCTL_UPD_START` — com.h:486.
    pub const UPD_START: i32 = 2;
    /// C: `RS_SYSCTL_UPD_RUN` — com.h:487.
    pub const UPD_RUN: i32 = 3;
    /// C: `RS_SYSCTL_UPD_STOP` — com.h:488.
    pub const UPD_STOP: i32 = 4;
    /// C: `RS_SYSCTL_UPD_STATUS` — com.h:489.
    pub const UPD_STATUS: i32 = 5;
}

/// Subfunctions for `RS_FI`. C: `RS_FI_CRASH` — com.h:492.
pub const RS_FI_CRASH: i32 = 1;

// ── Typed payload views (ARCH A-2, semantic layer) ─────────────────────────

/// Payload of the RS control/query requests.
///
/// C: `mess_rs_req` — ipc.h:1887-1896. Shared by `RS_UP`/`RS_DOWN`/
/// `RS_REFRESH`/`RS_RESTART`/`RS_UPDATE`/`RS_CLONE`/`RS_UNCLONE`/
/// `RS_LOOKUP`/`RS_GETSYSINFO`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RsReq {
    /// C: `len` — size of the buffered argument area.
    pub len: usize,
    /// C: `name_len` — length of the service label.
    pub name_len: usize,
    /// C: `endpoint` — target endpoint (e.g. `RS_SRV_KILL`).
    pub endpoint: Endpoint,
    /// C: `addr` — pointer to the buffered argument area.
    pub addr: VirBytes,
    /// C: `name` — pointer to the service label.
    pub name: VirBytes,
    /// C: `subtype` — sub-type (`RS_SYSCTL_*`/`RS_FI_*`).
    pub subtype: i32,
}

/// Payload of the `RS_INIT` message.
///
/// C: `mess_rs_init` — ipc.h:1858-1867.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RsInit {
    /// C: `result` — init result (0 = OK).
    pub result: i32,
    /// C: `type` — `SEF_INIT_*` init type.
    pub init_type: i32,
    /// C: `rproctab_gid` — grant for the public process table.
    pub rproctab_gid: i32,
    /// C: `old_endpoint` — endpoint of the previous incarnation.
    pub old_endpoint: Endpoint,
    /// C: `restarts` — number of restarts.
    pub restarts: i32,
    /// C: `flags` — `SEF_LU_*`/init flags.
    pub flags: i32,
    /// C: `buff_addr` — state-transfer buffer address.
    pub buff_addr: VirBytes,
    /// C: `buff_len` — state-transfer buffer length.
    pub buff_len: usize,
    /// C: `prepare_state` — `SEF_LU_STATE_*` prepare state.
    pub prepare_state: i32,
}

/// Payload of the `RS_LU_PREPARE` message.
///
/// C: `mess_rs_update` — ipc.h:1898-1906.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RsUpdate {
    /// C: `result` — prepare result (0 = OK).
    pub result: i32,
    /// C: `state` — `SEF_LU_STATE_*` reached state.
    pub state: i32,
    /// C: `prepare_maxtime` — requested prepare time budget.
    pub prepare_maxtime: i32,
    /// C: `flags` — `SEF_LU_*` flags.
    pub flags: i32,
    /// C: `state_data_gid` — grant for the state data.
    pub state_data_gid: i32,
}

/// Payload of the PM→RS exec-restart message.
///
/// C: `mess_rs_pm_exec_restart` — ipc.h:1869-1877.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RsPmExecRestart {
    /// C: `endpt` — the restarted process endpoint.
    pub endpt: Endpoint,
    /// C: `result` — exec result.
    pub result: i32,
    /// C: `pc` — program counter after restart.
    pub pc: VirBytes,
    /// C: `ps_str` — pointer to the process state string.
    pub ps_str: VirBytes,
}

/// Payload of the PM→RS srv-kill message.
///
/// C: `mess_rs_pm_srv_kill` — ipc.h:1879-1885.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RsPmSrvKill {
    /// C: `pid` — process to kill.
    pub pid: Pid,
    /// C: `nr` — slot number.
    pub nr: i32,
}

/// Payload of the getsysinfo request sent to RS.
///
/// C: `mess_lsys_getsysinfo` — ipc.h:1066-1072.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LsysGetsysinfo {
    /// C: `what` — `SI_*` table selector.
    pub what: i32,
    /// C: `where` — destination buffer.
    pub where_: VirBytes,
    /// C: `size` — buffer size.
    pub size: usize,
}

/// Payload of the fault-injection request sent to RS.
///
/// C: `mess_lsys_fi_ctl` — ipc.h:1048-1056.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LsysFiCtl {
    /// C: `gid` — grant for the fault-injection arguments.
    pub gid: i32,
    /// C: `size` — argument size.
    pub size: usize,
    /// C: `subtype` — `RS_FI_*` fault type.
    pub subtype: i32,
}

/// Payload of the fault-injection reply.
///
/// C: `mess_lsys_fi_reply` — ipc.h:1058-1063.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LsysFiReply {
    /// C: `status` — fault-injection status.
    pub status: i32,
}

/// Payload of the RS→PM srv-fork request.
///
/// C: `mess_lsys_pm_srv_fork` — ipc.h:1420-1428.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LsysPmSrvFork {
    /// C: `uid` — user id of the new process.
    pub uid: Uid,
    /// C: `gid` — group id of the new process.
    pub gid: Gid,
}

/// Payload of the RS→VFS mapdriver request.
///
/// C: `mess_lsys_vfs_mapdriver` — ipc.h:1466-1474.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LsysVfsMapdriver {
    /// C: `major` — device major number.
    pub major: i32,
    /// C: `labellen` — label length.
    pub labellen: usize,
    /// C: `label` — device label.
    pub label: VirBytes,
    /// C: `ndomains` — number of valid domains.
    pub ndomains: i32,
    /// C: `domains[NR_DOMAIN]` — socket driver domains (`NR_DOMAIN` = 8).
    pub domains: [i32; 8],
}

/// C: `VM_RS_MEM_PIN` — com.h:741 (vm_memctl: pin memory).
pub const VM_RS_MEM_PIN: i32 = 0;
/// C: `VM_RS_MEM_MAKE_VM` — com.h:742 (vm_memctl: make VM instance).
pub const VM_RS_MEM_MAKE_VM: i32 = 1;
/// C: `VM_RS_MEM_HEAP_PREALLOC` — com.h:743 (vm_memctl: preallocate heap).
pub const VM_RS_MEM_HEAP_PREALLOC: i32 = 2;
/// C: `VM_RS_MEM_MAP_PREALLOC` — com.h:744 (vm_memctl: preallocate mmap).
pub const VM_RS_MEM_MAP_PREALLOC: i32 = 3;
/// C: `VM_RS_MEM_GET_PREALLOC_MAP` — com.h:745 (vm_memctl: read prealloc map).
pub const VM_RS_MEM_GET_PREALLOC_MAP: i32 = 4;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rs_call_values() {
        // C: com.h:463-483.
        assert_eq!(RS_RQ_BASE, 0x700);
        assert_eq!(RS_UP, 0x700);
        assert_eq!(RS_DOWN, 0x701);
        assert_eq!(RS_REFRESH, 0x702);
        assert_eq!(RS_RESTART, 0x703);
        assert_eq!(RS_SHUTDOWN, 0x704);
        assert_eq!(RS_UPDATE, 0x705);
        assert_eq!(RS_CLONE, 0x706);
        assert_eq!(RS_UNCLONE, 0x707);
        assert_eq!(RS_LOOKUP, 0x708);
        assert_eq!(RS_GETSYSINFO, 0x709);
        assert_eq!(RS_INIT, 0x714);
        assert_eq!(RS_LU_PREPARE, 0x715);
        assert_eq!(RS_EDIT, 0x716);
        assert_eq!(RS_SYSCTL, 0x717);
        assert_eq!(RS_FI, 0x718);
    }

    #[test]
    fn test_rs_subfunctions() {
        assert_eq!(sysctl::SRV_STATUS, 1);
        assert_eq!(sysctl::UPD_START, 2);
        assert_eq!(sysctl::UPD_RUN, 3);
        assert_eq!(sysctl::UPD_STOP, 4);
        assert_eq!(sysctl::UPD_STATUS, 5);
        assert_eq!(RS_FI_CRASH, 1);
    }

    #[test]
    fn test_rs_req_payload() {
        // C: mess_rs_req — ipc.h:1887-1896.
        let req = RsReq {
            len: 16,
            name_len: 4,
            endpoint: Endpoint::RS,
            addr: VirBytes(0x1000),
            name: VirBytes(0x2000),
            subtype: sysctl::UPD_START,
        };
        assert_eq!(req.len, 16);
        assert_eq!(req.name_len, 4);
        assert_eq!(req.endpoint, Endpoint::RS);
        assert_eq!(req.addr, VirBytes(0x1000));
        assert_eq!(req.name, VirBytes(0x2000));
        assert_eq!(req.subtype, 2);
    }

    #[test]
    fn test_rs_init_payload() {
        // C: mess_rs_init — ipc.h:1858-1867.
        let init = RsInit {
            result: 0,
            init_type: 1, // SEF_INIT_LU
            rproctab_gid: 7,
            old_endpoint: Endpoint::PM,
            restarts: 2,
            flags: 0x100, // SEF_LU_SELF
            buff_addr: VirBytes(0x3000),
            buff_len: 64,
            prepare_state: 0, // SEF_LU_STATE_NULL
        };
        assert_eq!(init.result, 0);
        assert_eq!(init.init_type, 1);
        assert_eq!(init.rproctab_gid, 7);
        assert_eq!(init.old_endpoint, Endpoint::PM);
        assert_eq!(init.restarts, 2);
        assert_eq!(init.flags, 0x100);
        assert_eq!(init.buff_addr, VirBytes(0x3000));
        assert_eq!(init.buff_len, 64);
        assert_eq!(init.prepare_state, 0);
    }

    #[test]
    fn test_rs_update_payload() {
        // C: mess_rs_update — ipc.h:1898-1906.
        let upd = RsUpdate {
            result: 0,
            state: 5, // SEF_LU_STATE_UNREACHABLE
            prepare_maxtime: 100,
            flags: 0x800, // SEF_LU_INCLUDES_VM
            state_data_gid: 9,
        };
        assert_eq!(upd.result, 0);
        assert_eq!(upd.state, 5);
        assert_eq!(upd.prepare_maxtime, 100);
        assert_eq!(upd.flags, 0x800);
        assert_eq!(upd.state_data_gid, 9);
    }

    #[test]
    fn test_rs_pm_payloads() {
        // C: mess_rs_pm_exec_restart / mess_rs_pm_srv_kill — ipc.h:1869-1885.
        let exec = RsPmExecRestart {
            endpt: Endpoint::VFS,
            result: 0,
            pc: VirBytes(0x4000),
            ps_str: VirBytes(0x5000),
        };
        assert_eq!(exec.endpt, Endpoint::VFS);
        assert_eq!(exec.result, 0);
        assert_eq!(exec.pc, VirBytes(0x4000));
        assert_eq!(exec.ps_str, VirBytes(0x5000));

        let kill = RsPmSrvKill { pid: 42, nr: 3 };
        assert_eq!(kill.pid, 42);
        assert_eq!(kill.nr, 3);
    }

    #[test]
    fn test_lsys_rs_payloads() {
        // C: mess_lsys_getsysinfo / mess_lsys_fi_ctl / mess_lsys_fi_reply —
        // ipc.h:1048-1072.
        let gs = LsysGetsysinfo {
            what: 2, // SI_PROC_TAB
            where_: VirBytes(0x6000),
            size: 128,
        };
        assert_eq!(gs.what, 2);
        assert_eq!(gs.where_, VirBytes(0x6000));
        assert_eq!(gs.size, 128);

        let fi = LsysFiCtl {
            gid: 5,
            size: 32,
            subtype: RS_FI_CRASH,
        };
        assert_eq!(fi.gid, 5);
        assert_eq!(fi.size, 32);
        assert_eq!(fi.subtype, 1);

        let reply = LsysFiReply { status: 0 };
        assert_eq!(reply.status, 0);
    }

    #[test]
    fn test_lsys_rs_pm_and_mapdriver() {
        // C: mess_lsys_pm_srv_fork / mess_lsys_vfs_mapdriver —
        // ipc.h:1420-1428,1466-1474.
        let fork = LsysPmSrvFork { uid: 0, gid: 0 };
        assert_eq!(fork.uid, 0);
        assert_eq!(fork.gid, 0);

        let md = LsysVfsMapdriver {
            major: 1,
            labellen: 4,
            label: VirBytes(0x7000),
            ndomains: 1,
            domains: [0, 0, 0, 0, 0, 0, 0, 0],
        };
        assert_eq!(md.major, 1);
        assert_eq!(md.labellen, 4);
        assert_eq!(md.label, VirBytes(0x7000));
        assert_eq!(md.ndomains, 1);
        assert_eq!(md.domains.len(), 8);
    }

    #[test]
    fn test_vm_rs_mem_constants() {
        // C: com.h:741-745.
        assert_eq!(VM_RS_MEM_PIN, 0);
        assert_eq!(VM_RS_MEM_MAKE_VM, 1);
        assert_eq!(VM_RS_MEM_HEAP_PREALLOC, 2);
        assert_eq!(VM_RS_MEM_MAP_PREALLOC, 3);
        assert_eq!(VM_RS_MEM_GET_PREALLOC_MAP, 4);
    }
}
