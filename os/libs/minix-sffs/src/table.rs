//! The host-folder abstraction: one trait, fifteen verbs.
//!
//! C correspondence: `struct sffs_table` (`minix3/minix/include/minix/
//! sffs.h:30-48`) — the per-hypervisor operations every Shared Folder
//! server is built on (libvboxfs for VBFS, libhgfs for HGFS). The C
//! library reaches the host exclusively through this table; the Rust
//! [`SffsTable`] trait keeps that boundary: handles are opaque `u64`s
//! and paths are prefix-joined guest paths. The C bulk-transfer buffer
//! hooks (`t_readbuf`/`t_writebuf`) fold into the slice-based
//! read/write verbs.

/// One directory entry as the host reports it: name plus attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    /// Entry name (no path components).
    pub name: alloc::string::String,
    /// The entry's attributes.
    pub attr: crate::attr::SffsAttr,
}

/// The per-hypervisor host-folder operations.
pub trait SffsTable {
    /// Open a file for reading (`write = false`) or reading and writing.
    /// Returns an opaque handle. C: `t_open(path, flags, mode, &handle)`.
    fn open(&mut self, path: &str, write: bool) -> Result<u64, i32>;
    /// Read `buf.len()` bytes at `pos` from an open file handle.
    /// C: `t_read(handle, buf, size, pos)`.
    fn read(&mut self, handle: u64, pos: u64, buf: &mut [u8]) -> Result<usize, i32>;
    /// Write bytes at `pos` through an open file handle. C: `t_write`.
    fn write(&mut self, handle: u64, pos: u64, bytes: &[u8]) -> Result<usize, i32>;
    /// Close an open file handle. C: `t_close`.
    fn close(&mut self, handle: u64) -> Result<(), i32>;
    /// Open a directory for listing. C: `t_opendir(path, &handle)`.
    fn opendir(&mut self, path: &str) -> Result<u64, i32>;
    /// Read the entry at `index` of an open directory. End of the
    /// directory reports an error; the caller stops on any error
    /// (`search_dir` walk semantics).
    fn readdir(&mut self, handle: u64, index: u32) -> Result<DirEntry, i32>;
    /// Close an open directory handle. C: `t_closedir`.
    fn closedir(&mut self, handle: u64) -> Result<(), i32>;
    /// Fetch attributes for a path. C: `t_getattr(path, &attr)`.
    fn getattr(&mut self, path: &str) -> Result<crate::attr::SffsAttr, i32>;
    /// Store attributes for a path. C: `t_setattr(path, &attr)`.
    fn setattr(&mut self, path: &str, attr: &crate::attr::SffsAttr) -> Result<(), i32>;
    /// Create a directory. C: `t_mkdir(path, mode)`.
    fn mkdir(&mut self, path: &str, mode: u32) -> Result<(), i32>;
    /// Remove a file. C: `t_unlink(path)`.
    fn unlink(&mut self, path: &str) -> Result<(), i32>;
    /// Remove an empty directory. C: `t_rmdir(path)`.
    fn rmdir(&mut self, path: &str) -> Result<(), i32>;
    /// Rename (move) a path. C: `t_rename(opath, npath)`.
    fn rename(&mut self, old: &str, new: &str) -> Result<(), i32>;
    /// Free/total byte counts for the volume holding `path`.
    /// C: `t_queryvol(path, &free, &total)`.
    fn queryvol(&mut self, path: &str) -> Result<(u64, u64), i32>;
}
