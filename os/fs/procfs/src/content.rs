//! Content generators for static and per-process files (`root.c`,
//! `pid.c`, `cpuinfo.c`, `service.c`).
//!
//! Every generator renders into the caller's staging buffer and reads
//! only its arguments: kernel queries (process lists, memory statistics,
//! device maps) happen in the server layer, formatting happens here. That
//! split keeps every format string unit-testable without a running
//! kernel, and it mirrors how the C generators sit behind the read hook.

use super::buf::ProcBuf;

/// Clock frequency file (`root_hz`, `root.c:43-48`): one decimal number.
pub fn render_hz(buf: &mut ProcBuf, freq: u64) {
    buf.push_u64(freq);
    buf.push_str("\n");
}

/// Uptime file (`root_uptime`, `root.c:74-82`): ticks divided by the
/// frequency, printed with two fractional digits. The C code scales by
/// one hundred first so the remainder is hundredths of a second.
pub fn render_uptime(buf: &mut ProcBuf, ticks: u64, freq: u64) {
    if freq == 0 {
        buf.push_str("0.00\n");
        return;
    }
    let scaled = ticks.saturating_mul(100) / freq;
    buf.push_u64(scaled / 100);
    buf.push_str(".");
    let hundredths = scaled % 100;
    if hundredths < 10 {
        buf.push_str("0");
    }
    buf.push_u64(hundredths);
    buf.push_str("\n");
}

/// One load-average pair before formatting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadSample {
    /// Accumulated process ticks in the window.
    pub load: u64,
    /// Ticks spanned by the window.
    pub ticks: u64,
}

/// Load-average file (`root_loadavg`, `root.c:53-69`): three windows,
/// each scaled by one hundred and split into whole and fractional parts
/// with two digits. A window with no ticks reports zero rather than
/// dividing by zero.
pub fn render_loadavg(buf: &mut ProcBuf, windows: [LoadSample; 3]) {
    for (position, window) in windows.iter().enumerate() {
        if position > 0 {
            buf.push_str(" ");
        }
        if window.ticks == 0 {
            buf.push_str("0.00");
            continue;
        }
        let scaled = window.load.saturating_mul(100) / window.ticks;
        buf.push_u64(scaled / 100);
        buf.push_str(".");
        let hundredths = scaled % 100;
        if hundredths < 10 {
            buf.push_str("0");
        }
        buf.push_u64(hundredths);
    }
    buf.push_str("\n");
}

/// Kernel information file (`root_kinfo`, `root.c:87-96`).
pub fn render_kinfo(buf: &mut ProcBuf, process_count: u32, task_count: u32) {
    buf.push_u64(process_count as u64);
    buf.push_str(" ");
    buf.push_u64(task_count as u64);
    buf.push_str("\n");
}

/// Memory information file (`root_meminfo`, `root.c:101-111`).
pub fn render_meminfo(buf: &mut ProcBuf, page_size: u64, total: u64, free: u64, largest: u64, cached: u64) {
    buf.push_u64(page_size);
    for value in [total, free, largest, cached] {
        buf.push_str(" ");
        buf.push_u64(value);
    }
    buf.push_str("\n");
}

/// One driver-map row (`root_dmap`, `root.c:159-175`): unassigned slots
/// never reach the generator (the caller skips them).
pub fn render_dmap_row(buf: &mut ProcBuf, index: u32, label: &str, driver: u64) {
    buf.push_u64(index as u64);
    buf.push_str(" ");
    buf.push_str(label);
    buf.push_str(" ");
    buf.push_u64(driver);
    buf.push_str("\n");
}

/// One mount-table row (`root_mounts`, `root.c:213-227`).
pub fn render_mount_row(buf: &mut ProcBuf, from: &str, on: &str, fstype: &str, read_only: bool) {
    buf.push_str(from);
    buf.push_str(" on ");
    buf.push_str(on);
    buf.push_str(" type ");
    buf.push_str(fstype);
    buf.push_str(if read_only { " (ro)\n" } else { " (rw)\n" });
}

/// Process type letter (`pid_psinfo`, `pid.c:75-83`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessKind {
    /// Kernel task.
    Task,
    /// System server process.
    System,
    /// Ordinary user process.
    User,
}

/// Process state letter (`pid_psinfo`, `pid.c:90-97`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    /// Zombie.
    Zombie,
    /// Runnable.
    Running,
    /// Stopped.
    Stopped,
    /// Sleeping.
    Sleeping,
}

/// Fields printed for one process (`pid_psinfo`, `pid.c:114-132`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessFacts {
    /// Format version.
    pub version: u32,
    /// Process kind.
    pub kind: ProcessKind,
    /// Endpoint number.
    pub endpoint: i32,
    /// Process name (spaces already cut, `pid.c:111-112`).
    pub name: [u8; 16],
    /// Name length in use.
    pub name_len: usize,
    /// State letter.
    pub state: ProcessState,
    /// Endpoint blocked on, or zero for none.
    pub blocked_on: i32,
    /// Priority.
    pub priority: i32,
    /// User time.
    pub user_time: u64,
    /// System time.
    pub system_time: u64,
    /// Execution cycles.
    pub cycles: u64,
    /// Kernel IPC cycles.
    pub ipc_cycles: u64,
    /// Kernel call cycles.
    pub call_cycles: u64,
    /// Total memory.
    pub memory: u64,
    /// Nice value.
    pub nice: i32,
    /// Effective user identifier.
    pub uid: u16,
}

/// Process information file. Field order and spacing match the C format
/// string exactly, because the monitor tool parses positions.
pub fn render_psinfo(buf: &mut ProcBuf, facts: &ProcessFacts) {
    buf.push_u64(facts.version as u64);
    buf.push_str(" ");
    buf.push_str(match facts.kind {
        ProcessKind::Task => "t",
        ProcessKind::System => "s",
        ProcessKind::User => "u",
    });
    buf.push_str(" ");
    buf.push_i64(facts.endpoint as i64);
    buf.push_str(" ");
    buf.push_bytes(&facts.name[..facts.name_len.min(16)]);
    buf.push_str(" ");
    buf.push_str(match facts.state {
        ProcessState::Zombie => "Z",
        ProcessState::Running => "R",
        ProcessState::Stopped => "T",
        ProcessState::Sleeping => "S",
    });
    buf.push_str(" ");
    buf.push_i64(facts.blocked_on as i64);
    buf.push_str(" ");
    buf.push_i64(facts.priority as i64);
    buf.push_str(" ");
    buf.push_u64(facts.user_time);
    buf.push_str(" ");
    buf.push_u64(facts.system_time);
    buf.push_str(" ");
    buf.push_u64(facts.cycles);
    buf.push_str(" ");
    buf.push_u64(facts.ipc_cycles);
    buf.push_str(" ");
    buf.push_u64(facts.call_cycles);
    buf.push_str(" ");
    buf.push_u64(facts.memory);
    buf.push_str(" ");
    buf.push_i64(facts.nice as i64);
    buf.push_str(" ");
    buf.push_u64(facts.uid as u64);
    buf.push_str("\n");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buf::ProcBuf;

    fn buf() -> ProcBuf {
        ProcBuf::new(4096, 0, 4097)
    }

    #[test]
    fn test_hz_format() {
        let mut out = buf();
        render_hz(&mut out, 100);
        assert_eq!(out.result(), b"100\n");
    }

    #[test]
    fn test_uptime_two_digits() {
        let mut out = buf();
        // One hundred fifty ticks at one hundred hertz: one and a half
        // seconds, second digit kept.
        render_uptime(&mut out, 150, 100);
        assert_eq!(out.result(), b"1.50\n");
        // Ten ticks: leading zero in the fraction.
        let mut out = buf();
        render_uptime(&mut out, 10, 100);
        assert_eq!(out.result(), b"0.10\n");
        // Zero frequency never divides.
        let mut out = buf();
        render_uptime(&mut out, 10, 0);
        assert_eq!(out.result(), b"0.00\n");
    }

    #[test]
    fn test_loadavg_three_windows() {
        let mut out = buf();
        render_loadavg(
            &mut out,
            [
                LoadSample { load: 150, ticks: 100 },
                LoadSample { load: 0, ticks: 100 },
                LoadSample { load: 0, ticks: 0 },
            ],
        );
        assert_eq!(out.result(), b"1.50 0.00 0.00\n");
    }

    #[test]
    fn test_kinfo_and_meminfo() {
        let mut out = buf();
        render_kinfo(&mut out, 32, 8);
        assert_eq!(out.result(), b"32 8\n");
        let mut out = buf();
        render_meminfo(&mut out, 4096, 100, 40, 30, 10);
        assert_eq!(out.result(), b"4096 100 40 30 10\n");
    }

    #[test]
    fn test_mount_row_flags() {
        let mut out = buf();
        render_mount_row(&mut out, "/dev/ram", "/", "mfs", false);
        assert_eq!(out.result(), b"/dev/ram on / type mfs (rw)\n");
        let mut out = buf();
        render_mount_row(&mut out, "/dev/cd", "/mnt", "isofs", true);
        assert_eq!(out.result(), b"/dev/cd on /mnt type isofs (ro)\n");
    }

    #[test]
    fn test_psinfo_field_order() {
        let mut name = [0u8; 16];
        name[..4].copy_from_slice(b"init");
        let mut out = buf();
        render_psinfo(
            &mut out,
            &ProcessFacts {
                version: 1,
                kind: ProcessKind::System,
                endpoint: 4,
                name,
                name_len: 4,
                state: ProcessState::Sleeping,
                blocked_on: 0,
                priority: 10,
                user_time: 5,
                system_time: 6,
                cycles: 7,
                ipc_cycles: 8,
                call_cycles: 9,
                memory: 4096,
                nice: 0,
                uid: 0,
            },
        );
        assert_eq!(out.result(), b"1 s 4 init S 0 10 5 6 7 8 9 4096 0 0\n");
    }
}
