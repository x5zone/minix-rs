//! Request adapters: validated translation between wire fields and driver calls.
//!
//! C correspondence: `minix3/minix/lib/libfsdriver/call.c` (all thirty-one
//! adapters). Each adapter here does the same three steps as its C
//! counterpart — extract and validate the scalar fields, invoke the matching
//! [`FsDriver`](crate::driver::FsDriver) method, shape the reply — but the
//! message buffers themselves are represented as plain structs and closures
//! instead of raw `message` unions. Grant-backed copying stays in
//! [`crate::data`]; this module only validates offsets, lengths, counts, and
//! names.
//!
//! The adapters are grouped exactly like the C file suggests:
//! mount (five), data (six), namespace (nine), metadata (six), block (five).

use minix_types::{EEXIST, EINVAL, ENOSYS, ENOTEMPTY, EPERM, Errno};

use crate::driver::FsDriver;
use crate::protocol::{CapabilityFlags, FileNode, MountFlags};

/// Largest single transfer accepted by any adapter.
///
/// C: `SSIZE_MAX` (`call.c:167,291,337,914,979`). Lengths arrive as unsigned
/// message fields; anything that does not fit in a signed result is refused
/// before the driver runs.
pub const MAX_TRANSFER: usize = isize::MAX as usize;

/// Validate a file offset and byte length pair.
///
/// Both read-like and block-like adapters apply the same rule
/// (`call.c:167` for files, `call.c:914` for blocks): the position must not
/// be negative and the length must fit in a signed result.
pub const fn check_position_and_length(position: i64, length: usize) -> Result<(), Errno> {
    if position < 0 {
        return Err(Errno::from_i32(EINVAL));
    }
    if length > MAX_TRANSFER {
        return Err(Errno::from_i32(EINVAL));
    }
    Ok(())
}

/// Validate a truncation range: both ends must be non-negative
/// (`call.c:369-370`).
pub const fn check_truncate_range(start: i64, end: i64) -> Result<(), Errno> {
    if start < 0 || end < 0 {
        return Err(Errno::from_i32(EINVAL));
    }
    Ok(())
}

/// Validate a reference release count: zero and huge counts are refused
/// (`call.c:105-108`). `INT_MAX` is the C `int` ceiling.
pub const fn check_release_count(count: u32) -> Result<(), Errno> {
    if count == 0 || count > i32::MAX as u32 {
        return Err(Errno::from_i32(EINVAL));
    }
    Ok(())
}

/// Reject the dot names for operations that create a new name.
///
/// C: `create` / `mkdir` / `mknod` / `link` / `slink` all refuse `"."` and
/// `".."` with "already exists" (`call.c:426-427` and siblings).
pub fn check_name_for_create(name: &str) -> Result<(), Errno> {
    if name == "." || name == ".." {
        return Err(Errno::from_i32(EEXIST));
    }
    Ok(())
}

/// Reject the dot names for removal of a non-directory.
///
/// C: `unlink` refuses `"."` and `".."` with "operation not permitted"
/// (`call.c:569-570`).
pub fn check_name_for_unlink(name: &str) -> Result<(), Errno> {
    if name == "." || name == ".." {
        return Err(Errno::from_i32(EPERM));
    }
    Ok(())
}

/// Reject the dot names for removal of a directory.
///
/// C: `rmdir` refuses `"."` with "invalid" and `".."` with "not empty"
/// (`call.c:599-603`).
pub fn check_name_for_remove_dir(name: &str) -> Result<(), Errno> {
    if name == "." {
        return Err(Errno::from_i32(EINVAL));
    }
    if name == ".." {
        return Err(Errno::from_i32(ENOTEMPTY));
    }
    Ok(())
}

/// Reject the dot names on either side of a rename.
///
/// C: both the old and the new name refuse `"."` and `".."` with "invalid"
/// (`call.c:635-643`).
pub fn check_name_for_rename(name: &str) -> Result<(), Errno> {
    if name == "." || name == ".." {
        return Err(Errno::from_i32(EINVAL));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Mount group
// ---------------------------------------------------------------------------

/// Validated input for the mount (`ReadSuper`) adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MountInput {
    /// Device to mount from.
    pub device: u64,
    /// Mount flags from the request.
    pub flags: MountFlags,
}

/// Outcome of a successful mount: the root node plus the negotiated
/// capability flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MountReply {
    /// Root node reported by the server.
    pub root: FileNode,
    /// Capability flags echoed to the virtual file system service.
    pub capabilities: CapabilityFlags,
}

/// Run the mount handshake: refuse a second mount, bind the driver label,
/// call `mount`, then apply the framework-level capability rules.
///
/// C: `fsdriver_readsuper` (`call.c:11-68`). The two framework rules applied
/// after a successful mount are kept here:
/// - A server that implements both file and block peek (or that has no
///   backing device at all) is reported as peek-capable (`call.c:48-51`).
/// - The mount state update itself is done by the caller through
///   [`Server::did_mount`](crate::driver::Server::did_mount); this function
///   only computes the reply.
pub fn adapt_mount<D: FsDriver>(
    driver: &mut D,
    input: MountInput,
    already_mounted: bool,
    has_backing_device: bool,
    file_peek: bool,
    block_peek: bool,
) -> Result<MountReply, Errno> {
    if already_mounted {
        return Err(Errno::from_i32(minix_types::EBUSY));
    }
    driver.bound_driver(input.device, "");
    let mut capabilities = CapabilityFlags::EMPTY;
    let root = driver.mount(input.device, input.flags, &mut capabilities)?;
    capabilities = negotiate_peek(capabilities, file_peek, block_peek, has_backing_device);
    Ok(MountReply { root, capabilities })
}

/// Whether the framework may advertise peek support for this mount.
///
/// C: `call.c:49-51`. The flag is set when the server implements both peek
/// entry points, or when the device has no backing major (diskless servers
/// emulate peek through reads).
pub const fn negotiate_peek(
    mut capabilities: CapabilityFlags,
    file_peek: bool,
    block_peek: bool,
    has_backing_device: bool,
) -> CapabilityFlags {
    if (file_peek && block_peek) || !has_backing_device {
        capabilities = CapabilityFlags(capabilities.0 | CapabilityFlags::HAS_PEEK.0);
    }
    capabilities
}

/// Run the unmount adapter: notify the server, then let the caller clear the
/// mount state. Always succeeds. C: `fsdriver_unmount` (`call.c:74-90`).
pub fn adapt_unmount<D: FsDriver>(driver: &mut D) {
    driver.unmounted();
}

/// Validated input for allocating an unnamed inode (`NewNode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewNodeInput {
    /// Requested file mode.
    pub mode: u32,
    /// Owning user identifier.
    pub owner: u32,
    /// Owning group identifier.
    pub group: u32,
    /// Device number for device nodes.
    pub device: u64,
}

/// Run the new-node adapter. C: `fsdriver_newnode` (`call.c:120-148`).
pub fn adapt_new_node<D: FsDriver>(driver: &mut D, input: NewNodeInput) -> Result<FileNode, Errno> {
    driver.new_node(input.mode, input.owner, input.group, input.device)
}

/// Run the put-node adapter: validate the count, then release.
/// C: `fsdriver_putnode` (`call.c:96-114`).
pub fn adapt_put_node<D: FsDriver>(driver: &mut D, inode: u64, count: u32) -> Result<(), Errno> {
    check_release_count(count)?;
    driver.put_node(inode, count)
}

/// Run the mount-point check adapter. C: `fsdriver_mountpoint`
/// (`call.c:818-829`).
pub fn adapt_mount_point<D: FsDriver>(driver: &mut D, inode: u64) -> Result<(), Errno> {
    driver.is_mount_point(inode)
}

// ---------------------------------------------------------------------------
// Data group
// ---------------------------------------------------------------------------

/// Validated input shared by the read-like and write-like adapters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferInput {
    /// Inode to read from or write to.
    pub inode: u64,
    /// Starting file offset.
    pub position: i64,
    /// Number of bytes to move.
    pub length: usize,
}

impl TransferInput {
    /// Build and validate in one step.
    pub const fn new(inode: u64, position: i64, length: usize) -> Result<Self, Errno> {
        if let Err(error) = check_position_and_length(position, length) {
            return Err(error);
        }
        Ok(Self {
            inode,
            position,
            length,
        })
    }
}

/// Reply of a successful transfer: the new file position and the number of
/// bytes actually moved. C: `m_fs_vfs_readwrite.seek_pos` / `nbytes`
/// (`call.c:180-184`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferReply {
    /// File position after the transfer.
    pub new_position: i64,
    /// Bytes actually transferred.
    pub transferred: usize,
}

/// Run the read adapter. C: `fsdriver_read` + `read_write` (`call.c:153-202`).
pub fn adapt_read<D: FsDriver>(
    driver: &mut D,
    input: TransferInput,
    out: &mut dyn FnMut(&[u8]),
) -> Result<TransferReply, Errno> {
    let moved = driver.read(input.inode, input.position, input.length, out)?;
    Ok(TransferReply {
        new_position: input.position + moved as i64,
        transferred: moved,
    })
}

/// Run the write adapter. C: `fsdriver_write` + `read_write`
/// (`call.c:205-216`).
pub fn adapt_write<D: FsDriver>(
    driver: &mut D,
    input: TransferInput,
    data: &[u8],
) -> Result<TransferReply, Errno> {
    if data.len() > input.length {
        return Err(Errno::from_i32(EINVAL));
    }
    let moved = driver.write(input.inode, input.position, data)?;
    Ok(TransferReply {
        new_position: input.position + moved as i64,
        transferred: moved,
    })
}

/// How a peek request is satisfied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeekPath {
    /// The server implements peek directly.
    Direct,
    /// The server has no peek entry point but also no backing device, so the
    /// adapter emulates peek through `read` (`call.c:294-306`).
    EmulatedThroughRead,
}

/// Decide how a peek request is satisfied.
///
/// C: `fsdriver_peek` (`call.c:280-315`). A missing peek entry point is only
/// fatal for servers with a backing device; diskless servers fall back to
/// the read-based emulation.
pub const fn select_peek_path(has_peek: bool, has_backing_device: bool) -> Result<PeekPath, Errno> {
    if has_peek {
        Ok(PeekPath::Direct)
    } else if has_backing_device {
        Err(Errno::from_i32(ENOSYS))
    } else {
        Ok(PeekPath::EmulatedThroughRead)
    }
}

/// Run the peek adapter once the path is decided.
///
/// Unlike reads, a peek reports only the byte count, never a new position
/// (`call.c:308-312`: "Do not return a new position").
pub fn adapt_peek<D: FsDriver>(
    driver: &mut D,
    input: TransferInput,
    path: PeekPath,
    read_emulation: &mut dyn FnMut(&mut D, TransferInput) -> Result<usize, Errno>,
) -> Result<usize, Errno> {
    check_position_and_length(input.position, input.length)?;
    match path {
        PeekPath::Direct => driver.peek(input.inode, input.position, input.length),
        PeekPath::EmulatedThroughRead => read_emulation(driver, input),
    }
}

/// Run the directory listing adapter.
///
/// C: `fsdriver_getdents` (`call.c:320-353`). The position is resume-on-input
/// and stopped-at-on-output; the adapter writes the updated position back
/// into the reply on success.
pub fn adapt_get_dents<D: FsDriver>(
    driver: &mut D,
    inode: u64,
    position: &mut i64,
    capacity: usize,
    out: &mut dyn FnMut(&[u8]),
) -> Result<usize, Errno> {
    check_position_and_length(*position, capacity)?;
    let moved = driver.get_dents(inode, position, capacity, out)?;
    Ok(moved)
}

/// Run the truncate adapter. C: `fsdriver_trunc` (`call.c:360-376`).
pub fn adapt_truncate<D: FsDriver>(
    driver: &mut D,
    inode: u64,
    start: i64,
    end: i64,
) -> Result<(), Errno> {
    check_truncate_range(start, end)?;
    driver.truncate(inode, start, end)
}

/// Run the inhibit-read adapter: notify and always succeed.
/// C: `fsdriver_inhibread` (`call.c:381-393`).
pub fn adapt_inhibit_read<D: FsDriver>(driver: &mut D, inode: u64) {
    driver.sought(inode);
}

// ---------------------------------------------------------------------------
// Namespace group
// ---------------------------------------------------------------------------

/// Validated input for creating a named child (create, directory, device
/// node, hard link, symbolic link share the directory plus name prefix).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NameInput<'a> {
    /// Directory receiving the new name.
    pub directory: u64,
    /// New name (already checked for length and termination by the caller).
    pub name: &'a str,
}

/// Run the create adapter and report the new node.
/// C: `fsdriver_create` (`call.c:399-438`).
pub fn adapt_create<D: FsDriver>(
    driver: &mut D,
    input: NameInput<'_>,
    mode: u32,
    owner: u32,
    group: u32,
) -> Result<FileNode, Errno> {
    check_name_for_create(input.name)?;
    driver.create(input.directory, input.name, mode, owner, group)
}

/// Run the make-directory adapter. C: `fsdriver_mkdir` (`call.c:443-474`).
pub fn adapt_make_dir<D: FsDriver>(
    driver: &mut D,
    input: NameInput<'_>,
    mode: u32,
    owner: u32,
    group: u32,
) -> Result<(), Errno> {
    check_name_for_create(input.name)?;
    driver.make_dir(input.directory, input.name, mode, owner, group)
}

/// Run the make-device-node adapter. C: `fsdriver_mknod` (`call.c:479-512`).
pub fn adapt_make_node<D: FsDriver>(
    driver: &mut D,
    input: NameInput<'_>,
    mode: u32,
    owner: u32,
    group: u32,
    device: u64,
) -> Result<(), Errno> {
    check_name_for_create(input.name)?;
    driver.make_node(input.directory, input.name, mode, owner, group, device)
}

/// Run the hard-link adapter. C: `fsdriver_link` (`call.c:517-543`).
pub fn adapt_link<D: FsDriver>(
    driver: &mut D,
    input: NameInput<'_>,
    inode: u64,
) -> Result<(), Errno> {
    check_name_for_create(input.name)?;
    driver.link(input.directory, input.name, inode)
}

/// Run the unlink adapter. C: `fsdriver_unlink` (`call.c:548-573`).
pub fn adapt_unlink<D: FsDriver>(driver: &mut D, input: NameInput<'_>) -> Result<(), Errno> {
    check_name_for_unlink(input.name)?;
    driver.unlink(input.directory, input.name)
}

/// Run the remove-directory adapter. C: `fsdriver_rmdir` (`call.c:578-606`).
pub fn adapt_remove_dir<D: FsDriver>(driver: &mut D, input: NameInput<'_>) -> Result<(), Errno> {
    check_name_for_remove_dir(input.name)?;
    driver.remove_dir(input.directory, input.name)
}

/// Run the rename adapter. C: `fsdriver_rename` (`call.c:611-646`).
pub fn adapt_rename<D: FsDriver>(
    driver: &mut D,
    old: NameInput<'_>,
    new: NameInput<'_>,
) -> Result<(), Errno> {
    check_name_for_rename(old.name)?;
    check_name_for_rename(new.name)?;
    driver.rename(old.directory, old.name, new.directory, new.name)
}

/// Run the symbolic-link adapter. C: `fsdriver_slink` (`call.c:651-685`).
pub fn adapt_symbolic_link<D: FsDriver>(
    driver: &mut D,
    input: NameInput<'_>,
    owner: u32,
    group: u32,
    target: &[u8],
) -> Result<(), Errno> {
    check_name_for_create(input.name)?;
    driver.symbolic_link(input.directory, input.name, owner, group, target)
}

/// Run the read-link adapter: report only the byte count, like peek.
/// C: `fsdriver_rdlink` (`call.c:690-712`).
pub fn adapt_read_link<D: FsDriver>(
    driver: &mut D,
    inode: u64,
    capacity: usize,
    out: &mut dyn FnMut(&[u8]),
) -> Result<usize, Errno> {
    if capacity > MAX_TRANSFER {
        return Err(Errno::from_i32(EINVAL));
    }
    driver.read_link(inode, capacity, out)
}

// ---------------------------------------------------------------------------
// Metadata group
// ---------------------------------------------------------------------------

/// Run the stat adapter: the server fills the caller-supplied buffer.
/// C: `fsdriver_stat` (`call.c:717-741`). The device and inode number
/// pre-fill done by the C adapter is the caller's job; the adapter only
/// validates the plumbing and forwards.
pub fn adapt_stat<D: FsDriver>(driver: &mut D, inode: u64, out: &mut [u8]) -> Result<(), Errno> {
    driver.stat(inode, out)
}

/// Run the change-owner adapter and report the resulting mode.
/// C: `fsdriver_chown` (`call.c:746-767`).
pub fn adapt_change_owner<D: FsDriver>(
    driver: &mut D,
    inode: u64,
    owner: u32,
    group: u32,
) -> Result<u32, Errno> {
    driver.change_owner(inode, owner, group)
}

/// Run the change-mode adapter and report the resulting mode.
/// C: `fsdriver_chmod` (`call.c:772-790`).
pub fn adapt_change_mode<D: FsDriver>(driver: &mut D, inode: u64, mode: u32) -> Result<u32, Errno> {
    driver.change_mode(inode, mode)
}

/// Run the update-times adapter. C: `fsdriver_utime` (`call.c:795-812`).
pub fn adapt_update_times<D: FsDriver>(
    driver: &mut D,
    inode: u64,
    accessed: (i64, i64),
    modified: (i64, i64),
) -> Result<(), Errno> {
    driver.update_times(inode, accessed, modified)
}

/// Run the file system statistics adapter. C: `fsdriver_statvfs`
/// (`call.c:834-851`).
pub fn adapt_stat_vfs<D: FsDriver>(driver: &mut D, out: &mut [u8]) -> Result<(), Errno> {
    driver.stat_vfs(out)
}

/// Run the sync adapter: notify and always succeed.
/// C: `fsdriver_sync` (`call.c:856-866`).
pub fn adapt_sync<D: FsDriver>(driver: &mut D) {
    driver.synchronized();
}

// ---------------------------------------------------------------------------
// Block group
// ---------------------------------------------------------------------------

/// Validated input for raw device transfers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockTransferInput {
    /// Device to read from or write to.
    pub device: u64,
    /// Starting device offset.
    pub position: i64,
    /// Number of bytes to move.
    pub length: usize,
}

impl BlockTransferInput {
    /// Build and validate in one step.
    pub const fn new(device: u64, position: i64, length: usize) -> Result<Self, Errno> {
        if let Err(error) = check_position_and_length(position, length) {
            return Err(error);
        }
        Ok(Self {
            device,
            position,
            length,
        })
    }
}

/// Run the block-read adapter. C: `fsdriver_bread` + `bread_bwrite`
/// (`call.c:900-949`).
pub fn adapt_block_read<D: FsDriver>(
    driver: &mut D,
    input: BlockTransferInput,
    out: &mut dyn FnMut(&[u8]),
) -> Result<TransferReply, Errno> {
    let moved = driver.block_read(input.device, input.position, input.length, out)?;
    Ok(TransferReply {
        new_position: input.position + moved as i64,
        transferred: moved,
    })
}

/// Run the block-write adapter. C: `fsdriver_bwrite` + `bread_bwrite`
/// (`call.c:954-963`).
pub fn adapt_block_write<D: FsDriver>(
    driver: &mut D,
    input: BlockTransferInput,
    data: &[u8],
) -> Result<TransferReply, Errno> {
    if data.len() > input.length {
        return Err(Errno::from_i32(EINVAL));
    }
    let moved = driver.block_write(input.device, input.position, data)?;
    Ok(TransferReply {
        new_position: input.position + moved as i64,
        transferred: moved,
    })
}

/// Decide how a block-peek request is satisfied.
///
/// C: `fsdriver_bpeek` (`call.c:968-996`). Unlike file peek there is no
/// read-based emulation for devices: a missing entry point is always "not
/// implemented". The position and length are still validated first.
pub const fn check_block_peek(
    has_block_peek: bool,
    position: i64,
    length: usize,
) -> Result<(), Errno> {
    if !has_block_peek {
        return Err(Errno::from_i32(ENOSYS));
    }
    check_position_and_length(position, length)
}

/// Run the flush adapter: notify and always succeed.
/// C: `fsdriver_flush` (`call.c:1001-1013`).
pub fn adapt_flush<D: FsDriver>(driver: &mut D, device: u64) {
    driver.flushed(device);
}

/// Run the new-driver adapter: rebind the label, always succeed.
/// C: `fsdriver_newdriver` (`call.c:871-895`). When the server has no bind
/// hook the C adapter answers success without doing anything; the same rule
/// is expressed by giving [`FsDriver::bound_driver`] an empty default body,
/// so this adapter unconditionally calls it.
pub fn adapt_new_driver<D: FsDriver>(driver: &mut D, device: u64, label: &str) {
    driver.bound_driver(device, label);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::Server;
    use crate::protocol::FileNode;

    struct StubDriver {
        fail_with: Option<i32>,
        log: [u8; 8],
    }

    impl StubDriver {
        fn ok() -> Self {
            Self {
                fail_with: None,
                log: [0; 8],
            }
        }

        fn check(&self) -> Result<(), Errno> {
            match self.fail_with {
                Some(code) => Err(Errno::from_i32(code)),
                None => Ok(()),
            }
        }
    }

    impl FsDriver for StubDriver {
        fn mount(
            &mut self,
            device: u64,
            _flags: MountFlags,
            capabilities: &mut CapabilityFlags,
        ) -> Result<FileNode, Errno> {
            self.check()?;
            *capabilities = CapabilityFlags::EMPTY;
            Ok(FileNode::new(10, 0o040755, 0, 0, 0, device))
        }

        fn read(
            &mut self,
            _inode: u64,
            _position: i64,
            length: usize,
            out: &mut dyn FnMut(&[u8]),
        ) -> Result<usize, Errno> {
            self.check()?;
            let chunk = [7u8; 16];
            let take = length.min(chunk.len());
            out(&chunk[..take]);
            Ok(take)
        }

        fn write(&mut self, _inode: u64, _position: i64, data: &[u8]) -> Result<usize, Errno> {
            self.check()?;
            Ok(data.len())
        }

        fn stat(&mut self, _inode: u64, out: &mut [u8]) -> Result<(), Errno> {
            self.check()?;
            self.log[0] = 1;
            if !out.is_empty() {
                out[0] = 42;
            }
            Ok(())
        }
    }

    #[test]
    fn test_position_and_length_validation() {
        assert!(check_position_and_length(0, 10).is_ok());
        assert_eq!(
            check_position_and_length(-1, 10).unwrap_err().to_i32(),
            EINVAL
        );
        assert_eq!(
            check_position_and_length(0, MAX_TRANSFER + 1)
                .unwrap_err()
                .to_i32(),
            EINVAL
        );
        assert_eq!(check_truncate_range(-1, 5).unwrap_err().to_i32(), EINVAL);
        assert!(check_truncate_range(5, 2).is_ok());
    }

    #[test]
    fn test_release_count_validation() {
        assert!(check_release_count(1).is_ok());
        assert_eq!(check_release_count(0).unwrap_err().to_i32(), EINVAL);
        assert_eq!(check_release_count(u32::MAX).unwrap_err().to_i32(), EINVAL);
    }

    #[test]
    fn test_dot_name_rules_match_c() {
        // Create family refuses both dot names with EEXIST.
        assert_eq!(check_name_for_create(".").unwrap_err().to_i32(), EEXIST);
        assert_eq!(check_name_for_create("..").unwrap_err().to_i32(), EEXIST);
        assert!(check_name_for_create("file").is_ok());
        // Unlink refuses both with EPERM.
        assert_eq!(check_name_for_unlink(".").unwrap_err().to_i32(), EPERM);
        assert_eq!(check_name_for_unlink("..").unwrap_err().to_i32(), EPERM);
        // Remove-dir splits: "." is EINVAL, ".." is ENOTEMPTY.
        assert_eq!(check_name_for_remove_dir(".").unwrap_err().to_i32(), EINVAL);
        assert_eq!(
            check_name_for_remove_dir("..").unwrap_err().to_i32(),
            ENOTEMPTY
        );
        // Rename refuses both on both sides with EINVAL.
        assert_eq!(check_name_for_rename(".").unwrap_err().to_i32(), EINVAL);
        assert_eq!(check_name_for_rename("..").unwrap_err().to_i32(), EINVAL);
    }

    #[test]
    fn test_mount_refuses_second_mount() {
        let mut driver = StubDriver::ok();
        let input = MountInput {
            device: 3,
            flags: MountFlags::EMPTY,
        };
        // Diskless stub without peek entry points: the framework still
        // advertises peek because reads can emulate it.
        let reply = adapt_mount(&mut driver, input, false, false, false, false).unwrap();
        assert!(reply.capabilities.has_peek());
        // Backed device without peek entry points: no peek advertised.
        let mut driver = StubDriver::ok();
        let reply = adapt_mount(&mut driver, input, false, true, false, false).unwrap();
        assert_eq!(reply.root.inode_number, 10);
        assert!(!reply.capabilities.has_peek());
        assert_eq!(
            adapt_mount(&mut driver, input, true, true, false, false)
                .unwrap_err()
                .to_i32(),
            minix_types::EBUSY
        );
    }

    #[test]
    fn test_peek_path_selection() {
        assert_eq!(select_peek_path(true, true).unwrap(), PeekPath::Direct);
        assert_eq!(
            select_peek_path(false, false).unwrap(),
            PeekPath::EmulatedThroughRead
        );
        assert_eq!(select_peek_path(false, true).unwrap_err().to_i32(), ENOSYS);
    }

    #[test]
    fn test_read_reply_advances_position() {
        let mut driver = StubDriver::ok();
        let input = TransferInput::new(1, 100, 8).unwrap();
        let mut seen = 0;
        let reply = adapt_read(&mut driver, input, &mut |chunk: &[u8]| {
            seen += chunk.len();
        })
        .unwrap();
        assert_eq!(seen, 8);
        assert_eq!(reply.new_position, 108);
        assert_eq!(reply.transferred, 8);
    }

    #[test]
    fn test_write_rejects_oversized_buffer() {
        let mut driver = StubDriver::ok();
        let input = TransferInput::new(1, 0, 4).unwrap();
        assert_eq!(
            adapt_write(&mut driver, input, &[0; 8])
                .unwrap_err()
                .to_i32(),
            EINVAL
        );
    }

    #[test]
    fn test_error_propagates_from_driver() {
        let mut driver = StubDriver {
            fail_with: Some(minix_types::EIO),
            log: [0; 8],
        };
        let mut out = [0u8; 8];
        assert_eq!(
            adapt_stat(&mut driver, 1, &mut out).unwrap_err().to_i32(),
            minix_types::EIO
        );
    }

    #[test]
    fn test_mount_lifecycle_through_server() {
        let mut server = Server::new(StubDriver::ok());
        let reply = adapt_mount(
            &mut server.driver,
            MountInput {
                device: 4,
                flags: MountFlags::EMPTY,
            },
            server.state.mount.is_mounted(),
            true,
            false,
            false,
        )
        .unwrap();
        server.did_mount(4, reply.root);
        assert!(server.should_continue());
        adapt_unmount(&mut server.driver);
        server.did_unmount();
        assert!(!server.state.mount.is_mounted());
    }

    #[test]
    fn test_block_peek_requires_entry_point() {
        assert!(check_block_peek(true, 0, 16).is_ok());
        assert_eq!(check_block_peek(false, 0, 16).unwrap_err().to_i32(), ENOSYS);
        assert_eq!(check_block_peek(true, -1, 16).unwrap_err().to_i32(), EINVAL);
    }

    #[test]
    fn test_negotiate_peek_flag() {
        let caps = negotiate_peek(CapabilityFlags::EMPTY, true, true, true);
        assert!(caps.has_peek());
        let caps = negotiate_peek(CapabilityFlags::EMPTY, false, false, false);
        assert!(caps.has_peek());
        let caps = negotiate_peek(CapabilityFlags::EMPTY, false, true, true);
        assert!(!caps.has_peek());
    }
}
