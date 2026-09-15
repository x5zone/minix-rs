//! The two wires SCHED talks on: messages with the world, calls with the kernel.
//!
//! The loop (server.rs, 02) needs seven verbs — receive, send, get machine,
//! read the clock rate, register a takeover, send scheduling parameters, arm
//! the balance bell (`main.c:39,103,130` + `schedule.c:218,321,340`). This
//! module owns the seams and nothing else: which verbs exist, what they carry,
//! and how the real ends are assembled. The policy that decides when to call
//! them lives in server.rs (02); the wire packing half of the kernel calls
//! lives beside its siblings (schedctl.rs, schedule.rs).
//!
//! Two traits, not one, because C itself talks on two channels (`ipc_send`
//! versus `sys_*`, `minix3/minix/lib/libsys/`): messages come from peers,
//! calls go to the kernel, and a test double for one need not script the
//! other. The same split is the house style — VM pairs an IPC transport with
//! a kernel gateway (`os/servers/vm/src/ipc/transport.rs:120`), RS folds all
//! kernel calls into one `KernelApi` (`os/servers/rs/src/boot.rs:69`); this
//! module keeps both halves narrow enough that neither shape is missed.
//!
//! The real ends are final-shape but honest: every call is packed exactly as
//! production will send it and delegated to minix-sys's direct transports.
//! Until the user-space trap layer lands (edge E1), those report `EIO` —
//! the shape is committed, the power comes later (the VM campaign's
//! transport precedent, 02-stage-vm/todo.md T9).

use crate::kernel_api::schedctl::SchedctlCall;
use crate::kernel_api::schedule::Fanout;
use crate::sef::MachineInfo;
use minix_sys::ipc::{DirectTrapTransport, IpcStatus, TrapStatus};
use minix_sys::ipc::IpcTransport as SysIpcTransport;
use minix_sys::syscall::{perform_kernel_call, DirectKernelCallTransport};
use minix_types::{
    Endpoint, MessLsysKrnSchedule, MessLsysKrnSysGetinfo, MessLsysKrnSysSetalarm, Message,
    MessageUnion, GET_HZ, GET_MACHINE, SYS_GETINFO, SYS_SCHEDCTL, SYS_SETALARM, SYS_SCHEDULE,
};

/// Whether a receive status names a notification (C: `is_ipc_notify`,
/// com.h:92 — the call field equals `NOTIFY`; `CALL_NOTIFY` is 4,
/// minix-sys ipc.rs:53). minix-sys exposes the call number but not this
/// predicate, so it lives here where the loop (02) reads it.
pub const fn is_notify(status: IpcStatus) -> bool {
    status.call() == minix_sys::ipc::CALL_NOTIFY
}

/// The message wire: receive anything, send back.
///
/// Narrow on purpose — SCHED uses two of minix-sys's seven primitives
/// (`main.c:39` receives, `main.c:103` sends; no sendrec, no notify from
/// this side). Errors are Minix3 errno values (`i32`), the crate-wide
/// convention (table.rs:38's `SlotVerdict::errno` is the same contract).
pub trait IpcTransport {
    /// Wait for a message from anyone (C: `sef_receive_status(ANY, ...)`,
    /// `main.c:39`). Returns the message with its decoded status word —
    /// the status is how notifications and the kernel seal are recognized
    /// (02's `classify` and `noquantum_trust` consume the two halves).
    fn receive(&self) -> Result<(Message, IpcStatus), i32>;

    /// Send a message (C: `ipc_send`, `main.c:103` — the reply path).
    fn send(&self, to: Endpoint, message: &Message) -> Result<(), i32>;
}

/// Maps a trap failure to the crate's errno convention.
///
/// minix-sys reports trap failures as positive errno statuses
/// (`Err(TrapStatus(EIO))`, ipc.rs:531); the seam surfaces the same number
/// as an `i32`. The mapping lives in one place so edge E1 can refine it
/// without touching callers.
const fn trap_errno(status: TrapStatus) -> i32 {
    status.0
}

/// A zeroed message with a neutral source, ready for a receive or a call.
fn blank_message() -> Message {
    Message {
        m_source: Endpoint::ANY,
        m_type: 0,
        m_u: MessageUnion::zeroed(),
    }
}

/// The real message end: final call shape through minix-sys's direct trap
/// transport (`EIO` until edge E1 lands the trap bodies, ipc.rs:523-525).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KernelIpcTransport {
    inner: DirectTrapTransport,
}

impl KernelIpcTransport {
    /// Assemble the production end. No fields today; the type exists so the
    /// binary names its intent (this is the transport a real process gets).
    pub const fn new() -> Self {
        Self {
            inner: DirectTrapTransport,
        }
    }
}

impl Default for KernelIpcTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl IpcTransport for KernelIpcTransport {
    fn receive(&self) -> Result<(Message, IpcStatus), i32> {
        // C: sef_receive_status(ANY, &m_in, &ipc_status) — main.c:39.
        let mut message = blank_message();
        match self.inner.receive(Endpoint::ANY, &mut message) {
            Ok(status) => Ok((message, status)),
            Err(status) => Err(trap_errno(status)),
        }
    }

    fn send(&self, to: Endpoint, message: &Message) -> Result<(), i32> {
        // C: ipc_send(who_e, m_ptr) — main.c:103.
        self.inner.send(to, message).map_err(trap_errno)
    }
}

/// The kernel-call wire: the five calls SCHED makes downstairs.
///
/// One method per C call site, named for the C entry point. Errors are the
/// errno the kernel answered with (`i32`); a call that never got an answer
/// reports the transport's errno (`EIO` pre-E1). The wire packing for
/// `schedctl` and `schedule` reuses the mirrors that already exist
/// (schedctl.rs:137, schedule.rs:122-154); `get_machine`/`get_hz` pack the
/// GETMINFO pointer contract here, where the buffer is a local.
pub trait KernelApi {
    /// Read the machine (C: `sys_getmachine(&machine)`, `main.c:130`).
    fn get_machine(&mut self) -> Result<MachineInfo, i32>;

    /// Read the clock rate (C: `sys_hz()`, `sysutil.h:60` — a cached
    /// GET_HZ query, `com.h:333`).
    fn get_hz(&mut self) -> Result<u32, i32>;

    /// Register a takeover (C: `sys_schedctl`, `schedule.c:218`).
    fn schedctl(&mut self, call: &SchedctlCall) -> Result<(), i32>;

    /// Send scheduling parameters (C: `sys_schedule`, `schedule.c:321`).
    fn schedule(&mut self, fanout: &Fanout) -> Result<(), i32>;

    /// Arm the balance bell (C: `sys_setalarm`, `schedule.c:340,367`).
    fn setalarm(&mut self, ticks: u32) -> Result<(), i32>;
}

/// The safecopy buffer for GET_MACHINE (C: `struct machine`, type.h:122-131;
/// the kernel's copy source is the same layout,
/// `os/kernel/src/misc.rs:294-308`). GETMINFO carries a pointer, not a
/// payload (`os/kernel/src/misc.rs:1049` safecopies into caller memory), so
/// the buffer must match the C struct byte for byte — SCHED reads the first
/// two fields and lets the rest ride.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct MachineInfoBuf {
    /// C: `unsigned processors_count` — type.h:123.
    processors_count: u32,
    /// C: `unsigned bsp_id` — type.h:124.
    bsp_id: u32,
    /// C: `int padding` (used to be protected) — type.h:126.
    padding: i32,
    /// C: `int apic_enabled` — type.h:127.
    apic_enabled: i32,
    /// C: `phys_bytes acpi_rsdp` — type.h:128.
    acpi_rsdp: u64,
    /// C: `unsigned int board_id` — type.h:130.
    board_id: u32,
}

/// The real kernel end: packs each call into its wire message and sends it
/// through minix-sys's kernel-call protocol (`perform_kernel_call`,
/// syscall.rs:201 — the ENOTREADY retry ring is C's `_kernel_call` verbatim).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SysKernelApi {
    calls: DirectKernelCallTransport,
}

impl SysKernelApi {
    /// Assemble the production end.
    pub const fn new() -> Self {
        Self {
            calls: DirectKernelCallTransport,
        }
    }

    /// One kernel call: pack, send, decode the errno convention.
    ///
    /// `perform_kernel_call` returns the raw reply message type; the kernel
    /// writes failures as negative values into `m_type` (syscall.rs:119-124),
    /// so a negative reply is `-errno` and a non-negative one is `OK` —
    /// the same reading every minix-sys caller makes.
    fn call(&mut self, call_number: i32, pack: impl FnOnce(&mut Message)) -> Result<(), i32> {
        let mut message = blank_message();
        pack(&mut message);
        let reply = perform_kernel_call(&self.calls, call_number, &mut message, |_| {});
        if reply < 0 {
            Err(-reply)
        } else {
            Ok(())
        }
    }
}

impl Default for SysKernelApi {
    fn default() -> Self {
        Self::new()
    }
}

impl KernelApi for SysKernelApi {
    fn get_machine(&mut self) -> Result<MachineInfo, i32> {
        // C: sys_getmachine — getmachine.c packs GETMINFO(GET_MACHINE) with
        // a pointer to the caller's `machine` (kernel answers by safecopy,
        // misc.rs:1038-1050). The pointer cast itself needs no unsafe: the
        // write happens kernel-side, into this local, via the trap.
        let mut buf = MachineInfoBuf::default();
        let rv = self.call(SYS_GETINFO, |message| {
            message.m_u.m_lsys_krn_sys_getinfo = MessLsysKrnSysGetinfo {
                request: GET_MACHINE,
                endpt: 0,
                val_ptr: &mut buf as *mut MachineInfoBuf as u64,
                val_len: core::mem::size_of::<MachineInfoBuf>() as i32,
                val_ptr2: 0,
                val_len2_e: 0,
                _padding: [0; 20],
            };
        });
        // Read-back without unsafe: the kernel safecopied into `buf` before
        // the reply returned (misc.rs:1049); pre-E1 the call errs above and
        // the zeroed buffer is never read as data.
        rv.map(|()| MachineInfo {
            processors_count: buf.processors_count,
            bsp_id: buf.bsp_id,
        })
    }

    fn get_hz(&mut self) -> Result<u32, i32> {
        // C: sys_hz — sysutil.h:60 caches GETMINFO(GET_HZ) (kernel copies a
        // single i32, misc.rs:1033-1036). Fresh query each time: the cache
        // is libsys's convenience, not the contract (the balancer asks once
        // per boot, server.rs `init_scheduling`).
        let mut hz: u32 = 0;
        let rv = self.call(SYS_GETINFO, |message| {
            message.m_u.m_lsys_krn_sys_getinfo = MessLsysKrnSysGetinfo {
                request: GET_HZ,
                endpt: 0,
                val_ptr: &mut hz as *mut u32 as u64,
                val_len: core::mem::size_of::<u32>() as i32,
                val_ptr2: 0,
                val_len2_e: 0,
                _padding: [0; 20],
            };
        });
        rv.map(|()| hz)
    }

    fn schedctl(&mut self, call: &SchedctlCall) -> Result<(), i32> {
        // C: sys_schedctl — kernel_call.c packs the wire (the mirror's
        // order is the contract, schedctl.rs:131-149).
        self.call(SYS_SCHEDCTL, |message| {
            message.m_u.m_lsys_krn_schedctl = call.wire();
        })
    }

    fn schedule(&mut self, fanout: &Fanout) -> Result<(), i32> {
        // C: sys_schedule — sys_schedule.c packs endpoint, quantum,
        // priority, cpu, niced (the kernel reads them in that order,
        // doc 02 §2.7 row 6). The `KEEP` rendering lives on the Fanout.
        self.call(SYS_SCHEDULE, |message| {
            message.m_u.m_lsys_krn_schedule = MessLsysKrnSchedule {
                endpoint: fanout.endpoint.0,
                quantum: fanout.wire_quantum_ms(),
                priority: fanout.wire_priority(),
                cpu: fanout.wire_cpu(),
                niced: fanout.wire_niced(),
                _padding: [0; 36],
            };
        })
    }

    fn setalarm(&mut self, ticks: u32) -> Result<(), i32> {
        // C: sys_setalarm(balance_timeout, 0) — relative expiry
        // (`abs_time` 0), schedule.c:340. The reply's time_left/uptime
        // fields are news SCHED never reads.
        self.call(SYS_SETALARM, |message| {
            message.m_u.m_lsys_krn_sys_setalarm = MessLsysKrnSysSetalarm {
                exp_time: ticks as u64,
                time_left: 0,
                uptime: 0,
                abs_time: 0,
                _padding: [0; 28],
            };
        })
    }
}

#[cfg(test)]
pub(crate) mod mock {
    //! Test doubles for both wires: script the answers up front, inspect
    //! the recorded calls after. The loop tests (server.rs) drive whole
    //! turns through them without any kernel.

    use super::*;
    use core::cell::RefCell;

    /// Scripted message wire.
    ///
    /// Interior mutability because the trait takes shared `&self` (a real
    /// transport is stateless behind the trap); the cells are test-local
    /// and never cross threads (minix-sys's CannedTransport precedent,
    /// ipc.rs:562-566). An empty script receives `EIO`, so the loop's
    /// failure path is drivable without any setup.
    #[derive(Debug)]
    pub struct MockIpc {
        /// Receive script, handed out front-first.
        pub incoming: RefCell<std::vec::Vec<Result<(Message, IpcStatus), i32>>>,
        /// Every `send` lands here, in order.
        pub sent: RefCell<std::vec::Vec<(Endpoint, Message)>>,
        /// What `send` answers (default `Ok`; set `Err` to exercise the
        /// reply-failure rule, main.c:104-105).
        pub send_result: Result<(), i32>,
    }

    impl Default for MockIpc {
        fn default() -> Self {
            Self {
                incoming: RefCell::new(std::vec::Vec::new()),
                sent: RefCell::new(std::vec::Vec::new()),
                send_result: Ok(()),
            }
        }
    }

    impl MockIpc {
        /// Queue one incoming message with its status word.
        pub fn deliver(&self, message: Message, status: IpcStatus) {
            self.incoming.borrow_mut().push(Ok((message, status)));
        }
    }

    impl IpcTransport for MockIpc {
        fn receive(&self) -> Result<(Message, IpcStatus), i32> {
            // An empty script reads as a broken transport (EIO) — the
            // loop's failure path needs no setup. `remove(0)` hands out
            // the front answer by value (the entries are `Copy`).
            let mut script = self.incoming.borrow_mut();
            if script.is_empty() {
                return Err(minix_types::EIO);
            }
            script.remove(0)
        }

        fn send(&self, to: Endpoint, message: &Message) -> Result<(), i32> {
            self.sent.borrow_mut().push((to, *message));
            self.send_result
        }
    }

    /// Scripted kernel wire: each call pops its answer (empty script
    /// answers `OK`) and records the packed arguments.
    #[derive(Debug)]
    pub struct MockKernel {
        /// `get_machine` answer (the buffer read-back pre-E1 is zeroed on
        /// the real end; the mock hands the real numbers directly).
        pub machine: MachineInfo,
        /// `get_hz` answer.
        pub hz: u32,
        /// `schedctl` answers (raw errno, `0` = `OK`), front-first;
        /// empty means `Ok`.
        pub schedctl_rvs: RefCell<std::vec::Vec<i32>>,
        /// `schedule` answers (raw errno, `0` = `OK`), front-first;
        /// empty means `Ok`.
        pub schedule_rvs: RefCell<std::vec::Vec<i32>>,
        /// `setalarm` answers (raw errno, `0` = `OK`), front-first;
        /// empty means `Ok`.
        pub setalarm_rvs: RefCell<std::vec::Vec<i32>>,
        /// Recorded `schedctl` targets, in order.
        pub schedctl_calls: RefCell<std::vec::Vec<Endpoint>>,
        /// Recorded `schedule` wires, in order (already rendered: `KEEP`
        /// is `-1`, `niced` 0/1).
        pub schedule_calls: RefCell<std::vec::Vec<MessLsysKrnSchedule>>,
        /// Recorded `setalarm` tick counts, in order.
        pub setalarm_calls: RefCell<std::vec::Vec<u32>>,
    }

    impl MockKernel {
        /// A machine with `count` CPUs booting on `bsp`.
        pub fn new(count: u32, bsp: u32) -> Self {
            Self {
                machine: MachineInfo {
                    processors_count: count,
                    bsp_id: bsp,
                },
                hz: 60,
                schedctl_rvs: RefCell::new(std::vec::Vec::new()),
                schedule_rvs: RefCell::new(std::vec::Vec::new()),
                setalarm_rvs: RefCell::new(std::vec::Vec::new()),
                schedctl_calls: RefCell::new(std::vec::Vec::new()),
                schedule_calls: RefCell::new(std::vec::Vec::new()),
                setalarm_calls: RefCell::new(std::vec::Vec::new()),
            }
        }

        /// Queue a `schedule` answer (front-first).
        pub fn fail_next_schedule(&self, errno: i32) {
            self.schedule_rvs.borrow_mut().push(errno);
        }
    }

    impl Default for MockKernel {
        fn default() -> Self {
            Self::new(1, 0)
        }
    }

    fn next<T: Default>(script: &RefCell<std::vec::Vec<T>>) -> T {
        let mut queue = script.borrow_mut();
        if queue.is_empty() {
            T::default()
        } else {
            queue.remove(0)
        }
    }

    /// `0` is `OK`, anything else is the errno the kernel answered with.
    fn decode_errno(rv: i32) -> Result<(), i32> {
        if rv == 0 {
            Ok(())
        } else {
            Err(rv)
        }
    }

    impl KernelApi for MockKernel {
        fn get_machine(&mut self) -> Result<MachineInfo, i32> {
            Ok(self.machine)
        }

        fn get_hz(&mut self) -> Result<u32, i32> {
            Ok(self.hz)
        }

        fn schedctl(&mut self, call: &SchedctlCall) -> Result<(), i32> {
            self.schedctl_calls.borrow_mut().push(call.target);
            decode_errno(next(&self.schedctl_rvs))
        }

        fn schedule(&mut self, fanout: &Fanout) -> Result<(), i32> {
            self.schedule_calls.borrow_mut().push(MessLsysKrnSchedule {
                endpoint: fanout.endpoint.0,
                quantum: fanout.wire_quantum_ms(),
                priority: fanout.wire_priority(),
                cpu: fanout.wire_cpu(),
                niced: fanout.wire_niced(),
                _padding: [0; 36],
            });
            decode_errno(next(&self.schedule_rvs))
        }

        fn setalarm(&mut self, ticks: u32) -> Result<(), i32> {
            self.setalarm_calls.borrow_mut().push(ticks);
            decode_errno(next(&self.setalarm_rvs))
        }
    }
}
