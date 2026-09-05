//! Unix98 pseudoterminal slave-node filesystem (`/dev/pts`).
//!
//! C correspondence: `minix3/minix/fs/ptyfs/ptyfs.c` (four hundred
//! thirty-four lines) plus `node.c` (eighty-four lines) and `node.h`
//! (twenty lines). The server owns no terminal logic: the terminal
//! driver creates and deletes slave nodes through control messages, and
//! this server only names them, lists them, and reports their metadata.
//!
//! Like every file server here, PtyFS is a single-threaded event loop:
//! one message at a time, no shared mutable state across threads.
//!
//! Module map: [`table`] owns the fixed node array behind a bitmap,
//! [`names`] converts between node indexes and decimal names, and this
//! module resolves lookups, listings, status, and control messages on
//! top of both.

#![no_std]

extern crate alloc;

pub mod names;
pub mod table;

use minix_types::{EINVAL, ENAMETOOLONG, ENOENT, ENOMEM, ENOSYS, EIO, Errno};

/// Node number of the root directory (`ROOT_INO_NR`, one).
pub const ROOT_NUMBER: u64 = 1;
/// First node number for slave nodes (`BASE_INO_NR`, two): a slave with
/// index `i` shows number `i + 2`, leaving zero unused and one for root.
pub const SLAVE_BASE_NUMBER: u64 = 2;
/// Service label allowed to send control messages (`"pty"`).
pub const CONTROL_LABEL: &str = "pty";
/// Add-or-update control request (`PTYFS_SET`, `com.h:901`).
pub const REQUEST_SET: u32 = 0x1700;
/// Delete control request (`PTYFS_CLEAR`, `com.h:902`).
pub const REQUEST_CLEAR: u32 = 0x1701;
/// Name-query control request (`PTYFS_NAME`, `com.h:903`).
pub const REQUEST_NAME: u32 = 0x1702;
/// Permission bits kept across mode changes (`ALLPERMS`, nine bits).
pub const PERMISSION_MASK: u32 = 0o777;
/// Set-user-identity bit cleared on ownership change.
pub const SET_USER_ID: u32 = 0o4000;
/// Set-group-identity bit cleared on ownership change.
pub const SET_GROUP_ID: u32 = 0o2000;

/// Why a terminal-filesystem call failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PtyError {
    /// Bad request (unknown number, non-root directory use).
    Invalid,
    /// Name not found (`ENOENT`: unallocated index or foreign directory).
    NotFound,
    /// Generated name does not fit (`ENAMETOOLONG`).
    NameTooLong,
    /// Unknown control request (`ENOSYS`).
    NotSupported,
    /// Node index past the configured count (`ENOMEM`, `node.c:36-37`).
    NoSpace,
    /// Position counter overflow (`EIO`).
    Io,
}

impl PtyError {
    /// The wire error code.
    pub const fn to_errno(self) -> Errno {
        match self {
            Self::Invalid => Errno::from_i32(EINVAL),
            Self::NotFound => Errno::from_i32(ENOENT),
            Self::NameTooLong => Errno::from_i32(ENAMETOOLONG),
            Self::NotSupported => Errno::from_i32(ENOSYS),
            Self::NoSpace => Errno::from_i32(ENOMEM),
            Self::Io => Errno::from_i32(EIO),
        }
    }
}

/// Node details reported for lookups and status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeDetails {
    /// Caller-visible node number.
    pub number: u64,
    /// File mode.
    pub mode: u32,
    /// Owner user identifier.
    pub uid: u16,
    /// Owner group identifier.
    pub gid: u16,
    /// Device number.
    pub device: u64,
}

/// Metadata to store for a slave node (control `SET` payload).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeConfig {
    /// Device number of the slave side.
    pub device: u64,
    /// File mode.
    pub mode: u32,
    /// Owner user identifier.
    pub uid: u16,
    /// Owner group identifier.
    pub gid: u16,
    /// Creation time for status reports.
    pub created: i64,
}

/// Control request after authorization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlRequest {
    /// Store (or refresh) a slave node.
    Set {
        /// Node index.
        index: u32,
        /// Metadata to store.
        config: NodeConfig,
    },
    /// Delete a slave node (always succeeds, even when absent).
    Clear {
        /// Node index.
        index: u32,
    },
    /// Ask for a slave node's decimal name.
    Name {
        /// Node index.
        index: u32,
    },
}

/// Authorize and decode a control message (`ptyfs_other`,
/// `ptyfs.c:299-372`, decision half).
///
/// Only the terminal service (label `"pty"`) may send control messages;
/// anything else is refused without touching the table, and unknown
/// request codes report not-supported. Payload decoding happens here so
/// the executor below is a pure table update.
pub fn authorize_control(
    sender_label: &str,
    request: u32,
    index: u32,
    config: NodeConfig,
) -> Result<ControlRequest, PtyError> {
    if sender_label != CONTROL_LABEL {
        return Err(PtyError::Invalid);
    }
    match request {
        REQUEST_SET => Ok(ControlRequest::Set { index, config }),
        REQUEST_CLEAR => Ok(ControlRequest::Clear { index }),
        REQUEST_NAME => Ok(ControlRequest::Name { index }),
        _ => Err(PtyError::NotSupported),
    }
}

/// Resolve one name below a directory (`ptyfs_lookup`,
/// `ptyfs.c:107-147`).
///
/// Only the root directory lists anything; any other directory reports
/// missing. A single dot resolves to the root itself. Anything else must
/// parse as a slave index (decimal, no leading zeroes, no overflow) and
/// must name an allocated node, or it reports missing.
pub fn lookup(
    table: &table::NodeTable,
    root: &table::StoredNode,
    dir_number: u64,
    name: &[u8],
) -> Result<NodeDetails, PtyError> {
    if dir_number != ROOT_NUMBER {
        return Err(PtyError::NotFound);
    }
    if name == b"." {
        return Ok(describe_root(root));
    }
    let index = names::parse(name).ok_or(PtyError::NotFound)?;
    let stored = table.get(index).ok_or(PtyError::NotFound)?;
    Ok(NodeDetails {
        number: SLAVE_BASE_NUMBER + index as u64,
        mode: stored.mode,
        uid: stored.uid,
        gid: stored.gid,
        device: stored.device,
    })
}

/// One listing row produced for the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListRow {
    /// Caller-visible node number.
    pub number: u64,
    /// Entry name.
    pub name: alloc::vec::Vec<u8>,
    /// File-type byte (directory two, matching the shared encoding).
    pub file_type: u8,
}

/// List the root directory from a position (`ptyfs_getdents`,
/// `ptyfs.c:152-199`, without storage).
///
/// Positions count every index considered, including unallocated ones
/// that produce no output: dot is position zero, dot-dot is position
/// one, slave index `i` is position `i + 2`. Listing any other node
/// refuses. A full caller buffer stops the walk with the position
/// rewound to the blocking entry.
pub fn list(
    table: &table::NodeTable,
    node_number: u64,
    position: &mut u64,
    capacity: usize,
    out: &mut dyn FnMut(ListRow),
) -> Result<usize, PtyError> {
    if node_number != ROOT_NUMBER {
        return Err(PtyError::Invalid);
    }
    if *position == u64::MAX {
        return Err(PtyError::Io);
    }
    let mut emitted = 0usize;
    loop {
        let pos = *position;
        *position += 1;
        if pos == 0 {
            if emitted >= capacity {
                *position = pos;
                break;
            }
            out(ListRow { number: ROOT_NUMBER, name: alloc::vec![b'.'], file_type: 4 });
            emitted += 1;
        } else if pos == 1 {
            if emitted >= capacity {
                *position = pos;
                break;
            }
            out(ListRow { number: ROOT_NUMBER, name: alloc::vec![b'.', b'.'], file_type: 4 });
            emitted += 1;
        } else {
            let index = pos - 2;
            if index >= table.upper_bound() {
                break;
            }
            let Some(stored) = table.get(index as u32) else {
                continue;
            };
            let Ok(name) = names::render(index as u32) else {
                continue;
            };
            if emitted >= capacity {
                *position = pos;
                break;
            }
            out(ListRow {
                number: SLAVE_BASE_NUMBER + index,
                name: name.into_bytes(),
                file_type: file_type_of(stored.mode),
            });
            emitted += 1;
        }
        if *position == u64::MAX {
            break;
        }
    }
    Ok(emitted)
}

/// Change ownership (`ptyfs_chown`, `ptyfs.c:225-239`): both identifiers
/// land and both privilege bits clear, like the disk server.
pub fn change_owner(
    table: &mut table::NodeTable,
    root: &mut table::StoredNode,
    number: u64,
    uid: u16,
    gid: u16,
) -> Result<u32, PtyError> {
    if number == ROOT_NUMBER {
        root.uid = uid;
        root.gid = gid;
        root.mode &= !(SET_USER_ID | SET_GROUP_ID);
        return Ok(root.mode);
    }
    let stored = table.get_mut(slave_index(number)?).ok_or(PtyError::Invalid)?;
    stored.uid = uid;
    stored.gid = gid;
    stored.mode &= !(SET_USER_ID | SET_GROUP_ID);
    Ok(stored.mode)
}

/// Change mode (`ptyfs_chmod`, `ptyfs.c:246-257`): only permission bits
/// change, the file type stays.
pub fn change_mode(
    table: &mut table::NodeTable,
    root: &mut table::StoredNode,
    number: u64,
    mode: u32,
) -> Result<u32, PtyError> {
    if number == ROOT_NUMBER {
        root.mode = (root.mode & !PERMISSION_MASK) | (mode & PERMISSION_MASK);
        return Ok(root.mode);
    }
    let stored = table.get_mut(slave_index(number)?).ok_or(PtyError::Invalid)?;
    stored.mode = (stored.mode & !PERMISSION_MASK) | (mode & PERMISSION_MASK);
    Ok(stored.mode)
}

/// File status (`ptyfs_stat`, `ptyfs.c:263-280`): directories report two
/// links, files report one; all three times are the creation time.
pub fn file_stat(
    table: &table::NodeTable,
    root: &table::StoredNode,
    number: u64,
) -> Result<(NodeDetails, u16, i64), PtyError> {
    if number == ROOT_NUMBER {
        return Ok((describe_root(root), 2, root.created));
    }
    let stored = table.get(slave_index(number)?).ok_or(PtyError::Invalid)?;
    let links = if is_directory(stored.mode) { 2 } else { 1 };
    Ok((
        NodeDetails {
            number,
            mode: stored.mode,
            uid: stored.uid,
            gid: stored.gid,
            device: stored.device,
        },
        links,
        stored.created,
    ))
}

/// Describe the root directory from its stored metadata.
fn describe_root(root: &table::StoredNode) -> NodeDetails {
    NodeDetails {
        number: ROOT_NUMBER,
        mode: root.mode,
        uid: root.uid,
        gid: root.gid,
        device: root.device,
    }
}
fn slave_index(number: u64) -> Result<u32, PtyError> {
    if number < SLAVE_BASE_NUMBER {
        return Err(PtyError::Invalid);
    }
    let index = number - SLAVE_BASE_NUMBER;
    u32::try_from(index).map_err(|_| PtyError::Invalid)
}

/// Whether a mode describes a directory.
fn is_directory(mode: u32) -> bool {
    mode & 0o170000 == 0o040000
}

/// File-type byte from a mode (directory four, character device two,
/// matching the shared directory-type encoding).
fn file_type_of(mode: u32) -> u8 {
    (((mode) & 0o170000) >> 12) as u8
}

/// Filesystem statistics (`ptyfs_statvfs`): names never truncate.
pub fn filesystem_stat(name_max: usize) -> (bool, usize) {
    (true, name_max)
}

/// Service initialization entry (kept for the server binary).
pub fn init() {}

#[cfg(test)]
mod tests {
    use super::*;
    use table::{NodeTable, StoredNode};

    extern crate alloc;
    use alloc::vec::Vec;

    fn root() -> StoredNode {
        StoredNode { device: 0, mode: 0o040755, uid: 0, gid: 0, created: 111 }
    }

    fn slave(mode: u32) -> StoredNode {
        StoredNode { device: 7, mode, uid: 100, gid: 100, created: 222 }
    }

    fn table_with_slave(index: u32) -> NodeTable {
        let mut table = NodeTable::new();
        table.set(index, slave(0o020666)).unwrap();
        table
    }

    #[test]
    fn test_lookup_dot_and_slave() {
        let table = table_with_slave(3);
        assert_eq!(lookup(&table, &root(), ROOT_NUMBER, b".").unwrap().number, ROOT_NUMBER);
        let details = lookup(&table, &root(), ROOT_NUMBER, b"3").unwrap();
        assert_eq!(details.number, SLAVE_BASE_NUMBER + 3);
        assert_eq!(details.device, 7);
        // Foreign directories, unallocated indexes, and bad names miss.
        assert_eq!(lookup(&table, &root(), 9, b"3").unwrap_err(), PtyError::NotFound);
        assert_eq!(lookup(&table, &root(), ROOT_NUMBER, b"4").unwrap_err(), PtyError::NotFound);
        assert_eq!(lookup(&table, &root(), ROOT_NUMBER, b"03").unwrap_err(), PtyError::NotFound);
    }

    #[test]
    fn test_list_skips_idle_and_rewinds() {
        let table = table_with_slave(1);
        let mut position = 0u64;
        let mut rows = Vec::new();
        // Room for two rows: dot, dot-dot, then rewind at the slave.
        list(&table, ROOT_NUMBER, &mut position, 2, &mut |row| rows.push(row)).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(position, 3);
        // Resume lists the slave and parks past the bound.
        list(&table, ROOT_NUMBER, &mut position, 8, &mut |row| rows.push(row)).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[2].number, SLAVE_BASE_NUMBER + 1);
        assert_eq!(rows[2].name, b"1".to_vec());
        // The walk consumes the past-the-end position before noticing
        // the bound, like the C loop incrementing before its check.
        assert_eq!(position, table.upper_bound() + 3);
        // Other nodes refuse.
        let mut position = 0u64;
        assert_eq!(
            list(&table, 9, &mut position, 8, &mut |_| {}).unwrap_err(),
            PtyError::Invalid
        );
    }

    #[test]
    fn test_owner_and_mode_changes() {
        let mut table = table_with_slave(0);
        let mut root_node = root();
        assert_eq!(change_owner(&mut table, &mut root_node, SLAVE_BASE_NUMBER, 5, 6).unwrap() & 0o7777, 0o666);
        let stored = table.get(0).unwrap();
        assert_eq!((stored.uid, stored.gid), (5, 6));
        assert_eq!(change_mode(&mut table, &mut root_node, SLAVE_BASE_NUMBER, 0o020600).unwrap() & 0o777, 0o600);
        // Type bits survive mode changes.
        assert_eq!(table.get(0).unwrap().mode & 0o170000, 0o020000);
        // Root edits land on the root record.
        assert_eq!(change_mode(&mut table, &mut root_node, ROOT_NUMBER, 0o040700).unwrap() & 0o777, 0o700);
        assert_eq!(change_owner(&mut table, &mut root_node, ROOT_NUMBER, 1, 1).unwrap() & 0o777, 0o700);
        // Unknown numbers refuse.
        assert_eq!(
            change_mode(&mut table, &mut root_node, 999, 0o666).unwrap_err(),
            PtyError::Invalid
        );
    }

    #[test]
    fn test_stat_links_and_times() {
        let table = table_with_slave(2);
        let (details, links, time) = file_stat(&table, &root(), ROOT_NUMBER).unwrap();
        assert_eq!((details.number, links, time), (ROOT_NUMBER, 2, 111));
        let (details, links, time) =
            file_stat(&table, &root(), SLAVE_BASE_NUMBER + 2).unwrap();
        assert_eq!((details.number, links, time), (SLAVE_BASE_NUMBER + 2, 1, 222));
        assert_eq!(
            file_stat(&table, &root(), 999).unwrap_err(),
            PtyError::Invalid
        );
    }

    #[test]
    fn test_control_authorization() {
        let config = NodeConfig { device: 1, mode: 0o020666, uid: 0, gid: 0, created: 0 };
        // Foreign labels never reach the table.
        assert_eq!(
            authorize_control("tty", REQUEST_SET, 0, config).unwrap_err(),
            PtyError::Invalid
        );
        assert!(matches!(
            authorize_control("pty", REQUEST_SET, 0, config).unwrap(),
            ControlRequest::Set { .. }
        ));
        assert!(matches!(
            authorize_control("pty", REQUEST_CLEAR, 0, config).unwrap(),
            ControlRequest::Clear { .. }
        ));
        assert!(matches!(
            authorize_control("pty", REQUEST_NAME, 0, config).unwrap(),
            ControlRequest::Name { .. }
        ));
        assert_eq!(
            authorize_control("pty", 0x1799, 0, config).unwrap_err(),
            PtyError::NotSupported
        );
    }

    #[test]
    fn test_table_bounds() {
        let mut table = NodeTable::new();
        assert_eq!(table.upper_bound(), 32);
        assert_eq!(
            table.set(32, slave(0)).unwrap_err(),
            PtyError::NoSpace
        );
        // Clearing an idle index always succeeds.
        table.clear(5);
        assert!(table.get(5).is_none());
    }

    #[test]
    fn test_constants_match_c() {
        assert_eq!(ROOT_NUMBER, 1);
        assert_eq!(SLAVE_BASE_NUMBER, 2);
        assert_eq!(REQUEST_SET, 0x1700);
        assert_eq!(REQUEST_CLEAR, 0x1701);
        assert_eq!(REQUEST_NAME, 0x1702);
        assert_eq!(CONTROL_LABEL, "pty");
    }

    #[test]
    fn test_error_codes_match_c() {
        use minix_types::{EINVAL, ENAMETOOLONG, ENOENT, ENOMEM, ENOSYS, EIO};
        assert_eq!(PtyError::Invalid.to_errno().to_i32(), EINVAL);
        assert_eq!(PtyError::NotFound.to_errno().to_i32(), ENOENT);
        assert_eq!(PtyError::NameTooLong.to_errno().to_i32(), ENAMETOOLONG);
        assert_eq!(PtyError::NotSupported.to_errno().to_i32(), ENOSYS);
        assert_eq!(PtyError::NoSpace.to_errno().to_i32(), ENOMEM);
        assert_eq!(PtyError::Io.to_errno().to_i32(), EIO);
        assert_eq!(filesystem_stat(60), (true, 60));
    }
}
