//! The libvboxfs connect face: map a shared folder, and the path object.
//!
//! C correspondence: `minix3/minix/lib/libvboxfs/vboxfs.c` (`vboxfs_init`
//! `:60-98`, `vboxfs_cleanup` `:100-114`, the `sffs_table` `:12-33`) and
//! `path.c` (`vboxfs_set_path`/`vboxfs_get_path_size`). The operation table
//! (open/read/dir/attr families in `dir.c`/`file.c`/`link.c`/`attr.c`) is
//! the next slice; the connect flow here is what `vbfs.c`'s `init` runs
//! before the framework takes over.

#[allow(unused_imports)]
use alloc::vec;
use alloc::vec::Vec;
use minix_types::Errno;

use crate::vbox::{Backdoor, Param, VBOX_DIR_OUT};

/// `PATH_MAX` (`minix3/sys/sys/syslimits.h:64`) — the `vboxfs_path_t` data
/// capacity the length check refuses past.
pub const PATH_MAX: usize = 1024;

/// HGCM function numbers (`minix3/minix/lib/libvboxfs/vboxfs.h:9-19`).
pub const VBOXFS_CALL_UNMAP_FOLDER: u32 = 13;
pub const VBOXFS_CALL_SET_UTF8: u32 = 16;
pub const VBOXFS_CALL_MAP_FOLDER: u32 = 17;

/// The HGCM service name the connection opens (`vboxfs.c:70`).
pub const VBOX_SHARED_FOLDERS: &str = "VBoxSharedFolders";

/// The path separator parameter (`vboxfs.c:81`).
pub const PATH_SEPARATOR: u32 = 0x2f;

/// `vboxfs_path_t` (`vboxfs.h:40-44`): `u16 size; u16 len; char
/// data[PATH_MAX]`. The model keeps the used bytes (always NUL-terminated,
/// exactly what C's `strcpy` leaves); [`VboxfsPath::wire_size`] answers the
/// byte count the parameter grants.
pub struct VboxfsPath {
    /// `len` — the string length without the terminator.
    pub len: u16,
    /// `data` — the NUL-terminated string bytes.
    pub data: Vec<u8>,
}

impl VboxfsPath {
    /// C `vboxfs_set_path` (`path.c:11-30`): copy with terminator, refuse
    /// over-long names. The C FIXME about missing UTF-8 conversion stays a
    /// byte copy here too — same behavior, same limitation.
    pub fn set_path(name: &str) -> Result<Self, Errno> {
        if name.len() >= PATH_MAX {
            return Err(Errno::ENAMETOOLONG);
        }
        let mut data = Vec::with_capacity(name.len() + 1);
        data.extend_from_slice(name.as_bytes());
        data.push(0);
        Ok(Self {
            len: name.len() as u16,
            data,
        })
    }

    /// C `vboxfs_get_path_size` (`path.c:41-47`): `offsetof(data) + size`
    /// — the two `u16` header fields plus the string with its terminator.
    pub fn wire_size(&self) -> usize {
        4 + self.data.len()
    }
}

/// A mapped shared folder: the HGCM connection id and the folder's root
/// handle (C's `vboxfs_conn`/`vboxfs_root` pair, `vboxfs.c:16-17`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Connection {
    pub conn: i32,
    pub root: u32,
}

/// C `vboxfs_init` (`vboxfs.c:60-98`), step for step: build the path,
/// initialize the backdoor, open `VBoxSharedFolders`, switch the protocol
/// to UTF-8, then map the folder with the four-parameter layout —
/// `[Ptr(path, DIR_OUT), U32(root out), U32('/'), U32(TRUE)]` — and take
/// the root handle from the reply. Every failure after `open` closes the
/// connection again, exactly as C does.
pub fn init<B: Backdoor>(backdoor: &mut B, share: &str) -> Result<Connection, Errno> {
    let path = VboxfsPath::set_path(share)?;
    backdoor.init()?;
    let conn = backdoor.open(VBOX_SHARED_FOLDERS)?;

    // `VBOXFS_CALL_SET_UTF8` carries no parameters (`vboxfs.c:73-79`).
    let result = backdoor.call(conn, VBOXFS_CALL_SET_UTF8, &mut [])?;
    if result != 0 {
        let _ = backdoor.close(conn);
        return Err(Errno::from_i32(result));
    }

    let mut params = [Param::Invalid; 4];
    params[0] = backdoor.grant_ptr(
        &path as *const VboxfsPath as u64,
        path.wire_size(),
        VBOX_DIR_OUT,
    );
    params[1] = Param::U32(0); // root handle, filled by the host
    params[2] = Param::U32(PATH_SEPARATOR); // path separator
    params[3] = Param::U32(1); // case sensitivity — no effect? (`vboxfs.c:82`)
    let result = backdoor.call(conn, VBOXFS_CALL_MAP_FOLDER, &mut params)?;
    if result != 0 {
        let _ = backdoor.close(conn);
        return Err(Errno::from_i32(result));
    }
    let root = match params[1] {
        Param::U32(value) => value,
        _ => return Err(Errno::EINVAL),
    };
    Ok(Connection { conn, root })
}

/// C `vboxfs_cleanup` (`vboxfs.c:100-114`): unmap the folder, then close
/// the connection; both results are ignored.
pub fn cleanup<B: Backdoor>(backdoor: &mut B, connection: &Connection) {
    let mut params = [Param::U32(connection.root)];
    #[allow(clippy::let_underscore_must_use)] // C ignores both results here
    let _ = backdoor.call(connection.conn, VBOXFS_CALL_UNMAP_FOLDER, &mut params);
    #[allow(clippy::let_underscore_must_use)]
    let _ = backdoor.close(connection.conn);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vbox::VBOX_DIR_OUT;
    use alloc::string::String;

    const ROOT: u32 = 0x1234;

    /// A scripted backdoor: records the verb sequence, answers calls from a
    /// queue, and writes the root handle back on MAP_FOLDER the way the
    /// host side does.
    struct Scripted {
        inited: bool,
        opened: Option<String>,
        closed: Vec<i32>,
        calls: Vec<(i32, u32, Vec<Param>)>,
        answers: Vec<i32>,
        next_conn: i32,
    }

    impl Scripted {
        fn new(answers: Vec<i32>) -> Self {
            Self {
                inited: false,
                opened: None,
                closed: Vec::new(),
                calls: Vec::new(),
                answers,
                next_conn: 11,
            }
        }
    }

    impl Backdoor for Scripted {
        fn init(&mut self) -> Result<(), Errno> {
            self.inited = true;
            Ok(())
        }
        fn open(&mut self, service: &str) -> Result<i32, Errno> {
            self.opened = Some(String::from(service));
            let conn = self.next_conn;
            self.next_conn += 1;
            Ok(conn)
        }
        fn close(&mut self, conn: i32) -> Result<(), Errno> {
            self.closed.push(conn);
            Ok(())
        }
        fn grant_ptr(&mut self, _addr: u64, size: usize, dir: u32) -> Param {
            Param::Ptr {
                grant: 7,
                off: 0,
                size,
                dir,
            }
        }
        fn call(&mut self, conn: i32, function: u32, params: &mut [Param]) -> Result<i32, Errno> {
            self.calls.push((conn, function, params.to_vec()));
            if function == VBOXFS_CALL_MAP_FOLDER {
                params[1] = Param::U32(ROOT);
            }
            let answer = if self.answers.is_empty() {
                0
            } else {
                self.answers.remove(0)
            };
            Ok(answer)
        }
    }

    #[test]
    fn init_flow_walks_the_c_sequence() {
        let mut backdoor = Scripted::new(vec![0, 0]);
        let connection = init(&mut backdoor, "shares").expect("init");
        assert!(backdoor.inited);
        assert_eq!(backdoor.opened.as_deref(), Some(VBOX_SHARED_FOLDERS));
        assert_eq!(connection.conn, 11);
        assert_eq!(connection.root, ROOT);
        // Call order and shapes: SET_UTF8 bare, then MAP_FOLDER's four.
        assert_eq!(backdoor.calls.len(), 2);
        let (conn, function, ref params) = backdoor.calls[0];
        assert_eq!((conn, function), (11, VBOXFS_CALL_SET_UTF8));
        assert!(params.is_empty());
        let (conn, function, ref params) = backdoor.calls[1];
        assert_eq!((conn, function), (11, VBOXFS_CALL_MAP_FOLDER));
        assert_eq!(params.len(), 4);
        assert_eq!(
            params[0],
            Param::Ptr {
                grant: 7,
                off: 0,
                size: 4 + "shares".len() + 1,
                dir: VBOX_DIR_OUT
            }
        );
        assert_eq!(params[1], Param::U32(0));
        assert_eq!(params[2], Param::U32(PATH_SEPARATOR));
        assert_eq!(params[3], Param::U32(1));
        assert!(backdoor.closed.is_empty());
    }

    #[test]
    fn map_folder_failure_closes_the_connection() {
        let mut backdoor = Scripted::new(vec![0, 1]); // 1 = VBOXFS_PATH_NOT_FOUND
        let e = init(&mut backdoor, "shares").unwrap_err();
        assert_eq!(e.to_i32(), 1);
        assert_eq!(backdoor.closed, vec![11]);
    }

    #[test]
    fn set_utf8_failure_closes_the_connection() {
        let mut backdoor = Scripted::new(vec![5]);
        let e = init(&mut backdoor, "shares").unwrap_err();
        assert_eq!(e.to_i32(), 5);
        assert_eq!(backdoor.closed, vec![11]);
        assert_eq!(backdoor.calls.len(), 1, "MAP_FOLDER 不再发出");
    }

    #[test]
    fn over_long_share_is_refused_before_any_call() {
        let mut backdoor = Scripted::new(vec![]);
        let e = init(&mut backdoor, &"a".repeat(PATH_MAX)).unwrap_err();
        assert_eq!(e, Errno::ENAMETOOLONG);
        assert!(!backdoor.inited, "路径检查先于一切 backdoor 动作");
    }

    #[test]
    fn cleanup_unmaps_then_closes() {
        let mut backdoor = Scripted::new(vec![]);
        let connection = Connection {
            conn: 11,
            root: ROOT,
        };
        cleanup(&mut backdoor, &connection);
        assert_eq!(backdoor.calls.len(), 1);
        let (conn, function, ref params) = backdoor.calls[0];
        assert_eq!((conn, function), (11, VBOXFS_CALL_UNMAP_FOLDER));
        assert_eq!(params, &vec![Param::U32(ROOT)]);
        assert_eq!(backdoor.closed, vec![11]);
    }

    #[test]
    fn path_wire_size_carries_the_terminator() {
        let path = VboxfsPath::set_path("abc").unwrap();
        assert_eq!(path.len, 3);
        assert_eq!(path.data, b"abc\0");
        assert_eq!(path.wire_size(), 4 + 4);
        let path = VboxfsPath::set_path("").unwrap();
        assert_eq!(path.len, 0);
        assert_eq!(path.wire_size(), 5);
    }
}
