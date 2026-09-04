//! Core device-manager structures (doc 03-devm-structs).
//!
//! C: `minix3/minix/servers/devman/devman.h` (108 lines) —
//! `devman_device` / `devman_inode` / `devman_event` /
//! `devman_event_inode` / `devman_static_info_inode` /
//! `devman_inode_type` / `devman_read_fn`, plus constants
//! (`DEVMAN_STRING_LEN`, state values) and two deliberate omissions
//! (`devman_device_file`, `DEVMAN_DEFAULT_MODE` — dead in C, see below).
//!
//! [ARCH:A-2] C `malloc`/`TAILQ` become owned `String`/`Vec`/`Option` under
//! the single-threaded event loop (same model as 01/02).
//! [ARCH:A-5] C `int` ids/counters become `DeviceId(u32)` / explicit `u32`
//! counters; `dev_id` overflow is `ENOMEM` (C wraps, undefined).

use alloc::string::String;
use alloc::vec::Vec;
use minix_types::{Endpoint, Errno};

/// C: `DEVMAN_STRING_LEN 128` (`devman.h:42`) — event/static-info text cap
/// (bytes, NUL included on the wire).
pub const DEVMAN_STRING_LEN: usize = 128;

/// C: `DEVMAN_DEVICE_UNBOUND/BOUND/ZOMBIE 0/1/2` (`devman.h:89-91`,
/// `#define`d mid-struct — file scope, applies everywhere).
/// Values locked by `state_values_match_c`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum DeviceState {
    Unbound = 0,
    Bound = 1,
    Zombie = 2,
}

impl DeviceState {
    /// C `int` ↔ state. Unknown values are `EINVAL` (C would merely store
    /// them; Rust refuses — illegal states unrepresentable).
    pub fn from_i32(v: i32) -> Result<Self, Errno> {
        match v {
            0 => Ok(DeviceState::Unbound),
            1 => Ok(DeviceState::Bound),
            2 => Ok(DeviceState::Zombie),
            _ => Err(Errno::EINVAL),
        }
    }
}

/// C: `int dev_id` (`devman.h:83`, `devinfo.h:6`).
/// [ARCH:A-5] `u32` newtype: ids are never negative in practice
/// (`next_device_id` starts at 1, root is 0 — device.c:16/194).
/// `ROOT` is 0 (device.c:193 `root_dev.dev_id = 0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeviceId(pub u32);

impl DeviceId {
    pub const ROOT: DeviceId = DeviceId(0);
}

/// C: `struct devman_static_attribute` (`devinfo.h:14-17`, lib side) and
/// `struct devman_static_info_inode` (`devman.h:61-64`, server side) unified:
/// both are name→data text pairs; the file binding (`dev` back-pointer,
/// `read_fn`) is 06/07 business, not structure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    pub name: String,
    pub data: String,
    /// Framework/file binding of this attribute's file, set when 07
    /// materializes it (08 needs it to unregister on delete).
    /// Added for 07/08; 03 scan notes the additive extension.
    pub binding: Option<FileBinding>,
}

/// C: `struct devman_event` (`devman.h:66-69`) minus the list link
/// (queue membership is [`crate::event_queue`] business in 06 — here just
/// the text). Length-capped at construction: C `char data[128]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event(String);

impl Event {
    /// `data.len()` must fit `char[128]` incl. NUL (`< 128`); longer is
    /// `ENAMETOOLONG`. (06 decides truncate-vs-reject at call sites by
    /// pre-truncating; this constructor never silently truncates.)
    pub fn new(data: &str) -> Result<Self, Errno> {
        if data.len() >= DEVMAN_STRING_LEN {
            return Err(Errno::ENAMETOOLONG);
        }
        Ok(Event(String::from(data)))
    }

    pub fn text(&self) -> &str {
        self.0.as_str()
    }
}

/// C: `struct devman_inode` (`devman.h:75-80`) — per-file dispatch triple.
/// `read_fn`/`data` binding is 06/07 business; the framework inode number
/// is the 02 `Ino` handle. The C `inode_list` link is unneeded (lookup is
/// by tree position, not by list).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileBinding {
    /// 02-framework inode number of this file.
    pub ino: crate::vtreefs::Ino,
    /// 06-files cookie for static files (`None` for directories, which
    /// have no `FileEntry`). Added for 08 (`del_device` unregisters);
    /// 03 scan notes the additive extension.
    pub cookie: Option<usize>,
}

/// C: `struct devman_device` (`devman.h:82-107`) — the server-side device
/// object. Field notes (each verified against the header):
/// - `major`: **omitted** — write-only legacy (`device.c:193` sets -1 on
///   root; zero readers tree-wide). Omitting unread state is
///   behavior-neutral; see 03 §2.5.
/// - `ref_count`: explicit `u32` (transitions owned by 08; mirrors C).
/// - `owner`: `None` ≡ C `0` (root inits `owner = 0`, device.c:195).
/// - `name`: `None` for root (C never sets `root_dev.name` — BSS NULL);
///   `Some` once 07 decodes the wire name.
/// - `info`: parsed wire info, `None` until 07 fills it.
/// - `inode`: framework binding, `None` until inserted (04 sets it).
/// - `children`/`siblings`: `Vec` (parent→children only; C's `siblings`
///   back-link is traversal detail, unobservable).
/// - `infos`: attribute files on this device (07 fills).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub id: DeviceId,
    pub name: Option<String>,
    pub refcount: u32,
    pub state: DeviceState,
    pub owner: Option<Endpoint>,
    pub binding: Option<FileBinding>,
    pub parent: Option<DeviceId>,
    pub info: Option<crate::wire::ParsedDevice>,
    pub children: Vec<DeviceId>,
    pub attrs: Vec<Attribute>,
}

impl Device {
    /// Root shape (device.c:193-196 + BSS zeros made explicit):
    /// id 0, no name, refcount 0, UNBOUND, no owner, no info.
    /// (`major = -1` has no Rust counterpart — see `major` note above.)
    pub fn root() -> Self {
        Device {
            id: DeviceId::ROOT,
            name: None,
            refcount: 0,
            state: DeviceState::Unbound,
            owner: None,
            binding: None,
            parent: None,
            info: None,
            children: Vec::new(),
            attrs: Vec::new(),
        }
    }
}

// ── Deliberately unmodeled (dead in C, plan §5.5) ──
//
// - `struct devman_device_file` (devman.h:56-59): zero uses tree-wide.
// - `DEVMAN_DEFAULT_MODE` (devman.h:41): zero uses tree-wide.
// Referenced here (not modeled) so future grep finds the decision:
// `devman_device_file`, `DEVMAN_DEFAULT_MODE`.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_values_match_c() {
        // C: devman.h:89-91.
        assert_eq!(DeviceState::Unbound as i32, 0);
        assert_eq!(DeviceState::Bound as i32, 1);
        assert_eq!(DeviceState::Zombie as i32, 2);
        assert_eq!(DeviceState::from_i32(1).unwrap(), DeviceState::Bound);
        assert_eq!(DeviceState::from_i32(9), Err(Errno::EINVAL));
    }

    #[test]
    fn root_shape_matches_c_bss() {
        // C: device.c:193-196 + static zero-init.
        let r = Device::root();
        assert_eq!(r.id, DeviceId::ROOT);
        assert_eq!(r.name, None);
        assert_eq!(r.refcount, 0);
        assert_eq!(r.state, DeviceState::Unbound);
        assert_eq!(r.owner, None);
        assert_eq!(r.info, None);
    }

    #[test]
    fn event_text_capped() {
        // C: char data[DEVMAN_STRING_LEN].
        assert!(Event::new("ADD ./devices/usb/ 0x00000001").is_ok());
        let long = alloc::string::String::from_utf8(alloc::vec![b'x'; 128]).unwrap();
        assert_eq!(Event::new(long.as_str()), Err(Errno::ENAMETOOLONG));
        let edge = alloc::string::String::from_utf8(alloc::vec![b'x'; 127]).unwrap();
        assert_eq!(Event::new(edge.as_str()).unwrap().text(), edge);
    }
}
