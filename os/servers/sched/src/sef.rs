//! SCHED startup: settle in, learn the machine, wait to be asked.
//!
//! Mirrors `sef_local_startup()` + `sef_cb_init_fresh()` +
//! `struct machine` (`minix3/minix/servers/sched/main.c:17,111-136`).
//! 01-sched-init-main.md.
//!
//! The lifecycle owns the role split and nothing else: which init names
//! exist, what fresh boot carries, and what order the boot proves.
//! Transport (`sys_getmachine`), the balancer arm (`init_scheduling`,
//! 11), dispatch and replies (02) stay out.

/// The init names SCHED registers (`sef_local_startup`, `main.c:114-115`).
///
/// Only two: a fresh boot runs sched code; a restart reuses the libsef
/// generic (`SEF_CB_INIT_RESTART_STATEFUL`, `sef.h:85`) and needs no
/// sched body — so this is an enum, not a trait (a one-method trait
/// would be decoration: no second implementor, never a bound).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedInitKind {
    /// Fresh boot. C: `sef_setcb_init_fresh(sef_cb_init_fresh)` — main.c:114.
    Fresh,
    /// Restart with state. C: `sef_setcb_init_restart
    /// (SEF_CB_INIT_RESTART_STATEFUL)` — main.c:115; the generic lives in
    /// libsef, SCHED contributes no code.
    RestartStateful,
}

/// Machine news (`struct machine`, `type.h:123-124`).
///
/// Only the two numbers SCHED consumes travel here; the rest of the
/// struct (`padding`/`apic_enabled`/`acpi_rsdp`/`board_id`) is kernel
/// business SCHED never reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MachineInfo {
    /// How many CPUs are available. C: `processors_count` — type.h:123.
    pub processors_count: u32,
    /// The bootstrap CPU. C: `bsp_id` — type.h:124.
    pub bsp_id: u32,
}

/// Fresh boot proven (`sef_cb_init_fresh` returned `OK`, `main.c:135`).
///
/// Carries the machine, so the balancer arm (11) cannot run before the
/// machine is known: no boot, no wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FreshReady {
    /// The learned machine.
    pub machine: MachineInfo,
}

/// Run the fresh boot (`sef_cb_init_fresh`, `main.c:126-136`).
///
/// The machine walks in, the token walks out; the transport failure
/// (`sys_getmachine` Err → C `panic`s, `131`) and the balancer arm
/// (`init_scheduling`, 11) both stay caller-side.
pub fn init_fresh(machine: MachineInfo) -> FreshReady {
    FreshReady { machine }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_kinds() {
        // Two registrations (`main.c:114-115`); restart is generic (sef.h:85).
        assert_eq!(SchedInitKind::Fresh, SchedInitKind::Fresh);
        assert_ne!(SchedInitKind::Fresh, SchedInitKind::RestartStateful);
    }

    #[test]
    fn test_machine_walks_in_token_walks_out() {
        // Fresh boot carries the machine through (`main.c:126-136`).
        let machine = MachineInfo {
            processors_count: 4,
            bsp_id: 0,
        };
        let ready = init_fresh(machine);
        assert_eq!(ready.machine, machine);
        assert_eq!(ready.machine.processors_count, 4);
        assert_eq!(ready.machine.bsp_id, 0);
    }

    #[test]
    fn test_single_cpu_boot() {
        // One CPU is a legal machine (10 narrows it, 01 only carries).
        let ready = init_fresh(MachineInfo {
            processors_count: 1,
            bsp_id: 0,
        });
        assert_eq!(ready.machine.processors_count, 1);
    }
}
