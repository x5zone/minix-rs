//! Pseudo-terminal pairs: master/slave roles and pair state.
//!
//! C correspondence: the pair structure `pty_t` with its state flags,
//! `get_free_pty`, `pty_master_open`, `pty_reset`, `pty_master_close` in
//! `minix3/minix/drivers/tty/pty/pty.c:46-260`, and the pair count
//! `NR_PTYS 32` in `minix3/minix/include/minix/config.h:46`.

/// Number of pseudo-terminal pairs.
///
/// C: `NR_PTYS 32` (`config.h:46`).
pub const PAIR_COUNT: usize = 32;

/// State flag: terminal side open and active.
///
/// C: `TTY_ACTIVE 0x01` (`pty.c:82`).
pub const TTY_ACTIVE: u8 = 0x01;
/// State flag: pseudo side open and active.
///
/// C: `PTY_ACTIVE 0x02` (`pty.c:83`).
pub const PTY_ACTIVE: u8 = 0x02;
/// State flag: terminal side has closed.
///
/// C: `TTY_CLOSED 0x04` (`pty.c:84`).
pub const TTY_CLOSED: u8 = 0x04;
/// State flag: pseudo side has closed.
///
/// C: `PTY_CLOSED 0x08` (`pty.c:85`).
pub const PTY_CLOSED: u8 = 0x08;
/// State flag: Unix98 pair (allocated through the clone device).
///
/// C: `PTY_UNIX98 0x10` (`pty.c:86`).
pub const PTY_UNIX98: u8 = 0x10;
/// State flag: packet mode (minimal receipt-mode support).
///
/// C: `PTY_PKTMODE 0x20` (`pty.c:87`).
pub const PTY_PACKET_MODE: u8 = 0x20;

/// Which end of a pair a request targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairEnd {
    /// Master end (controller program side).
    Master,
    /// Slave end (shell side, a full terminal line).
    Slave,
}

/// Outcome of opening a master end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MasterOpen {
    /// Classic master opened (already-indexed pair).
    Opened,
    /// Clone device allocated this pair index (Unix98).
    Cloned(usize),
}

/// One pair's state: flags only; buffers live in their own types.
///
/// C: the `state` field of `pty_t` (`pty.c:46-56`). Read/write suspend
/// slots live with the buffer module; select watches live with the select
/// module; this type owns the open/close/reset rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PairState {
    flags: u8,
}

impl PairState {
    /// Fresh pair: both ends closed, classic kind.
    pub const fn new() -> PairState {
        PairState { flags: 0 }
    }

    /// Raw flags (test inspection and service-crate wiring).
    pub const fn flags(self) -> u8 {
        self.flags
    }

    /// True when neither end is active (allocatable).
    pub const fn is_free(self) -> bool {
        self.flags & (PTY_ACTIVE | TTY_ACTIVE) == 0
    }

    /// True for a Unix98 pair.
    pub const fn is_unix98(self) -> bool {
        self.flags & PTY_UNIX98 != 0
    }

    /// True while packet mode is on.
    pub const fn packet_mode(self) -> bool {
        self.flags & PTY_PACKET_MODE != 0
    }

    /// Open the master end of a classic pair.
    ///
    /// C: the non-clone branch of `pty_master_open` (`pty.c:163-184`):
    /// the slave may precede the master, but the master opens only once.
    pub fn open_master(&mut self) -> Result<MasterOpen, i32> {
        if self.is_unix98() {
            return Err(-eio_code());
        }
        if self.flags & PTY_ACTIVE != 0 {
            return Err(-eio_code());
        }
        self.flags |= PTY_ACTIVE;
        Ok(MasterOpen::Opened)
    }

    /// Allocate this pair through the clone device (Unix98).
    ///
    /// C: the clone branch of `pty_master_open` (`pty.c:141-161`): the
    /// caller checks pair availability first (see [`PairTable`]), then
    /// marks Unix98 and active. Filesystem clearance happens in the
    /// service crate through the [`super::ptyfs::PtyFs`] trait.
    pub fn clone_master(&mut self) {
        self.flags |= PTY_UNIX98 | PTY_ACTIVE;
    }

    /// Open the slave end.
    ///
    /// C: `pty_slave_mayopen`/`pty_slave_open` (`pty.c:736-773`): classic
    /// slaves open freely; a Unix98 slave needs its filesystem node (the
    /// service crate checks that before calling).
    pub fn open_slave(&mut self) {
        self.flags |= TTY_ACTIVE;
    }

    /// Close one end; both closed resets the pair.
    ///
    /// C: `pty_master_close` plus `pty_reset` (`pty.c:205-260`): closing
    /// marks the end closed, and the last close clears everything
    /// (including the filesystem node for Unix98 pairs, handled by the
    /// service crate on observing a fresh [`PairState::is_free`]).
    pub fn close(&mut self, end: PairEnd) {
        match end {
            PairEnd::Master => {
                self.flags &= !PTY_ACTIVE;
                self.flags |= PTY_CLOSED;
            }
            PairEnd::Slave => {
                self.flags &= !TTY_ACTIVE;
                self.flags |= TTY_CLOSED;
            }
        }
        if self.flags & (PTY_ACTIVE | TTY_ACTIVE) == 0 {
            self.flags = 0;
        }
    }

    /// Set or clear packet mode.
    pub fn set_packet_mode(&mut self, on: bool) {
        if on {
            self.flags |= PTY_PACKET_MODE;
        } else {
            self.flags &= !PTY_PACKET_MODE;
        }
    }
}

impl Default for PairState {
    fn default() -> Self {
        PairState::new()
    }
}

/// Table of all pairs with clone allocation.
///
/// C: `pty_table[NR_PTYS]` plus `get_free_pty` (`pty.c:97,121-136`).
#[derive(Debug, Clone)]
pub struct PairTable {
    pairs: [PairState; PAIR_COUNT],
}

impl PairTable {
    /// All pairs fresh.
    pub const fn new() -> PairTable {
        PairTable {
            pairs: [PairState::new(); PAIR_COUNT],
        }
    }

    /// Borrow one pair by index.
    pub fn get(&self, index: usize) -> Option<&PairState> {
        self.pairs.get(index)
    }

    /// Mutably borrow one pair by index.
    pub fn get_mut(&mut self, index: usize) -> Option<&mut PairState> {
        self.pairs.get_mut(index)
    }

    /// Index of the first free pair, if any.
    ///
    /// C: `get_free_pty` returns null when full; the clone caller answers
    /// "try again" (`pty.c:147-148`).
    pub fn first_free(&self) -> Option<usize> {
        self.pairs.iter().position(|pair| pair.is_free())
    }
}

impl Default for PairTable {
    fn default() -> Self {
        PairTable::new()
    }
}

/// Error for double master open or kind mismatch.
const fn eio_code() -> i32 {
    minix_types::EIO
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fresh_pair_is_free_and_classic() {
        let pair = PairState::new();
        assert!(pair.is_free());
        assert!(!pair.is_unix98());
    }

    #[test]
    fn test_master_opens_once_then_refuses() {
        let mut pair = PairState::new();
        assert_eq!(pair.open_master(), Ok(MasterOpen::Opened));
        assert!(!pair.is_free());
        assert!(pair.open_master().is_err());
    }

    #[test]
    fn test_slave_may_precede_master() {
        let mut pair = PairState::new();
        pair.open_slave();
        assert!(!pair.is_free());
        assert_eq!(pair.open_master(), Ok(MasterOpen::Opened));
    }

    #[test]
    fn test_last_close_resets_everything() {
        let mut pair = PairState::new();
        pair.clone_master();
        pair.open_slave();
        pair.close(PairEnd::Master);
        assert!(!pair.is_free());
        pair.close(PairEnd::Slave);
        assert!(pair.is_free());
        assert_eq!(pair.flags(), 0);
    }

    #[test]
    fn test_packet_mode_toggles() {
        let mut pair = PairState::new();
        pair.set_packet_mode(true);
        assert!(pair.packet_mode());
        pair.set_packet_mode(false);
        assert!(!pair.packet_mode());
    }

    #[test]
    fn test_table_allocates_first_free() {
        let mut table = PairTable::new();
        assert_eq!(table.first_free(), Some(0));
        table.get_mut(0).unwrap().clone_master();
        assert_eq!(table.first_free(), Some(1));
        assert_eq!(PAIR_COUNT, 32);
    }
}
