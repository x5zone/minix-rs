//! Read dispatch: which file a read targets (doc 06-event-buf).
//!
//! C: `devman_inode { read_fn, data }` per file (devman.h:75-80) — the
//! `read_hook` (main.c:60-67) jumps through the file's own `read_fn`.
//! Rust keeps the shape (strategy-per-file) with two file kinds:
//! the events file (queue-backed) and static-info files (text-backed).
//! Attribute *content* management is 07's; this module is the mechanism.
//!
//! Reachability without globals leaking: the single process file table
//! lives in one `static` (`AssumeSyncCell<RefCell<…>>`, same single-thread
//! justification as VM's `AssumeSyncCell` statics — see `crate` docs).
//! Files hand out **indices** as `cbdata` cookies (validated on use);
//! no raw-pointer cookies, no unsafe aliasing. Reentrant dispatch panics
//! via `RefCell` instead of corrupting (cannot happen: the hook never
//! reenters the table).

use alloc::string::String;
use alloc::vec::Vec;
use core::cell::RefCell;
use minix_types::{AssumeSyncCell, Errno};

use crate::event_queue::EventQueue;

/// The two readable file kinds (C: `read_fn` targets).
pub enum FileKind {
    /// The `events` file: oldest-first queue drain (06 §2.4).
    Events(EventFile),
    /// A static-info file: fixed text + `\n` (06 §2.5).
    Static(StaticFile),
}

/// C: `event_inode_data` + `devman_event_read` state, per file.
pub struct EventFile {
    pub queue: EventQueue,
}

/// C: `devman_static_info_inode.data`, per file.
pub struct StaticFile {
    pub text: String,
}

/// One registered readable file.
pub struct FileEntry {
    pub kind: FileKind,
}

/// Process file table: register files, resolve cookies.
/// Append-only slot-wise (`Vec<Option<…>>` — `unregister` tombstones,
/// indices stay valid for process lifetime). Added for 08 (`del_device`
/// unregisters attribute files); 06 scan notes the additive extension.
#[derive(Default)]
pub struct FileStore {
    entries: Vec<Option<FileEntry>>,
}

impl FileStore {
    pub fn new() -> Self {
        FileStore {
            entries: Vec::new(),
        }
    }

    /// Register a file; returns its `cbdata` cookie (the index).
    /// (C: the cookie is `&inode`/`&event_inode`; an index is the
    /// validated equivalent — 06 §3.4.)
    pub fn register(&mut self, entry: FileEntry) -> Result<usize, Errno> {
        self.entries.try_reserve(1).map_err(|_| Errno::ENOMEM)?;
        self.entries.push(Some(entry));
        Ok(self.entries.len() - 1)
    }

    /// 08: release a cookie (tombstone; the slot never compacts, so live
    /// cookies keep resolving). Unknown/already-released → `EINVAL`.
    pub fn unregister(&mut self, cookie: usize) -> Result<(), Errno> {
        match self.entries.get_mut(cookie) {
            Some(slot @ Some(_)) => {
                *slot = None;
                Ok(())
            }
            _ => Err(Errno::EINVAL),
        }
    }

    pub fn get(&self, cookie: usize) -> Option<&FileEntry> {
        self.entries.get(cookie).and_then(|s| s.as_ref())
    }

    pub fn get_mut(&mut self, cookie: usize) -> Option<&mut FileEntry> {
        self.entries.get_mut(cookie).and_then(|s| s.as_mut())
    }

    /// 07 updates static text when attributes change (content management;
    /// mechanism stays here). Non-static cookies are `EINVAL`.
    pub fn set_static_text(&mut self, cookie: usize, text: String) -> Result<(), Errno> {
        match self.entries.get_mut(cookie) {
            Some(Some(FileEntry {
                kind: FileKind::Static(f),
            })) => {
                f.text = text;
                Ok(())
            }
            _ => Err(Errno::EINVAL),
        }
    }
}

static FILES: AssumeSyncCell<RefCell<FileStore>> =
    AssumeSyncCell::new(RefCell::new(FileStore { entries: Vec::new() }));

/// Run `f` against the process file table (the only accessor).
/// Reentrancy panics (`RefCell`), it cannot corrupt: dispatch never
/// reenters the table (hook → Buf only).
/// `pub(crate)`: 09's server assembly shares the table (06 scan notes it).
pub(crate) fn with_files<R>(f: impl FnOnce(&mut FileStore) -> R) -> R {
    // SAFETY: single-threaded event loop (crate docs) — no other thread
    // exists to alias; `RefCell` guards same-thread reentry at runtime.
    // `FILES` is never accessed except through this function.
    let cell = unsafe { &*FILES.as_ptr() };
    f(&mut cell.borrow_mut())
}

/// Register a file process-wide; returns its `cbdata` cookie.
/// (04/07 call this when creating framework inodes.)
pub fn register_file(entry: FileEntry) -> Result<usize, Errno> {
    with_files(|s| s.register(entry))
}

/// 06's half of `read_hook`: resolve the cookie and run the file's own
/// read (C: `d_inode->read_fn(…)` indirection, main.c:64-66).
/// Returns bytes filled, 0 on EOF, negative errno on error — the exact
/// `ReadHookFn` contract (01 §2.3). Unknown cookies read EOF (fail-closed;
/// C would dereference NULL — Rust refuses, 06 §3.4).
pub fn dispatch_read(buf: &mut [u8], len: usize, offset: i64, cookie: usize) -> i64 {
    let out: Vec<u8> = with_files(|s| match s.get_mut(cookie) {
        Some(FileEntry {
            kind: FileKind::Events(f),
        }) => f
            .queue
            .read_oldest(len, offset.max(0) as usize)
            .unwrap_or_default(),
        Some(FileEntry {
            kind: FileKind::Static(f),
        }) => EventQueue::read_static(&f.text, len, offset.max(0) as usize)
            .unwrap_or_default(),
        None => Vec::new(),
    });
    let take = out.len().min(buf.len()).min(len);
    buf[..take].copy_from_slice(&out[..take]);
    take as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_resolves_index() {
        let mut s = FileStore::new();
        let c = s
            .register(FileEntry {
                kind: FileKind::Static(StaticFile {
                    text: String::from("USB_DEV"),
                }),
            })
            .unwrap();
        assert!(s.get(c).is_some());
        assert!(s.get(c + 1).is_none());
        s.set_static_text(c, String::from("USB_INTF")).unwrap();
        assert_eq!(s.set_static_text(c + 1, String::from("x")), Err(Errno::EINVAL));
    }

    #[test]
    fn dispatch_end_to_end() {
        // Global table + hook-shaped call (mirrors 02 read() → hook path).
        let c = register_file(FileEntry {
            kind: FileKind::Static(StaticFile {
                text: String::from("hi"),
            }),
        })
        .unwrap();
        let mut buf = [0u8; 16];
        assert_eq!(dispatch_read(&mut buf, 16, 0, c), 3); // "hi\n"
        assert_eq!(&buf[..3], b"hi\n");
        assert_eq!(dispatch_read(&mut buf, 16, 0, c + 9999), 0); // unknown → EOF
    }
}
