//! SCHED's near side of the kernel contract: who schedules whom.
//!
//! Mirrors the registration half of the contract (`minix3/minix/kernel/
//! system/do_schedctl.c:7-46`): which flag bits may travel, which
//! assignment they name, and in what order the call packs the wire.
//! 12-kernel-interface.md.
//!
//! The module owns the packing and nothing else: flag validation,
//! assignment naming, and wire order. Sending the call, resolving the
//! endpoint, and judging the flags stay kernel-side — the kernel holds
//! the gate (`dispatch_schedctl`, `os/kernel/src/syscall_process.rs:630`),
//! this module only mirrors its answers early (06 `admit()`'s precedent:
//! same answer up front, one IPC round-trip saved on refusal).
//!
//! Single-threaded event loop: pure functions, no shared state.

use bitflags::bitflags;
use minix_types::{EINVAL, Endpoint, MessLsysKrnSchedctl};

/// Schedctl flag: the kernel becomes the scheduler (`com.h:449`).
///
/// The only defined bit. The wire truth lives kernel-side
/// (`os/kernel/src/syscall_process.rs:49`, private); this side mirrors
/// the value for packing and early refusal (the DEFAULT_HZ twin-copy
/// precedent: separate crates, the judge holds truth, the mirror notes
/// it — Proposal #12).
pub const SCHEDCTL_FLAG_KERNEL: u32 = 1;

bitflags! {
    /// Which scheduler-claim bits travel (`do_schedctl.c:17-21`).
    ///
    /// One named bit today; the set form keeps unknown bits nameable —
    /// and refusable — when tomorrow defines bit 1.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SchedctlFlags: u32 {
        /// The kernel schedules the target. C: `SCHEDCTL_FLAG_KERNEL`.
        const KERNEL = SCHEDCTL_FLAG_KERNEL;
    }
}

impl SchedctlFlags {
    /// Mirror the flag gate (`do_schedctl.c:17-21`).
    ///
    /// Unknown bits refuse with `EINVAL`, exactly as the kernel answers —
    /// the mirror never outruns the judge. Empty flags (plain
    /// registration, 06's shape) pass.
    pub fn validate_flags(raw: u32) -> Result<Self, i32> {
        Self::from_bits(raw).ok_or(EINVAL)
    }
}

/// Who schedules the target after the call (`proc.h:178-179`).
///
/// C reads a bare `p_scheduler` pointer in three shapes (null / self /
/// server); null and self both mean "the kernel schedules", so the shapes
/// are really two assignments (S-1: the kernel side already merged them
/// into `Option`, 11's domain; this enum merges them server-side).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulerAssignment {
    /// The kernel schedules the target. C: `p_scheduler = NULL` (`39`).
    Kernel,
    /// The caller schedules the target. C: `p_scheduler = caller` (`42`).
    Server,
}

impl SchedulerAssignment {
    /// Read the assignment from validated flags (`do_schedctl.c:28-42`).
    ///
    /// The KERNEL bit names [`SchedulerAssignment::Kernel`]; its absence
    /// names [`SchedulerAssignment::Server`] (plain registration).
    pub const fn from_flags(flags: SchedctlFlags) -> Self {
        if flags.contains(SchedctlFlags::KERNEL) {
            Self::Kernel
        } else {
            Self::Server
        }
    }
}

/// A schedctl call, packed but not sent (`do_schedctl.c:7-46`).
///
/// The packing half of `sys_schedctl(flags, endpoint, priority, quantum,
/// cpu)`: which target, which assignment, which numbers ride. Sending
/// stays caller-side (the loop holds IPC, 02); resolving the endpoint
/// (`isokendpt`, `23-24`) stays kernel-side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchedctlCall {
    /// Whose scheduler changes. C: `endpoint` — do_schedctl.c:23.
    pub target: Endpoint,
    /// Who schedules it after the call. C: the `28-42` branch.
    pub assignment: SchedulerAssignment,
    /// Queue rung (kernel branch only). C: `priority` — do_schedctl.c:32.
    pub priority: i32,
    /// Share in ms (kernel branch only). C: `quantum` — do_schedctl.c:33.
    pub quantum: i32,
    /// Home CPU (kernel branch only). C: `cpu` — do_schedctl.c:34.
    pub cpu: i32,
}

impl SchedctlCall {
    /// Claim a target for the caller (`do_schedctl.c:40-43`).
    ///
    /// Flags zero, numbers zero — 06's `sys_schedctl(0, ep, 0, 0, 0)`
    /// shape. The register branch never reads the numbers (`40-43`
    /// touches only endpoint and caller), so zero reads the same as
    /// "no numbers": placing zero and not reading coincide.
    pub const fn register(target: Endpoint) -> Self {
        Self {
            target,
            assignment: SchedulerAssignment::Server,
            priority: 0,
            quantum: 0,
            cpu: 0,
        }
    }

    /// Hand a target to the kernel, with numbers (`do_schedctl.c:28-39`).
    ///
    /// The kernel branch reads all three numbers into `sched_proc`
    /// (`37`, `-1` keeps — 09's `KEEP`); the assignment rides the flag.
    pub const fn to_kernel(target: Endpoint, priority: i32, quantum: i32, cpu: i32) -> Self {
        Self {
            target,
            assignment: SchedulerAssignment::Kernel,
            priority,
            quantum,
            cpu,
        }
    }

    /// Pack the wire order (`message.rs:821-831`).
    ///
    /// Field order is the contract: flags, endpoint, priority, quantum,
    /// cpu. The flag bit follows the assignment (`KERNEL` iff
    /// [`SchedulerAssignment::Kernel`]) — the packer cannot disagree
    /// with the namer.
    pub const fn wire(&self) -> MessLsysKrnSchedctl {
        MessLsysKrnSchedctl {
            flags: match self.assignment {
                SchedulerAssignment::Kernel => SCHEDCTL_FLAG_KERNEL,
                SchedulerAssignment::Server => 0,
            },
            endpoint: self.target.0,
            priority: self.priority,
            quantum: self.quantum,
            cpu: self.cpu,
            _padding: [0; 36],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ep(value: i32) -> Endpoint {
        Endpoint(value)
    }

    #[test]
    fn test_flags_validate() {
        // Empty flags (plain registration, 06's shape) pass (`17` false).
        assert_eq!(SchedctlFlags::validate_flags(0), Ok(SchedctlFlags::empty()));
        // The one defined bit passes (`com.h:449`).
        assert_eq!(SchedctlFlags::validate_flags(1), Ok(SchedctlFlags::KERNEL));
        // Unknown bits refuse, exactly as the kernel answers (`17-21`).
        assert_eq!(SchedctlFlags::validate_flags(2), Err(EINVAL));
        assert_eq!(SchedctlFlags::validate_flags(0xFFFF_FFFE), Err(EINVAL));
    }

    #[test]
    fn test_assignment_from_flags() {
        // The KERNEL bit names the kernel (`28`); its absence names the
        // caller (`40-42`) — two shapes, two assignments, no third.
        assert_eq!(
            SchedulerAssignment::from_flags(SchedctlFlags::KERNEL),
            SchedulerAssignment::Kernel
        );
        assert_eq!(
            SchedulerAssignment::from_flags(SchedctlFlags::empty()),
            SchedulerAssignment::Server
        );
    }

    #[test]
    fn test_register_call() {
        // Registration carries no numbers (`40-43` reads none): flags
        // zero, numbers zero, assignment the caller.
        let call = SchedctlCall::register(ep(7));
        assert_eq!(call.assignment, SchedulerAssignment::Server);
        assert_eq!((call.priority, call.quantum, call.cpu), (0, 0, 0));
        let wire = call.wire();
        assert_eq!(wire.flags, 0);
        assert_eq!(wire.endpoint, 7);
    }

    #[test]
    fn test_kernel_call() {
        // The kernel branch carries all three numbers (`32-34`); the
        // assignment rides the flag (`28`).
        let call = SchedctlCall::to_kernel(ep(7), 8, 200, 1);
        assert_eq!(call.assignment, SchedulerAssignment::Kernel);
        let wire = call.wire();
        assert_eq!(wire.flags, SCHEDCTL_FLAG_KERNEL);
        assert_eq!((wire.priority, wire.quantum, wire.cpu), (8, 200, 1));
    }

    #[test]
    fn test_wire_order() {
        // Field order is the contract (`message.rs:821-831`): flags,
        // endpoint, priority, quantum, cpu — the packer follows it.
        let wire = SchedctlCall::to_kernel(ep(-3), -1, -1, -1).wire();
        assert_eq!(wire.flags, SCHEDCTL_FLAG_KERNEL);
        assert_eq!(wire.endpoint, -3);
        assert_eq!((wire.priority, wire.quantum, wire.cpu), (-1, -1, -1));
    }
}
