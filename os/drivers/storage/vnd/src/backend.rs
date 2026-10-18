//! The backing-file seam: what the driver needs from the file it serves.
//!
//! C correspondence: the file-descriptor legs of `vnd.c` — `pread`/
//! `pwrite` in the transfer loop (`vnd.c:259`, `:279`; the C library's
//! `pread` is itself `lseek`+`read`+`lseek`, `libc/sys/pread.c:11-37`),
//! `fsync` (`vnd.c:299`), `fstat` and `close` in the configure and
//! unconfigure paths (`vnd.c:387-402`, `:449`), and the `copyfd` backcall
//! that steals the caller's descriptor (`vnd.c:380-385`).
//!
//! In C these are libc calls that go through VFS; in the rewrite they are
//! VFS client calls, which need a transport the transfer hook's data plane
//! does not carry (the plane moves bytes through the *caller's* grant; the
//! file traffic is this driver's own IPC to VFS). The trait keeps that
//! traffic behind one seam so the transfer and configure logic stays
//! host-testable, and [`VfsBackend`] is the production implementation over
//! the VFS client.

use minix_sys::ipc::DirectTrapTransport;
use minix_types::types::stat::Stat;

/// What the driver needs to know about the backing file after `fstat`
/// (`vnd.c:387-402` uses the size, the device, the inode, and the file
/// type).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileInfo {
    /// File size in bytes (`st_size`).
    pub size: u64,
    /// Device the file lives on (`st_dev`).
    pub dev: u64,
    /// Inode number (`st_ino`).
    pub ino: u64,
    /// Whether the file is a regular file (`S_ISREG(st_mode)`).
    pub regular: bool,
}

/// The backing-file operations the driver performs.
pub trait FileBackend {
    /// Steal a descriptor from the configuring process
    /// (`copyfd(user_endpt, fd, COPYFD_FROM)`, `vnd.c:380`).
    ///
    /// C's call is a VFS backcall; the rewrite's VFS server implements it
    /// (`do_copyfd`), but no client wrapper has landed yet, so the
    /// production implementation reports `ENOSYS` rather than pretending a
    /// descriptor was handed over.
    fn copyfd(&mut self, user_endpoint: i32, fd: i32) -> Result<i32, i32>;

    /// Read `buf.len()` bytes at `offset` (`pread`, `vnd.c:259`).
    fn pread(&mut self, fd: i32, offset: u64, buf: &mut [u8]) -> Result<usize, i32>;

    /// Write `buf` at `offset` (`pwrite`, `vnd.c:279`).
    fn pwrite(&mut self, fd: i32, offset: u64, buf: &[u8]) -> Result<usize, i32>;

    /// Flush the file (`fsync`, `vnd.c:299`).
    fn fsync(&mut self, fd: i32) -> Result<(), i32>;

    /// Stat the file (`fstat`, `vnd.c:387`).
    fn fstat(&mut self, fd: i32) -> Result<FileInfo, i32>;

    /// Close the file (`close`, `vnd.c:402`).
    fn close(&mut self, fd: i32) -> Result<(), i32>;
}

/// The production backend: the VFS client over the direct trap.
pub struct VfsBackend;

impl FileBackend for VfsBackend {
    fn copyfd(&mut self, user_endpoint: i32, fd: i32) -> Result<i32, i32> {
        // The VFS backcall (`copyfd.c:7-19`); the server-side protocol
        // answers the new descriptor as the reply type.
        minix_sys::vfs::copyfd_via(
            &DirectTrapTransport,
            user_endpoint,
            fd,
            minix_sys::vfs::COPYFD_FROM,
        )
        .map_err(|error| -error.to_i32())
    }

    fn pread(&mut self, fd: i32, offset: u64, buf: &mut [u8]) -> Result<usize, i32> {
        // The C library's `pread` is `lseek`+`read`+`lseek`; the seek
        // back to the saved position is what keeps a descriptor usable by
        // its other holders, and `lseek_via`'s current-position form gives
        // us the same three steps.
        let here = minix_sys::vfs::lseek_via(&DirectTrapTransport, fd, 0, SEEK_CUR)
            .map_err(|error| -error.to_i32())?;
        minix_sys::vfs::lseek_via(&DirectTrapTransport, fd, offset as i64, SEEK_SET)
            .map_err(|error| -error.to_i32())?;
        let read =
            minix_sys::vfs::read_via(&DirectTrapTransport, fd, buf.as_mut_ptr() as u64, buf.len())
                .map_err(|error| -error.to_i32());
        // The restore runs on both paths, exactly like the C wrapper.
        minix_sys::vfs::lseek_via(&DirectTrapTransport, fd, here, SEEK_SET)
            .map_err(|error| -error.to_i32())?;
        read
    }

    fn pwrite(&mut self, fd: i32, offset: u64, buf: &[u8]) -> Result<usize, i32> {
        let here = minix_sys::vfs::lseek_via(&DirectTrapTransport, fd, 0, SEEK_CUR)
            .map_err(|error| -error.to_i32())?;
        minix_sys::vfs::lseek_via(&DirectTrapTransport, fd, offset as i64, SEEK_SET)
            .map_err(|error| -error.to_i32())?;
        let written =
            minix_sys::vfs::write_via(&DirectTrapTransport, fd, buf.as_ptr() as u64, buf.len())
                .map_err(|error| -error.to_i32());
        minix_sys::vfs::lseek_via(&DirectTrapTransport, fd, here, SEEK_SET)
            .map_err(|error| -error.to_i32())?;
        written
    }

    fn fsync(&mut self, fd: i32) -> Result<(), i32> {
        minix_sys::vfs::fsync_via(&DirectTrapTransport, fd).map_err(|error| -error.to_i32())
    }

    fn fstat(&mut self, fd: i32) -> Result<FileInfo, i32> {
        let mut raw = [0u8; core::mem::size_of::<Stat>()];
        minix_sys::vfs::fstat_via(&DirectTrapTransport, fd, raw.as_mut_ptr() as u64)
            .map_err(|error| -error.to_i32())?;
        // SAFETY: the client wrote one `struct stat` into the buffer; the
        // type's layout is the same one the client encodes against.
        let stat: Stat = unsafe { core::ptr::read_unaligned(raw.as_ptr() as *const Stat) };
        Ok(FileInfo {
            // C keeps `st_size` in an off_t; a negative size cannot reach
            // here (VFS reports the real file length), and the device size
            // is unsigned downstream.
            size: stat.st_size.max(0) as u64,
            dev: stat.st_dev,
            ino: stat.st_ino,
            regular: (stat.st_mode & S_IFMT) == S_IFREG,
        })
    }

    fn close(&mut self, fd: i32) -> Result<(), i32> {
        minix_sys::vfs::close_via(&DirectTrapTransport, fd).map_err(|error| -error.to_i32())
    }
}

/// Seek from the start (`SEEK_SET`).
pub const SEEK_SET: i32 = 0;
/// Seek from the current position (`SEEK_CUR`).
pub const SEEK_CUR: i32 = 1;
/// File-type mask (`S_IFMT`).
pub const S_IFMT: u32 = 0o170000;
/// Regular-file type (`S_IFREG`).
pub const S_IFREG: u32 = 0o100000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seek_and_mode_constants_match_the_c_headers() {
        assert_eq!(SEEK_SET, 0);
        assert_eq!(SEEK_CUR, 1);
        assert_eq!(S_IFMT, 0o170000);
        assert_eq!(S_IFREG, 0o100000);
        // S_ISREG on a regular file and on a directory.
        assert_eq!(0o100644 & S_IFMT, S_IFREG);
        assert_ne!(0o040755 & S_IFMT, S_IFREG);
    }

    #[test]
    fn test_production_backend_reaches_the_vfs_calls() {
        // The backend goes through the real trap transport, so in a host
        // test the call cannot complete (no kernel answers the trap) and
        // the failure is the transport's, not a stand-in: the two legs are
        // wired to `minix-sys`'s VFS client wrappers.
        let mut backend = VfsBackend;
        assert_eq!(backend.copyfd(7, 3), Err(-minix_types::EIO));
        assert_eq!(backend.fsync(3), Err(-minix_types::EIO));
    }
}
