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

/// What the service must perform after one end closes.
///
/// C: `pty_master_close` (`pty.c:225-246`) has two branches — with a live
/// slave it marks the pseudo side closed and hangs the line up
/// (`c_ospeed = B0` makes slave reads see EOF; `sigchar(SIGHUP)` notifies
/// the slave's session); without one it resets the pair. `pty_slave_close`
/// (`pty.c:775-796`) replies pending master transfers (buffer module) and
/// resets when the master had already closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseEffect {
    /// Only a flag changed; nothing observable.
    Quiet,
    /// The slave survives: make slave reads see EOF (speed B0) and raise
    /// SIGHUP on the slave's session. Both are service actions.
    SlaveHangup,
    /// Both sides are done and the pair reset; the service clears the
    /// Unix98 node when the pair had one.
    PairReset,
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

    /// Close one end; returns what the service must perform.
    ///
    /// C: `pty_master_close` (`pty.c:225-246`): with a live slave (active
    /// and not already closed) the master marks only `PTY_CLOSED` and
    /// hangs the line — EOF plus SIGHUP on the slave; otherwise the pair
    /// resets outright. `pty_slave_close` (`pty.c:775-796`) does nothing
    /// without an active master, resets when the master had already
    /// closed, and merely marks `TTY_CLOSED` otherwise.
    pub fn close(&mut self, end: PairEnd) -> CloseEffect {
        match end {
            PairEnd::Master => {
                if self.flags & (TTY_ACTIVE | TTY_CLOSED) != TTY_ACTIVE {
                    self.flags = 0;
                    CloseEffect::PairReset
                } else {
                    self.flags |= PTY_CLOSED;
                    CloseEffect::SlaveHangup
                }
            }
            PairEnd::Slave => {
                if self.flags & PTY_ACTIVE == 0 {
                    return CloseEffect::Quiet;
                }
                if self.flags & PTY_CLOSED != 0 {
                    self.flags = 0;
                    CloseEffect::PairReset
                } else {
                    self.flags |= TTY_CLOSED;
                    CloseEffect::Quiet
                }
            }
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
        // Master close with a live slave: hang the slave up, no reset yet
        // (pty.c:240-244 — only PTY_CLOSED is set).
        assert_eq!(pair.close(PairEnd::Master), CloseEffect::SlaveHangup);
        assert!(!pair.is_free());
        // The surviving slave's close spends the pair.
        assert_eq!(pair.close(PairEnd::Slave), CloseEffect::PairReset);
        assert!(pair.is_free());
        assert_eq!(pair.flags(), 0);
    }

    #[test]
    fn test_master_close_resets_when_slave_gone() {
        // Without a live slave, master close resets the pair outright
        // (pty.c:234-236).
        let mut pair = PairState::new();
        pair.clone_master();
        assert_eq!(pair.close(PairEnd::Master), CloseEffect::PairReset);
        assert!(pair.is_free());
        assert_eq!(pair.flags(), 0);
    }

    #[test]
    fn test_slave_close_without_master_is_quiet() {
        // C: `!(pp->state & PTY_ACTIVE) → return 0` — no flags change
        // (pty.c:778-780).
        let mut pair = PairState::new();
        assert_eq!(pair.close(PairEnd::Slave), CloseEffect::Quiet);
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
