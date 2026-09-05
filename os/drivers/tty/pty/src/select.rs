//! Master-side readiness: when a master read or write will not block.
//!
//! C correspondence: `select_try_pty` and `select_retry_pty`
//! (`pty.c:448-481`).

/// Operation bit: read readiness (shared vocabulary with the terminal).
pub const OP_READ: u32 = 0x01;
/// Operation bit: write readiness.
pub const OP_WRITE: u32 = 0x02;
/// Operation bit: error condition.
pub const OP_ERROR: u32 = 0x04;

/// Facts the readiness probe needs, owned by the caller.
///
/// The probe reads slave input counts, master suspend facts, buffered
/// output, close flags, and the input capacity; it writes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadinessFacts {
    /// Slave side closed: reads and writes report ready on error.
    pub slave_closed: bool,
    /// A master write is parked (it cannot block).
    pub write_parked: bool,
    /// Bytes already arrived on the parked write.
    pub write_arrived: bool,
    /// Slave input queue length.
    pub slave_queued: usize,
    /// Slave input queue capacity.
    pub slave_capacity: usize,
    /// A master read is parked.
    pub read_parked: bool,
    /// Bytes already arrived on the parked read.
    pub read_arrived: bool,
    /// Buffered slave output bytes.
    pub output_buffered: usize,
}

/// Probe master-side readiness now (never parks).
///
/// C: `select_try_pty` (`pty.c:448-471`). Writes are ready on slave-close,
/// on a parked write, or while the slave queue has room; reads are ready
/// on slave-close, on a parked read, or while output waits.
pub const fn probe(facts: &ReadinessFacts, ops: u32) -> u32 {
    let mut ready = 0;
    if ops & OP_WRITE != 0
        && (facts.slave_closed
            || facts.write_parked
            || facts.write_arrived
            || facts.slave_queued < facts.slave_capacity)
    {
        ready |= OP_WRITE;
    }
    if ops & OP_READ != 0
        && (facts.slave_closed
            || facts.read_parked
            || facts.read_arrived
            || facts.output_buffered > 0)
    {
        ready |= OP_READ;
    }
    ready
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idle() -> ReadinessFacts {
        ReadinessFacts {
            slave_closed: false,
            write_parked: false,
            write_arrived: false,
            slave_queued: 0,
            slave_capacity: 256,
            read_parked: false,
            read_arrived: false,
            output_buffered: 0,
        }
    }

    #[test]
    fn test_idle_master_can_write_but_not_read() {
        assert_eq!(probe(&idle(), OP_READ | OP_WRITE), OP_WRITE);
    }

    #[test]
    fn test_buffered_output_makes_read_ready() {
        let mut facts = idle();
        facts.output_buffered = 4;
        assert_eq!(probe(&facts, OP_READ), OP_READ);
    }

    #[test]
    fn test_closed_slave_reports_everything_ready_on_error() {
        let mut facts = idle();
        facts.slave_closed = true;
        assert_eq!(probe(&facts, OP_READ | OP_WRITE), OP_READ | OP_WRITE);
    }

    #[test]
    fn test_parked_calls_report_ready() {
        let mut facts = idle();
        facts.read_parked = true;
        facts.write_parked = true;
        assert_eq!(probe(&facts, OP_READ | OP_WRITE), OP_READ | OP_WRITE);
    }

    #[test]
    fn test_full_slave_queue_blocks_writes() {
        let mut facts = idle();
        facts.slave_queued = 256;
        assert_eq!(probe(&facts, OP_WRITE), 0);
    }
}
