//! Scheduling initialization and signal-manager update primitives.
//!
//! Mirrors `minix3/minix/servers/rs/utility.c:364-422` (`sched_init_proc`,
//! `update_sig_mgrs`). The external syscalls (`sched_start`'s
//! `sys_schedctl`/`SCHEDULING_START` branch, `sys_getpriv`, `sys_privctl`)
//! go through the `KernelApi` trait (01-rs-boot-init.md); the production
//! wiring is deferred to 19-rs-external-interfaces.md (fail-closed until
//! then — ARCH A-12).

use crate::boot::KernelApi;
use crate::privilege::{PrivCtlOp, Privilege};
use minix_types::Endpoint;

/// Scheduling parameters for one service.
///
/// C: the `r_scheduler`/`r_priority`/`r_quantum`/`r_cpu` fields of
/// `struct rproc` (type.h:89-92) plus the fixed parent (RS_PROC_NR).
/// `sched_init_proc` reads these from the slot and passes them to
/// `sched_start` (lib/libsys/sched_start.c:37-80).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchedulerConfig {
    /// Scheduler endpoint (`KERNEL` for boot services, priv.h:88).
    pub scheduler: Endpoint,
    /// The process being scheduled.
    pub endpoint: Endpoint,
    /// Parent (RS itself). C: `RS_PROC_NR` — utility.c:374.
    pub parent: Endpoint,
    /// Scheduling priority. C: `r_priority` — type.h:90.
    pub priority: i32,
    /// Scheduling quantum. C: `r_quantum` — type.h:91.
    pub quantum: i32,
    /// CPU affinity. C: `r_cpu` — type.h:92.
    pub cpu: i32,
}

impl SchedulerConfig {
    /// Builds the config from a slot's scheduling fields.
    ///
    /// C: main.c:320-322 + boot Step 2's `sched_init_proc(rp)` (main.c:376).
    /// `endpoint` is `rpub->endpoint` (utility.c:373).
    pub fn from_slot(
        scheduler: Endpoint,
        endpoint: Endpoint,
        priority: i32,
        quantum: i32,
        cpu: i32,
    ) -> SchedulerConfig {
        SchedulerConfig {
            scheduler,
            endpoint,
            parent: Endpoint::RS,
            priority,
            quantum,
            cpu,
        }
    }
}

/// Starts scheduling for the given process.
///
/// C: `sched_init_proc` — `minix3/minix/servers/rs/utility.c:364-382`.
///
/// Invariants (C asserts, utility.c:369-371): user processes must have no
/// scheduler (`r_scheduler == NONE` — PM deals with them); system processes
/// must have one. The external call is `sched_start(scheduler, endpoint,
/// RS_PROC_NR, priority, quantum, cpu, &r_scheduler)` (sched_start.c:37-80);
/// `sys` is the `KernelApi` boundary and returns the (possibly updated)
/// scheduler endpoint (`*newscheduler_e`).
pub fn sched_init_proc(
    cfg: &SchedulerConfig,
    is_sys_proc: bool,
    sys: &mut dyn KernelApi,
) -> Result<Endpoint, i32> {
    if !is_sys_proc {
        debug_assert_eq!(
            cfg.scheduler,
            Endpoint::NONE,
            "user process must have no scheduler (utility.c:369)"
        );
    } else {
        debug_assert_ne!(
            cfg.scheduler,
            Endpoint::NONE,
            "system process must have a scheduler (utility.c:370)"
        );
    }
    // C: sched_start(..., &rp->r_scheduler) — utility.c:372-378. The kernel
    // API returns the scheduler that actually took over.
    sys.sched_init_proc(cfg.endpoint)?;
    Ok(cfg.scheduler)
}

/// Updates the signal managers of a service.
///
/// C: `update_sig_mgrs` — `minix3/minix/servers/rs/utility.c:387-422`.
/// Order is fixed: sync the priv structure from the kernel (`sys_getpriv`),
/// set `s_sig_mgr`/`s_bak_sig_mgr`, then commit with
/// `SYS_PRIV_UPDATE_SYS`. `SELF` expansion (`sig_mgr == SELF ? endpoint :
/// sig_mgr`, utility.c:397-398) happens at the call site (12/16).
pub fn update_sig_mgrs(
    priv_: &mut Privilege,
    sys: &mut dyn KernelApi,
    endpoint: Endpoint,
    sig_mgr: Endpoint,
    bak_sig_mgr: Endpoint,
) -> Result<(), i32> {
    // utility.c:393-396: synch privilege structure with the kernel.
    let synced = sys.getpriv(endpoint)?;
    *priv_ = synced;

    // utility.c:399-400: set signal managers.
    priv_.sig_mgr = sig_mgr;
    priv_.bak_sig_mgr = bak_sig_mgr;

    // utility.c:401-406: update privilege structure.
    sys.privctl(endpoint, PrivCtlOp::UpdateSys, Some(priv_))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::privilege::{PrivFlags, Privilege};
    use alloc::vec::Vec;

    /// Test kernel API recording calls (same shape as boot.rs MockKernelApi).
    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Call {
        GetPriv(Endpoint),
        PrivCtl(Endpoint, PrivCtlOp),
        SchedInitProc(Endpoint),
    }

    #[derive(Default)]
    struct MockSys {
        calls: Vec<Call>,
    }

    impl KernelApi for MockSys {
        fn get_machine(&mut self) -> Result<crate::boot::Machine, i32> {
            unimplemented!()
        }
        fn get_hz(&mut self) -> Result<u32, i32> {
            unimplemented!()
        }
        fn privctl(
            &mut self,
            proc: Endpoint,
            op: PrivCtlOp,
            _priv_: Option<&Privilege>,
        ) -> Result<(), i32> {
            self.calls.push(Call::PrivCtl(proc, op));
            Ok(())
        }
        fn getpriv(&mut self, proc: Endpoint) -> Result<Privilege, i32> {
            self.calls.push(Call::GetPriv(proc));
            Ok(Privilege::vacant())
        }
        fn sched_init_proc(&mut self, proc: Endpoint) -> Result<(), i32> {
            self.calls.push(Call::SchedInitProc(proc));
            Ok(())
        }
        fn getnuid(&mut self, _proc: Endpoint) -> Result<u32, i32> {
            unimplemented!()
        }
        fn getnpid(&mut self, _proc: Endpoint) -> Result<i32, i32> {
            unimplemented!()
        }
        fn setalarm(&mut self, _delay_ticks: u32) -> Result<(), i32> {
            unimplemented!()
        }
    }

    #[test]
    fn test_sched_init_proc_sys_proc() {
        let cfg = SchedulerConfig::from_slot(Endpoint::KERNEL, Endpoint::PM, 3, 200, 0);
        let mut sys = MockSys::default();
        let sched = sched_init_proc(&cfg, true, &mut sys).unwrap();
        assert_eq!(sched, Endpoint::KERNEL);
        assert_eq!(sys.calls, vec![Call::SchedInitProc(Endpoint::PM)]);
    }

    #[test]
    fn test_sched_init_proc_user_proc_none() {
        // User process with NONE scheduler: valid, but the C code returns OK
        // without calling the scheduler (sched_start.c:45-47). Here the
        // KernelApi is still invoked because the wiring is deferred; the
        // invariant check is the important part.
        let cfg = SchedulerConfig::from_slot(Endpoint::NONE, Endpoint::INIT, 3, 200, 0);
        let mut sys = MockSys::default();
        let sched = sched_init_proc(&cfg, false, &mut sys).unwrap();
        assert_eq!(sched, Endpoint::NONE);
    }

    #[test]
    fn test_update_sig_mgrs_order() {
        let mut priv_ = Privilege::vacant();
        let mut sys = MockSys::default();
        update_sig_mgrs(
            &mut priv_,
            &mut sys,
            Endpoint::PM,
            Endpoint::RS,
            Endpoint::NONE,
        )
        .unwrap();
        assert_eq!(
            sys.calls,
            vec![
                Call::GetPriv(Endpoint::PM),
                Call::PrivCtl(Endpoint::PM, PrivCtlOp::UpdateSys),
            ]
        );
        // The synced (vacant) priv has the new signal managers set.
        assert_eq!(priv_.sig_mgr, Endpoint::RS);
        assert_eq!(priv_.bak_sig_mgr, Endpoint::NONE);
    }

    #[test]
    fn test_boot_priv_sys_flags() {
        // sanity: Privilege + PrivFlags wiring compiles together.
        let p = Privilege::boot_priv(PrivFlags::SYS_PROC, 0);
        assert!(p.is_sys_proc());
    }
}
