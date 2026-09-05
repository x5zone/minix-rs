//! Dispatch table: which request is served where, and what is still open.
//!
//! C correspondence: `minix3/minix/fs/mfs/table.c` (all forty-five lines).
//! The C table wires thirty-one callbacks; most of them are implemented by
//! later documents (lookup, read-write, namespace, metadata, mount), while
//! the block group already runs through the block-transfer stage
//! (document 05). This module records that split explicitly: one constant
//! row per C initializer, each naming the request, the owning document, and
//! whether the entry is already live. As later documents land, rows flip
//! from pending to live; nothing is ever silently unimplemented.

use minix_fs::protocol::RequestNumber;

/// How a table row is served today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryStatus {
    /// Served through the block-transfer stage (document 05).
    LiveViaBlockTransfer,
    /// Owned by a later document; the framework answers "not implemented"
    /// until that document lands.
    PendingDocument,
}

/// One row of the dispatch table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableEntry {
    /// C initializer name, e.g. `fs_mount` (`table.c:14`).
    pub c_handler: &'static str,
    /// Request it serves.
    pub request: RequestNumber,
    /// Document that owns the implementation (`05-block-io.md`, ...).
    pub owner: &'static str,
    /// Current status.
    pub status: EntryStatus,
}

/// The dispatch table in C initializer order (`table.c:13-45`).
pub const MFS_TABLE: [TableEntry; 31] = [
    TableEntry {
        c_handler: "fs_mount",
        request: RequestNumber::ReadSuper,
        owner: "10-mfs-mount.md",
        status: EntryStatus::PendingDocument,
    },
    TableEntry {
        c_handler: "fs_unmount",
        request: RequestNumber::Unmount,
        owner: "10-mfs-mount.md",
        status: EntryStatus::PendingDocument,
    },
    TableEntry {
        c_handler: "fs_lookup",
        request: RequestNumber::Lookup,
        owner: "11-mfs-path.md",
        status: EntryStatus::PendingDocument,
    },
    TableEntry {
        c_handler: "fs_putnode",
        request: RequestNumber::PutNode,
        owner: "09-mfs-inode.md",
        status: EntryStatus::PendingDocument,
    },
    TableEntry {
        c_handler: "fs_readwrite",
        request: RequestNumber::Read,
        owner: "14-mfs-read.md",
        status: EntryStatus::PendingDocument,
    },
    TableEntry {
        c_handler: "fs_readwrite",
        request: RequestNumber::Write,
        owner: "15-mfs-write.md",
        status: EntryStatus::PendingDocument,
    },
    TableEntry {
        c_handler: "fs_readwrite",
        request: RequestNumber::Peek,
        owner: "14-mfs-read.md",
        status: EntryStatus::PendingDocument,
    },
    TableEntry {
        c_handler: "fs_getdents",
        request: RequestNumber::GetDents,
        owner: "14-mfs-read.md",
        status: EntryStatus::PendingDocument,
    },
    TableEntry {
        c_handler: "fs_trunc",
        request: RequestNumber::Truncate,
        owner: "13-mfs-link.md",
        status: EntryStatus::PendingDocument,
    },
    TableEntry {
        c_handler: "fs_seek",
        request: RequestNumber::InhibitRead,
        owner: "14-mfs-read.md",
        status: EntryStatus::PendingDocument,
    },
    TableEntry {
        c_handler: "fs_create",
        request: RequestNumber::Create,
        owner: "12-mfs-open.md",
        status: EntryStatus::PendingDocument,
    },
    TableEntry {
        c_handler: "fs_mkdir",
        request: RequestNumber::MakeDir,
        owner: "12-mfs-open.md",
        status: EntryStatus::PendingDocument,
    },
    TableEntry {
        c_handler: "fs_mknod",
        request: RequestNumber::MakeNode,
        owner: "12-mfs-open.md",
        status: EntryStatus::PendingDocument,
    },
    TableEntry {
        c_handler: "fs_link",
        request: RequestNumber::Link,
        owner: "13-mfs-link.md",
        status: EntryStatus::PendingDocument,
    },
    TableEntry {
        c_handler: "fs_unlink",
        request: RequestNumber::Unlink,
        owner: "13-mfs-link.md",
        status: EntryStatus::PendingDocument,
    },
    TableEntry {
        c_handler: "fs_unlink",
        request: RequestNumber::RemoveDir,
        owner: "13-mfs-link.md",
        status: EntryStatus::PendingDocument,
    },
    TableEntry {
        c_handler: "fs_rename",
        request: RequestNumber::Rename,
        owner: "13-mfs-link.md",
        status: EntryStatus::PendingDocument,
    },
    TableEntry {
        c_handler: "fs_slink",
        request: RequestNumber::SymbolicLink,
        owner: "12-mfs-open.md",
        status: EntryStatus::PendingDocument,
    },
    TableEntry {
        c_handler: "fs_rdlink",
        request: RequestNumber::ReadLink,
        owner: "13-mfs-link.md",
        status: EntryStatus::PendingDocument,
    },
    TableEntry {
        c_handler: "fs_stat",
        request: RequestNumber::Stat,
        owner: "16-mfs-metadata.md",
        status: EntryStatus::PendingDocument,
    },
    TableEntry {
        c_handler: "fs_chown",
        request: RequestNumber::ChangeOwner,
        owner: "16-mfs-metadata.md",
        status: EntryStatus::PendingDocument,
    },
    TableEntry {
        c_handler: "fs_chmod",
        request: RequestNumber::ChangeMode,
        owner: "16-mfs-metadata.md",
        status: EntryStatus::PendingDocument,
    },
    TableEntry {
        c_handler: "fs_utime",
        request: RequestNumber::UpdateTimes,
        owner: "16-mfs-metadata.md",
        status: EntryStatus::PendingDocument,
    },
    TableEntry {
        c_handler: "fs_mountpt",
        request: RequestNumber::MountPoint,
        owner: "10-mfs-mount.md",
        status: EntryStatus::PendingDocument,
    },
    TableEntry {
        c_handler: "fs_statvfs",
        request: RequestNumber::StatVfs,
        owner: "16-mfs-metadata.md",
        status: EntryStatus::PendingDocument,
    },
    TableEntry {
        c_handler: "fs_sync",
        request: RequestNumber::Sync,
        owner: "17-mfs-maint.md",
        status: EntryStatus::PendingDocument,
    },
    TableEntry {
        c_handler: "lmfs_driver",
        request: RequestNumber::NewDriver,
        owner: "05-block-io.md",
        status: EntryStatus::LiveViaBlockTransfer,
    },
    TableEntry {
        c_handler: "lmfs_bio",
        request: RequestNumber::BlockRead,
        owner: "05-block-io.md",
        status: EntryStatus::LiveViaBlockTransfer,
    },
    TableEntry {
        c_handler: "lmfs_bio",
        request: RequestNumber::BlockWrite,
        owner: "05-block-io.md",
        status: EntryStatus::LiveViaBlockTransfer,
    },
    TableEntry {
        c_handler: "lmfs_bio",
        request: RequestNumber::BlockPeek,
        owner: "05-block-io.md",
        status: EntryStatus::LiveViaBlockTransfer,
    },
    TableEntry {
        c_handler: "lmfs_bflush",
        request: RequestNumber::Flush,
        owner: "05-block-io.md",
        status: EntryStatus::LiveViaBlockTransfer,
    },
];

/// Rows already served through the block-transfer stage.
pub fn live_entries() -> impl Iterator<Item = &'static TableEntry> {
    MFS_TABLE
        .iter()
        .filter(|entry| entry.status == EntryStatus::LiveViaBlockTransfer)
}

/// Rows awaiting their owning document.
pub fn pending_entries() -> impl Iterator<Item = &'static TableEntry> {
    MFS_TABLE
        .iter()
        .filter(|entry| entry.status == EntryStatus::PendingDocument)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_covers_every_dispatchable_request() {
        // The table must serve every dispatched request the C table serves:
        // all but the reserved request (no slot anywhere) and the new-node
        // request (no row in `table.c`; the framework default answers for
        // it — see the test below).
        for index in 2..=33u32 {
            let request = RequestNumber::from_index(index).expect("valid index");
            if request == RequestNumber::GetNode || request == RequestNumber::NewNode {
                continue;
            }
            assert!(
                MFS_TABLE.iter().any(|entry| entry.request == request),
                "request {request:?} has no table row"
            );
        }
        assert!(
            MFS_TABLE
                .iter()
                .all(|entry| entry.request != RequestNumber::GetNode),
            "reserved request must stay out of the table"
        );
    }

    #[test]
    fn test_table_has_thirty_one_rows_like_c() {
        assert_eq!(MFS_TABLE.len(), 31);
        assert_eq!(live_entries().count(), 5);
        assert_eq!(pending_entries().count(), 26);
    }

    #[test]
    fn test_shared_handlers_match_c() {
        // `table.c` reuses three handlers for several requests; the reuse
        // must be spelled the same way here.
        let readwrite = MFS_TABLE
            .iter()
            .filter(|entry| entry.c_handler == "fs_readwrite")
            .count();
        assert_eq!(readwrite, 3);
        let unlink = MFS_TABLE
            .iter()
            .filter(|entry| entry.c_handler == "fs_unlink")
            .count();
        assert_eq!(unlink, 2);
        let bio = MFS_TABLE
            .iter()
            .filter(|entry| entry.c_handler == "lmfs_bio")
            .count();
        assert_eq!(bio, 3);
    }

    #[test]
    fn test_newnode_absent_like_c() {
        // `table.c` wires no new-node handler (pipes live in PFS, document
        // 06); the framework default answers for it.
        assert!(
            MFS_TABLE
                .iter()
                .all(|entry| entry.request != RequestNumber::NewNode),
            "new-node stays out until a document claims it"
        );
    }
}
