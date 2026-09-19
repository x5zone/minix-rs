//! Wire decoding and encoding for the file-server requests.
//!
//! C correspondence: the "extract and validate" halves of the libfsdriver
//! adapters (`minix3/minix/lib/libfsdriver/call.c`) and their reply
//! shaping. One direction per side: the virtual file system service
//! encodes with the shared offset tables in `minix_types::ipc::fs_driver`
//! (its `request.rs` encoders are the reference consumers); this module
//! decodes the same fields back out and encodes the replies the VFS
//! decoders expect. Names travel in grants and are fetched into owned
//! buffers here, like `fsdriver_getname` (`utility.c`).
//!
//! Offset tables missing from `minix-types` (requests whose VFS-side
//! encoder does not exist yet) are defined locally at the bottom, marked
//! for promotion — same discipline, single home per table once both ends
//! exist.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use minix_fs::protocol::{FileNode, LookupFlags, MountFlags, RequestNumber, TransactionId};
use minix_fs::task::{FsReply, ReplyPayload, RequestBody};
use minix_fs::lookup::LookupOutcome;
use minix_types as off;
use minix_types::{EIO, EINVAL, Errno, Message};

/// Requests whose payload is a bare `ino_t inode` (`mess_vfs_fs_mountpoint`,
/// ipc.h; `mess_vfs_fs_putnode` adds a trailing count).
mod local_off {
    /// `mess_vfs_fs_mountpoint { ino_t inode; }`.
    pub mod mountpoint {
        pub const INODE: usize = 0;
    }
    /// `mess_vfs_fs_putnode { ino_t inode; unsigned count; }`.
    pub mod putnode {
        pub const INODE: usize = 0;
        pub const COUNT: usize = 8;
    }
    /// `mess_vfs_fs_newnode { dev_t device; mode_t mode; uid_t uid;
    /// gid_t gid; }` — the VFS encoder (`request.rs::encode_newnode`)
    /// writes exactly this order.
    pub mod newnode {
        pub const DEVICE: usize = 0;
        pub const MODE: usize = 8;
        pub const UID: usize = 12;
        pub const GID: usize = 16;
    }
    /// `mess_vfs_fs_breadwrite { dev_t device; off_t seek_pos;
    /// cp_grant_id_t grant; size_t nbytes; }` — natural LP64 alignment
    /// (`nbytes` is 8-byte aligned, hence the gap at 20). No Rust-side
    /// peer yet (the VFS block arms are unwired); the layout is asserted
    /// end-to-end at real-machine bring-up (E5).
    pub mod breadwrite {
        pub const DEVICE: usize = 0;
        pub const SEEK_POS: usize = 8;
        pub const GRANT: usize = 16;
        pub const NBYTES: usize = 24;
    }
    /// `REQ_STATVFS { cp_grant_id_t grant; }` (the VFS encoder
    /// `encode_statvfs` writes the grant at offset zero).
    pub mod statvfs {
        pub const GRANT: usize = 0;
    }
    /// `mess_vfs_fs_mkdir` 的 `path_len@24`——C 结构与 create 同形
    /// （`call.c:535` 读 `m_vfs_fs_mkdir.path_len`），但 `minix-types` 的
    /// `mkdir_req_off` 表漏列了这一域；本地补位，待表上收后同撤。
    pub mod mkdir_len {
        pub const PATH_LEN: usize = 24;
    }
}

/// Fetch `len` bytes from a grant and turn them into an owned NUL-free
/// name (`fsdriver_getname`, `utility.c`: the C helper copies into a local
/// buffer, checks the terminator, and refuses oversized names).
fn fetch_name(
    ipc: &mut impl crate::ipc::RtIpc,
    peer: minix_types::Endpoint,
    grant: i32,
    len: usize,
) -> Result<String, Errno> {
    if len == 0 || len > minix_fs::lookup::PATH_MAX {
        return Err(Errno::from_i32(EINVAL));
    }
    let mut buf = vec![0u8; len];
    ipc.copy_from(peer, grant, 0, &mut buf).map_err(|_| Errno::from_i32(EIO))?;
    // The name must be NUL-terminated inside the window (`utility.c:38-45`).
    let used = buf.iter().position(|b| *b == 0).ok_or(Errno::from_i32(EINVAL))?;
    buf.truncate(used);
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// Read a name out of a grant, using the message fields for the grant id
/// and length (the closure-free form: `ipc` is borrowed once per call).
fn grant_str(
    raw: &[u8],
    grant_at: usize,
    len_at: usize,
    ipc: &mut impl crate::ipc::RtIpc,
    peer: minix_types::Endpoint,
) -> Result<String, Errno> {
    let grant = i32::from_le_bytes(raw[grant_at..grant_at + 4].try_into().unwrap());
    let len = u64::from_le_bytes(raw[len_at..len_at + 8].try_into().unwrap()) as usize;
    fetch_name(ipc, peer, grant, len)
}

/// Read raw grant bytes out of a message field pair.
fn grant_buf(
    raw: &[u8],
    grant_at: usize,
    len_at: usize,
    ipc: &mut impl crate::ipc::RtIpc,
    peer: minix_types::Endpoint,
) -> Result<Vec<u8>, Errno> {
    let grant = i32::from_le_bytes(raw[grant_at..grant_at + 4].try_into().unwrap());
    let len = u64::from_le_bytes(raw[len_at..len_at + 8].try_into().unwrap()) as usize;
    fetch_bytes(ipc, peer, grant, len)
}

/// Fetch raw grant bytes (the symbolic-link target travels this way).
fn fetch_bytes(
    ipc: &mut impl crate::ipc::RtIpc,
    peer: minix_types::Endpoint,
    grant: i32,
    len: usize,
) -> Result<Vec<u8>, Errno> {
    if len > minix_fs::lookup::PATH_MAX {
        return Err(Errno::from_i32(EINVAL));
    }
    let mut buf = vec![0u8; len];
    ipc.copy_from(peer, grant, 0, &mut buf).map_err(|_| Errno::from_i32(EIO))?;
    Ok(buf)
}

/// Decode one dispatchable request body out of the raw message.
///
/// `ipc`/`peer` back the grant reads the name-bearing requests need. Every
/// offset comes from the shared `minix_types` tables (or the local ones
/// above for the yet-unshared layouts).
pub fn decode_body(
    request: RequestNumber,
    msg: &Message,
    ipc: &mut impl crate::ipc::RtIpc,
    peer: minix_types::Endpoint,
) -> Result<RequestBody, Errno> {
    // SAFETY: every read below is bounded by the offset table plus a fixed
    // width well inside `MESSAGE_PAYLOAD_SIZE`.
    let raw = unsafe { &msg.m_u.raw };
    let u64_at = |at: usize| -> u64 {
        u64::from_le_bytes(raw[at..at + 8].try_into().unwrap())
    };
    let u32_at = |at: usize| -> u32 {
        u32::from_le_bytes(raw[at..at + 4].try_into().unwrap())
    };
    let i64_at = |at: usize| -> i64 {
        i64::from_le_bytes(raw[at..at + 8].try_into().unwrap())
    };
    let body = match request {
        RequestNumber::ReadSuper => RequestBody::ReadSuper {
            device: u64_at(off::readsuper_req_off::DEVICE),
            flags: MountFlags(u32_at(off::readsuper_req_off::FLAGS)),
            label: grant_str(raw, off::readsuper_req_off::GRANT, off::readsuper_req_off::PATH_LEN, ipc, peer)?,
        },
        RequestNumber::PutNode => RequestBody::PutNode {
            inode: u64_at(local_off::putnode::INODE),
            count: u32_at(local_off::putnode::COUNT),
        },
        RequestNumber::SymbolicLink => RequestBody::SymbolicLink {
            directory: u64_at(off::slink_req_off::INODE),
            name: grant_str(raw, off::slink_req_off::GRANT_PATH, off::slink_req_off::PATH_LEN, ipc, peer)?,
            owner: u32_at(off::slink_req_off::UID),
            group: u32_at(off::slink_req_off::GID),
            target: grant_buf(raw, off::slink_req_off::GRANT_TARGET, off::slink_req_off::MEM_SIZE, ipc, peer)?,
        },
        RequestNumber::Truncate => RequestBody::Truncate {
            inode: u64_at(off::ftrunc_req_off::INODE),
            start: i64_at(off::ftrunc_req_off::TRC_START),
            end: i64_at(off::ftrunc_req_off::TRC_END),
        },
        RequestNumber::ChangeOwner => RequestBody::ChangeOwner {
            inode: u64_at(off::chown_req_off::INODE),
            owner: u32_at(off::chown_req_off::UID),
            group: u32_at(off::chown_req_off::GID),
        },
        RequestNumber::ChangeMode => RequestBody::ChangeMode {
            inode: u64_at(off::chmod_req_off::INODE),
            mode: u32_at(off::chmod_req_off::MODE),
        },
        RequestNumber::InhibitRead => RequestBody::InhibitRead {
            inode: u64_at(off::transfer_req_off::INODE),
        },
        RequestNumber::Stat => RequestBody::Stat {
            inode: u64_at(off::stat_req_off::INODE),
        },
        RequestNumber::UpdateTimes => RequestBody::UpdateTimes {
            inode: u64_at(off::utime_req_off::INODE),
            accessed: (
                i64_at(off::utime_req_off::ACTIME),
                i64_at(off::utime_req_off::ACNSEC),
            ),
            modified: (
                i64_at(off::utime_req_off::MODTIME),
                i64_at(off::utime_req_off::MODNSEC),
            ),
        },
        RequestNumber::StatVfs => RequestBody::StatVfs,
        RequestNumber::BlockRead => RequestBody::BlockRead {
            device: u64_at(local_off::breadwrite::DEVICE),
            position: i64_at(local_off::breadwrite::SEEK_POS),
            length: u64_at(local_off::breadwrite::NBYTES) as usize,
        },
        RequestNumber::BlockWrite => RequestBody::BlockWrite {
            device: u64_at(local_off::breadwrite::DEVICE),
            position: i64_at(local_off::breadwrite::SEEK_POS),
            length: u64_at(local_off::breadwrite::NBYTES) as usize,
        },
        RequestNumber::Unlink => RequestBody::Unlink {
            directory: u64_at(off::unlink_req_off::INODE),
            name: grant_str(raw, off::unlink_req_off::GRANT, off::unlink_req_off::PATH_LEN, ipc, peer)?,
        },
        RequestNumber::RemoveDir => RequestBody::RemoveDir {
            directory: u64_at(off::unlink_req_off::INODE),
            name: grant_str(raw, off::unlink_req_off::GRANT, off::unlink_req_off::PATH_LEN, ipc, peer)?,
        },
        RequestNumber::Unmount => RequestBody::Unmount,
        RequestNumber::Sync => RequestBody::Sync,
        RequestNumber::NewDriver => RequestBody::NewDriver {
            device: u64_at(off::new_driver_req_off::DEVICE),
            label: grant_str(raw, off::new_driver_req_off::GRANT, off::new_driver_req_off::PATH_LEN, ipc, peer)?,
        },
        RequestNumber::Flush => RequestBody::Flush {
            device: u64_at(0),
        },
        RequestNumber::Read => RequestBody::Read {
            inode: u64_at(off::transfer_req_off::INODE),
            position: i64_at(off::transfer_req_off::SEEK_POS),
            length: u64_at(off::transfer_req_off::BYTES) as usize,
        },
        RequestNumber::Write => RequestBody::Write {
            inode: u64_at(off::transfer_req_off::INODE),
            position: i64_at(off::transfer_req_off::SEEK_POS),
            length: u64_at(off::transfer_req_off::BYTES) as usize,
        },
        RequestNumber::MakeNode => RequestBody::MakeNode {
            directory: u64_at(off::mknod_req_off::INODE),
            name: grant_str(raw, off::mknod_req_off::GRANT, off::mknod_req_off::PATH_LEN, ipc, peer)?,
            mode: u32_at(off::mknod_req_off::MODE),
            owner: u32_at(off::mknod_req_off::UID),
            group: u32_at(off::mknod_req_off::GID),
            device: u64_at(off::mknod_req_off::DEVICE),
        },
        RequestNumber::MakeDir => RequestBody::MakeDir {
            directory: u64_at(off::mkdir_req_off::INODE),
            name: grant_str(raw, off::mkdir_req_off::GRANT, local_off::mkdir_len::PATH_LEN, ipc, peer)?,
            mode: u32_at(off::mkdir_req_off::MODE),
            owner: u32_at(off::mkdir_req_off::UID),
            group: u32_at(off::mkdir_req_off::GID),
        },
        RequestNumber::Create => RequestBody::Create {
            directory: u64_at(off::create_req_off::INODE),
            name: grant_str(raw, off::create_req_off::GRANT, off::create_req_off::PATH_LEN, ipc, peer)?,
            mode: u32_at(off::create_req_off::MODE),
            owner: u32_at(off::create_req_off::UID),
            group: u32_at(off::create_req_off::GID),
        },
        RequestNumber::Link => RequestBody::Link {
            directory: u64_at(off::link_req_off::DIR_INO),
            name: grant_str(raw, off::link_req_off::GRANT, off::link_req_off::PATH_LEN, ipc, peer)?,
            inode: u64_at(off::link_req_off::INODE),
        },
        RequestNumber::Rename => RequestBody::Rename {
            old_directory: u64_at(off::rename_req_off::DIR_OLD),
            old_name: grant_str(raw, off::rename_req_off::GRANT_OLD, off::rename_req_off::LEN_OLD, ipc, peer)?,
            new_directory: u64_at(off::rename_req_off::DIR_NEW),
            new_name: grant_str(raw, off::rename_req_off::GRANT_NEW, off::rename_req_off::LEN_NEW, ipc, peer)?,
        },
        RequestNumber::Lookup => {
            let path =
                grant_buf(raw, off::lookup_req_off::GRANT_PATH, off::lookup_req_off::PATH_LEN, ipc, peer)?;
            // Credentials: the VFS encoder does not send them yet
            // (`ucred_size == 0` plus a gate on `PATH_GET_UCRED`), so the
            // decode mirrors that state — defaults, and the flag ignored
            // until both ends grow the grant.
            let _ = u64_at(off::lookup_req_off::UCRED_SIZE);
            let flags = LookupFlags(u32_at(off::lookup_req_off::FLAGS));
            let _ = flags;
            RequestBody::Lookup {
                start_directory: u64_at(off::lookup_req_off::DIR_INO),
                root_inode: u64_at(off::lookup_req_off::ROOT_INO),
                path: {
                    let mut s = String::from_utf8_lossy(&path).into_owned();
                    if s.ends_with('\0') {
                        s.pop();
                    }
                    s
                },
                flags,
                credentials: {
                    // ucred_size == 0: the VFS encoder does not send
                    // credentials yet, so the decode fills the neutral
                    // identity (no user, no group, no supplements).
                    minix_fs::lookup::Credentials {
                        user: 0,
                        group: 0,
                        extra_groups: [0; 16],
                        extra_group_count: 0,
                    }
                },
            }
        }
        RequestNumber::MountPoint => RequestBody::MountPoint {
            inode: u64_at(local_off::mountpoint::INODE),
        },
        RequestNumber::NewNode => {
            let _ = u32_at(local_off::newnode::MODE);
            RequestBody::NewNode {
                mode: u32_at(local_off::newnode::MODE),
                owner: u32_at(local_off::newnode::UID),
                group: u32_at(local_off::newnode::GID),
                device: u64_at(local_off::newnode::DEVICE),
            }
        }
        RequestNumber::ReadLink => RequestBody::ReadLink {
            inode: u64_at(off::rdlink_req_off::INODE),
            capacity: u64_at(off::rdlink_req_off::MEM_SIZE) as usize,
        },
        RequestNumber::GetDents => RequestBody::GetDents {
            inode: u64_at(off::getdents_req_off::INODE),
            position: i64_at(off::getdents_req_off::SEEK_POS),
            capacity: u64_at(off::getdents_req_off::MEM_SIZE) as usize,
        },
        RequestNumber::Peek => RequestBody::Peek {
            inode: u64_at(off::transfer_req_off::INODE),
            position: i64_at(off::transfer_req_off::SEEK_POS),
            length: u64_at(off::transfer_req_off::BYTES) as usize,
        },
        RequestNumber::BlockPeek => RequestBody::BlockPeek {
            device: u64_at(local_off::breadwrite::DEVICE),
            position: i64_at(local_off::breadwrite::SEEK_POS),
            length: u64_at(local_off::breadwrite::NBYTES) as usize,
        },
        RequestNumber::GetNode => return Err(Errno::from_i32(EINVAL)),
    };
    Ok(body)
}

/// The data-plane grant of one request: the window the `copy_in`/`copy_out`
/// hooks stream through (the read grant, the write grant, the listing
/// window, the status buffer, the link-target buffer). Requests without a
/// data plane report the invalid grant.
pub fn data_grant(request: RequestNumber, msg: &Message) -> i32 {
    // SAFETY: bounded reads inside the payload area.
    let raw = unsafe { &msg.m_u.raw };
    let g32 = |at: usize| i32::from_le_bytes(raw[at..at + 4].try_into().unwrap());
    match request {
        RequestNumber::Read
        | RequestNumber::Write
        | RequestNumber::GetDents => g32(off::transfer_req_off::GRANT),
        RequestNumber::Stat => g32(off::stat_req_off::GRANT),
        RequestNumber::ReadLink => g32(off::rdlink_req_off::GRANT),
        RequestNumber::StatVfs => g32(local_off::statvfs::GRANT),
        RequestNumber::BlockRead | RequestNumber::BlockWrite => {
            g32(local_off::breadwrite::GRANT)
        }
        RequestNumber::Lookup => g32(off::lookup_req_off::GRANT_PATH),
        _ => minix_types::GRANT_INVALID,
    }
}

/// Encode a reply the way the VFS decoders read it.
///
/// `request` disambiguates the node payload: the mount and new-node
/// replies carry the device (`node_details`-shaped layouts), the create
/// reply does not (`mess_fs_vfs_create` has no device field). Returns the
/// message with `m_type = TRNS_ADD_ID(status, transaction)`
/// (`fsdriver.c:50-56`).
pub fn encode_reply(request: RequestNumber, reply: &FsReply) -> Message {
    let mut msg = Message {
        m_type: TransactionId::encode_reply(reply.status, reply.transaction),
        ..Message::default()
    };
    // SAFETY: bounded writes inside the payload area.
    let raw = unsafe { &mut msg.m_u.raw };
    match &reply.payload {
        ReplyPayload::Empty => {}
        ReplyPayload::Transfer(t) => {
            put8(raw, off::transfer_reply_off::SEEK_POS, t.new_position as u64);
            put8(raw, off::transfer_reply_off::NBYTES, t.transferred as u64);
        }
        ReplyPayload::Node(node) => encode_node(request, node, raw),
        ReplyPayload::Lookup(outcome) => encode_lookup(outcome, raw),
    }
    msg
}

/// Write one `u64` into the payload (helper so the encoders read as the
/// offset tables do).
fn put8(raw: &mut [u8], at: usize, v: u64) {
    raw[at..at + 8].copy_from_slice(&v.to_le_bytes());
}

/// Write one `u32` into the payload.
fn put4(raw: &mut [u8], at: usize, v: u32) {
    raw[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

/// Write one `u16` into the payload.
fn put2(raw: &mut [u8], at: usize, v: u16) {
    raw[at..at + 2].copy_from_slice(&v.to_le_bytes());
}

/// Bytes of an absolute-symlink rewrite to stream back through the path
/// grant, if the outcome carries one (`ESYMLINK` replies restart resolution
/// with the rewritten path; the bytes leave through the same grant the
/// request came in on).
pub fn lookup_rewrite_bytes(reply: &FsReply) -> Option<Vec<u8>> {
    match &reply.payload {
        ReplyPayload::Lookup(LookupOutcome::AbsoluteSymlink { path, path_length, .. }) => {
            Some(path[..*path_length].to_vec())
        }
        _ => None,
    }
}

fn encode_node(request: RequestNumber, node: &FileNode, raw: &mut [u8]) {
    match request {
        // Mount reply: `node_details` plus flags and the concurrency window
        // (the VFS decoder reads `file_size@0, device@8, inode@16,
        // flags@24, mode@28, uid@32, gid@36, con_reqs@40`). The FS-side
        // flags/con_reqs knobs are runtime state this runtime leaves at
        // zero (single-threaded server, no threaded-requests capability
        // yet).
        RequestNumber::ReadSuper => {
            put8(raw, 0, node.size as u64);
            put8(raw, 8, node.device);
            put8(raw, 16, node.inode_number);
            put4(raw, 24, 0);
            put4(raw, 28, node.mode);
            put4(raw, 32, node.owner);
            put4(raw, 36, node.group);
            put2(raw, 40, 0);
        }
        // New-node reply shares the create layout with the device appended
        // (`mess_fs_vfs_newnode { file_size, inode, mode, uid, gid,
        // device }`, request.c:647-655); the create reply itself stops at
        // gid. The VFS decoders read the same offsets.
        _ => {
            put8(raw, off::create_reply_off::FILE_SIZE, node.size as u64);
            put8(raw, off::create_reply_off::INODE, node.inode_number);
            put4(raw, off::create_reply_off::MODE, node.mode);
            put4(raw, off::create_reply_off::UID, node.owner);
            put4(raw, off::create_reply_off::GID, node.group);
            if request == RequestNumber::NewNode {
                put8(raw, 28, node.device);
            }
        }
    }
}

fn encode_lookup(outcome: &LookupOutcome, raw: &mut [u8]) {
    match outcome {
        LookupOutcome::Found(node) => {
            put8(raw, off::lookup_reply_off::OFFSET, 0);
            put8(raw, off::lookup_reply_off::FILE_SIZE, node.size as u64);
            put8(raw, off::lookup_reply_off::DEVICE, node.device);
            put8(raw, off::lookup_reply_off::INODE, node.inode_number);
            put4(raw, off::lookup_reply_off::MODE, node.mode);
            put4(raw, off::lookup_reply_off::UID, node.owner);
            put4(raw, off::lookup_reply_off::GID, node.group);
            put4(raw, off::lookup_reply_off::SYMLOOP, 0);
        }
        LookupOutcome::EnterMount { inode, offset, links_resolved } => {
            put8(raw, off::lookup_reply_off::OFFSET, *offset as u64);
            put8(raw, off::lookup_reply_off::INODE, *inode);
            put4(raw, off::lookup_reply_off::SYMLOOP, *links_resolved);
        }
        LookupOutcome::LeaveMount { offset, links_resolved } => {
            put8(raw, off::lookup_reply_off::OFFSET, *offset as u64);
            put4(raw, off::lookup_reply_off::SYMLOOP, *links_resolved);
        }
        LookupOutcome::AbsoluteSymlink { offset, links_resolved, .. } => {
            // Status rides the message type (`ESYMLINK`); the rewritten
            // path leaves through the path grant (streamed by the runtime
            // before this message is sent).
            put8(raw, off::lookup_reply_off::OFFSET, *offset as u64);
            put4(raw, off::lookup_reply_off::SYMLOOP, *links_resolved);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::ScriptedRtIpc;

    fn decode(request: RequestNumber, msg: &Message) -> (Result<RequestBody, Errno>, ScriptedRtIpc) {
        // Default name payload every grant read returns; tests that care
        // overwrite it.
        let mut ipc = ScriptedRtIpc::staging(Vec::new(), b"alpha\0".to_vec());
        let body = decode_body(request, msg, &mut ipc, minix_types::Endpoint::VFS);
        (body, ipc)
    }

    fn msg_with(fill: impl Fn(&mut [u8])) -> Message {
        let mut m = Message::default();
        // SAFETY(test): tests write payloads through the shared raw view.
        unsafe { fill(&mut m.m_u.raw) };
        m
    }

    #[test]
    fn test_decode_mount_roundtrip() {
        // The VFS encoder (request.rs::encode_readsuper) writes this shape;
        // decode must read back the same fields.
        let m = msg_with(|raw| {
            raw[0..8].copy_from_slice(&0x0301u64.to_le_bytes()); // device
            raw[8..12].copy_from_slice(&1u32.to_le_bytes()); // REQ_RDONLY
            raw[16..24].copy_from_slice(&6u64.to_le_bytes()); // path_len
            raw[24..28].copy_from_slice(&7i32.to_le_bytes()); // grant
        });
        let (body, ipc) = decode(RequestNumber::ReadSuper, &m);
        // One grant read happened, for the label, against the VFS endpoint.
        assert_eq!(ipc.reads.len(), 1);
        assert_eq!(ipc.reads[0], (minix_types::Endpoint::VFS, 7, 0, 6));
        match body.unwrap() {
            RequestBody::ReadSuper { device, flags, label } => {
                assert_eq!(device, 0x0301);
                assert_eq!(flags, MountFlags::READ_ONLY);
                assert_eq!(label, "alpha");
            }
            other => panic!("wrong body: {other:?}"),
        }
    }

    #[test]
    fn test_decode_transfer_and_data_grant() {
        let m = msg_with(|raw| {
            raw[0..8].copy_from_slice(&5u64.to_le_bytes());
            raw[8..16].copy_from_slice(&120i64.to_le_bytes());
            raw[16..20].copy_from_slice(&9i32.to_le_bytes());
            raw[24..32].copy_from_slice(&64u64.to_le_bytes());
        });
        let (body, ipc) = decode(RequestNumber::Read, &m);
        assert!(ipc.reads.is_empty(), "transfer carries no name");
        match body.unwrap() {
            RequestBody::Read { inode, position, length } => {
                assert_eq!((inode, position, length), (5, 120, 64));
            }
            other => panic!("wrong body: {other:?}"),
        }
        assert_eq!(data_grant(RequestNumber::Read, &m), 9);
    }

    #[test]
    fn test_decode_unlink_symmetric_with_vfs_encoder() {
        // Shape mirrored from request.rs::encode_unlink: dir_ino, len, grant.
        let m = msg_with(|raw| {
            raw[0..8].copy_from_slice(&11u64.to_le_bytes());
            raw[8..16].copy_from_slice(&4u64.to_le_bytes());
            raw[16..20].copy_from_slice(&3i32.to_le_bytes());
        });
        let mut ipc = ScriptedRtIpc::staging(Vec::new(), b"ab\0".to_vec());
        let body = decode_body(RequestNumber::Unlink, &m, &mut ipc, minix_types::Endpoint::VFS).unwrap();
        match body {
            RequestBody::Unlink { directory, name } => {
                assert_eq!(directory, 11);
                assert_eq!(name, "ab");
            }
            other => panic!("wrong body: {other:?}"),
        }
    }

    #[test]
    fn test_encode_transfer_reply_matches_vfs_decoder() {
        let reply = FsReply {
            status: 0,
            transaction: TransactionId(4),
            payload: ReplyPayload::Transfer(minix_fs::call::TransferReply {
                new_position: 184,
                transferred: 64,
            }),
        };
        let m = encode_reply(RequestNumber::Read, &reply);
        assert_eq!(m.m_type, TransactionId::encode_reply(0, TransactionId(4)));
        // SAFETY(test): reading back the shared offsets.
        let raw = unsafe { &m.m_u.raw };
        assert_eq!(
            i64::from_le_bytes(raw[off::transfer_reply_off::SEEK_POS..off::transfer_reply_off::SEEK_POS + 8].try_into().unwrap()),
            184
        );
        assert_eq!(
            u64::from_le_bytes(raw[off::transfer_reply_off::NBYTES..off::transfer_reply_off::NBYTES + 8].try_into().unwrap()),
            64
        );
    }

    #[test]
    fn test_encode_mount_reply_matches_vfs_decoder() {
        let reply = FsReply {
            status: 0,
            transaction: TransactionId(1),
            payload: ReplyPayload::Node(FileNode {
                inode_number: 2,
                mode: 0o040755,
                size: 512,
                owner: 0,
                group: 0,
                device: 0x0301,
            }),
        };
        let m = encode_reply(RequestNumber::ReadSuper, &reply);
        // The VFS decoder (request.rs::decode_readsuper_reply) reads these
        // exact offsets.
        // SAFETY(test): reading back the mount reply layout.
        let raw = unsafe { &m.m_u.raw };
        let u64_at = |o: usize| u64::from_le_bytes(raw[o..o + 8].try_into().unwrap());
        let u32_at = |o: usize| u32::from_le_bytes(raw[o..o + 4].try_into().unwrap());
        assert_eq!(u64_at(0), 512); // size
        assert_eq!(u64_at(8), 0x0301); // device
        assert_eq!(u64_at(16), 2); // inode
        assert_eq!(u32_at(28), 0o040755); // mode
    }

    #[test]
    fn test_lookup_rewrite_bytes() {
        let reply = FsReply {
            status: minix_types::ESYMLINK,
            transaction: TransactionId(2),
            payload: ReplyPayload::Lookup(LookupOutcome::AbsoluteSymlink {
                path: {
                    let mut p = [0u8; minix_fs::lookup::PATH_MAX];
                    p[..3].copy_from_slice(b"/x\0");
                    alloc::boxed::Box::new(p)
                },
                path_length: 3,
                offset: 0,
                links_resolved: 1,
            }),
        };
        let bytes = lookup_rewrite_bytes(&reply).unwrap();
        assert_eq!(&bytes, b"/x\0");
    }

    #[test]
    fn test_fetch_name_refuses_oversize() {
        let mut ipc = ScriptedRtIpc::default();
        assert_eq!(
            fetch_name(&mut ipc, minix_types::Endpoint::VFS, 1, minix_fs::lookup::PATH_MAX + 1).unwrap_err(),
            Errno::from_i32(EINVAL)
        );
        // A window without a terminator is refused too (utility.c:38-45).
        ipc.grant_data = (*b"abcd").to_vec();
        assert_eq!(
            fetch_name(&mut ipc, minix_types::Endpoint::VFS, 1, 4).unwrap_err(),
            Errno::from_i32(EINVAL)
        );
    }
}
