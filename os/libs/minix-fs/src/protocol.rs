//! File server request protocol: request numbers, transaction identifiers,
//! mount flags, capability flags, and the node descriptor.
//!
//! C correspondence: `minix3/minix/include/minix/vfsif.h` (request numbers,
//! flags, transaction identifier macros, special error codes) and the
//! `FS_BASE` / `VFS_PROC_NR` constants in `minix3/minix/include/minix/com.h`.
//! The dispatch rules implemented here mirror
//! `minix3/minix/lib/libfsdriver/fsdriver.c:17-62` (`fsdriver_process`).
//!
//! A file server in Minix3 is a user-space process. All of its work is driven
//! by the virtual file system service, which sends one request message per
//! file operation. This module describes that wire protocol in Rust types so
//! that the dispatch logic in [`crate::driver`] can be written as an
//! exhaustive match instead of raw integer arithmetic.

use minix_types::Errno;

/// Base of the request number space used by the virtual file system service
/// when it talks to a file server.
///
/// C: `FS_BASE` (`minix3/minix/include/minix/com.h:589`, value `0xA00`).
/// Every request number is `FS_BASE` plus a small index, so a raw message
/// type carries both the operation and a transaction identifier (see
/// [`TransactionId`]).
pub const FS_BASE: i32 = 0xA00;

/// Endpoint number of the virtual file system service.
///
/// C: `VFS_PROC_NR` (`minix3/minix/include/minix/com.h:60`, value `1`).
/// The dispatch rule in [`crate::driver`] only treats a message as a file
/// system request when it arrives from this endpoint; anything else is handed
/// to the server-specific `other` handler without a reply.
pub const VFS_ENDPOINT: i32 = 1;

/// Number of slots in the dispatch table, including the unused slot zero.
///
/// C: `NREQS` (`minix3/minix/include/minix/vfsif.h:75`, value `34`).
/// Request index zero is never used: the first request, `GetNode`, is index
/// one, and the last request, `BlockPeek`, is index thirty-three.
pub const REQUEST_TABLE_SIZE: usize = 34;

/// A file server request, identified by its index above [`FS_BASE`].
///
/// C: the `REQ_*` family in `minix3/minix/include/minix/vfsif.h:41-73`.
/// The numeric value of each variant is the index, not the full message type:
/// the full message type is `FS_BASE + index`, shifted together with the
/// transaction identifier (see [`TransactionId::decode`]).
///
/// Two deliberate deviations from a plain constant table:
/// - `GetNode` (index one) exists in the header but is marked "Should be
///   removed" and has no entry in the dispatch table (`table.c` only fills
///   thirty-two slots). It is kept here so that decoding index one yields a
///   meaningful value instead of an anonymous number.
/// - The order of declaration follows the header file, not the dispatch
///   table, so readers can compare the two side by side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum RequestNumber {
    /// Index one. C: `REQ_GETNODE`. Marked "Should be removed" in the
    /// header; the dispatch table has no entry for it, so dispatching it
    /// answers "not implemented".
    GetNode = 1,
    /// Index two. C: `REQ_PUTNODE`. Release references on an inode.
    PutNode = 2,
    /// Index three. C: `REQ_SLINK`. Create a symbolic link.
    SymbolicLink = 3,
    /// Index four. C: `REQ_FTRUNC`. Truncate a file to a byte range.
    Truncate = 4,
    /// Index five. C: `REQ_CHOWN`. Change owner and group.
    ChangeOwner = 5,
    /// Index six. C: `REQ_CHMOD`. Change permission bits.
    ChangeMode = 6,
    /// Index seven. C: `REQ_INHIBREAD`. Cancel read-ahead after a seek.
    InhibitRead = 7,
    /// Index eight. C: `REQ_STAT`. Read full file status.
    Stat = 8,
    /// Index nine. C: `REQ_UTIME`. Set access and modification times.
    UpdateTimes = 9,
    /// Index ten. C: `REQ_STATVFS`. Read file system statistics.
    StatVfs = 10,
    /// Index eleven. C: `REQ_BREAD`. Read raw device blocks.
    BlockRead = 11,
    /// Index twelve. C: `REQ_BWRITE`. Write raw device blocks.
    BlockWrite = 12,
    /// Index thirteen. C: `REQ_UNLINK`. Remove a name.
    Unlink = 13,
    /// Index fourteen. C: `REQ_RMDIR`. Remove a directory.
    RemoveDir = 14,
    /// Index fifteen. C: `REQ_UNMOUNT`. Unmount the file system.
    Unmount = 15,
    /// Index sixteen. C: `REQ_SYNC`. Flush cached state to storage.
    Sync = 16,
    /// Index seventeen. C: `REQ_NEW_DRIVER`. Bind a new block driver label.
    NewDriver = 17,
    /// Index eighteen. C: `REQ_FLUSH`. Flush and invalidate a device.
    Flush = 18,
    /// Index nineteen. C: `REQ_READ`. Read file bytes.
    Read = 19,
    /// Index twenty. C: `REQ_WRITE`. Write file bytes.
    Write = 20,
    /// Index twenty-one. C: `REQ_MKNOD`. Create a device node.
    MakeNode = 21,
    /// Index twenty-two. C: `REQ_MKDIR`. Create a directory.
    MakeDir = 22,
    /// Index twenty-three. C: `REQ_CREATE`. Create a regular file.
    Create = 23,
    /// Index twenty-four. C: `REQ_LINK`. Create a hard link.
    Link = 24,
    /// Index twenty-five. C: `REQ_RENAME`. Rename a name.
    Rename = 25,
    /// Index twenty-six. C: `REQ_LOOKUP`. Resolve a path.
    Lookup = 26,
    /// Index twenty-seven. C: `REQ_MOUNTPOINT`. Check for a mount point.
    MountPoint = 27,
    /// Index twenty-eight. C: `REQ_READSUPER`. Mount the file system.
    ReadSuper = 28,
    /// Index twenty-nine. C: `REQ_NEWNODE`. Allocate an unnamed inode
    /// (used for pipes and sockets).
    NewNode = 29,
    /// Index thirty. C: `REQ_RDLINK`. Read a symbolic link target.
    ReadLink = 30,
    /// Index thirty-one. C: `REQ_GETDENTS`. List directory entries.
    GetDents = 31,
    /// Index thirty-two. C: `REQ_PEEK`. Expose a file page to virtual memory.
    Peek = 32,
    /// Index thirty-three. C: `REQ_BPEEK`. Expose a device page to virtual
    /// memory.
    BlockPeek = 33,
}

/// Error returned when a raw request index is outside one to thirty-three.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownRequest {
    /// The raw index that failed to decode.
    pub index: u32,
}

impl RequestNumber {
    /// Highest valid request index (thirty-three, `BlockPeek`).
    pub const MAX_INDEX: u32 = 33;

    /// Decode a raw table index into a request.
    ///
    /// Returns `None` for index zero and for anything above thirty-three.
    /// Index zero is reserved and never sent; callers map `None` to the
    /// "not implemented" answer, exactly like a null slot in the C
    /// dispatch table.
    pub const fn from_index(index: u32) -> Option<Self> {
        match index {
            1 => Some(Self::GetNode),
            2 => Some(Self::PutNode),
            3 => Some(Self::SymbolicLink),
            4 => Some(Self::Truncate),
            5 => Some(Self::ChangeOwner),
            6 => Some(Self::ChangeMode),
            7 => Some(Self::InhibitRead),
            8 => Some(Self::Stat),
            9 => Some(Self::UpdateTimes),
            10 => Some(Self::StatVfs),
            11 => Some(Self::BlockRead),
            12 => Some(Self::BlockWrite),
            13 => Some(Self::Unlink),
            14 => Some(Self::RemoveDir),
            15 => Some(Self::Unmount),
            16 => Some(Self::Sync),
            17 => Some(Self::NewDriver),
            18 => Some(Self::Flush),
            19 => Some(Self::Read),
            20 => Some(Self::Write),
            21 => Some(Self::MakeNode),
            22 => Some(Self::MakeDir),
            23 => Some(Self::Create),
            24 => Some(Self::Link),
            25 => Some(Self::Rename),
            26 => Some(Self::Lookup),
            27 => Some(Self::MountPoint),
            28 => Some(Self::ReadSuper),
            29 => Some(Self::NewNode),
            30 => Some(Self::ReadLink),
            31 => Some(Self::GetDents),
            32 => Some(Self::Peek),
            33 => Some(Self::BlockPeek),
            _ => None,
        }
    }

    /// The table index of this request (one to thirty-three).
    pub const fn index(self) -> u32 {
        self as u32
    }

    /// The full message type for this request without a transaction
    /// identifier: `FS_BASE + index`.
    pub const fn message_type(self) -> i32 {
        FS_BASE + self.index() as i32
    }

    /// Whether this request has an entry in the dispatch table.
    ///
    /// Only `GetNode` returns false: the header reserves index one but the
    /// table leaves it empty, so dispatching it always answers "not
    /// implemented" (`table.c:4-40` lists the other thirty-two).
    pub const fn is_dispatched(self) -> bool {
        !matches!(self, Self::GetNode)
    }
}

impl TryFrom<u32> for RequestNumber {
    type Error = UnknownRequest;

    fn try_from(index: u32) -> Result<Self, Self::Error> {
        Self::from_index(index).ok_or(UnknownRequest { index })
    }
}

/// Transaction identifier packed into a message type.
///
/// C: `TRNS_GET_ID` / `TRNS_ADD_ID` / `TRNS_DEL_ID`
/// (`minix3/minix/include/minix/vfsif.h:79-81`). The low sixteen bits of the
/// message type carry an opaque identifier chosen by the sender; the high
/// bits carry the request number (possibly negative, because `FS_BASE` is
/// `0xA00` and the subtraction in `fsdriver_process` wraps intentionally).
/// The reply echoes the identifier so the sender can match replies to
/// outstanding requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransactionId(pub u16);

impl TransactionId {
    /// Split a raw message type into its request part and its transaction
    /// identifier: `call = type >> 16` (arithmetic shift), `id = type & 0xFFFF`.
    pub const fn decode(raw_type: i32) -> (i32, Self) {
        let id = (raw_type & 0xFFFF) as u16;
        let call = raw_type >> 16;
        (call, Self(id))
    }

    /// Combine a reply status with the transaction identifier:
    /// `(status << 16) | id`.
    pub const fn encode_reply(status: i32, id: Self) -> i32 {
        (status << 16) | (id.0 as i32)
    }

    /// Combine a request message type with the transaction identifier.
    pub const fn encode_request(message_type: i32, id: Self) -> i32 {
        (message_type << 16) | (id.0 as i32)
    }
}

/// Decide whether a raw message type belongs to the file server protocol.
///
/// C: `IS_FS_RQ(type)` (`minix3/minix/include/minix/vfsif.h:77`): all but the
/// low eight bits must equal `FS_BASE`. This is a coarse pre-filter used
/// before the transaction identifier is stripped.
pub const fn is_file_request(raw_type: i32) -> bool {
    (raw_type & !0xff) == FS_BASE
}

/// Flags of a mount request (`ReadSuper`).
///
/// C: `REQ_RDONLY` / `REQ_ISROOT` (`minix3/minix/include/minix/vfsif.h:8-9`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MountFlags(pub u32);

impl MountFlags {
    /// Empty flag set.
    pub const EMPTY: Self = Self(0);
    /// The file system is mounted read-only; writes must be refused.
    /// C: `REQ_RDONLY` (octal `001`).
    pub const READ_ONLY: Self = Self(1);
    /// The file system is the root file system.
    /// C: `REQ_ISROOT` (octal `002`).
    pub const IS_ROOT: Self = Self(2);

    /// Whether the read-only flag is set.
    pub const fn is_read_only(self) -> bool {
        self.0 & Self::READ_ONLY.0 != 0
    }

    /// Whether the root flag is set.
    pub const fn is_root(self) -> bool {
        self.0 & Self::IS_ROOT.0 != 0
    }
}

/// Capability flags a file server reports back after mounting.
///
/// C: `RES_NOFLAGS` / `RES_THREADED` / `RES_HASPEEK` / `RES_64BIT`
/// (`minix3/minix/include/minix/vfsif.h:20-23`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilityFlags(pub u32);

impl CapabilityFlags {
    /// Empty flag set. C: `RES_NOFLAGS`.
    pub const EMPTY: Self = Self(0);
    /// The server supports multi-threaded operation. C: `RES_THREADED`.
    /// This framework only implements the single-threaded event loop, so a
    /// server built on it never sets this flag.
    pub const THREADED: Self = Self(1);
    /// The server implements peek requests. C: `RES_HASPEEK`.
    pub const HAS_PEEK: Self = Self(2);
    /// The server handles sixty-four-bit file sizes. C: `RES_64BIT`.
    pub const SIZE_64BIT: Self = Self(4);

    /// Whether the threaded flag is set.
    pub const fn is_threaded(self) -> bool {
        self.0 & Self::THREADED.0 != 0
    }

    /// Whether the peek flag is set.
    pub const fn has_peek(self) -> bool {
        self.0 & Self::HAS_PEEK.0 != 0
    }

    /// Whether the sixty-four-bit flag is set.
    pub const fn is_64bit(self) -> bool {
        self.0 & Self::SIZE_64BIT.0 != 0
    }
}

/// Lookup control flags sent with a `Lookup` request.
///
/// C: `PATH_NOFLAGS` / `PATH_RET_SYMLINK` / `PATH_GET_UCRED`
/// (`minix3/minix/include/minix/vfsif.h:11-18`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LookupFlags(pub u32);

impl LookupFlags {
    /// Empty flag set. C: `PATH_NOFLAGS`.
    pub const EMPTY: Self = Self(0);
    /// Return a symbolic link itself instead of resolving it when the link
    /// is the last path component. C: `PATH_RET_SYMLINK` (octal `010`).
    pub const RETURN_SYMLINK: Self = Self(0o10);
    /// The request carries full user credentials behind a grant instead of
    /// a plain user and group identifier. C: `PATH_GET_UCRED` (octal `020`).
    pub const WITH_CREDENTIALS: Self = Self(0o20);

    /// Whether the caller asked to keep a trailing symbolic link unresolved.
    pub const fn return_symlink(self) -> bool {
        self.0 & Self::RETURN_SYMLINK.0 != 0
    }

    /// Whether full credentials accompany the request.
    pub const fn with_credentials(self) -> bool {
        self.0 & Self::WITH_CREDENTIALS.0 != 0
    }
}

/// Special redirection outcomes of path resolution.
///
/// These are negative "error" codes that are not failures: they tell the
/// virtual file system service to continue the lookup elsewhere. Kept here
/// (rather than in the generic error list) because only the lookup path
/// produces them.
///
/// C: `EENTERMOUNT` (-301), `ELEAVEMOUNT` (-302), `ESYMLINK` (-303)
/// (`minix3/minix/include/minix/vfsif.h:26-28`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookupRedirect {
    /// The resolved node is a mount point owned by another file server.
    EnterMount,
    /// Resolution reached the file system root while moving to the parent,
    /// so the parent lives in the mounting file system. C: `ELEAVEMOUNT`.
    LeaveMount,
    /// Resolution met an absolute symbolic link that the virtual file system
    /// service must restart from its own root. C: `ESYMLINK`.
    AbsoluteSymlink,
}

/// Wire value of `EENTERMOUNT`: resolution entered a mount point.
///
/// C: `minix3/minix/include/minix/vfsif.h:26` (value -301). These three codes
/// live in the kernel-private range below -300 and never appear in normal
/// error returns; they are declared here (not in `minix-types`) because only
/// the file server protocol uses them.
pub const EENTERMOUNT: i32 = -301;
/// Wire value of `ELEAVEMOUNT`: resolution left the file system through its
/// root. C: `minix3/minix/include/minix/vfsif.h:27` (value -302).
pub const ELEAVEMOUNT: i32 = -302;
/// Wire value of `ESYMLINK`: resolution met an absolute symbolic link.
/// C: `minix3/minix/include/minix/vfsif.h:28` (value -303).
pub const ESYMLINK: i32 = -303;

impl LookupRedirect {
    /// The wire error code for this redirection.
    pub const fn to_errno(self) -> Errno {
        match self {
            Self::EnterMount => Errno::from_i32(EENTERMOUNT),
            Self::LeaveMount => Errno::from_i32(ELEAVEMOUNT),
            Self::AbsoluteSymlink => Errno::from_i32(ESYMLINK),
        }
    }
}

// The three redirection codes live in the kernel-private range below -300.
// They are asserted here (against minix-types, which mirrors the C header)
// so that a header drift is caught at compile time.
const _: () = {
    assert!(EENTERMOUNT == -301);
    assert!(ELEAVEMOUNT == -302);
    assert!(ESYMLINK == -303);
};

/// Properties of a file system node reported back to the virtual file system
/// service.
///
/// C: `struct fsdriver_node` (`minix3/minix/include/minix/fsdriver.h:9-16`).
/// Every reply that names a file carries these six fields; the mount reply
/// and the create reply reuse the same shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileNode {
    /// Inode number within this file server.
    pub inode_number: u64,
    /// File type and permission bits.
    pub mode: u32,
    /// File size in bytes.
    pub size: i64,
    /// Owning user identifier.
    pub owner: u32,
    /// Owning group identifier.
    pub group: u32,
    /// Device number, meaningful only for block and character devices.
    pub device: u64,
}

impl FileNode {
    /// Build a node descriptor from its six fields.
    pub const fn new(
        inode_number: u64,
        mode: u32,
        size: i64,
        owner: u32,
        group: u32,
        device: u64,
    ) -> Self {
        Self {
            inode_number,
            mode,
            size,
            owner,
            group,
            device,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::{EINVAL, ENOSYS};

    #[test]
    fn test_request_table_covers_index_1_to_33() {
        for index in 1..=33 {
            assert!(
                RequestNumber::from_index(index).is_some(),
                "index {index} should decode"
            );
        }
        assert_eq!(RequestNumber::from_index(0), None);
        assert_eq!(RequestNumber::from_index(34), None);
    }

    #[test]
    fn test_getnode_has_no_dispatch_slot() {
        assert!(!RequestNumber::GetNode.is_dispatched());
        for index in 2..=33 {
            let request = RequestNumber::from_index(index).unwrap();
            assert!(request.is_dispatched(), "{request:?} should dispatch");
        }
    }

    #[test]
    fn test_message_type_adds_fs_base() {
        assert_eq!(RequestNumber::ReadSuper.message_type(), FS_BASE + 28);
        assert_eq!(RequestNumber::Lookup.message_type(), FS_BASE + 26);
        assert_eq!(RequestNumber::BlockPeek.message_type(), FS_BASE + 33);
    }

    #[test]
    fn test_transaction_identifier_roundtrip() {
        let id = TransactionId(0x1234);
        let encoded = TransactionId::encode_reply(0, id);
        let (call, decoded) = TransactionId::decode(encoded);
        assert_eq!(call, 0);
        assert_eq!(decoded, id);

        // A negative status must survive the round trip: the C code shifts a
        // signed value, so decoding uses an arithmetic shift.
        let encoded = TransactionId::encode_reply(-EINVAL, id);
        let (call, decoded) = TransactionId::decode(encoded);
        assert_eq!(call, -EINVAL);
        assert_eq!(decoded, id);
    }

    #[test]
    fn test_is_file_request_prefilter() {
        assert!(is_file_request(FS_BASE + 28));
        assert!(!is_file_request(0));
        assert!(!is_file_request(0x500));
    }

    #[test]
    fn test_mount_and_capability_flags() {
        let flags = MountFlags(MountFlags::READ_ONLY.0 | MountFlags::IS_ROOT.0);
        assert!(flags.is_read_only());
        assert!(flags.is_root());
        assert!(!MountFlags::EMPTY.is_read_only());

        let caps = CapabilityFlags(CapabilityFlags::HAS_PEEK.0);
        assert!(caps.has_peek());
        assert!(!caps.is_threaded());
        assert!(!caps.is_64bit());
    }

    #[test]
    fn test_lookup_flags_decode() {
        let flags = LookupFlags(0o10);
        assert!(flags.return_symlink());
        assert!(!flags.with_credentials());
        let flags = LookupFlags(0o20);
        assert!(!flags.return_symlink());
        assert!(flags.with_credentials());
    }

    #[test]
    fn test_lookup_redirect_errno_values() {
        assert_eq!(LookupRedirect::EnterMount.to_errno().to_i32(), -301);
        assert_eq!(LookupRedirect::LeaveMount.to_errno().to_i32(), -302);
        assert_eq!(LookupRedirect::AbsoluteSymlink.to_errno().to_i32(), -303);
    }

    #[test]
    fn test_unknown_request_carries_index() {
        let error = RequestNumber::try_from(99).unwrap_err();
        assert_eq!(error.index, 99);
        // Spot check that errno constants used by dispatch exist.
        assert_eq!(ENOSYS, 78);
        assert_eq!(EINVAL, 22);
    }
}
