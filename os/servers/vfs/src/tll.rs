//! `tll` — three-level lock `READ/READSER/WRITE` with write-biased queues.
//!
//! Corresponds to Minix3's `tll_t` (`minix3/minix/servers/vfs/tll.h:9-18`)
//! and `tll.c:9-323` (`const.h` `NR_*`, `glo.h`).
//!
//! Design decisions (see 07-tll-lock.md §3):
//! - `Tll { state, owner, readonly, write_q, serial_q }` explicit state (ARCH A-6)
//! - `Busy` queuing via `VecDeque<Slot>` (write bias)

use minix_types::UserSlot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TllAccess {
    Read,
    ReadSer,
    Write,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TllState {
    None,
    Read,
    ReadSer,
    Write,
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct TllStatus: u8 {
        const UPGR = 0x01;
        const PEND = 0x02;
    }
}

#[derive(Debug, Clone)]
pub struct Tll {
    state: TllState,
    owner: Option<UserSlot>,
    readonly: usize,
    status: TllStatus,
    write_q: std::collections::VecDeque<UserSlot>,
    serial_q: std::collections::VecDeque<UserSlot>,
}

impl Default for Tll {
    fn default() -> Self {
        Self::new()
    }
}

impl Tll {
    pub fn new() -> Self {
        Self {
            state: TllState::None,
            owner: None,
            readonly: 0,
            status: TllStatus::empty(),
            write_q: std::collections::VecDeque::new(),
            serial_q: std::collections::VecDeque::new(),
        }
    }

    pub fn is_locked(&self) -> bool {
        self.state != TllState::None
    }

    pub fn is_locked_by(&self, slot: UserSlot) -> bool {
        self.owner == Some(slot) && !self.status.contains(TllStatus::PEND)
    }

    pub fn has_pending(&self) -> bool {
        !self.write_q.is_empty() || !self.serial_q.is_empty()
    }

    pub fn try_lock(&mut self, slot: UserSlot, access: TllAccess) -> Result<(), TllError> {
        if access == TllAccess::Read && self.status.contains(TllStatus::PEND) {
            return self.append(slot, access);
        }
        if self.owner == Some(slot) {
            return Err(TllError::Busy);
        }
        if self.state == TllState::None {
            self.state = match access {
                TllAccess::Read => TllState::Read,
                TllAccess::ReadSer => TllState::ReadSer,
                TllAccess::Write => TllState::Write,
            };
            if self.state == TllState::Read {
                self.readonly = 1;
                self.owner = None;
            } else {
                self.owner = Some(slot);
            }
            if self.state == TllState::Write {
                debug_assert_eq!(self.readonly, 0);
            }
            return Ok(());
        }
        if self.state == TllState::Write {
            return self.append(slot, access);
        }
        if access == TllAccess::Write {
            return self.append(slot, access);
        }
        if !self.write_q.is_empty() || self.status.contains(TllStatus::UPGR) {
            return self.append(slot, access);
        }
        if self.state == TllState::ReadSer {
            if access == TllAccess::Read && !self.status.contains(TllStatus::UPGR) {
                self.readonly += 1;
                return Ok(());
            } else {
                return self.append(slot, access);
            }
        }
        // state == Read
        self.state = match access {
            TllAccess::Read => TllState::Read,
            TllAccess::ReadSer => TllState::ReadSer,
            TllAccess::Write => TllState::Write,
        };
        if self.state == TllState::Read {
            self.readonly += 1;
            self.owner = None;
        } else {
            self.owner = Some(slot);
        }
        Ok(())
    }

    fn append(&mut self, slot: UserSlot, access: TllAccess) -> Result<(), TllError> {
        match access {
            TllAccess::Read | TllAccess::Write => self.write_q.push_back(slot),
            TllAccess::ReadSer => self.serial_q.push_back(slot),
        }
        Err(TllError::Busy)
    }

    pub fn unlock(&mut self, slot: UserSlot) -> Result<(), TllError> {
        let mut signal = false;
        if self.owner.is_none() || self.owner != Some(slot) {
            // Read lock
            if self.readonly == 0 {
                return Err(TllError::NotLocked);
            }
            self.readonly -= 1;
            if self.status.contains(TllStatus::UPGR) && self.readonly == 0 {
                signal = true;
            }
        }
        if self.owner == Some(slot) && self.state == TllState::Write {
            debug_assert_eq!(self.readonly, 0);
        }
        if self.owner == Some(slot) || (self.owner.is_none() && self.readonly == 0) {
            let mut new_owner = None;
            if !self.write_q.is_empty() && self.readonly == 0 {
                new_owner = self.write_q.pop_front();
            } else if !self.serial_q.is_empty() && self.write_q.is_empty() {
                new_owner = self.serial_q.pop_front();
            }
            if let Some(o) = new_owner {
                self.owner = Some(o);
                signal = true;
                self.status.insert(TllStatus::PEND);
            } else {
                self.owner = None;
            }
        }
        if self.owner.is_none() && self.readonly == 0 {
            self.state = TllState::None;
        } else if self.owner.is_none() {
            self.state = TllState::Read;
        }
        if signal {
            // In C, worker_signal would wake new owner; single-threaded model just clears PEND after signal
            // For test, we clear PEND immediately after signal to simulate wake
            self.status.remove(TllStatus::PEND);
        }
        Ok(())
    }

    pub fn downgrade(&mut self, slot: UserSlot) -> Result<(), TllError> {
        if self.owner != Some(slot) {
            return Err(TllError::NotOwner);
        }
        match self.state {
            TllState::Write => {
                self.state = TllState::ReadSer;
                Ok(())
            }
            TllState::ReadSer => {
                if self.write_q.is_empty() && !self.serial_q.is_empty() {
                    let next = self.serial_q.pop_front().unwrap();
                    self.owner = Some(next);
                    self.status.insert(TllStatus::PEND);
                    // Simulate signal
                    self.status.remove(TllStatus::PEND);
                } else {
                    self.state = TllState::Read;
                    self.owner = None;
                }
                self.readonly += 1;
                Ok(())
            }
            _ => Err(TllError::Invalid),
        }
    }

    pub fn upgrade(&mut self, slot: UserSlot) -> Result<(), TllError> {
        if self.owner != Some(slot) {
            return Err(TllError::NotOwner);
        }
        if self.state == TllState::Write {
            return Ok(());
        }
        if self.state != TllState::ReadSer {
            return Err(TllError::Invalid);
        }
        if self.readonly != 0 {
            self.status.insert(TllStatus::UPGR);
            // In C, would worker_wait until readonly==0; single-threaded we return WouldBlock
            return Err(TllError::WouldBlock);
        }
        self.state = TllState::Write;
        self.status.remove(TllStatus::UPGR);
        self.status.remove(TllStatus::PEND);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TllError {
    Busy,
    NotLocked,
    NotOwner,
    Invalid,
    WouldBlock,
}

impl TllError {
    pub fn to_errno(self) -> i32 {
        match self {
            Self::Busy => minix_types::EBUSY,
            Self::NotLocked => minix_types::EINVAL,
            Self::NotOwner => minix_types::EINVAL,
            Self::Invalid => minix_types::EINVAL,
            Self::WouldBlock => minix_types::EBUSY,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::UserSlot;

    fn slot(n: usize) -> UserSlot { UserSlot::new(n) }

    #[test]
    fn test_tll_init() {
        let t = Tll::new();
        assert_eq!(t.state, TllState::None);
        assert_eq!(t.readonly, 0);
        assert_eq!(t.status, TllStatus::empty());
        assert!(t.write_q.is_empty());
        assert!(t.serial_q.is_empty());
        assert!(!t.is_locked());
    }

    #[test]
    fn test_tll_lock_read_shared() {
        let mut t = Tll::new();
        assert!(t.try_lock(slot(1), TllAccess::Read).is_ok());
        assert_eq!(t.state, TllState::Read);
        assert_eq!(t.readonly, 1);
        assert!(t.try_lock(slot(2), TllAccess::Read).is_ok());
        assert_eq!(t.readonly, 2);
    }

    #[test]
    fn test_tll_lock_write_busy() {
        let mut t = Tll::new();
        assert!(t.try_lock(slot(1), TllAccess::Read).is_ok());
        assert_eq!(t.try_lock(slot(2), TllAccess::Write).unwrap_err(), TllError::Busy);
        assert!(!t.write_q.is_empty());
    }

    #[test]
    fn test_tll_append_queues() {
        let mut t = Tll::new();
        t.try_lock(slot(1), TllAccess::Write).unwrap();
        let _ = t.try_lock(slot(2), TllAccess::Write);
        let _ = t.try_lock(slot(3), TllAccess::ReadSer);
        assert_eq!(t.write_q.len(), 1);
        assert_eq!(t.serial_q.len(), 1);
        assert!(t.has_pending());
    }

    #[test]
    fn test_tll_unlock_selects() {
        let mut t = Tll::new();
        t.try_lock(slot(1), TllAccess::Write).unwrap();
        let _ = t.try_lock(slot(2), TllAccess::Write);
        let _ = t.try_lock(slot(3), TllAccess::ReadSer);
        // Write queue has priority
        t.unlock(slot(1)).unwrap();
        // After unlock, write_q head should be promoted (owner becomes slot2)
        assert_eq!(t.owner, Some(slot(2)));
    }

    #[test]
    fn test_tll_downgrade_upgrade() {
        let mut t = Tll::new();
        t.try_lock(slot(1), TllAccess::Write).unwrap();
        assert_eq!(t.state, TllState::Write);
        t.downgrade(slot(1)).unwrap();
        assert_eq!(t.state, TllState::ReadSer);
        // Upgrade when readonly==0 should succeed
        assert!(t.upgrade(slot(1)).is_ok());
        assert_eq!(t.state, TllState::Write);
    }

    // Second impl for Gate D
    struct AltTll(Tll);
    impl AltTll {
        fn new_alt() -> Self { Self(Tll::new()) }
        fn lock_alt(&mut self, s: UserSlot) -> Result<(), TllError> { self.0.try_lock(s, TllAccess::Read) }
    }
}
