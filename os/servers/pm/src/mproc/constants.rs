//! PM private constant definitions.
//!
//! These constants are private to the PM service and should not be in minix-types.
//!
//! # Why in PM crate?
//!
//! 1. **Separation of concerns**: PID range is PM's private logic
//! 2. **Microkernel principle**: Other services don't need to know PM's PID generation rules
//!
//! # Minix3 Source Mapping
//!
//! ```c
//! // minix3/minix/servers/pm/const.h
//! #define NR_PIDS    30000    // Maximum PID
//! #define INIT_PID   1        // init process PID
//! #define NO_PID     0        // Invalid PID
//! #define NO_TRACER  0        // No tracer (slot 0 is PM, never a tracer)
//! ```

use minix_types::Pid;

/// Maximum PID.
///
/// Minix3 definition: `#define NR_PIDS 30000`
///
/// PID range: 2 ~ 30000 (INIT_PID+1 to NR_PIDS)
pub const NR_PIDS: Pid = 30000;

/// init process PID.
///
/// Minix3 definition: `#define INIT_PID 1`
///
/// PID 1 is the init process, will not be reallocated
pub const INIT_PID: Pid = 1;

/// Invalid PID.
///
/// Minix3 definition: `#define NO_PID 0`
///
/// Used to indicate invalid or unset PID
pub const NO_PID: Pid = 0;

/// No tracer index.
///
/// Minix3 definition: `#define NO_TRACER 0`
///
/// Note: In Minix3, NO_TRACER = 0 because process table slot 0 is PM, a
/// system process that never calls PTRACE, so the sentinel never collides
/// with a real tracer (INIT is slot 11, `INIT_PROC_NR`, not slot 0).
///
/// This differs from minix-types' NO_TRACER = UserSlot(usize::MAX),
/// but the semantics are the same: "no tracer".
pub const NO_TRACER_INDEX: usize = 0;
