//! Printer status: busy line, paper check, online retries.
//!
//! C correspondence: the status bits (`BUSY_STATUS 0x10`,
//! `NORMAL_STATUS 0x90`, `STATUS_MASK 0xB0`, `NO_PAPER 0x20`,
//! `ON_LINE 0x10`, `printer.c:46-52`), the retry budget
//! (`MAX_ONLINE_RETRIES 120`, about sixty seconds at half a second
//! each, `printer.c:52`), and the completion checks (`output_done`,
//! `printer.c:208-241`: offline means input-output error, out of
//! paper means try again).
//!
//! Port traffic stays in the service binary; this module owns the
//! pure reading half: what a status byte means and when waiting
//! gives up.

/// Bit marking the printer busy (`BUSY_STATUS`).
pub const STATUS_BUSY: u8 = 0x10;

/// Bits marking a healthy printer (`NORMAL_STATUS`).
pub const STATUS_NORMAL: u8 = 0x90;

/// Mask selecting the meaningful status bits (`STATUS_MASK`).
pub const STATUS_MASK: u8 = 0xB0;

/// Bit marking missing paper (`NO_PAPER`).
pub const STATUS_NO_PAPER: u8 = 0x20;

/// Bit marking the printer online (`ON_LINE`).
pub const STATUS_ONLINE: u8 = 0x10;

/// Online polls before giving up (`MAX_ONLINE_RETRIES`).
pub const MAX_ONLINE_RETRIES: u32 = 120;

/// What one status byte says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrinterState {
    /// Ready for the next byte.
    Ready,
    /// Still printing the previous byte.
    Busy,
    /// Out of paper.
    NoPaper,
    /// Offline.
    Offline,
}

/// Read a raw status byte (`printer_intr` / `output_done`,
/// `printer.c:208-241,345-423`): paper first, then online, then the
/// masked value against the healthy constant.
pub fn read_status(raw: u8) -> PrinterState {
    if raw & STATUS_NO_PAPER != 0 {
        PrinterState::NoPaper
    } else if raw & STATUS_ONLINE == 0 {
        PrinterState::Offline
    } else if raw & STATUS_MASK == STATUS_NORMAL {
        PrinterState::Ready
    } else {
        PrinterState::Busy
    }
}

/// Whether waiting continues: true while the printer is busy and
/// the retry budget is not exhausted.
pub fn keep_waiting(state: PrinterState, retries_used: u32) -> bool {
    state == PrinterState::Busy && retries_used < MAX_ONLINE_RETRIES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_bits_match_header() {
        assert_eq!(STATUS_BUSY, 0x10);
        assert_eq!(STATUS_NORMAL, 0x90);
        assert_eq!(STATUS_MASK, 0xB0);
        assert_eq!(STATUS_NO_PAPER, 0x20);
        assert_eq!(STATUS_ONLINE, 0x10);
        assert_eq!(MAX_ONLINE_RETRIES, 120);
    }

    #[test]
    fn test_status_byte_reads_in_priority_order() {
        assert_eq!(read_status(0x90), PrinterState::Ready);
        assert_eq!(read_status(0x10), PrinterState::Busy);
        assert_eq!(read_status(0xB0), PrinterState::NoPaper);
        assert_eq!(read_status(0x80), PrinterState::Offline);
    }

    #[test]
    fn test_waiting_stops_at_budget() {
        assert!(keep_waiting(PrinterState::Busy, 0));
        assert!(keep_waiting(PrinterState::Busy, 119));
        assert!(!keep_waiting(PrinterState::Busy, 120));
        assert!(!keep_waiting(PrinterState::Ready, 0));
    }
}
