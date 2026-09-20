//! Working-directory query: the user-space traversal behind `getcwd`.
//!
//! C correspondence: `minix3/minix/lib/libc/sys/__getcwd.c` (Kees J. Bot's
//! stat/opendir climb). Minix3 has no kernel call that prints the working
//! directory — the `__getcwd.S` stub named in
//! `minix3/lib/libc/sys/Makefile.inc:112` does not exist in the tree — so
//! the C library answers the question with ordinary calls: `stat(".")` and
//! `stat("..")` recognize the root by device-and-inode pair, the parent
//! listing is searched for the entry that is the current directory (a
//! record-inode quick pass, falling back to a stat-verified full pass for
//! mount points), each found name is prepended into the caller buffer from
//! its end, and `chdir("..")` climbs while `recover` walks the built path
//! back down so the working directory is unchanged after the call — on
//! failure as well as on success.
//!
//! The Rust face keeps that contract: identical errno outcomes (`EINVAL`
//! for a buffer of one byte or less, `ERANGE` when the path does not fit,
//! `ENOENT` when a parent listing has no matching entry), the working
//! directory restored on every exit, and the NUL-terminated path written
//! to the buffer start; the return borrows the path without the
//! terminator.
//!
//! Known platform gap (lifted 2026-09-21, NL10): path requests used to
//! carry the name inline only and both sides rejected longer names, so a
//! parent entry longer than the inline window could not be stat-verified
//! and was skipped. The path family now ships the name pointer and length
//! on every request (C `loadname.c:15-17`) and the server fetches long
//! names from the caller's address space (C `copy_path` → `fetch_name`,
//! utility.c:31-32), so this traversal matches C again; a candidate is
//! skipped only when its probe stat genuinely fails (`getcwd.c:118`).

use crate::ipc::IpcTransport;
use crate::vfs;
use crate::{Errno, Stat};
use minix_types::ERANGE;

/// Offset of the name inside a directory record.
///
/// C: `_DIRENT_NAMEOFF` (`minix3/sys/sys/dirent.h:94-99`): inode number
/// (eight bytes) plus record length, name length, and type fields (two
/// plus two plus one bytes).
const RECORD_NAME_OFFSET: usize = 13;

/// Longest name a listing carries.
///
/// C: `NAME_MAX` (`minix3/sys/sys/syslimits.h:57`, value 511); the file
/// server's encoder refuses past `minix_fs::MAX_NAME_LENGTH`.
const NAME_MAX: usize = 511;

/// Listing refill chunk.
///
/// C's DIR stream sizes its buffer from the file-system block size
/// (`gen/opendir.c`); any chunk works because the server writes
/// record-aligned portions and never splits a record across refills.
const LISTING_CHUNK: usize = 512;

/// The syscall seam the traversal climbs on.
///
/// Production wires it to the VFS wire ([`WireCwdSyscalls`]); tests
/// substitute an in-memory file system. The method set is exactly the
/// libc face the C mechanism uses, which keeps the traversal honest: it
/// cannot do anything the C code could not.
pub(crate) trait CwdSyscalls {
    /// C: `stat(path, sb)` — status by path, following symlinks.
    fn stat_path(&self, path: &[u8], out: &mut Stat) -> Result<(), Errno>;
    /// C: `opendir(path)` — open a directory for listing (`gen/opendir.c`).
    fn open_directory(&self, path: &[u8]) -> Result<i32, Errno>;
    /// C: `getdents(fd, buf, nbytes)` — one refill of the listing stream.
    fn read_entries(&self, fd: i32, buffer: &mut [u8]) -> Result<usize, Errno>;
    /// C: `rewinddir(dir)` — restart the listing (`gen/rewinddir.c`).
    fn rewind(&self, fd: i32) -> Result<(), Errno>;
    /// C: `closedir(dir)` — drop the listing handle.
    fn close(&self, fd: i32) -> Result<(), Errno>;
    /// C: `chdir(path)` — move the working directory.
    fn change_dir(&self, path: &[u8]) -> Result<(), Errno>;
}

/// Production seam: the traversal's calls go out over the VFS wire.
struct WireCwdSyscalls<'a, T: IpcTransport> {
    transport: &'a T,
}

impl<'a, T: IpcTransport> WireCwdSyscalls<'a, T> {
    /// Open flags for the listing handle.
    ///
    /// C: `opendir` opens `O_RDONLY|O_DIRECTORY|O_NONBLOCK|O_CLOEXEC`
    /// (`minix3/lib/libc/gen/opendir.c:81`); `O_DIRECTORY` is
    /// `0x00200000` (`minix3/sys/sys/fcntl.h:117`). The traversal holds
    /// the handle within one call, so the exec-sweep (`O_CLOEXEC`) and
    /// nonblocking faces are inert here and only the directory demand is
    /// carried.
    const LISTING_OPEN_FLAGS: i32 = 0x0020_0000;
}

impl<T: IpcTransport> CwdSyscalls for WireCwdSyscalls<'_, T> {
    fn stat_path(&self, path: &[u8], out: &mut Stat) -> Result<(), Errno> {
        // SAFETY: `path` and `out` live in this same stack frame and the
        // wire call is synchronous — the library reads its own buffers,
        // the C `stat` wrapper shape (`minix3/minix/lib/libc/sys/stat.c`).
        vfs::stat_via(
            self.transport,
            path.as_ptr() as u64,
            path.len().saturating_add(1),
            out as *mut Stat as u64,
        )
    }

    fn open_directory(&self, path: &[u8]) -> Result<i32, Errno> {
        // SAFETY: path bytes live in this same stack frame (see above).
        vfs::open_existing_via(
            self.transport,
            path.as_ptr() as u64,
            path.len().saturating_add(1),
            Self::LISTING_OPEN_FLAGS,
        )
    }

    fn read_entries(&self, fd: i32, buffer: &mut [u8]) -> Result<usize, Errno> {
        // SAFETY: buffer bytes live in this same stack frame (see above).
        vfs::getdents_via(self.transport, fd, buffer.as_mut_ptr() as u64, buffer.len())
    }

    fn rewind(&self, fd: i32) -> Result<(), Errno> {
        // C `rewinddir` seeks the descriptor back to the start; origin
        // zero is `SEEK_SET`.
        vfs::lseek_via(self.transport, fd, 0, 0).map(|_| ())
    }

    fn close(&self, fd: i32) -> Result<(), Errno> {
        vfs::close_via(self.transport, fd)
    }

    fn change_dir(&self, path: &[u8]) -> Result<(), Errno> {
        // SAFETY: path bytes live in this same stack frame (see above).
        vfs::chdir_via(
            self.transport,
            path.as_ptr() as u64,
            path.len().saturating_add(1),
        )
    }
}

/// Runs the traversal over an explicit transport.
///
/// The zero-argument [`crate::getcwd`] delegates here with the
/// direct-trap transport; scripted tests call this face with
/// [`crate::ipc::CannedTransport`].
pub fn getcwd_via<'a, T: IpcTransport>(
    transport: &'a T,
    buffer: &'a mut [u8],
) -> Result<&'a [u8], Errno> {
    run_traversal(&WireCwdSyscalls { transport }, buffer)
}

/// Runs the traversal over any syscall seam.
///
/// `buffer` receives the NUL-terminated absolute path at its front, built
/// backwards exactly like the C original; the return borrows the path
/// without the terminator.
pub(crate) fn run_traversal<'a, S: CwdSyscalls>(
    sys: &S,
    buffer: &'a mut [u8],
) -> Result<&'a [u8], Errno> {
    // C: `if (path == NULL || size <= 1)` — the slice length plays the
    // NULL role, and nothing fits behind the mandatory terminator in a
    // one-byte buffer (`getcwd.c:68`).
    if buffer.len() <= 1 {
        return Err(Errno::EINVAL);
    }

    // C: `p = path + size; *--p = 0;` — the string grows backwards from
    // the buffer end; `front` indexes the newest byte (`getcwd.c:70-71`).
    let end = buffer.len();
    let mut front = end - 1;
    buffer[front] = 0;

    let mut listing = [0u8; LISTING_CHUNK];
    let mut probe = [0u8; 3 + NAME_MAX + 1];

    // C: `stat(".", &current)` — failure exits without recover because
    // nothing has been built or climbed yet (`getcwd.c:73`).
    let mut current = fresh_stat();
    sys.stat_path(compose(&mut probe, &[b"."]), &mut current)?;

    loop {
        // C: `stat(dotdot, &above)` — the parent, resolved from the
        // (possibly already climbed) working directory (`getcwd.c:76`).
        let mut above = fresh_stat();
        if let Err(e) = sys.stat_path(compose(&mut probe, &[b".."]), &mut above) {
            return Err(fail_traversal(sys, buffer, front, e));
        }
        // C: same device and inode pair means the root is above us and
        // the climb is done (`getcwd.c:78-80`).
        if above.st_dev == current.st_dev && above.st_ino == current.st_ino {
            break;
        }

        // C: `opendir(dotdot)` (`getcwd.c:82`).
        let fd = match sys.open_directory(compose(&mut probe, &[b".."])) {
            Ok(fd) => fd,
            Err(e) => return Err(fail_traversal(sys, buffer, front, e)),
        };

        // C: the quick record-inode pass only runs while the parent sits
        // on the same device; across a mount point the climb starts
        // straight into the stat-verified pass (`getcwd.c:84-87`).
        let found = if above.st_dev == current.st_dev {
            match scan_once(sys, fd, &current, &mut probe, &mut listing, true) {
                Some(name) => Some(name),
                None => {
                    // C: the quick pass ran dry — `rewinddir` and repeat
                    // as the full pass (`getcwd.c:94-97`).
                    let _ = sys.rewind(fd);
                    scan_once(sys, fd, &current, &mut probe, &mut listing, false)
                }
            }
        } else {
            scan_once(sys, fd, &current, &mut probe, &mut listing, false)
        };

        let name = match found {
            Some(name) => name,
            // C: second exhaustion closes the handle, reports ENOENT and
            // walks back (`getcwd.c:98-102`).
            None => {
                let _ = sys.close(fd);
                return Err(fail_traversal(sys, buffer, front, Errno::ENOENT));
            }
        };

        // C: prepend "name/" before the front pointer; a full buffer is
        // ERANGE with the handle closed first (`getcwd.c:124-130`).
        if addpath(buffer, &mut front, name.as_slice()).is_err() {
            let _ = sys.close(fd);
            return Err(fail_traversal(sys, buffer, front, Errno::from_i32(ERANGE)));
        }
        // C: `closedir(d)` after the name is banked (`getcwd.c:131`).
        let _ = sys.close(fd);

        // C: `chdir(dotdot)` climbs; failure walks back with the handle
        // already closed (`getcwd.c:133`).
        if let Err(e) = sys.change_dir(compose(&mut probe, &[b".."])) {
            return Err(fail_traversal(sys, buffer, front, e));
        }
        // C: `current = above` — the parent becomes the next subject
        // (`getcwd.c:136`).
        current = above;
    }

    // C: `recover(p)` — re-enter every built component so the working
    // directory comes out where it started (`getcwd.c:138`).
    recover(sys, &buffer[front..end - 1])?;
    // C: nothing added means the working directory is the root
    // (`getcwd.c:139`).
    if front == end - 1 {
        front -= 1;
        buffer[front] = b'/';
    }
    // C: `strcpy(path, p)` moves the built string to the buffer start
    // (`getcwd.c:140`); the copy overlaps, so it is a block move here.
    if front > 0 {
        buffer.copy_within(front..end, 0);
    }
    Ok(&buffer[..end - 1 - front])
}

/// One pass over the parent listing.
///
/// `quick` is C cycle 0: only entries whose directory-record inode equals
/// the current inode are stat-verified (`getcwd.c:108-112`). The full
/// pass — C cycle 1 — stats every entry, which is how the climb resolves
/// a mount point whose record inode differs from the mounted root
/// (`getcwd.c:114-119`). A failing probe stat merely skips the candidate
/// (`getcwd.c:118`); a failing or empty refill ends the pass the same way
/// an exhausted stream does for C's `readdir`.
fn scan_once<S: CwdSyscalls>(
    sys: &S,
    fd: i32,
    current: &Stat,
    probe: &mut [u8],
    listing: &mut [u8],
    quick: bool,
) -> Option<EntryName> {
    let mut consumed = 0;
    let mut filled = 0;
    loop {
        if consumed == filled {
            match sys.read_entries(fd, listing) {
                Ok(n) if n > 0 => {
                    filled = n;
                    consumed = 0;
                }
                _ => return None,
            }
        }
        let record = match parse_record(&listing[consumed..filled]) {
            Some((record_len, entry)) => {
                consumed += record_len;
                entry
            }
            // C: a shape-broken record ends the listing the way the
            // bogus-pointer and reclen guards return NULL
            // (`gen/readdir.c:76-80`).
            None => return None,
        };
        let (inode, name) = record;
        // C `readdir`: deleted entries are invisible
        // (`gen/readdir.c:82-83`), and the dot entries never name a child.
        if inode == 0 || name == b"." || name == b".." {
            continue;
        }
        // The server never emits past NAME_MAX; the guard keeps the probe
        // bound honest against a shape-valid but oversized record.
        if name.len() > NAME_MAX {
            continue;
        }
        if quick && inode != current.st_ino {
            continue;
        }
        // C: `stat("../name")` and match both fields
        // (`getcwd.c:116-122`).
        let mut candidate = fresh_stat();
        if sys
            .stat_path(compose(probe, &[b"../", name]), &mut candidate)
            .is_err()
        {
            continue;
        }
        if candidate.st_ino == current.st_ino && candidate.st_dev == current.st_dev {
            let mut held = EntryName {
                length: name.len(),
                bytes: [0u8; NAME_MAX + 1],
            };
            held.bytes[..name.len()].copy_from_slice(name);
            return Some(held);
        }
    }
}

/// A matched entry name held by value, so the listing buffer can be
/// refilled after the match without borrowing tangles.
struct EntryName {
    length: usize,
    bytes: [u8; NAME_MAX + 1],
}

impl EntryName {
    fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.length]
    }
}

/// Parses one directory record from the front of `bytes`.
///
/// Record layout (C `struct dirent`, `minix3/sys/sys/dirent.h:60-67`):
/// `d_fileno` sixty-four bits, `d_reclen` and `d_namlen` sixteen bits
/// each, `d_type` eight bits, name from offset thirteen. The stream
/// advances by `d_reclen` (`gen/readdir.c:74-81`). Returns the record
/// length and the `(inode, name)` pair, or `None` when the tail is not a
/// whole record.
fn parse_record(bytes: &[u8]) -> Option<(usize, (u64, &[u8]))> {
    if bytes.len() < RECORD_NAME_OFFSET {
        return None;
    }
    let inode = u64::from_le_bytes(bytes[0..8].try_into().ok()?);
    let record_len = u16::from_le_bytes(bytes[8..10].try_into().ok()?) as usize;
    let name_len = u16::from_le_bytes(bytes[10..12].try_into().ok()?) as usize;
    if record_len == 0 || record_len > bytes.len() {
        return None;
    }
    let name_end = RECORD_NAME_OFFSET + name_len;
    // The name plus its terminator must fit inside the record.
    if name_end + 1 > record_len {
        return None;
    }
    Some((record_len, (inode, &bytes[RECORD_NAME_OFFSET..name_end])))
}

/// Prepends `"/name"` before `front`.
///
/// C `addpath` (`getcwd.c:20-36`): the growing string always starts with
/// a slash, and reaching the buffer start is the caller's ERANGE.
fn addpath(buffer: &mut [u8], front: &mut usize, name: &[u8]) -> Result<(), Errno> {
    if *front < name.len() + 1 {
        return Err(Errno::from_i32(ERANGE));
    }
    *front -= name.len();
    buffer[*front..*front + name.len()].copy_from_slice(name);
    *front -= 1;
    buffer[*front] = b'/';
    Ok(())
}

/// Walks the built path back down, component by component.
///
/// C `recover` (`getcwd.c:38-57`) chdirs one component at a time — the
/// comment there says why: the whole pathname may be too long for one
/// call. The empty build (failure before the first component) walks
/// nothing.
fn recover(sys: &impl CwdSyscalls, built: &[u8]) -> Result<(), Errno> {
    for component in built.split(|&b| b == b'/') {
        if component.is_empty() {
            continue;
        }
        sys.change_dir(component)?;
    }
    Ok(())
}

/// Reports `saved` after walking the working directory back.
///
/// C: `recover(p); return -1;` — recover restores the original error when
/// it succeeds and reports its own chdir failure otherwise
/// (`getcwd.c:43`, `:55-56`).
fn fail_traversal(sys: &impl CwdSyscalls, buffer: &mut [u8], front: usize, saved: Errno) -> Errno {
    let end = buffer.len();
    match recover(sys, &buffer[front..end - 1]) {
        Ok(()) => saved,
        Err(e) => e,
    }
}

/// Fills the front of `probe` with `parts` and returns the joined bytes.
///
/// C builds `"../name"` in `char name[3 + NAME_MAX + 1]`
/// (`getcwd.c:90`, `:116-117`); callers here only ever join constants and
/// one [`NAME_MAX`]-bounded name.
fn compose<'a>(probe: &'a mut [u8], parts: &[&[u8]]) -> &'a [u8] {
    let mut filled = 0;
    for part in parts {
        probe[filled..filled + part.len()].copy_from_slice(part);
        filled += part.len();
    }
    &probe[..filled]
}

/// A zeroed status receiver, the C `struct stat` on the stack.
fn fresh_stat() -> Stat {
    // SAFETY: `Stat` is plain `repr(C)` data; an all-zero value is the
    // starting point the subsequent status call overwrites.
    unsafe { core::mem::zeroed() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::CannedTransport;
    use alloc::collections::BTreeMap;
    use alloc::vec;
    use alloc::vec::Vec;
    use core::cell::RefCell;

    /// Encodes one directory record into `out` with the pinned layout and
    /// returns the record length: inode, record length, name length, type
    /// byte, name, zero padding to the eight-byte boundary (C
    /// `_DIRENT_RECLEN`, `minix3/sys/sys/dirent.h:105-107`).
    fn encode_record(out: &mut [u8], inode: u64, name: &[u8]) -> usize {
        let record_len = (RECORD_NAME_OFFSET + name.len() + 1 + 7) & !7;
        out[0..8].copy_from_slice(&inode.to_le_bytes());
        out[8..10].copy_from_slice(&(record_len as u16).to_le_bytes());
        out[10..12].copy_from_slice(&(name.len() as u16).to_le_bytes());
        out[12] = 4; // C `DT_DIR`, `minix3/sys/sys/dirent.h:70`.
        out[RECORD_NAME_OFFSET..RECORD_NAME_OFFSET + name.len()].copy_from_slice(name);
        record_len
    }

    /// In-memory stand-in for the VFS face: paths resolve against a
    /// mutable working directory, directories hand out one record per
    /// refill (forcing the same refill-and-continue shape C's DIR stream
    /// has), and every working-directory move is logged.
    struct FakeFs {
        /// Every stat-able path, absolute-keyed, to `(device, inode)`.
        files: BTreeMap<Vec<u8>, (u64, u64)>,
        /// Directory listings: absolute dir key to `(record inode, name)`.
        listings: BTreeMap<Vec<u8>, Vec<(u64, Vec<u8>)>>,
        /// Working directory as components (empty vector is the root).
        cwd: RefCell<Vec<Vec<u8>>>,
        /// Every `chdir` argument, in call order.
        chdir_log: RefCell<Vec<Vec<u8>>>,
        /// Every rewind, in call order.
        rewind_log: RefCell<Vec<i32>>,
        /// Open handles: descriptor to `(dir key, next record index)`.
        handles: RefCell<BTreeMap<i32, (Vec<u8>, usize)>>,
        next_fd: core::cell::Cell<i32>,
    }

    impl FakeFs {
        /// Builds a file system from `(absolute path, device, inode)`
        /// triples plus per-directory record lists.
        fn new(
            files: &[(&str, u64, u64)],
            listings: &[(&str, Vec<(u64, &str)>)],
            cwd: &[&str],
        ) -> Self {
            let mut table = BTreeMap::new();
            for (path, device, inode) in files {
                table.insert(Vec::from(*path), (*device, *inode));
            }
            let mut dirs = BTreeMap::new();
            for (dir, records) in listings {
                dirs.insert(
                    Vec::from(*dir),
                    records
                        .iter()
                        .map(|(ino, name)| (*ino, Vec::from(*name)))
                        .collect(),
                );
            }
            FakeFs {
                files: table,
                listings: dirs,
                cwd: RefCell::new(cwd.iter().map(|p| Vec::from(*p)).collect()),
                chdir_log: RefCell::new(Vec::new()),
                rewind_log: RefCell::new(Vec::new()),
                handles: RefCell::new(BTreeMap::new()),
                next_fd: core::cell::Cell::new(10),
            }
        }

        /// Resolves a traversal path against the working directory and
        /// returns the absolute key.
        fn resolve(&self, path: &[u8]) -> Vec<u8> {
            let mut components = self.cwd.borrow().clone();
            for part in path.split(|&b| b == b'/') {
                match part {
                    b"" | b"." => {}
                    b".." => {
                        components.pop();
                    }
                    name => components.push(Vec::from(name)),
                }
            }
            let mut key = Vec::new();
            for component in &components {
                key.push(b'/');
                key.extend_from_slice(component);
            }
            if key.is_empty() {
                key.push(b'/');
            }
            key
        }
    }

    impl CwdSyscalls for FakeFs {
        fn stat_path(&self, path: &[u8], out: &mut Stat) -> Result<(), Errno> {
            let key = self.resolve(path);
            match self.files.get(&key) {
                Some(&(device, inode)) => {
                    out.st_dev = device;
                    out.st_ino = inode;
                    Ok(())
                }
                None => Err(Errno::ENOENT),
            }
        }

        fn open_directory(&self, path: &[u8]) -> Result<i32, Errno> {
            let key = self.resolve(path);
            if !self.listings.contains_key(&key) {
                return Err(Errno::ENOENT);
            }
            let fd = self.next_fd.get();
            self.next_fd.set(fd + 1);
            self.handles.borrow_mut().insert(fd, (key, 0));
            Ok(fd)
        }

        fn read_entries(&self, fd: i32, buffer: &mut [u8]) -> Result<usize, Errno> {
            let mut handles = self.handles.borrow_mut();
            let Some((key, index)) = handles.get_mut(&fd) else {
                return Err(Errno::EBADF);
            };
            let Some(records) = self.listings.get(key) else {
                return Err(Errno::ENOENT);
            };
            let Some((inode, name)) = records.get(*index) else {
                return Ok(0);
            };
            *index += 1;
            let written = encode_record(buffer, *inode, name);
            Ok(written)
        }

        fn rewind(&self, fd: i32) -> Result<(), Errno> {
            self.rewind_log.borrow_mut().push(fd);
            if let Some((_, index)) = self.handles.borrow_mut().get_mut(&fd) {
                *index = 0;
            }
            Ok(())
        }

        fn close(&self, fd: i32) -> Result<(), Errno> {
            self.handles.borrow_mut().remove(&fd);
            Ok(())
        }

        fn change_dir(&self, path: &[u8]) -> Result<(), Errno> {
            let key = self.resolve(path);
            if !self.files.contains_key(&key) {
                return Err(Errno::ENOENT);
            }
            self.chdir_log.borrow_mut().push(Vec::from(path));
            let mut components = self.cwd.borrow().clone();
            for part in path.split(|&b| b == b'/') {
                match part {
                    b"" | b"." => {}
                    b".." => {
                        components.pop();
                    }
                    name => components.push(Vec::from(name)),
                }
            }
            *self.cwd.borrow_mut() = components;
            Ok(())
        }
    }

    /// A nested tree answers the full path; the climb and the walk-back
    /// are both visible in the chdir log (`..` twice down, `a`, `b` up).
    #[test]
    fn test_getcwd_reports_nested_path_and_restores_directory() {
        let fs = FakeFs::new(
            &[
                ("/", 1, 1),
                ("/a", 1, 10),
                ("/a/b", 1, 20),
                ("/etc", 1, 30),
                ("/tmp", 1, 40),
            ],
            &[
                ("/", vec![(1, "a"), (1, "etc")]),
                ("/a", vec![(1, "b"), (1, "tmp")]),
            ],
            &["a", "b"],
        );
        let mut buffer = [0u8; 64];
        let path = run_traversal(&fs, &mut buffer).expect("nested traversal");
        assert_eq!(path, b"/a/b");
        // The buffer also carries the C face: NUL-terminated at the front.
        assert_eq!(&buffer[..5], b"/a/b\0");
        assert_eq!(
            fs.chdir_log.borrow().as_slice(),
            [b"..".to_vec(), b"..".to_vec(), b"a".to_vec(), b"b".to_vec()]
        );
        assert_eq!(fs.cwd.borrow().as_slice(), [b"a".to_vec(), b"b".to_vec()]);
    }

    /// The root working directory reports a single slash and never opens
    /// a listing.
    #[test]
    fn test_getcwd_root_reports_slash() {
        let fs = FakeFs::new(&[("/", 1, 1)], &[], &[]);
        let mut buffer = [0u8; 64];
        let path = run_traversal(&fs, &mut buffer).expect("root traversal");
        assert_eq!(path, b"/");
        assert!(fs.handles.borrow().is_empty());
    }

    /// One-byte and empty buffers are EINVAL before any call goes out
    /// (C `getcwd.c:68`).
    #[test]
    fn test_getcwd_tiny_buffer_is_einval() {
        let fs = FakeFs::new(&[("/", 1, 1)], &[], &[]);
        let mut one = [0u8; 1];
        assert_eq!(run_traversal(&fs, &mut one).unwrap_err(), Errno::EINVAL);
        let mut none = [0u8; 0];
        assert_eq!(run_traversal(&fs, &mut none).unwrap_err(), Errno::EINVAL);
        assert!(fs.chdir_log.borrow().is_empty());
    }

    /// A buffer too small for the built path is ERANGE and the working
    /// directory still comes out unchanged (C `getcwd.c:125-130`).
    #[test]
    fn test_getcwd_small_buffer_is_erange_and_restores_directory() {
        let fs = FakeFs::new(
            &[
                ("/", 1, 1),
                ("/a", 1, 10),
                ("/a/b", 1, 20),
                ("/a/tmp", 1, 40),
            ],
            &[("/", vec![(1, "a")]), ("/a", vec![(1, "b"), (1, "tmp")])],
            &["a", "b"],
        );
        // "/a/b" plus the terminator needs five bytes; three cannot hold
        // even the first component.
        let mut buffer = [0u8; 3];
        let error = run_traversal(&fs, &mut buffer).unwrap_err();
        assert_eq!(error, Errno::from_i32(ERANGE));
        assert_eq!(fs.cwd.borrow().as_slice(), [b"a".to_vec(), b"b".to_vec()]);
    }

    /// Deleted records — inode zero in the listing — are invisible even
    /// when their name still stats, so the live name under the same inode
    /// wins (C `readdir` deleted skip, `gen/readdir.c:82-83`).
    #[test]
    fn test_getcwd_skips_deleted_entries() {
        let fs = FakeFs::new(
            &[
                ("/", 1, 1),
                ("/a", 1, 10),
                ("/a/b", 1, 20),
                ("/a/ghost", 1, 20),
            ],
            &[
                ("/", vec![(1, "a")]),
                // "ghost" carries inode zero in the record: deleted on
                // disk, though stat still resolves it.
                ("/a", vec![(0, "ghost"), (20, "b")]),
            ],
            &["a", "b"],
        );
        let mut buffer = [0u8; 64];
        let path = run_traversal(&fs, &mut buffer).expect("deleted-skip traversal");
        assert_eq!(path, b"/a/b");
    }

    /// A mount point — working directory on another device than the
    /// parent record — goes straight to the stat-verified full pass and
    /// finds the entry the record-inode hint cannot name.
    #[test]
    fn test_getcwd_mountpoint_uses_full_scan() {
        let fs = FakeFs::new(
            &[
                ("/", 1, 1),
                ("/a", 1, 10),
                ("/a/other", 1, 50),
                ("/a/mnt", 1, 7),
                ("/a/mnt/etc", 2, 30),
            ],
            &[
                ("/", vec![(1, "a")]),
                ("/a", vec![(1, "mnt")]),
                // The parent of the mount point lists "etc" on device one;
                // only the stat-verified pass can match the mounted root.
                ("/a/mnt", vec![(1, "other"), (1, "etc")]),
            ],
            &["a", "mnt", "etc"],
        );
        let mut buffer = [0u8; 64];
        let path = run_traversal(&fs, &mut buffer).expect("mountpoint traversal");
        assert_eq!(path, b"/a/mnt/etc");
    }

    /// When the record-inode hint misses (same device, wrong record
    /// inode) the quick pass runs dry, the listing is rewound, and the
    /// full pass finds the directory by stat.
    #[test]
    fn test_getcwd_hint_miss_falls_back_to_full_scan() {
        let fs = FakeFs::new(
            &[
                ("/", 1, 1),
                ("/a", 1, 10),
                ("/a/b", 1, 20),
                ("/a/tmp", 1, 40),
            ],
            // The record claims inode 999 for "b"; only the stat-verified
            // pass can identify it. The root listing keeps the true hint,
            // so exactly one rewind happens in the whole traversal.
            &[("/", vec![(10, "a")]), ("/a", vec![(999, "b"), (1, "tmp")])],
            &["a", "b"],
        );
        let mut buffer = [0u8; 64];
        let path = run_traversal(&fs, &mut buffer).expect("fallback traversal");
        assert_eq!(path, b"/a/b");
        assert_eq!(fs.rewind_log.borrow().len(), 1);
    }

    /// A failing parent stat mid-climb walks the built path back down and
    /// reports the original error (C `getcwd.c:76` recover path).
    #[test]
    fn test_getcwd_midclimb_failure_restores_directory() {
        let fs = FakeFs::new(
            &[
                // Deliberately no "/" entry: the second climb's
                // `stat("..")` fails after the first component is banked.
                ("/a", 1, 10),
                ("/a/b", 1, 20),
                ("/a/tmp", 1, 40),
            ],
            &[("/", vec![(1, "a")]), ("/a", vec![(1, "b"), (1, "tmp")])],
            &["a", "b"],
        );
        let mut buffer = [0u8; 64];
        let error = run_traversal(&fs, &mut buffer).unwrap_err();
        assert_eq!(error, Errno::ENOENT);
        // The walk-back re-entered "b" from the parent.
        assert_eq!(
            fs.chdir_log.borrow().as_slice(),
            [b"..".to_vec(), b"b".to_vec()]
        );
        assert_eq!(fs.cwd.borrow().as_slice(), [b"a".to_vec(), b"b".to_vec()]);
    }

    /// The wire face opens with the dot-stat and carries the path inline
    /// exactly like every other path request; the scripted transport
    /// pins the opening calls (dot first, dot-dot second — both stat).
    #[test]
    fn test_getcwd_wire_opens_with_dot_stat() {
        let mut transport = CannedTransport::new();
        let mut message = crate::syscall::cleared_message();
        message.m_type = 0;
        transport.reply_sendrec(Ok(message));
        let mut buffer = [0u8; 64];
        let _ = getcwd_via(&transport, &mut buffer);
        let sent_log = transport.sent.borrow();
        assert_eq!(sent_log.len(), 2);
        let (destination, sent) = sent_log[0];
        assert_eq!(sent.m_type, vfs::VFS_CALL_STAT);
        assert_eq!(sent.m_type, 0x115);
        assert_eq!(destination, vfs::vfs_endpoint());
        // SAFETY(test): 读回 StatPathPayload 三 lane（length@0、name@8、
        // buffer@16；stat 载荷无内联窗，路径经指针交付）。
        let raw = unsafe { &sent.m_u.raw };
        assert_eq!(u64::from_ne_bytes(raw[0..8].try_into().unwrap()), 2);
        assert_ne!(u64::from_ne_bytes(raw[8..16].try_into().unwrap()), 0);
        assert_ne!(u64::from_ne_bytes(raw[16..24].try_into().unwrap()), 0);
        // The second call stats the parent — the climb's recurring probe.
        let (_, second) = sent_log[1];
        assert_eq!(second.m_type, vfs::VFS_CALL_STAT);
        // SAFETY(test): 同上，读 dot-dot 的长度 lane。
        let raw = unsafe { &second.m_u.raw };
        assert_eq!(u64::from_ne_bytes(raw[0..8].try_into().unwrap()), 3);
    }

    /// A one-byte buffer is EINVAL without a single wire call.
    #[test]
    fn test_getcwd_wire_tiny_buffer_sends_nothing() {
        let transport = CannedTransport::new();
        let mut buffer = [0u8; 1];
        assert_eq!(
            getcwd_via(&transport, &mut buffer).unwrap_err(),
            Errno::EINVAL
        );
        assert!(transport.sent.borrow().is_empty());
    }

    /// The record parser walks two zero-padded records and stops at a
    /// truncated tail, and the encoder-side layout arithmetic matches the
    /// client-side parse (`minix3/sys/sys/dirent.h:60-67`).
    #[test]
    fn test_parse_record_walks_padded_records() {
        let mut listing = [0u8; 64];
        let first = encode_record(&mut listing[..], 20, b"ab");
        // (13 + 2 + 1 + 7) & !7 == 16.
        assert_eq!(first, 16);
        let second = encode_record(&mut listing[first..], 40, b"abcde");
        // (13 + 5 + 1 + 7) & !7 == 24.
        assert_eq!(second, 24);
        let (len, (inode, name)) = parse_record(&listing).expect("first record");
        assert_eq!(len, 16);
        assert_eq!(inode, 20);
        assert_eq!(name, b"ab");
        let (len, (inode, name)) =
            parse_record(&listing[first..first + second]).expect("second record");
        assert_eq!(len, 24);
        assert_eq!(inode, 40);
        assert_eq!(name, b"abcde");
        // A record length running past the slice is rejected.
        assert!(parse_record(&listing[first..first + 3]).is_none());
    }
}
