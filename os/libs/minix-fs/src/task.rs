//! The server task loop: receive, classify, adapt, reply.
//!
//! C correspondence: `fsdriver_task` (`minix3/minix/lib/libfsdriver/fsdriver.c:79-97`)
//! and the request half of `fsdriver_process` (`fsdriver.c:17-62`). The C
//! loop receives a raw message, hands it to the process function, and sends
//! the reply; the process function classifies the message and calls the one
//! table entry the request names. This module keeps that shape while
//! staying independent of any kernel interface: the event source behind
//! [`FsTransport::receive`] and the byte moves behind [`FsTransport::copy_in`]
//! and [`FsTransport::copy_out`] are the transport's business, exactly like
//! grants are the business of the C adapters' caller.
//!
//! Two structures carry what the raw C message carries:
//! - [`RequestBody`] holds the typed fields of one dispatchable request.
//!   Names travel as owned strings because the C adapter copies the name
//!   out of its grant into a local buffer before anyone looks at it
//!   (`fsdriver_getname`), so ownership, not borrowing, is the faithful
//!   rendering.
//! - [`FsReply`] holds the status, the echoed transaction identifier, and
//!   the reply payload; the payloads mirror the reply fields the C
//!   adapters fill (`m_fs_vfs_readwrite`, `m_fs_vfs_newnode`,
//!   `m_fs_vfs_lookup`).
//!
//! The byte-oriented replies (reads, listings, status buffers) never travel
//! in the payload: the loop streams them through the transport's copy hooks
//! while the driver produces them, which is the grant-write path of the C
//! code.

use alloc::string::String;
use alloc::vec::Vec;

use minix_types::{EINVAL, ENOSYS, Errno};

use crate::call::{
    adapt_block_read, adapt_block_write, adapt_change_mode, adapt_change_owner,
    adapt_create, adapt_flush, adapt_get_dents, adapt_inhibit_read, adapt_link,
    adapt_make_dir, adapt_make_node, adapt_mount, adapt_mount_point, adapt_new_driver,
    adapt_new_node, adapt_peek, adapt_put_node, adapt_read, adapt_read_link,
    adapt_remove_dir, adapt_rename, adapt_stat, adapt_stat_vfs, adapt_symbolic_link,
    adapt_unlink,
    adapt_sync, adapt_truncate, adapt_unmount, adapt_update_times, adapt_write, check_block_peek,
    select_peek_path, PeekPath, TransferInput, TransferReply,
};
use crate::driver::{FsDriver, Server};
use crate::lookup::{resolve_path, Credentials, LookupInput, LookupOutcome};
use crate::protocol::{FileNode, LookupFlags, MountFlags, TransactionId};

/// One received message envelope: who sent it, whether the kernel flagged it
/// as a notification, and the raw message type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Envelope {
    /// Endpoint the message arrived from.
    pub source: i32,
    /// Whether the kernel reports this as a notification.
    pub is_notification: bool,
    /// Raw message type (request number and transaction identifier packed).
    pub message_type: i32,
}

/// The typed fields of one dispatchable request.
///
/// One variant per table entry the C `fsdriver` adapters serve, with names
/// and link targets already fetched into owned buffers. The data plane
/// (grant reads and writes) is not part of the body: reads stream out
/// through [`FsTransport::copy_out`], writes stage in through
/// [`FsTransport::copy_in`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestBody {
    /// Mount the file system (`ReadSuper`).
    ReadSuper {
        /// Device to mount from.
        device: u64,
        /// Mount flags.
        flags: MountFlags,
        /// Driver label, fetched by the transport (`fsdriver_getname`,
        /// `call.c:36-41`).
        label: alloc::string::String,
    },
    /// Release references on an inode (`PutNode`).
    PutNode {
        /// Inode being released.
        inode: u64,
        /// Reference count to drop.
        count: u32,
    },
    /// Create a symbolic link (`SymbolicLink`).
    SymbolicLink {
        /// Directory receiving the link.
        directory: u64,
        /// Link name.
        name: String,
        /// Owner user identifier.
        owner: u32,
        /// Owner group identifier.
        group: u32,
        /// Link target bytes.
        target: Vec<u8>,
    },
    /// Truncate a file (`Truncate`).
    Truncate {
        /// Inode to truncate.
        inode: u64,
        /// Start of the kept range.
        start: i64,
        /// End of the kept range.
        end: i64,
    },
    /// Change owner and group (`ChangeOwner`).
    ChangeOwner {
        /// Inode to change.
        inode: u64,
        /// New owner.
        owner: u32,
        /// New group.
        group: u32,
    },
    /// Change permission bits (`ChangeMode`).
    ChangeMode {
        /// Inode to change.
        inode: u64,
        /// New mode.
        mode: u32,
    },
    /// Cancel read-ahead after a seek (`InhibitRead`).
    InhibitRead {
        /// Inode whose read-ahead stops.
        inode: u64,
    },
    /// Read full file status (`Stat`).
    Stat {
        /// Inode to describe.
        inode: u64,
    },
    /// Set access and modification times (`UpdateTimes`).
    UpdateTimes {
        /// Inode to stamp.
        inode: u64,
        /// Access time as seconds plus nanoseconds.
        accessed: (i64, i64),
        /// Modification time as seconds plus nanoseconds.
        modified: (i64, i64),
    },
    /// Read file system statistics (`StatVfs`).
    StatVfs,
    /// Read raw device blocks (`BlockRead`).
    BlockRead {
        /// Device to read.
        device: u64,
        /// Starting device offset.
        position: i64,
        /// Byte count.
        length: usize,
    },
    /// Write raw device blocks (`BlockWrite`).
    BlockWrite {
        /// Device to write.
        device: u64,
        /// Starting device offset.
        position: i64,
        /// Byte count.
        length: usize,
    },
    /// Remove a name (`Unlink`).
    Unlink {
        /// Directory holding the name.
        directory: u64,
        /// Name to remove.
        name: String,
    },
    /// Remove a directory (`RemoveDir`).
    RemoveDir {
        /// Directory holding the name.
        directory: u64,
        /// Name to remove.
        name: String,
    },
    /// Unmount the file system (`Unmount`).
    Unmount,
    /// Flush cached state to storage (`Sync`).
    Sync,
    /// Rebind a driver label (`NewDriver`).
    NewDriver {
        /// Device being rebound.
        device: u64,
        /// Driver label.
        label: String,
    },
    /// Flush and invalidate a device (`Flush`).
    Flush {
        /// Device to drop.
        device: u64,
    },
    /// Read file bytes (`Read`).
    Read {
        /// Inode to read.
        inode: u64,
        /// Starting file offset.
        position: i64,
        /// Byte count.
        length: usize,
    },
    /// Write file bytes (`Write`).
    Write {
        /// Inode to write.
        inode: u64,
        /// Starting file offset.
        position: i64,
        /// Byte count.
        length: usize,
    },
    /// Create a device node (`MakeNode`).
    MakeNode {
        /// Directory receiving the node.
        directory: u64,
        /// Node name.
        name: String,
        /// Mode bits.
        mode: u32,
        /// Owner user identifier.
        owner: u32,
        /// Owner group identifier.
        group: u32,
        /// Device number.
        device: u64,
    },
    /// Create a directory (`MakeDir`).
    MakeDir {
        /// Directory receiving the child.
        directory: u64,
        /// Child name.
        name: String,
        /// Mode bits.
        mode: u32,
        /// Owner user identifier.
        owner: u32,
        /// Owner group identifier.
        group: u32,
    },
    /// Create a regular file (`Create`).
    Create {
        /// Directory receiving the file.
        directory: u64,
        /// File name.
        name: String,
        /// Mode bits.
        mode: u32,
        /// Owner user identifier.
        owner: u32,
        /// Owner group identifier.
        group: u32,
    },
    /// Create a hard link (`Link`).
    Link {
        /// Directory receiving the link.
        directory: u64,
        /// Link name.
        name: String,
        /// Linked inode.
        inode: u64,
    },
    /// Rename a name, possibly across directories (`Rename`).
    Rename {
        /// Directory holding the old name.
        old_directory: u64,
        /// Old name.
        old_name: String,
        /// Directory receiving the new name.
        new_directory: u64,
        /// New name.
        new_name: String,
    },
    /// Resolve one path (`Lookup`).
    Lookup {
        /// Directory where resolution starts.
        start_directory: u64,
        /// Process root (the framework supplies the file system root from
        /// its own mount state, like the C globals do).
        root_inode: u64,
        /// Path to resolve.
        path: String,
        /// Lookup control flags.
        flags: LookupFlags,
        /// Caller credentials.
        credentials: Credentials,
    },
    /// Check for a mount point (`MountPoint`).
    MountPoint {
        /// Inode to inspect.
        inode: u64,
    },
    /// Allocate an unnamed inode (`NewNode`).
    NewNode {
        /// Requested mode.
        mode: u32,
        /// Owner user identifier.
        owner: u32,
        /// Owner group identifier.
        group: u32,
        /// Device number for device nodes.
        device: u64,
    },
    /// Read a symbolic link target (`ReadLink`).
    ReadLink {
        /// Link inode.
        inode: u64,
        /// Byte capacity of the target.
        capacity: usize,
    },
    /// List directory entries (`GetDents`).
    GetDents {
        /// Directory inode.
        inode: u64,
        /// Resume position.
        position: i64,
        /// Byte capacity offered.
        capacity: usize,
    },
    /// Expose a device page to memory without copying (`BlockPeek`).
    BlockPeek {
        /// Device to expose.
        device: u64,
        /// Starting device offset.
        position: i64,
        /// Byte count.
        length: usize,
    },
    /// Expose a file page to memory without copying (`Peek`).
    Peek {
        /// Inode to expose.
        inode: u64,
        /// Starting file offset.
        position: i64,
        /// Byte count.
        length: usize,
    },
}

/// What one receive hands the loop.
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    /// A notification or a message from another endpoint: the `other` hook
    /// runs, nothing is replied (`fsdriver.c:26-31`).
    Other(Envelope),
    /// A file system request number with no dispatch slot (the reserved
    /// `GetNode` or anything out of range): answered "not implemented"
    /// (`fsdriver.c:52-56`).
    Unserved(Envelope),
    /// A dispatchable request with its typed fields.
    Request(Envelope, RequestBody),
    /// The receive was cancelled (`fsdriver_terminate` cancels the pending
    /// receive, `fsdriver.c:71-73`); the loop leaves without replying.
    Cancelled,
}

/// Payload carried beside the status in a reply.
#[derive(Debug, Clone, PartialEq)]
pub enum ReplyPayload {
    /// Status only (most refusals and notifications-shaped answers).
    Empty,
    /// Position and count answers (`m_fs_vfs_readwrite`, getdents, rdlink,
    /// peek, block transfers). Unused position fields stay zero.
    Transfer(TransferReply),
    /// Node descriptors (`ReadSuper`, `NewNode`, `Create`).
    Node(FileNode),
    /// Lookup redirections and results (`m_fs_vfs_lookup`).
    Lookup(LookupOutcome),
}

/// A complete reply: status, echoed transaction identifier, payload.
#[derive(Debug, Clone, PartialEq)]
pub struct FsReply {
    /// Status code: zero or a negative error number.
    pub status: i32,
    /// Transaction identifier echoed from the request.
    pub transaction: TransactionId,
    /// Reply fields beside the status.
    pub payload: ReplyPayload,
}

impl FsReply {
    /// Status-only reply.
    pub const fn status(status: i32, transaction: TransactionId) -> Self {
        Self {
            status,
            transaction,
            payload: ReplyPayload::Empty,
        }
    }
}

/// The outside world of the task loop.
///
/// The production implementation belongs to the server runtime (receiving
/// through the kernel interface, grants behind the copy hooks); tests use
/// scripted transports. This split is what lets the whole dispatch ladder
/// run without any kernel.
pub trait FsTransport {
    /// Wait for the next message.
    fn receive(&mut self) -> Incoming;
    /// Send a reply for one request.
    fn reply(&mut self, to: i32, reply: FsReply);
    /// Move request bytes in: stage the next slice of the write payload at
    /// `offset` into `out`.
    fn copy_in(&mut self, offset: usize, out: &mut [u8]) -> Result<(), Errno>;
    /// Move result bytes out: publish `bytes` at `offset` of the reply
    /// payload (read data, listing entries, status buffers).
    fn copy_out(&mut self, offset: usize, bytes: &[u8]);
}

/// Run the task loop until termination is requested and the file system is
/// unmounted (`fsdriver_task`, `fsdriver.c:79-97`).
pub fn run<D: FsDriver, T: FsTransport>(server: &mut Server<D>, transport: &mut T) {
    while server.should_continue() {
        match transport.receive() {
            Incoming::Other(envelope) => {
                server.driver.other(envelope.is_notification);
            }
            Incoming::Cancelled => break,
            Incoming::Unserved(envelope) => {
                let (_, transaction) = TransactionId::decode(envelope.message_type);
                transport.reply(envelope.source, FsReply::status(ENOSYS, transaction));
                server.driver.post_call();
            }
            Incoming::Request(envelope, body) => {
                let (_, transaction) = TransactionId::decode(envelope.message_type);
                // The mount gate from `fsdriver_process` (`fsdriver.c:46-47`):
                // an unmounted server answers everything but the mount
                // request with "invalid".
                let reply = if !matches!(body, RequestBody::ReadSuper { .. })
                    && !server.state.mount.is_mounted()
                {
                    FsReply::status(EINVAL, transaction)
                } else {
                    dispatch(server, body, transport, transaction)
                };
                transport.reply(envelope.source, reply);
                server.driver.post_call();
            }
        }
    }
}

/// Adapt one dispatchable request: fields from the body, data plane through
/// the transport, state updates on the server.
fn dispatch<D: FsDriver, T: FsTransport>(
    server: &mut Server<D>,
    body: RequestBody,
    transport: &mut T,
    transaction: TransactionId,
) -> FsReply {
    let error = |status: i32| FsReply::status(status, transaction);
    let err = |e: Errno| error(e.to_i32());
    let zero = || FsReply::status(0, transaction);
    match body {
        RequestBody::ReadSuper { device, flags, label } => {
            let already = server.state.mount.is_mounted();
            let (file_peek, block_peek, has_backing_device) = server.peek_knobs();
            match adapt_mount(
                &mut server.driver,
                crate::call::MountInput {
                    device,
                    flags,
                    label,
                },
                already,
                has_backing_device,
                file_peek,
                block_peek,
            ) {
                Ok(reply) => {
                    let root = reply.root;
                    server.did_mount(device, root);
                    FsReply {
                        status: 0,
                        transaction,
                        payload: ReplyPayload::Node(root),
                    }
                }
                Err(e) => err(e),
            }
        }
        RequestBody::Unmount => {
            adapt_unmount(&mut server.driver);
            server.did_unmount();
            zero()
        }
        RequestBody::PutNode { inode, count } => match adapt_put_node(&mut server.driver, inode, count) {
            Ok(()) => zero(),
            Err(e) => err(e),
        },
        RequestBody::NewNode {
            mode,
            owner,
            group,
            device,
        } => match adapt_new_node(&mut server.driver, crate::call::NewNodeInput { mode, owner, group, device }) {
            Ok(node) => FsReply {
                status: 0,
                transaction,
                payload: ReplyPayload::Node(node),
            },
            Err(e) => err(e),
        },
        RequestBody::MountPoint { inode } => match adapt_mount_point(&mut server.driver, inode) {
            Ok(()) => zero(),
            Err(e) => err(e),
        },
        RequestBody::Read {
            inode,
            position,
            length,
        } => {
            let input = match TransferInput::new(inode, position, length) {
                Ok(input) => input,
                Err(e) => return err(e),
            };
            let mut moved = 0usize;
            adapt_read(&mut server.driver, input, &mut |bytes: &[u8]| {
                transport.copy_out(moved, bytes);
                moved += bytes.len();
            })
            .map_or_else(&err, |reply| FsReply {
                status: 0,
                transaction,
                payload: ReplyPayload::Transfer(reply),
            })
        }
        RequestBody::Write {
            inode,
            position,
            length,
        } => {
            let input = match TransferInput::new(inode, position, length) {
                Ok(input) => input,
                Err(e) => return err(e),
            };
            // The whole payload stages in one buffer because the driver's
            // write contract is one slice; production transports bound the
            // length by the wire grant before this point.
            let mut data = alloc::vec![0u8; length];
            if let Err(e) = transport.copy_in(0, &mut data) {
                return err(e);
            }
            adapt_write(&mut server.driver, input, &data)
                .map_or_else(&err, |reply| FsReply {
                    status: 0,
                    transaction,
                    payload: ReplyPayload::Transfer(reply),
                })
        }
        RequestBody::BlockPeek {
            device,
            position,
            length,
        } => {
            let (file_peek, block_peek, has_backing_device) = server.peek_knobs();
            let _ = (file_peek, has_backing_device);
            if let Err(e) = check_block_peek(block_peek, position, length) {
                return err(e);
            }
            match server.driver.block_peek(device, position, length) {
                Ok(count) => FsReply {
                    status: 0,
                    transaction,
                    payload: ReplyPayload::Transfer(TransferReply {
                        new_position: 0,
                        transferred: count,
                    }),
                },
                Err(e) => err(e),
            }
        }
        RequestBody::Peek {
            inode,
            position,
            length,
        } => {
            let input = match TransferInput::new(inode, position, length) {
                Ok(input) => input,
                Err(e) => return err(e),
            };
            let (file_peek, _block_peek, has_backing_device) = server.peek_knobs();
            let path = match select_peek_path(file_peek, has_backing_device) {
                Ok(path) => path,
                Err(e) => return err(e),
            };
            let mut moved = 0usize;
            let result = match path {
                PeekPath::Direct => adapt_peek(&mut server.driver, input, path, &mut |driver, input| {
                    driver.peek(input.inode, input.position, input.length)
                }),
                PeekPath::EmulatedThroughRead => adapt_peek(
                    &mut server.driver,
                    input,
                    path,
                    &mut |driver, input| {
                        driver
                            .read(input.inode, input.position, input.length, &mut |bytes: &[u8]| {
                                transport.copy_out(moved, bytes);
                                moved += bytes.len();
                            })
                    },
                ),
            };
            match result {
                Ok(count) => FsReply {
                    status: 0,
                    transaction,
                    payload: ReplyPayload::Transfer(TransferReply {
                        new_position: 0,
                        transferred: count,
                    }),
                },
                Err(e) => err(e),
            }
        }
        RequestBody::GetDents {
            inode,
            position,
            capacity,
        } => {
            let mut resume = position;
            let mut moved = 0usize;
            let result = adapt_get_dents(
                &mut server.driver,
                inode,
                &mut resume,
                capacity,
                &mut |bytes: &[u8]| {
                    transport.copy_out(moved, bytes);
                    moved += bytes.len();
                },
            );
            match result {
                Ok(count) => FsReply {
                    status: 0,
                    transaction,
                    payload: ReplyPayload::Transfer(TransferReply {
                        new_position: resume,
                        transferred: count,
                    }),
                },
                Err(e) => err(e),
            }
        }
        RequestBody::Truncate { inode, start, end } => match adapt_truncate(&mut server.driver, inode, start, end) {
            Ok(()) => zero(),
            Err(e) => err(e),
        },
        RequestBody::InhibitRead { inode } => {
            adapt_inhibit_read(&mut server.driver, inode);
            zero()
        }
        RequestBody::Create {
            directory,
            name,
            mode,
            owner,
            group,
        } => match adapt_create(
            &mut server.driver,
            crate::call::NameInput {
                directory,
                name: &name,
            },
            mode,
            owner,
            group,
        ) {
            Ok(node) => FsReply {
                status: 0,
                transaction,
                payload: ReplyPayload::Node(node),
            },
            Err(e) => err(e),
        },
        RequestBody::MakeDir {
            directory,
            name,
            mode,
            owner,
            group,
        } => match adapt_make_dir(
            &mut server.driver,
            crate::call::NameInput {
                directory,
                name: &name,
            },
            mode,
            owner,
            group,
        ) {
            Ok(()) => zero(),
            Err(e) => err(e),
        },
        RequestBody::MakeNode {
            directory,
            name,
            mode,
            owner,
            group,
            device,
        } => match adapt_make_node(
            &mut server.driver,
            crate::call::NameInput {
                directory,
                name: &name,
            },
            mode,
            owner,
            group,
            device,
        ) {
            Ok(()) => zero(),
            Err(e) => err(e),
        },
        RequestBody::Link {
            directory,
            name,
            inode,
        } => match adapt_link(
            &mut server.driver,
            crate::call::NameInput {
                directory,
                name: &name,
            },
            inode,
        ) {
            Ok(()) => zero(),
            Err(e) => err(e),
        },
        RequestBody::Unlink { directory, name } => {
            match adapt_unlink(
                &mut server.driver,
                crate::call::NameInput {
                    directory,
                    name: &name,
                },
            ) {
                Ok(()) => zero(),
                Err(e) => err(e),
            }
        }
        RequestBody::RemoveDir { directory, name } => {
            match adapt_remove_dir(
                &mut server.driver,
                crate::call::NameInput {
                    directory,
                    name: &name,
                },
            ) {
                Ok(()) => zero(),
                Err(e) => err(e),
            }
        }
        RequestBody::Rename {
            old_directory,
            old_name,
            new_directory,
            new_name,
        } => match adapt_rename(
            &mut server.driver,
            crate::call::NameInput {
                directory: old_directory,
                name: &old_name,
            },
            crate::call::NameInput {
                directory: new_directory,
                name: &new_name,
            },
        ) {
            Ok(()) => zero(),
            Err(e) => err(e),
        },
        RequestBody::SymbolicLink {
            directory,
            name,
            owner,
            group,
            target,
        } => match adapt_symbolic_link(
            &mut server.driver,
            crate::call::NameInput {
                directory,
                name: &name,
            },
            owner,
            group,
            &target,
        ) {
            Ok(()) => zero(),
            Err(e) => err(e),
        },
        RequestBody::ReadLink { inode, capacity } => {
            let mut moved = 0usize;
            let result = adapt_read_link(&mut server.driver, inode, capacity, &mut |bytes: &[u8]| {
                transport.copy_out(moved, bytes);
                moved += bytes.len();
            });
            match result {
                Ok(count) => FsReply {
                    status: 0,
                    transaction,
                    payload: ReplyPayload::Transfer(TransferReply {
                        new_position: 0,
                        transferred: count,
                    }),
                },
                Err(e) => err(e),
            }
        }
        RequestBody::Stat { inode } => {
            let mut stat = minix_types::Stat::zeroed();
            // C `fsdriver_stat`（call.c:733-734）在问驱动前先预填 `st_ino`
            // （`st_dev` 取 `fsdriver_device`，框架侧无该通道，按 call.rs 既有
            // 契约由各 FS 驱动自填——MFS 即如此）。预填 inode 零成本且对齐 C。
            stat.inode = inode;
            match adapt_stat(&mut server.driver, inode, &mut stat) {
                Ok(()) => {
                    // C `fsdriver_stat`（call.c:717-741）：驱动填好后，magic
                    // grant 把**用户 `struct stat`** 缓冲授权给 FS 直写——FS
                    // 必须按 C 布局（`st_mode@8`…）copy_out 到调用方缓冲，
                    // 否则用户缓冲停留在零值（`st_mode=0` → 目录判型失败，
                    // 18-stage `ls` 把 `/bin` 当普通文件的根因）。
                    let user = stat.write_user_stat();
                    // SAFETY: `user` 是 `#[repr(C)]` 无不变量 POD，取其字节
                    // 视图写入 grant；长度用 `USER_STAT_SIZE` 与授权窗口对齐。
                    let bytes = unsafe {
                        core::slice::from_raw_parts(
                            &user as *const _ as *const u8,
                            minix_types::Stat::USER_STAT_SIZE,
                        )
                    };
                    transport.copy_out(0, bytes);
                    zero()
                }
                Err(e) => err(e),
            }
        }
        RequestBody::ChangeOwner {
            inode,
            owner,
            group,
        } => match adapt_change_owner(&mut server.driver, inode, owner, group) {
            Ok(_) => zero(),
            Err(e) => err(e),
        },
        RequestBody::ChangeMode { inode, mode } => {
            match adapt_change_mode(&mut server.driver, inode, mode) {
                Ok(_) => zero(),
                Err(e) => err(e),
            }
        }
        RequestBody::UpdateTimes {
            inode,
            accessed,
            modified,
        } => match adapt_update_times(&mut server.driver, inode, accessed, modified) {
            Ok(()) => zero(),
            Err(e) => err(e),
        },
        RequestBody::StatVfs => {
            // TODO 同 `RequestBody::Stat` 本轮修复的根因：此腿也只 `zero()`
            // 丢弃 `vfs`、从不按 C `struct statvfs` 布局 copy_out 到 grant →
            // `df`/`statvfs` 用户缓冲恒零。df 不属 18-stage 核心命令面（echo/ls/cat），
            // 且须走 `minix_types::statvfs_off` 的真实 C 偏移（非已死的
            // `StatVfs::write_to` 那个 10×u64 自定义布局），故拆为独立后续。
            let mut vfs = minix_types::StatVfs::zeroed();
            match adapt_stat_vfs(&mut server.driver, &mut vfs) {
                Ok(()) => zero(),
                Err(e) => err(e),
            }
        }
        RequestBody::Sync => {
            adapt_sync(&mut server.driver);
            zero()
        }
        RequestBody::NewDriver { device, label } => {
            adapt_new_driver(&mut server.driver, device, &label);
            zero()
        }
        RequestBody::BlockRead {
            device,
            position,
            length,
        } => {
            let input = match crate::call::BlockTransferInput::new(device, position, length) {
                Ok(input) => input,
                Err(e) => return err(e),
            };
            let mut moved = 0usize;
            let _ = position;
            adapt_block_read(&mut server.driver, input, &mut |bytes: &[u8]| {
                transport.copy_out(moved, bytes);
                moved += bytes.len();
            })
            .map_or_else(&err, |reply| FsReply {
                status: 0,
                transaction,
                payload: ReplyPayload::Transfer(reply),
            })
        }
        RequestBody::BlockWrite {
            device,
            position,
            length,
        } => {
            let input = match crate::call::BlockTransferInput::new(device, position, length) {
                Ok(input) => input,
                Err(e) => return err(e),
            };
            let mut data = alloc::vec![0u8; length];
            if let Err(e) = transport.copy_in(0, &mut data) {
                return err(e);
            }
            adapt_block_write(&mut server.driver, input, &data)
                .map_or_else(&err, |reply| FsReply {
                    status: 0,
                    transaction,
                    payload: ReplyPayload::Transfer(reply),
                })
        }
        RequestBody::Flush { device } => {
            adapt_flush(&mut server.driver, device);
            zero()
        }
        RequestBody::Lookup {
            start_directory,
            root_inode,
            path,
            flags,
            credentials,
        } => {
            // The file system root comes from the mount state, the same way
            // the C adapters read `fsdriver_root` (`fsdriver.c:5-7`).
            let filesystem_root = match server.state.mount {
                crate::driver::MountState::Mounted { root_inode, .. } => root_inode,
                crate::driver::MountState::Unmounted => 0,
            };
            let input = LookupInput {
                start_directory,
                root_inode,
                filesystem_root,
                path: &path,
                flags,
                credentials,
            };
            match resolve_path(&mut server.driver, &input) {
                Ok(outcome) => FsReply {
                    status: 0,
                    transaction,
                    payload: ReplyPayload::Lookup(outcome),
                },
                Err(e) => err(e),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::{MountState, NullDriver};
    use crate::protocol::{CapabilityFlags, RequestNumber};
    use alloc::vec;

    /// Records everything the loop does: replies, streamed-out bytes, and
    /// staged-in payloads. The scripted incoming queue drives the loop.
    struct ScriptedTransport {
        incoming: alloc::vec::Vec<Incoming>,
        replies: alloc::vec::Vec<(i32, FsReply)>,
        out_bytes: alloc::vec::Vec<u8>,
        in_bytes: alloc::vec::Vec<u8>,
    }

    impl ScriptedTransport {
        fn new(incoming: alloc::vec::Vec<Incoming>) -> Self {
            Self {
                incoming,
                replies: Vec::new(),
                out_bytes: Vec::new(),
                in_bytes: Vec::new(),
            }
        }
    }

    impl FsTransport for ScriptedTransport {
        fn receive(&mut self) -> Incoming {
            if self.incoming.is_empty() {
                // Nothing left: report the cancelled receive so the loop
                // leaves, like a terminated server's receive does.
                Incoming::Cancelled
            } else {
                self.incoming.remove(0)
            }
        }

        fn reply(&mut self, to: i32, reply: FsReply) {
            self.replies.push((to, reply));
        }

        fn copy_in(&mut self, offset: usize, out: &mut [u8]) -> Result<(), Errno> {
            let fill = 0xABu8;
            for byte in out.iter_mut() {
                *byte = fill;
            }
            let _ = offset;
            self.in_bytes = out.to_vec();
            Ok(())
        }

        fn copy_out(&mut self, offset: usize, bytes: &[u8]) {
            while self.out_bytes.len() < offset + bytes.len() {
                self.out_bytes.push(0);
            }
            self.out_bytes[offset..offset + bytes.len()].copy_from_slice(bytes);
        }
    }

    fn envelope(message_type: i32) -> Envelope {
        Envelope {
            source: crate::protocol::VFS_ENDPOINT,
            is_notification: false,
            message_type,
        }
    }

    /// A driver that mounts and counts calls, for gate tests.
    #[derive(Default)]
    struct MountOnlyDriver {
        others: u32,
        post_calls: u32,
    }

    impl crate::driver::FsDriver for MountOnlyDriver {
        fn mount(
            &mut self,
            device: u64,
            _flags: MountFlags,
            capabilities: &mut CapabilityFlags,
        ) -> Result<FileNode, Errno> {
            *capabilities = CapabilityFlags::EMPTY;
            Ok(FileNode::new(1, 0o040755, 0, 0, 0, device))
        }

        fn other(&mut self, _is_notification: bool) {
            self.others += 1;
        }

        fn post_call(&mut self) {
            self.post_calls += 1;
        }
    }

    fn raw(request: RequestNumber, id: u16) -> i32 {
        TransactionId::encode_request(request.message_type(), TransactionId(id))
    }

    #[test]
    fn test_notification_goes_to_other_without_reply() {
        let incoming = vec![Incoming::Other(Envelope {
            source: 7,
            is_notification: true,
            message_type: 0,
        })];
        let mut transport = ScriptedTransport::new(incoming);
        let mut server: Server<MountOnlyDriver> = Server::new(MountOnlyDriver::default());
        run(&mut server, &mut transport);
        assert_eq!(server.driver.others, 1);
        assert!(transport.replies.is_empty());
        // `other` is not a request: no post-call.
        assert_eq!(server.driver.post_calls, 0);
    }

    #[test]
    fn test_unserved_request_answers_enosys() {
        let incoming = vec![Incoming::Unserved(envelope(raw(RequestNumber::GetNode, 9)))];
        let mut transport = ScriptedTransport::new(incoming);
        let mut server: Server<MountOnlyDriver> = Server::new(MountOnlyDriver::default());
        run(&mut server, &mut transport);
        assert_eq!(transport.replies.len(), 1);
        let (to, reply) = &transport.replies[0];
        assert_eq!(*to, crate::protocol::VFS_ENDPOINT);
        assert_eq!(reply.status, ENOSYS);
        assert_eq!(reply.transaction, TransactionId(9));
        assert_eq!(server.driver.post_calls, 1);
    }

    #[test]
    fn test_unmounted_server_refuses_non_mount_requests() {
        let incoming = vec![Incoming::Request(
            envelope(raw(RequestNumber::Stat, 5)),
            RequestBody::Stat { inode: 1 },
        )];
        let mut transport = ScriptedTransport::new(incoming);
        let mut server: Server<MountOnlyDriver> = Server::new(MountOnlyDriver::default());
        run(&mut server, &mut transport);
        assert_eq!(transport.replies.len(), 1);
        assert_eq!(transport.replies[0].1.status, minix_types::EINVAL);
        // The refusal happens before the driver is consulted: no post-call
        // would still be wrong (the C ladder runs postcall for refusals too).
        assert_eq!(server.driver.post_calls, 1);
    }

    #[test]
    fn test_mount_then_unmount_updates_state_and_replies_node() {
        let incoming = vec![
            Incoming::Request(
                envelope(raw(RequestNumber::ReadSuper, 1)),
                RequestBody::ReadSuper {
                    device: 4,
                    flags: MountFlags::EMPTY,
                    label: alloc::string::String::new(),
                },
            ),
            Incoming::Request(
                envelope(raw(RequestNumber::Unmount, 2)),
                RequestBody::Unmount,
            ),
        ];
        let mut transport = ScriptedTransport::new(incoming);
        let mut server: Server<MountOnlyDriver> = Server::new(MountOnlyDriver::default());
        run(&mut server, &mut transport);
        assert_eq!(transport.replies.len(), 2);
        let (_, mount_reply) = &transport.replies[0];
        assert_eq!(mount_reply.status, 0);
        assert!(matches!(mount_reply.payload, ReplyPayload::Node(_)));
        assert_eq!(server.state.mount, MountState::Unmounted);
        // Both requests answered: two post-calls.
        assert_eq!(server.driver.post_calls, 2);
    }

    #[test]
    fn test_read_streams_bytes_through_copy_out() {
        struct ReadDriver;

        impl crate::driver::FsDriver for ReadDriver {
            fn mount(
                &mut self,
                device: u64,
                _flags: MountFlags,
                capabilities: &mut CapabilityFlags,
            ) -> Result<FileNode, Errno> {
                *capabilities = CapabilityFlags::EMPTY;
                Ok(FileNode::new(1, 0o040755, 0, 0, 0, device))
            }

            fn read(
                &mut self,
                _inode: u64,
                _position: i64,
                length: usize,
                out: &mut dyn FnMut(&[u8]),
            ) -> Result<usize, Errno> {
                let chunk = [7u8; 8];
                let take = length.min(chunk.len());
                out(&chunk[..take]);
                Ok(take)
            }
        }

        let incoming = vec![
            Incoming::Request(
                envelope(raw(RequestNumber::ReadSuper, 1)),
                RequestBody::ReadSuper {
                    device: 4,
                    flags: MountFlags::EMPTY,
                    label: alloc::string::String::new(),
                },
            ),
            Incoming::Request(
                envelope(raw(RequestNumber::Read, 2)),
                RequestBody::Read {
                    inode: 1,
                    position: 0,
                    length: 8,
                },
            ),
        ];
        let mut transport = ScriptedTransport::new(incoming);
        let mut server: Server<ReadDriver> = Server::new(ReadDriver);
        run(&mut server, &mut transport);
        assert_eq!(transport.out_bytes, alloc::vec![7u8; 8]);
        let (_, read_reply) = &transport.replies[1];
        assert_eq!(read_reply.status, 0);
        assert_eq!(
            read_reply.payload,
            ReplyPayload::Transfer(TransferReply {
                new_position: 8,
                transferred: 8,
            })
        );
    }

    /// stat 回复必须像 read 一样经 `copy_out` 落进用户的 magic grant 缓冲，
    /// 且按 C `struct stat` 布局（`st_mode@8`、`st_size@112`）。此前此腿只
    /// 回 status、从不落字节，导致用户缓冲停留在 `st_mode=0`——18-stage
    /// `ls` 把 `/bin` 目录判成普通文件、只打印自身名的根因回归针。
    #[test]
    fn test_stat_streams_struct_stat_through_copy_out() {
        struct StatDriver;

        impl crate::driver::FsDriver for StatDriver {
            fn mount(
                &mut self,
                device: u64,
                _flags: MountFlags,
                capabilities: &mut CapabilityFlags,
            ) -> Result<FileNode, Errno> {
                *capabilities = CapabilityFlags::EMPTY;
                Ok(FileNode::new(1, 0o040755, 0, 0, 0, device))
            }

            fn stat(&mut self, _inode: u64, stat: &mut minix_types::Stat) -> Result<(), Errno> {
                stat.mode = 0o040755; // S_IFDIR | 0755
                stat.size = 4096;
                Ok(())
            }
        }

        let incoming = vec![
            Incoming::Request(
                envelope(raw(RequestNumber::ReadSuper, 1)),
                RequestBody::ReadSuper {
                    device: 4,
                    flags: MountFlags::EMPTY,
                    label: alloc::string::String::new(),
                },
            ),
            Incoming::Request(
                envelope(raw(RequestNumber::Stat, 2)),
                RequestBody::Stat { inode: 1 },
            ),
        ];
        let mut transport = ScriptedTransport::new(incoming);
        let mut server: Server<StatDriver> = Server::new(StatDriver);
        run(&mut server, &mut transport);
        // 整个用户 `struct stat` 缓冲应落到 grant（至少覆盖 st_size@112）。
        assert!(
            transport.out_bytes.len() >= 120,
            "stat 字节必须经 copy_out 达 grant"
        );
        assert_eq!(
            u32::from_le_bytes(transport.out_bytes[8..12].try_into().unwrap()),
            0o040755,
            "st_mode 必须落在 C struct stat 偏移 8"
        );
        assert_eq!(
            i64::from_le_bytes(transport.out_bytes[112..120].try_into().unwrap()),
            4096,
            "st_size 必须落在 C struct stat 偏移 112"
        );
        assert_eq!(transport.replies[1].1.status, 0);
    }

    #[test]
    fn test_null_driver_request_while_mounted_answers_enosys() {
        let incoming = vec![
            Incoming::Request(
                envelope(raw(RequestNumber::ReadSuper, 1)),
                RequestBody::ReadSuper {
                    device: 9,
                    flags: MountFlags::EMPTY,
                    label: alloc::string::String::new(),
                },
            ),
            Incoming::Request(
                envelope(raw(RequestNumber::Sync, 2)),
                RequestBody::Sync,
            ),
            Incoming::Request(
                envelope(raw(RequestNumber::Stat, 3)),
                RequestBody::Stat { inode: 1 },
            ),
        ];
        let mut transport = ScriptedTransport::new(incoming);
        let mut server: Server<NullDriver> = Server::new(NullDriver::default());
        run(&mut server, &mut transport);
        assert!(server.state.mount.is_mounted());
        // Sync answers success (empty default body); Stat answers ENOSYS.
        assert_eq!(transport.replies[1].1.status, 0);
        assert_eq!(transport.replies[2].1.status, ENOSYS);
    }
}
