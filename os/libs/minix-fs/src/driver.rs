//! File server skeleton: the driver trait, the mount state machine, and the
//! message dispatch rules.
//!
//! C correspondence: `minix3/minix/lib/libfsdriver/fsdriver.c` (the main
//! loop, the per-message dispatch, termination) and
//! `minix3/minix/include/minix/fsdriver.h:52-105` (the callback table).
//! The table itself (`table.c`) is expressed as [`RequestNumber`] matching
//! in [`dispatch`] rather than as a function pointer array.
//!
//! The central design decision is to replace the C callback table — a struct
//! full of nullable function pointers — with a Rust trait. A file server
//! implements [`FsDriver`]; the framework calls its methods. Optional
//! operations are trait methods with default bodies that answer "not
//! implemented", which makes the thirty-two dispatch slots visible in one
//! place and lets the compiler check that every implemented operation is
//! actually wired to a request.

use minix_types::{EBUSY, EINVAL, ENOSYS, Errno};

use crate::protocol::{CapabilityFlags, FileNode, MountFlags, RequestNumber, TransactionId};

/// Operations a file server provides to the framework.
///
/// C: `struct fsdriver` (`minix3/minix/include/minix/fsdriver.h:53-105`).
/// Each method corresponds to one `fdr_*` callback; the documentation on
/// each method names the request (or requests) that reach it and the adapter
/// in [`crate::call`] that performs the call.
///
/// Methods that a minimal server may omit have default bodies:
/// - Methods returning `Result<_, Errno>` default to `Err(ENOSYS)`,
///   mirroring the C rule "null callback means not implemented".
/// - The notification hooks (`unmounted`, `synchronized`, `flushed`,
///   `bound_driver`, `sought`, `released_mount`) default to doing nothing,
///   mirroring the C null checks that simply skip the call.
///
/// The trait deliberately stays whole instead of being split per operation
/// group (as Linux splits superblock, inode, and file operations): the wire
/// protocol addresses one server through one table, and splitting the trait
/// would let an implementation satisfy one half while the dispatch expects
/// the other. The six operation groups from the adapters are documented on
/// the methods so readers still see the structure.
pub trait FsDriver {
    // -- Mount group --------------------------------------------------

    /// Mount the file system on a device.
    ///
    /// C: `fdr_mount`. Reached through the `ReadSuper` request. Reports the
    /// root node and the capability flags. Required: without it the server
    /// can never mount.
    fn mount(
        &mut self,
        device: u64,
        flags: MountFlags,
        capabilities: &mut CapabilityFlags,
    ) -> Result<FileNode, Errno>;

    /// Release all state after an unmount.
    ///
    /// C: `fdr_unmount`. Reached through the `Unmount` request. Optional.
    fn unmounted(&mut self) {}

    /// Check whether an inode is a mount point.
    ///
    /// C: `fdr_mountpt`. Reached through the `MountPoint` request.
    fn is_mount_point(&mut self, inode: u64) -> Result<(), Errno> {
        let _ = inode;
        Err(Errno::from_i32(ENOSYS))
    }

    /// Allocate an unnamed inode (pipes and sockets).
    ///
    /// C: `fdr_newnode`. Reached through the `NewNode` request.
    fn new_node(
        &mut self,
        mode: u32,
        owner: u32,
        group: u32,
        device: u64,
    ) -> Result<FileNode, Errno> {
        let _ = (mode, owner, group, device);
        Err(Errno::from_i32(ENOSYS))
    }

    /// Release references on an inode.
    ///
    /// C: `fdr_putnode`. Reached through the `PutNode` request. The default
    /// accepts and ignores the release, matching the C adapter which answers
    /// success when the callback is null (`call.c:110-114`).
    fn put_node(&mut self, inode: u64, count: u32) -> Result<(), Errno> {
        let _ = (inode, count);
        Ok(())
    }

    // -- Data group ---------------------------------------------------

    /// Read file bytes.
    ///
    /// C: `fdr_read`. Reached through `Read` (and, for servers without a
    /// dedicated peek path, through the peek emulation in the adapter).
    fn read(
        &mut self,
        inode: u64,
        position: i64,
        length: usize,
        out: &mut dyn FnMut(&[u8]),
    ) -> Result<usize, Errno> {
        let _ = (inode, position, length, out);
        Err(Errno::from_i32(ENOSYS))
    }

    /// Write file bytes.
    ///
    /// C: `fdr_write`. Reached through the `Write` request.
    fn write(&mut self, inode: u64, position: i64, data: &[u8]) -> Result<usize, Errno> {
        let _ = (inode, position, data);
        Err(Errno::from_i32(ENOSYS))
    }

    /// Expose a file page to virtual memory without copying.
    ///
    /// C: `fdr_peek`. Reached through the `Peek` request. Servers without
    /// backing storage leave this unimplemented and the adapter emulates
    /// peek through `read` instead (`call.c:294-306`).
    fn peek(&mut self, inode: u64, position: i64, length: usize) -> Result<usize, Errno> {
        let _ = (inode, position, length);
        Err(Errno::from_i32(ENOSYS))
    }

    /// List directory entries.
    ///
    /// C: `fdr_getdents`. Reached through the `GetDents` request. The
    /// `position` is both input (where to resume) and output (where the
    /// listing stopped); the adapter writes it back on success.
    fn get_dents(
        &mut self,
        inode: u64,
        position: &mut i64,
        capacity: usize,
        out: &mut dyn FnMut(&[u8]),
    ) -> Result<usize, Errno> {
        let _ = (inode, position, capacity, out);
        Err(Errno::from_i32(ENOSYS))
    }

    /// Truncate or punch a byte range of a file.
    ///
    /// C: `fdr_trunc`. Reached through the `Truncate` request.
    fn truncate(&mut self, inode: u64, start: i64, end: i64) -> Result<(), Errno> {
        let _ = (inode, start, end);
        Err(Errno::from_i32(ENOSYS))
    }

    /// Cancel read-ahead after the caller repositioned the file offset.
    ///
    /// C: `fdr_seek`. Reached through the `InhibitRead` request. Optional;
    /// the adapter always answers success (`call.c:381-393`).
    fn sought(&mut self, _inode: u64) {}

    // -- Namespace group ----------------------------------------------

    /// Resolve one path component inside a directory.
    ///
    /// C: `fdr_lookup`. Used by the lookup helper, not directly by a table
    /// slot. Reports whether the child is a mount point.
    fn lookup_child(&mut self, directory: u64, name: &str) -> Result<(FileNode, bool), Errno> {
        let _ = (directory, name);
        Err(Errno::from_i32(ENOSYS))
    }

    /// Create a regular file and report the new node.
    ///
    /// C: `fdr_create`. Reached through the `Create` request.
    fn create(
        &mut self,
        directory: u64,
        name: &str,
        mode: u32,
        owner: u32,
        group: u32,
    ) -> Result<FileNode, Errno> {
        let _ = (directory, name, mode, owner, group);
        Err(Errno::from_i32(ENOSYS))
    }

    /// Create a directory. C: `fdr_mkdir`, request `MakeDir`.
    fn make_dir(
        &mut self,
        directory: u64,
        name: &str,
        mode: u32,
        owner: u32,
        group: u32,
    ) -> Result<(), Errno> {
        let _ = (directory, name, mode, owner, group);
        Err(Errno::from_i32(ENOSYS))
    }

    /// Create a device node. C: `fdr_mknod`, request `MakeNode`.
    fn make_node(
        &mut self,
        directory: u64,
        name: &str,
        mode: u32,
        owner: u32,
        group: u32,
        device: u64,
    ) -> Result<(), Errno> {
        let _ = (directory, name, mode, owner, group, device);
        Err(Errno::from_i32(ENOSYS))
    }

    /// Create a hard link. C: `fdr_link`, request `Link`.
    fn link(&mut self, directory: u64, name: &str, inode: u64) -> Result<(), Errno> {
        let _ = (directory, name, inode);
        Err(Errno::from_i32(ENOSYS))
    }

    /// Remove a name. C: `fdr_unlink`, request `Unlink`.
    fn unlink(&mut self, directory: u64, name: &str) -> Result<(), Errno> {
        let _ = (directory, name);
        Err(Errno::from_i32(ENOSYS))
    }

    /// Remove a directory. C: `fdr_rmdir`, request `RemoveDir`.
    fn remove_dir(&mut self, directory: u64, name: &str) -> Result<(), Errno> {
        let _ = (directory, name);
        Err(Errno::from_i32(ENOSYS))
    }

    /// Rename a name, possibly across directories.
    /// C: `fdr_rename`, request `Rename`.
    fn rename(
        &mut self,
        old_directory: u64,
        old_name: &str,
        new_directory: u64,
        new_name: &str,
    ) -> Result<(), Errno> {
        let _ = (old_directory, old_name, new_directory, new_name);
        Err(Errno::from_i32(ENOSYS))
    }

    /// Create a symbolic link with the given target bytes.
    /// C: `fdr_slink`, request `SymbolicLink`.
    fn symbolic_link(
        &mut self,
        directory: u64,
        name: &str,
        owner: u32,
        group: u32,
        target: &[u8],
    ) -> Result<(), Errno> {
        let _ = (directory, name, owner, group, target);
        Err(Errno::from_i32(ENOSYS))
    }

    /// Read a symbolic link target, up to `capacity` bytes.
    /// C: `fdr_rdlink`, request `ReadLink`.
    fn read_link(
        &mut self,
        inode: u64,
        capacity: usize,
        out: &mut dyn FnMut(&[u8]),
    ) -> Result<usize, Errno> {
        let _ = (inode, capacity, out);
        Err(Errno::from_i32(ENOSYS))
    }

    // -- Metadata group -----------------------------------------------

    /// Read full file status into the caller-supplied buffer.
    ///
    /// C: `fdr_stat`. Reached through the `Stat` request. The byte layout of
    /// the status buffer is defined by the caller; the default adapter
    /// contract passes a mutable byte slice so servers fill it field by
    /// field without depending on a C structure definition.
    fn stat(&mut self, inode: u64, out: &mut [u8]) -> Result<(), Errno> {
        let _ = (inode, out);
        Err(Errno::from_i32(ENOSYS))
    }

    /// Change owner and group; reports the resulting mode.
    /// C: `fdr_chown`, request `ChangeOwner`.
    fn change_owner(&mut self, inode: u64, owner: u32, group: u32) -> Result<u32, Errno> {
        let _ = (inode, owner, group);
        Err(Errno::from_i32(ENOSYS))
    }

    /// Change permission bits; reports the resulting mode.
    /// C: `fdr_chmod`, request `ChangeMode`.
    fn change_mode(&mut self, inode: u64, mode: u32) -> Result<u32, Errno> {
        let _ = (inode, mode);
        Err(Errno::from_i32(ENOSYS))
    }

    /// Set access and modification times (seconds plus nanoseconds).
    /// C: `fdr_utime`, request `UpdateTimes`.
    fn update_times(
        &mut self,
        inode: u64,
        accessed: (i64, i64),
        modified: (i64, i64),
    ) -> Result<(), Errno> {
        let _ = (inode, accessed, modified);
        Err(Errno::from_i32(ENOSYS))
    }

    /// Read file system statistics into the caller-supplied buffer.
    /// C: `fdr_statvfs`, request `StatVfs`.
    fn stat_vfs(&mut self, out: &mut [u8]) -> Result<(), Errno> {
        let _ = out;
        Err(Errno::from_i32(ENOSYS))
    }

    /// Flush cached state to storage. C: `fdr_sync`, request `Sync`.
    /// Optional; the adapter always answers success.
    fn synchronized(&mut self) {}

    // -- Block group --------------------------------------------------

    /// Remember the driver label for a device.
    ///
    /// C: `fdr_driver`. Called during mount and on `NewDriver`. Optional;
    /// the adapter answers success when it is missing (`call.c:885-886`).
    fn bound_driver(&mut self, _device: u64, _label: &str) {}

    /// Read raw device blocks. C: `fdr_bread`, request `BlockRead`.
    fn block_read(
        &mut self,
        device: u64,
        position: i64,
        length: usize,
        out: &mut dyn FnMut(&[u8]),
    ) -> Result<usize, Errno> {
        let _ = (device, position, length, out);
        Err(Errno::from_i32(ENOSYS))
    }

    /// Write raw device blocks. C: `fdr_bwrite`, request `BlockWrite`.
    fn block_write(&mut self, device: u64, position: i64, data: &[u8]) -> Result<usize, Errno> {
        let _ = (device, position, data);
        Err(Errno::from_i32(ENOSYS))
    }

    /// Expose a device page to virtual memory. C: `fdr_bpeek`.
    fn block_peek(&mut self, device: u64, position: i64, length: usize) -> Result<usize, Errno> {
        let _ = (device, position, length);
        Err(Errno::from_i32(ENOSYS))
    }

    /// Flush and invalidate a device. C: `fdr_bflush`, request `Flush`.
    /// Optional; the adapter always answers success.
    fn flushed(&mut self, _device: u64) {}

    // -- Non-request hooks --------------------------------------------

    /// Run bookkeeping after any request that produced a reply.
    ///
    /// C: `fdr_postcall`, invoked at the end of `fsdriver_process`
    /// (`fsdriver.c:60-61`). Optional.
    fn post_call(&mut self) {}

    /// Handle a message that is not a file system request.
    ///
    /// C: `fdr_other`, invoked for notifications and for messages from
    /// endpoints other than the virtual file system service
    /// (`fsdriver.c:26-31`). No reply is sent for these messages. Optional.
    fn other(&mut self, _is_notification: bool) {}
}

/// Whether the file system is currently mounted.
///
/// C: the `fsdriver_mounted` flag with its `fsdriver_device` / `fsdriver_root`
/// companions (`fsdriver.c:5-7`). Grouped into one value so the three pieces
/// cannot disagree: either nothing is mounted, or the device and the root
/// inode number are both known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountState {
    /// No file system mounted. Only the mount request is accepted.
    Unmounted,
    /// A file system is mounted on this device with this root inode number.
    Mounted {
        /// Device the file system was mounted from.
        device: u64,
        /// Inode number of the file system root.
        root_inode: u64,
    },
}

impl MountState {
    /// Whether a file system is currently mounted.
    pub const fn is_mounted(self) -> bool {
        matches!(self, Self::Mounted { .. })
    }
}

/// Runtime state of a file server: the event loop condition plus the mount
/// state.
///
/// C: `fsdriver_running` together with `fsdriver_mounted`
/// (`fsdriver.c:7-9`). The event loop continues while the server is running
/// or a file system is still mounted, so that termination waits for the
/// unmount (`fsdriver.c:87`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerState {
    /// Whether the server should keep serving. Cleared by [`Server::terminate`].
    pub running: bool,
    /// Current mount state.
    pub mount: MountState,
}

impl ServerState {
    /// Initial state of a freshly started server: running, nothing mounted.
    pub const fn fresh() -> Self {
        Self {
            running: true,
            mount: MountState::Unmounted,
        }
    }

    /// The event loop condition: keep going while running or still mounted.
    pub const fn should_continue(self) -> bool {
        self.running || self.mount.is_mounted()
    }
}

/// How an incoming message is classified before dispatch.
///
/// This is the Rust rendering of the three-way branch at the top of
/// `fsdriver_process` (`fsdriver.c:26-47`): non-requests go to the `other`
/// hook with no reply, requests for an unmounted server (other than mount)
/// are refused, and the rest are dispatched by request number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// Not a file system request: a notification or a message from another
    /// endpoint. Hand to the `other` hook; send no reply.
    Other,
    /// A file system request that arrived while nothing is mounted and that
    /// is not the mount request itself. Refused without calling the server.
    NotMounted {
        /// Decoded request, when the index was valid.
        request: Option<RequestNumber>,
        /// Transaction identifier echoed in the refusal.
        transaction: TransactionId,
    },
    /// A dispatchable request with its transaction identifier.
    Dispatch {
        /// Decoded request.
        request: RequestNumber,
        /// Transaction identifier echoed in the reply.
        transaction: TransactionId,
    },
    /// A request whose index decodes to no known operation (including the
    /// reserved `GetNode` slot). Answered "not implemented".
    Unknown {
        /// Transaction identifier echoed in the refusal.
        transaction: TransactionId,
    },
}

/// Classify one incoming message header.
///
/// - `is_notification`: whether the kernel reports this message as a
///   notification rather than a regular message (`is_ipc_notify`).
/// - `sender`: the endpoint the message arrived from; only the virtual file
///   system service endpoint carries requests (`m_source != VFS_PROC_NR`).
/// - `raw_type`: the raw message type carrying the request number and the
///   transaction identifier.
///
/// The mount gate (`fsdriver_mounted || call_nr == REQ_READSUPER`) is applied
/// here using the passed mount state, so callers cannot forget it.
pub fn classify(
    is_notification: bool,
    sender: i32,
    raw_type: i32,
    mount: MountState,
    vfs_endpoint: i32,
) -> Route {
    if is_notification || sender != vfs_endpoint {
        return Route::Other;
    }
    let (call_part, transaction) = TransactionId::decode(raw_type);
    // The C code subtracts FS_BASE with wrapping arithmetic on an unsigned
    // value, then range-checks the result. Decoding the low bits of the
    // difference is equivalent and stays in signed arithmetic.
    let index = call_part.wrapping_sub(crate::protocol::FS_BASE) as u32;
    let request = RequestNumber::from_index(index);
    match request {
        None => Route::Unknown { transaction },
        Some(request) => {
            if request == RequestNumber::ReadSuper || mount.is_mounted() {
                if request.is_dispatched() {
                    Route::Dispatch {
                        request,
                        transaction,
                    }
                } else {
                    Route::Unknown { transaction }
                }
            } else {
                Route::NotMounted {
                    request: Some(request),
                    transaction,
                }
            }
        }
    }
}

/// Outcome of handling one message: what reply status to send, if any.
///
/// Sending itself stays outside this framework (it needs the kernel
/// interface); this value tells the event loop what to do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handling {
    /// Send a reply carrying this status and the echoed transaction id.
    Reply {
        /// Status code: success or an error number.
        status: i32,
        /// Transaction identifier echoed from the request.
        transaction: TransactionId,
    },
    /// Send nothing (non-request messages only).
    NoReply,
}

/// Handle one message header through classification only.
///
/// This covers the dispatch rules that do not need the server: `Other`
/// produces no reply, `NotMounted` is refused as invalid, and `Unknown` is
/// refused as not implemented. `Dispatch` still needs the adapter layer in
/// [`crate::call`], so it is reported back for the caller to continue.
/// The `run_post_call` flag tells the caller whether the server's post-call
/// hook must run (only after a dispatched request that produced a reply).
pub fn handle_header(route: Route) -> (Handling, Option<RequestNumber>) {
    match route {
        Route::Other => (Handling::NoReply, None),
        Route::NotMounted { transaction, .. } => (
            Handling::Reply {
                status: EINVAL,
                transaction,
            },
            None,
        ),
        Route::Unknown { transaction } => (
            Handling::Reply {
                status: ENOSYS,
                transaction,
            },
            None,
        ),
        Route::Dispatch {
            request,
            transaction,
        } => (
            Handling::Reply {
                status: 0,
                transaction,
            },
            Some(request),
        ),
    }
}

/// A running file server: framework state plus the implementation.
///
/// The event loop itself (receive, classify, adapt, send) needs the kernel
/// receive primitive and therefore lives in the server crate; this type owns
/// the two pieces of framework state the loop threads through: the server
/// state and the implementation.
pub struct Server<D: FsDriver> {
    /// Event loop and mount state.
    pub state: ServerState,
    /// The file server implementation.
    pub driver: D,
}

impl<D: FsDriver> Server<D> {
    /// Start a server around an implementation.
    pub const fn new(driver: D) -> Self {
        Self {
            state: ServerState::fresh(),
            driver,
        }
    }

    /// Request termination: the loop exits once the file system is also
    /// unmounted. C: `fsdriver_terminate` (`fsdriver.c:67-74`), which clears
    /// the running flag and cancels the pending receive.
    pub const fn terminate(&mut self) {
        self.state.running = false;
    }

    /// Record a successful mount: store the device and root inode number.
    ///
    /// C: the state update at the end of `fsdriver_readsuper`
    /// (`call.c:60-64`). Kept next to the state so adapters cannot update
    /// one field and forget the other.
    pub const fn did_mount(&mut self, device: u64, root: FileNode) {
        self.state.mount = MountState::Mounted {
            device,
            root_inode: root.inode_number,
        };
    }

    /// Record an unmount: clear the mount state.
    ///
    /// C: `fsdriver_unmount` (`call.c:82-89`). The virtual-memory cache
    /// cleanup in the C version belongs to the block layer and is handled
    /// by the adapter caller, not here.
    pub const fn did_unmount(&mut self) {
        self.state.mount = MountState::Unmounted;
    }

    /// Whether the event loop should receive another message.
    pub const fn should_continue(&self) -> bool {
        self.state.should_continue()
    }
}

/// Refuse a second mount while one is active.
///
/// C: the `EBUSY` check at the top of `fsdriver_readsuper` (`call.c:31-34`).
/// Factored out so the mount adapter and tests share one definition.
pub const fn check_not_mounted(mount: MountState) -> Result<(), Errno> {
    if mount.is_mounted() {
        Err(Errno::from_i32(EBUSY))
    } else {
        Ok(())
    }
}

/// A server that mounts but implements nothing else.
///
/// Every optional operation keeps its default body, so every request but
/// mount and unmount is answered "not implemented". Useful as a protocol
/// test peer (it exercises classification, mount state, and refusal paths)
/// and as the starting point for a new server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NullDriver {
    /// Device the server was mounted from, if any.
    pub device: Option<u64>,
}

impl FsDriver for NullDriver {
    fn mount(
        &mut self,
        device: u64,
        _flags: MountFlags,
        capabilities: &mut CapabilityFlags,
    ) -> Result<FileNode, Errno> {
        *capabilities = CapabilityFlags::EMPTY;
        self.device = Some(device);
        Ok(FileNode::new(1, 0o040755, 0, 0, 0, device))
    }

    fn unmounted(&mut self) {
        self.device = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{FS_BASE, VFS_ENDPOINT};

    struct MinimalDriver {
        post_calls: u32,
        others: u32,
    }

    impl FsDriver for MinimalDriver {
        fn mount(
            &mut self,
            device: u64,
            _flags: MountFlags,
            capabilities: &mut CapabilityFlags,
        ) -> Result<FileNode, Errno> {
            *capabilities = CapabilityFlags::EMPTY;
            Ok(FileNode::new(1, 0o040755, 0, 0, 0, device))
        }

        fn post_call(&mut self) {
            self.post_calls += 1;
        }

        fn other(&mut self, _is_notification: bool) {
            self.others += 1;
        }
    }

    fn raw(request: RequestNumber, id: u16) -> i32 {
        TransactionId::encode_request(request.message_type(), TransactionId(id))
    }

    #[test]
    fn test_notification_routes_to_other_without_reply() {
        let route = classify(
            true,
            VFS_ENDPOINT,
            raw(RequestNumber::Read, 7),
            MountState::Unmounted,
            VFS_ENDPOINT,
        );
        assert_eq!(route, Route::Other);
        let (handling, request) = handle_header(route);
        assert_eq!(handling, Handling::NoReply);
        assert_eq!(request, None);
    }

    #[test]
    fn test_foreign_sender_routes_to_other_without_reply() {
        let route = classify(
            false,
            99,
            raw(RequestNumber::Read, 7),
            MountState::Mounted {
                device: 1,
                root_inode: 1,
            },
            VFS_ENDPOINT,
        );
        assert_eq!(route, Route::Other);
    }

    #[test]
    fn test_unmounted_server_only_accepts_mount() {
        let mount_route = classify(
            false,
            VFS_ENDPOINT,
            raw(RequestNumber::ReadSuper, 3),
            MountState::Unmounted,
            VFS_ENDPOINT,
        );
        assert!(matches!(mount_route, Route::Dispatch { .. }));

        let read_route = classify(
            false,
            VFS_ENDPOINT,
            raw(RequestNumber::Read, 3),
            MountState::Unmounted,
            VFS_ENDPOINT,
        );
        assert!(matches!(read_route, Route::NotMounted { .. }));
        let (handling, _) = handle_header(read_route);
        assert_eq!(
            handling,
            Handling::Reply {
                status: EINVAL,
                transaction: TransactionId(3)
            }
        );
    }

    #[test]
    fn test_mounted_server_dispatches_known_requests() {
        let mount = MountState::Mounted {
            device: 5,
            root_inode: 1,
        };
        let route = classify(
            false,
            VFS_ENDPOINT,
            raw(RequestNumber::Read, 11),
            mount,
            VFS_ENDPOINT,
        );
        assert_eq!(
            route,
            Route::Dispatch {
                request: RequestNumber::Read,
                transaction: TransactionId(11)
            }
        );
    }

    #[test]
    fn test_reserved_getnode_answers_not_implemented() {
        let mount = MountState::Mounted {
            device: 5,
            root_inode: 1,
        };
        let route = classify(
            false,
            VFS_ENDPOINT,
            raw(RequestNumber::GetNode, 2),
            mount,
            VFS_ENDPOINT,
        );
        assert_eq!(
            route,
            Route::Unknown {
                transaction: TransactionId(2)
            }
        );
        let (handling, _) = handle_header(route);
        assert_eq!(
            handling,
            Handling::Reply {
                status: ENOSYS,
                transaction: TransactionId(2)
            }
        );
    }

    #[test]
    fn test_server_state_tracks_mount_lifecycle() {
        let mut server = Server::new(MinimalDriver {
            post_calls: 0,
            others: 0,
        });
        assert!(server.should_continue());
        let root = FileNode::new(1, 0o040755, 0, 0, 0, 9);
        server.did_mount(9, root);
        assert_eq!(
            server.state.mount,
            MountState::Mounted {
                device: 9,
                root_inode: 1
            }
        );
        server.terminate();
        // Still mounted, so the loop continues until the unmount arrives.
        assert!(server.should_continue());
        server.did_unmount();
        assert!(!server.should_continue());
    }

    #[test]
    fn test_double_mount_is_busy() {
        let mount = MountState::Mounted {
            device: 1,
            root_inode: 1,
        };
        assert_eq!(check_not_mounted(mount).unwrap_err().to_i32(), EBUSY);
        assert!(check_not_mounted(MountState::Unmounted).is_ok());
    }

    #[test]
    fn test_default_trait_methods_answer_not_implemented() {
        let mut driver = MinimalDriver {
            post_calls: 0,
            others: 0,
        };
        assert_eq!(driver.stat_vfs(&mut [0; 64]).unwrap_err().to_i32(), ENOSYS);
        assert_eq!(driver.truncate(1, 0, 10).unwrap_err().to_i32(), ENOSYS);
        assert!(driver.put_node(1, 2).is_ok());
        driver.unmounted();
        driver.synchronized();
        assert_eq!(FS_BASE, 0xA00);
    }

    #[test]
    fn test_null_driver_mounts_and_rejects_operations() {
        let mut driver = NullDriver::default();
        let mut caps = CapabilityFlags::EMPTY;
        let root = driver.mount(6, MountFlags::EMPTY, &mut caps).unwrap();
        assert_eq!(root.inode_number, 1);
        assert_eq!(driver.device, Some(6));
        // Nothing else is implemented.
        assert_eq!(driver.lookup_child(1, "x").unwrap_err().to_i32(), ENOSYS);
        driver.unmounted();
        assert_eq!(driver.device, None);
    }
}
