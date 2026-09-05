//! Pipe file server: unnamed pipes and cloned devices.
//!
//! C correspondence: `minix3/minix/fs/pfs/pfs.c` (all four hundred fifty-one
//! lines). PFS is the smallest complete file server: it mounts first during
//! boot, serves only pipes and device nodes created through the new-node
//! request, and keeps everything in a fixed table of five hundred twelve
//! inodes. It is the reference consumer of the driver framework from
//! [`minix_fs`].
//!
//! The service runtime (startup handshake, privilege drop, signal wiring,
//! event loop) belongs to the service-runtime stage; this crate owns the
//! file-system logic behind the [`FsDriver`](minix_fs::driver::FsDriver)
//! trait. Time comes from a [`Clock`] so tests control it.

#![no_std]

extern crate alloc;

use alloc::vec::Vec;

use minix_fs::driver::FsDriver;
use minix_fs::protocol::{CapabilityFlags, FileNode, MountFlags};
use minix_types::{EFBIG, EINVAL, ENFILE, ENOSPC, Errno};

/// Maximum inodes in PFS, hence the system-wide ceiling on open pipes and
/// cloned devices.
///
/// C: `PFS_NR_INODES` (`pfs.c:16`, value five hundred twelve). Kept in sync
/// with the virtual file system service inode ceiling by convention; the
/// comment in the C source asks for exactly this.
pub const PFS_INODE_COUNT: usize = 512;

/// Pipe buffer size in bytes.
///
/// C: `PIPE_BUF` (`minix3/sys/sys/syslimits.h:64`, thirty-two thousand seven
/// hundred sixty-eight on Minix targets). One buffer per pipe, allocated at
/// creation and freed at release. Reads and writes past this size are
/// refused with "file too big".
pub const PIPE_BUFFER_SIZE: usize = 32768;

/// Block size reported in status (`S_BLKSIZE`, `minix3/sys/sys/stat.h:195`).
pub const STATUS_BLOCK_SIZE: u64 = 512;

/// File-type mask and types (octal, `minix3/sys/sys/stat.h:138-146`).
pub const FILE_TYPE_MASK: u32 = 0o170000;
/// Named pipe. C: `S_IFIFO` (octal `0010000`).
pub const TYPE_FIFO: u32 = 0o010000;
/// Character device. C: `S_IFCHR` (octal `0020000`).
pub const TYPE_CHARACTER: u32 = 0o020000;
/// Block device. C: `S_IFBLK` (octal `0060000`).
pub const TYPE_BLOCK: u32 = 0o060000;
/// Socket. C: `S_IFSOCK` (octal `0140000`).
pub const TYPE_SOCKET: u32 = 0o140000;

/// Permission bits preserved across mode changes.
///
/// C: `ALLPERMS` (`minix3/sys/sys/stat.h:191`): set-user-ID, set-group-ID,
/// sticky, plus the nine read-write-execute bits (octal `0007777`). The file
/// type bits are never touched by a mode change.
pub const ALL_PERMISSIONS: u32 = 0o7777;

/// Lazy time-update requests (`pfs.c:19-21`).
pub const UPDATE_ACCESS: u8 = 0x1;
/// Lazy modification-time update request.
pub const UPDATE_MODIFY: u8 = 0x2;
/// Lazy change-time update request.
pub const UPDATE_CHANGE: u8 = 0x4;

/// Status buffer size in bytes: device (eight), mode (four), link count
/// (four), owner (four), group (four), device again (eight), size (eight),
/// three times (eight each), block size (eight), block count (eight).
/// This layout is local to PFS and used by its own tests; the system-wide
/// status layout belongs to the system-call interface stage.
pub const STATUS_BUFFER_SIZE: usize = 80;

/// Clock: current time in seconds since the epoch.
///
/// C: `clock_time(NULL)` (`pfs.c:332`). Abstracted so tests control time;
/// the service wires the real clock at startup.
pub trait Clock {
    /// Current time in seconds.
    fn now_seconds(&self) -> i64;
}

/// Fixed clock: always reports the same second.
///
/// Production use: services started in a frozen test harness. Tests use it
/// wherever time must stand still.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedClock(pub i64);

impl Clock for FixedClock {
    fn now_seconds(&self) -> i64 {
        self.0
    }
}

/// Stepping clock: advances by a fixed step on every read.
///
/// Production use: deterministic replay of time-dependent sequences. Tests
/// use it to prove lazy time updates happen exactly once. Not `Copy`: the
/// counter moves on every read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepClock {
    next: core::cell::Cell<i64>,
    step: i64,
}

impl StepClock {
    /// Clock starting at `first`, advancing `step` seconds per read.
    pub const fn new(first: i64, step: i64) -> Self {
        Self {
            next: core::cell::Cell::new(first),
            step,
        }
    }
}

impl Clock for StepClock {
    fn now_seconds(&self) -> i64 {
        let now = self.next.get();
        self.next.set(now + self.step);
        now
    }
}

/// One PFS inode.
///
/// C: `struct inode` (`pfs.c:23-42`). The free-list links live in a separate
/// stack in [`PfsServer`]; the sanity flag is a plain boolean.
#[derive(Debug, Clone)]
struct PfsInode {
    number: u32,
    mode: u32,
    owner: u32,
    group: u32,
    size: usize,
    device: u64,
    accessed: i64,
    modified: i64,
    changed: i64,
    /// Pipe contents; `None` for device nodes (mirrors `i_data == NULL`).
    data: Option<Vec<u8>>,
    /// Read offset into the buffer (mirrors `i_start`).
    start: usize,
    /// Pending lazy time updates (mirrors `i_update`).
    pending_updates: u8,
    in_use: bool,
}

impl PfsInode {
    fn fresh(number: u32) -> Self {
        Self {
            number,
            mode: 0,
            owner: 0,
            group: 0,
            size: 0,
            device: 0,
            accessed: 0,
            modified: 0,
            changed: 0,
            data: None,
            start: 0,
            pending_updates: 0,
            in_use: false,
        }
    }
}

/// Pipe file server.
///
/// Holds the fixed inode table, the free stack, and the clock. Inode
/// numbers are one-based: number zero is reserved and never handed out
/// (`pfs.c:66`, `pfs_findnode`).
pub struct PfsServer<C: Clock> {
    inodes: Vec<PfsInode>,
    /// Free inode numbers, lowest on top (mirrors the backwards init that
    /// puts low numbers first, `pfs.c:63-71`).
    free: Vec<u32>,
    clock: C,
    /// Inodes still in use at the last unmount (the C code prints a warning;
    /// a library reports the count for the caller to log).
    pub busy_at_unmount: usize,
}

impl<C: Clock> PfsServer<C> {
    /// Empty server; call [`FsDriver::mount`] before serving.
    pub fn new(clock: C) -> Self {
        let mut inodes = Vec::with_capacity(PFS_INODE_COUNT);
        for number in 1..=PFS_INODE_COUNT as u32 {
            inodes.push(PfsInode::fresh(number));
        }
        Self {
            inodes,
            free: Vec::new(),
            clock,
            busy_at_unmount: 0,
        }
    }

    /// Look up a live inode by number (`pfs_findnode`, `pfs.c:104-120`).
    fn find(&self, number: u64) -> Result<&PfsInode, Errno> {
        if number < 1 || number > PFS_INODE_COUNT as u64 {
            return Err(Errno::from_i32(EINVAL));
        }
        let inode = &self.inodes[number as usize - 1];
        debug_assert_eq!(inode.number, number as u32);
        if !inode.in_use {
            return Err(Errno::from_i32(EINVAL));
        }
        Ok(inode)
    }

    /// Mutable twin of [`PfsServer::find`].
    fn find_mut(&mut self, number: u64) -> Result<&mut PfsInode, Errno> {
        if number < 1 || number > PFS_INODE_COUNT as u64 {
            return Err(Errno::from_i32(EINVAL));
        }
        let inode = &mut self.inodes[number as usize - 1];
        debug_assert_eq!(inode.number, number as u32);
        if !inode.in_use {
            return Err(Errno::from_i32(EINVAL));
        }
        Ok(inode)
    }

    /// Apply pending lazy time updates with a single clock reading
    /// (`pfs_stat`, `pfs.c:331-339`). Static so the clock read and the
    /// inode borrow never overlap.
    fn apply_pending_times(inode: &mut PfsInode, now: i64) {
        if inode.pending_updates != 0 {
            if inode.pending_updates & UPDATE_ACCESS != 0 {
                inode.accessed = now;
            }
            if inode.pending_updates & UPDATE_MODIFY != 0 {
                inode.modified = now;
            }
            if inode.pending_updates & UPDATE_CHANGE != 0 {
                inode.changed = now;
            }
            inode.pending_updates = 0;
        }
    }

    /// Describe a live inode as a framework node.
    fn describe(&self, number: u64) -> Result<FileNode, Errno> {
        let inode = self.find(number)?;
        Ok(FileNode::new(
            number,
            inode.mode,
            inode.size as i64,
            inode.owner,
            inode.group,
            inode.device,
        ))
    }
}

/// Whether a mode word names a pipe.
const fn is_fifo(mode: u32) -> bool {
    mode & FILE_TYPE_MASK == TYPE_FIFO
}

/// Whether a mode word names a supported device node (block, character, or
/// socket), the only other kind PFS creates (`pfs.c:135`).
const fn is_supported_device(mode: u32) -> bool {
    matches!(
        mode & FILE_TYPE_MASK,
        TYPE_BLOCK | TYPE_CHARACTER | TYPE_SOCKET
    )
}

impl<C: Clock> FsDriver for PfsServer<C> {
    fn mount(
        &mut self,
        _device: u64,
        _flags: MountFlags,
        capabilities: &mut CapabilityFlags,
    ) -> Result<FileNode, Errno> {
        // Rebuild the table from scratch: every inode free, lowest numbers
        // on top of the free stack (`pfs.c:56-71`).
        for inode in self.inodes.iter_mut() {
            inode.in_use = false;
            inode.data = None;
            inode.size = 0;
            inode.start = 0;
            inode.pending_updates = 0;
        }
        self.free.clear();
        for number in (1..=PFS_INODE_COUNT as u32).rev() {
            self.free.push(number);
        }
        self.busy_at_unmount = 0;
        // PFS has no root node; the caller ignores the details anyway. The
        // all-zero node keeps the framework free of special cases
        // (`pfs.c:73-80`).
        *capabilities = CapabilityFlags(CapabilityFlags::SIZE_64BIT.0);
        Ok(FileNode::new(0, 0, 0, 0, 0, 0))
    }

    fn unmounted(&mut self) {
        // Count in-use inodes for the busy warning (`pfs.c:92-98`).
        self.busy_at_unmount = self.inodes.iter().filter(|inode| inode.in_use).count();
    }

    fn new_node(
        &mut self,
        mode: u32,
        owner: u32,
        group: u32,
        device: u64,
    ) -> Result<FileNode, Errno> {
        let fifo = is_fifo(mode);
        if !fifo && !is_supported_device(mode) {
            // Anything else means the caller is misbehaving (`pfs.c:138`).
            return Err(Errno::from_i32(EINVAL));
        }
        let number = self.free.last().copied().ok_or(Errno::from_i32(ENFILE))?;
        let mut buffer = None;
        if fifo {
            // Allocation failure surfaces as "no space", like malloc
            // failing in the C code (`pfs.c:146-147`).
            let mut data = Vec::new();
            data.try_reserve(PIPE_BUFFER_SIZE)
                .map_err(|_| Errno::from_i32(ENOSPC))?;
            data.resize(PIPE_BUFFER_SIZE, 0);
            buffer = Some(data);
        }
        self.free.pop();
        let inode = &mut self.inodes[number as usize - 1];
        debug_assert!(!inode.in_use);
        inode.in_use = true;
        inode.mode = mode;
        inode.owner = owner;
        inode.group = group;
        inode.size = 0;
        inode.pending_updates = UPDATE_ACCESS | UPDATE_MODIFY | UPDATE_CHANGE;
        inode.device = if fifo { 0 } else { device };
        inode.data = buffer;
        inode.start = 0;
        self.describe(number as u64)
    }

    fn put_node(&mut self, inode_number: u64, count: u32) -> Result<(), Errno> {
        self.find(inode_number)?;
        // New-node is the only way to open an inode and counts never grow,
        // so the count is always exactly one (`pfs.c:191-198`).
        if count != 1 {
            return Err(Errno::from_i32(EINVAL));
        }
        let inode = &mut self.inodes[inode_number as usize - 1];
        inode.data = None;
        inode.in_use = false;
        self.free.push(inode_number as u32);
        Ok(())
    }

    fn read(
        &mut self,
        inode_number: u64,
        _position: i64,
        length: usize,
        out: &mut dyn FnMut(&[u8]),
    ) -> Result<usize, Errno> {
        // Pipes are streams: the position is ignored, like the C code
        // ignoring `pos` (`pfs.c:217-219`).
        let inode = self.find(inode_number)?;
        if !is_fifo(inode.mode) {
            return Err(Errno::from_i32(EINVAL));
        }
        if length > PIPE_BUFFER_SIZE {
            return Err(Errno::from_i32(EFBIG));
        }
        let take = length.min(inode.size);
        if take > 0 {
            let data = inode.data.as_ref().expect("pipe holds a buffer");
            out(&data[inode.start..inode.start + take]);
        }
        let inode = self.find_mut(inode_number)?;
        inode.size -= take;
        inode.start += take;
        inode.pending_updates |= UPDATE_ACCESS;
        Ok(take)
    }

    fn write(&mut self, inode_number: u64, _position: i64, data: &[u8]) -> Result<usize, Errno> {
        let inode = self.find(inode_number)?;
        if !is_fifo(inode.mode) {
            return Err(Errno::from_i32(EINVAL));
        }
        if inode.size + data.len() > PIPE_BUFFER_SIZE {
            return Err(Errno::from_i32(EFBIG));
        }
        let inode = self.find_mut(inode_number)?;
        // Compact on write, not on read: many small reads then cost no
        // moves, and a linear buffer costs fewer kernel calls than a ring
        // (`pfs.c:268-273`).
        if inode.start > 0 {
            if inode.size > 0 {
                let buffer = inode.data.as_mut().expect("pipe holds a buffer");
                buffer.copy_within(inode.start..inode.start + inode.size, 0);
            }
            inode.start = 0;
        }
        {
            let buffer = inode.data.as_mut().expect("pipe holds a buffer");
            buffer[inode.size..inode.size + data.len()].copy_from_slice(data);
        }
        inode.size += data.len();
        inode.pending_updates |= UPDATE_CHANGE | UPDATE_MODIFY;
        Ok(data.len())
    }

    fn truncate(&mut self, inode_number: u64, start: i64, end: i64) -> Result<(), Errno> {
        let inode = self.find(inode_number)?;
        if !is_fifo(inode.mode) {
            return Err(Errno::from_i32(EINVAL));
        }
        // Only full truncation is supported (`pfs.c:307-309`).
        if start != 0 || end != 0 {
            return Err(Errno::from_i32(EINVAL));
        }
        let inode = self.find_mut(inode_number)?;
        inode.size = 0;
        inode.pending_updates |= UPDATE_CHANGE | UPDATE_MODIFY;
        Ok(())
    }

    fn stat(&mut self, inode_number: u64, out: &mut [u8]) -> Result<(), Errno> {
        if out.len() < STATUS_BUFFER_SIZE {
            return Err(Errno::from_i32(EINVAL));
        }
        // Refresh lazy times before reporting, exactly once per dirty set.
        // The clock is read only when something is pending, like the C
        // code reading it inside `if (rip->i_update != 0)`.
        let pending = self.find(inode_number)?.pending_updates;
        if pending != 0 {
            // One clock read serves all three fields, like the single
            // `now = clock_time(NULL)` in the C code.
            let now = self.clock.now_seconds();
            let inode = self.find_mut(inode_number)?;
            Self::apply_pending_times(inode, now);
        }
        let inode = self.find(inode_number)?;
        // Device field carries the device number: workaround for an old
        // socketpair bug, kept verbatim (`pfs.c:342`).
        out[0..8].copy_from_slice(&inode.device.to_le_bytes());
        out[8..12].copy_from_slice(&inode.mode.to_le_bytes());
        out[12..16].copy_from_slice(&0u32.to_le_bytes());
        out[16..20].copy_from_slice(&inode.owner.to_le_bytes());
        out[20..24].copy_from_slice(&inode.group.to_le_bytes());
        out[24..32].copy_from_slice(&inode.device.to_le_bytes());
        out[32..40].copy_from_slice(&(inode.size as u64).to_le_bytes());
        out[40..48].copy_from_slice(&inode.accessed.to_le_bytes());
        out[48..56].copy_from_slice(&inode.modified.to_le_bytes());
        out[56..64].copy_from_slice(&inode.changed.to_le_bytes());
        out[64..72].copy_from_slice(&(PIPE_BUFFER_SIZE as u64).to_le_bytes());
        let blocks = (inode.size as u64).div_ceil(STATUS_BLOCK_SIZE);
        out[72..80].copy_from_slice(&blocks.to_le_bytes());
        Ok(())
    }

    fn change_mode(&mut self, inode_number: u64, mode: u32) -> Result<u32, Errno> {
        let inode = self.find_mut(inode_number)?;
        // Preserve the type bits; only permission bits change (`pfs.c:370`).
        inode.mode = (inode.mode & !ALL_PERMISSIONS) | (mode & ALL_PERMISSIONS);
        inode.pending_updates |= UPDATE_MODIFY | UPDATE_CHANGE;
        Ok(inode.mode)
    }
}

/// Build a server with a frozen clock (service entry convenience).
pub fn server() -> PfsServer<FixedClock> {
    PfsServer::new(FixedClock(0))
}

/// Service initialization entry: constructs the server value.
///
/// The process runtime (startup handshake, privilege drop, signal wiring,
/// event loop) belongs to the service-runtime stage and is wired in
/// `main.rs` once that stage lands.
pub fn init() -> PfsServer<FixedClock> {
    server()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mounted() -> PfsServer<StepClock> {
        let mut server = PfsServer::new(StepClock::new(1_000, 10));
        let mut caps = CapabilityFlags::EMPTY;
        let root = server.mount(0, MountFlags::EMPTY, &mut caps).unwrap();
        assert_eq!(root.inode_number, 0);
        assert!(caps.is_64bit());
        server
    }

    fn new_pipe(server: &mut PfsServer<StepClock>) -> u64 {
        server.new_node(0o010600, 100, 100, 0).unwrap().inode_number
    }

    #[test]
    fn test_mount_resets_table_lowest_first() {
        let mut server = mounted();
        assert_eq!(server.free.len(), PFS_INODE_COUNT);
        let first = new_pipe(&mut server);
        assert_eq!(first, 1);
        let second = new_pipe(&mut server);
        assert_eq!(second, 2);
    }

    #[test]
    fn test_newnode_rejects_regular_files() {
        let mut server = mounted();
        assert_eq!(
            server.new_node(0o100644, 0, 0, 0).unwrap_err().to_i32(),
            EINVAL
        );
        // Devices and sockets are accepted.
        assert!(server.new_node(0o020600, 0, 0, 9).is_ok());
        assert!(server.new_node(0o060600, 0, 0, 9).is_ok());
        assert!(server.new_node(0o140777, 0, 0, 0).is_ok());
    }

    #[test]
    fn test_table_exhaustion_reports_file_table_full() {
        let mut server = mounted();
        for _ in 0..PFS_INODE_COUNT {
            new_pipe(&mut server);
        }
        assert_eq!(
            server.new_node(0o010600, 0, 0, 0).unwrap_err().to_i32(),
            ENFILE
        );
    }

    #[test]
    fn test_putnode_frees_and_reuses() {
        let mut server = mounted();
        let first = new_pipe(&mut server);
        server.put_node(first, 1).unwrap();
        // Freed numbers go back on top of the stack.
        let reused = new_pipe(&mut server);
        assert_eq!(reused, first);
        assert_eq!(server.put_node(9999, 1).unwrap_err().to_i32(), EINVAL);
        assert_eq!(server.put_node(reused, 2).unwrap_err().to_i32(), EINVAL);
    }

    #[test]
    fn test_write_read_roundtrip_with_compaction() {
        let mut server = mounted();
        let pipe = new_pipe(&mut server);
        assert_eq!(server.write(pipe, 0, b"hello world").unwrap(), 11);
        let mut first = Vec::new();
        let moved = server
            .read(pipe, 0, 5, &mut |chunk: &[u8]| {
                first.extend_from_slice(chunk)
            })
            .unwrap();
        assert_eq!(moved, 5);
        assert_eq!(first, b"hello");
        // Write more: the leftover compacts to the front first.
        assert_eq!(server.write(pipe, 0, b"!!!").unwrap(), 3);
        let mut rest = Vec::new();
        let moved = server
            .read(pipe, 0, 64, &mut |chunk: &[u8]| {
                rest.extend_from_slice(chunk)
            })
            .unwrap();
        assert_eq!(moved, 9);
        assert_eq!(rest, b" world!!!");
        // Reading past the content stops at the size.
        let mut empty = Vec::new();
        let moved = server
            .read(pipe, 0, 64, &mut |chunk: &[u8]| {
                empty.extend_from_slice(chunk)
            })
            .unwrap();
        assert_eq!(moved, 0);
        assert!(empty.is_empty());
    }

    #[test]
    fn test_pipe_size_limits() {
        let mut server = mounted();
        let pipe = new_pipe(&mut server);
        let mut sink = Vec::new();
        assert_eq!(
            server
                .read(pipe, 0, PIPE_BUFFER_SIZE + 1, &mut |chunk: &[u8]| sink
                    .extend_from_slice(chunk))
                .unwrap_err()
                .to_i32(),
            EFBIG
        );
        let big = alloc::vec![1u8; PIPE_BUFFER_SIZE + 1];
        assert_eq!(server.write(pipe, 0, &big).unwrap_err().to_i32(), EFBIG);
        // Exactly full is fine.
        let full = alloc::vec![2u8; PIPE_BUFFER_SIZE];
        assert_eq!(server.write(pipe, 0, &full).unwrap(), PIPE_BUFFER_SIZE);
        assert_eq!(server.write(pipe, 0, b"x").unwrap_err().to_i32(), EFBIG);
    }

    #[test]
    fn test_non_pipe_read_write_refused() {
        let mut server = mounted();
        let node = server.new_node(0o020600, 0, 0, 7).unwrap().inode_number;
        let mut sink = Vec::new();
        assert_eq!(
            server
                .read(node, 0, 4, &mut |chunk: &[u8]| sink
                    .extend_from_slice(chunk))
                .unwrap_err()
                .to_i32(),
            EINVAL
        );
        assert_eq!(server.write(node, 0, b"x").unwrap_err().to_i32(), EINVAL);
        assert_eq!(server.truncate(node, 0, 0).unwrap_err().to_i32(), EINVAL);
    }

    #[test]
    fn test_truncate_only_full() {
        let mut server = mounted();
        let pipe = new_pipe(&mut server);
        server.write(pipe, 0, b"data").unwrap();
        assert_eq!(server.truncate(pipe, 0, 1).unwrap_err().to_i32(), EINVAL);
        assert_eq!(server.truncate(pipe, 1, 0).unwrap_err().to_i32(), EINVAL);
        server.truncate(pipe, 0, 0).unwrap();
        let mut sink = Vec::new();
        let moved = server
            .read(pipe, 0, 64, &mut |chunk: &[u8]| {
                sink.extend_from_slice(chunk)
            })
            .unwrap();
        assert_eq!(moved, 0);
    }

    #[test]
    fn test_stat_lazy_times_update_once() {
        let mut server = mounted();
        let pipe = new_pipe(&mut server);
        // Creation pends all three updates; first stat consumes second 1000.
        let mut status = [0u8; STATUS_BUFFER_SIZE];
        server.stat(pipe, &mut status).unwrap();
        assert_eq!(&status[40..48], &1000i64.to_le_bytes());
        // Second stat consumes 1010 but nothing is pending: times stand still.
        server.stat(pipe, &mut status).unwrap();
        assert_eq!(&status[40..48], &1000i64.to_le_bytes());
        // A write pends modify+change; next stat picks up 1010 (the clock
        // moved once for the first stat and stands still while clean).
        server.write(pipe, 0, b"x").unwrap();
        server.stat(pipe, &mut status).unwrap();
        assert_eq!(&status[48..56], &1010i64.to_le_bytes());
        // Size and block count follow content.
        assert_eq!(&status[32..40], &1u64.to_le_bytes());
        assert_eq!(&status[72..80], &1u64.to_le_bytes());
        assert_eq!(&status[64..72], &(PIPE_BUFFER_SIZE as u64).to_le_bytes());
    }

    #[test]
    fn test_chmod_keeps_type_bits() {
        let mut server = mounted();
        let pipe = new_pipe(&mut server);
        let mode = server.change_mode(pipe, 0o777).unwrap();
        assert_eq!(mode & FILE_TYPE_MASK, TYPE_FIFO);
        assert_eq!(mode & ALL_PERMISSIONS, 0o777);
        let node = server.describe(pipe).unwrap();
        assert_eq!(node.mode & FILE_TYPE_MASK, TYPE_FIFO);
    }

    #[test]
    fn test_unmount_reports_busy_count() {
        let mut server = mounted();
        new_pipe(&mut server);
        new_pipe(&mut server);
        server.unmounted();
        assert_eq!(server.busy_at_unmount, 2);
    }

    #[test]
    fn test_constants_match_c() {
        assert_eq!(PFS_INODE_COUNT, 512);
        assert_eq!(PIPE_BUFFER_SIZE, 32768);
        assert_eq!(STATUS_BLOCK_SIZE, 512);
        assert_eq!(STATUS_BUFFER_SIZE, 80);
        assert_eq!(ALL_PERMISSIONS, 0o7777);
    }
}
