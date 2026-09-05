//! Path resolution: credential checks, component parsing, and the lookup walk.
//!
//! C correspondence: `minix3/minix/lib/libfsdriver/lookup.c` (the `LOOKUP`
//! adapter with its `access_as_dir`, `next_name`, and `resolve_link`
//! helpers). The virtual file system service sends one `Lookup` request per
//! whole path; the framework walks the path one component at a time by
//! calling back into the server through
//! [`FsDriver::lookup_child`](crate::driver::FsDriver::lookup_child).
//!
//! Three redirection outcomes escape the walk instead of resolving to a
//! node: entering a mount point, leaving through the file system root, and
//! meeting an absolute symbolic link. They are values of [`LookupOutcome`],
//! not errors, because the virtual file system service continues the lookup
//! elsewhere on each of them.

use minix_types::{EACCES, EINVAL, ELOOP, ENAMETOOLONG, ENOTDIR, Errno};

use alloc::boxed::Box;

use crate::data::NAME_MAX;
use crate::driver::FsDriver;
use crate::protocol::{FileNode, LookupFlags, LookupRedirect};

/// Longest path accepted for resolution.
///
/// C: `PATH_MAX` (`minix3/sys/sys/syslimits.h:64`, value 1024). The lookup
/// buffer is exactly this large (`lookup.c:123`).
pub const PATH_MAX: usize = 1024;

/// How many symbolic links the walk follows before giving up.
///
/// C: `_POSIX_SYMLOOP_MAX` (`minix3/include/limits.h:61`, value 8),
/// enforced in `lookup.c:256-260`.
pub const MAX_SYMLINK_DEPTH: u32 = 8;

/// Identifier of the superuser, who may search any directory.
/// C: `ROOT_UID` (`minix3/minix/lib/libfsdriver/fsdriver.h:8`, value 0).
pub const SUPERUSER_ID: u32 = 0;

/// Caller credentials used for directory search checks.
///
/// C: `vfs_ucred_t` (`minix3/minix/include/minix/vfsif.h:33-38`): a user
/// identifier, a group identifier, and a supplementary group list. Requests
/// without the credentials flag carry only the first two; the list is then
/// empty (`lookup.c:158-162`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credentials {
    /// Calling user identifier.
    pub user: u32,
    /// Calling group identifier.
    pub group: u32,
    /// Supplementary groups (empty for plain requests).
    pub extra_groups: [u32; 16],
    /// How many entries of `extra_groups` are valid.
    pub extra_group_count: usize,
}

impl Credentials {
    /// Credentials carrying only a user and a group identifier.
    pub const fn plain(user: u32, group: u32) -> Self {
        Self {
            user,
            group,
            extra_groups: [0; 16],
            extra_group_count: 0,
        }
    }

    /// Whether the caller belongs to `group`, directly or through the
    /// supplementary list.
    pub fn is_member(&self, group: u32) -> bool {
        if self.group == group {
            return true;
        }
        self.extra_groups[..self.extra_group_count.min(self.extra_groups.len())].contains(&group)
    }
}

/// Check that a node may be searched as a directory by this caller.
///
/// C: `access_as_dir` (`lookup.c:8-37`). The rules, in order:
/// 1. The node must be a directory, or the answer is "not a directory".
/// 2. The superuser may search anything.
/// 3. Otherwise the caller needs the search (execute) bit for its class:
///    owner, group (direct or supplementary), or other.
pub fn check_searchable(
    mode: u32,
    owner: u32,
    group: u32,
    credentials: &Credentials,
    is_directory: bool,
) -> Result<(), Errno> {
    if !is_directory {
        return Err(Errno::from_i32(ENOTDIR));
    }
    if credentials.user == SUPERUSER_ID {
        return Ok(());
    }
    const OWNER_SEARCH: u32 = 0o100;
    const GROUP_SEARCH: u32 = 0o010;
    const OTHER_SEARCH: u32 = 0o001;
    let needed = if credentials.user == owner {
        OWNER_SEARCH
    } else if credentials.is_member(group) {
        GROUP_SEARCH
    } else {
        OTHER_SEARCH
    };
    if mode & needed != 0 {
        Ok(())
    } else {
        Err(Errno::from_i32(EACCES))
    }
}

/// Split the next component off a path.
///
/// C: `next_name` (`lookup.c:43-76`). Leading slashes are skipped; the
/// component runs to the next slash or the terminator. An empty remainder
/// yields the current-directory marker `"."`. Overlong components are
/// refused. Returns the component plus the rest of the path after it.
///
/// The returned rest starts at the slash (or terminator) that ended the
/// component, matching the C pointer behavior where `*ptr` inspects the
/// separator.
pub fn next_component(path: &str) -> Result<(&str, &str), Errno> {
    let trimmed = path.trim_start_matches('/');
    if trimmed.is_empty() {
        return Ok((".", ""));
    }
    let end = trimmed.find('/').unwrap_or(trimmed.len());
    let (component, rest) = trimmed.split_at(end);
    if component.len() > NAME_MAX {
        return Err(Errno::from_i32(ENAMETOOLONG));
    }
    Ok((component, rest))
}

/// Outcome of resolving a whole path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LookupOutcome {
    /// Resolved to a node, which stays referenced for the caller.
    Found(FileNode),
    /// The resolved node is a mount point: the virtual file system service
    /// continues in the mounted file system. Carries the mount point inode.
    EnterMount {
        /// Inode number of the mount point.
        inode: u64,
        /// Path offset where resolution stopped.
        offset: usize,
        /// Links resolved so far (for loop detection upstream).
        links_resolved: u32,
    },
    /// Resolution tried to leave through the file system root. Carries the
    /// path offset where it stopped.
    LeaveMount {
        /// Path offset where resolution stopped.
        offset: usize,
        /// Links resolved so far.
        links_resolved: u32,
    },
    /// Resolution met an absolute symbolic link: the virtual file system
    /// service restarts from its own root with the rewritten path. The path
    /// lives behind a box so the common `Found` case stays small.
    AbsoluteSymlink {
        /// Rewritten path (link target plus the unresolved tail).
        path: Box<[u8; PATH_MAX]>,
        /// Length of the rewritten path in use.
        path_length: usize,
        /// Path offset where resolution stopped.
        offset: usize,
        /// Links resolved so far.
        links_resolved: u32,
    },
}

impl LookupOutcome {
    /// The wire code for a redirection outcome, or success for `Found`.
    pub const fn to_errno(&self) -> Errno {
        match self {
            Self::Found(_) => Errno::from_i32(0),
            Self::EnterMount { .. } => LookupRedirect::EnterMount.to_errno(),
            Self::LeaveMount { .. } => LookupRedirect::LeaveMount.to_errno(),
            Self::AbsoluteSymlink { .. } => LookupRedirect::AbsoluteSymlink.to_errno(),
        }
    }
}

/// Input of one whole-path resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LookupInput<'a> {
    /// Directory where resolution starts.
    pub start_directory: u64,
    /// Process root: moving above it stays put (`lookup.c:216-217`).
    pub root_inode: u64,
    /// Root inode number of this file system (leaving through it escapes).
    pub filesystem_root: u64,
    /// Path to resolve.
    pub path: &'a str,
    /// Lookup control flags.
    pub flags: LookupFlags,
    /// Caller credentials.
    pub credentials: Credentials,
}

/// Resolve a whole path one component at a time.
///
/// This is the framework half of `fsdriver_lookup` (`lookup.c:117-333`): the
/// walk logic, the permission checks, the mount and link redirections, and
/// the reference discipline (each step releases the previous node through
/// `put_node`). The server half — single-component resolution and link
/// reading — runs through the [`FsDriver`] methods.
///
/// Reference discipline note: the starting node is acquired first; every
/// successful step releases the previous node and keeps the next. On any
/// failure the current node is released, so the server never leaks a
/// reference (`lookup.c:280-281`, `lookup.c:329-330`).
pub fn resolve_path<D: FsDriver>(
    driver: &mut D,
    input: &LookupInput<'_>,
) -> Result<LookupOutcome, Errno> {
    if input.path.len() + 1 > PATH_MAX {
        return Err(Errno::from_i32(ENAMETOOLONG));
    }

    // Acquire the starting node: resolving "." through the server also
    // reports whether the start itself is a mount point (lookup.c:167-169).
    let (mut current, mut is_mount_point) = driver.lookup_child(input.start_directory, ".")?;

    let mut links_resolved: u32 = 0;
    let mut working_path = [0u8; PATH_MAX];
    working_path[..input.path.len()].copy_from_slice(input.path.as_bytes());
    let mut working_length = input.path.len();
    let mut position = 0usize;

    loop {
        // Phase one: read-only inspection of the remaining path. The parsed
        // component and tail borrow `working_path`; both borrows end before
        // phase two mutates the buffer (the borrow checker enforces the
        // phase split: nothing borrowed here is used after a mutation).
        enum Action {
            Done,
            CurrentDir {
                /// Path offset just past the current-directory component.
                next_position: usize,
            },
            MountViolation,
            SearchFailed(Errno),
            Step {
                child_name_end: usize,
                child_name_start: usize,
                moving_up: bool,
                at_end: bool,
                tail_start: usize,
                component_start: usize,
            },
        }
        let action = {
            let remaining = core::str::from_utf8(&working_path[position..working_length])
                .map_err(|_| Errno::from_i32(EINVAL))?;
            if remaining.is_empty() || remaining == "/" {
                Action::Done
            } else {
                let (component, rest) = next_component(remaining)?;
                let consumed = remaining.len() - rest.len();
                let component_start = position + consumed - component.len();
                let tail_start = position + consumed;
                if is_mount_point {
                    // Starting from a mount point, the next component must
                    // move up; anything else violates the protocol
                    // (lookup.c:182-192).
                    if component != ".." {
                        Action::MountViolation
                    } else {
                        Action::Step {
                            child_name_end: component_start + component.len(),
                            child_name_start: component_start,
                            moving_up: true,
                            at_end: rest.is_empty() || rest == "/",
                            tail_start,
                            component_start,
                        }
                    }
                } else if let Err(error) = check_searchable(
                    current.mode,
                    current.owner,
                    current.group,
                    &input.credentials,
                    is_directory_mode(current.mode),
                ) {
                    Action::SearchFailed(error)
                } else if component == "." {
                    Action::CurrentDir {
                        next_position: tail_start,
                    }
                } else {
                    Action::Step {
                        child_name_end: component_start + component.len(),
                        child_name_start: component_start,
                        moving_up: component == "..",
                        at_end: rest.is_empty() || rest == "/",
                        tail_start,
                        component_start,
                    }
                }
            }
        };

        // Phase two: act on the parsed step. String views are re-derived
        // from the recorded offsets for immediate use only.
        match action {
            Action::Done => break,
            Action::CurrentDir { next_position } => {
                position = next_position;
                continue;
            }
            Action::MountViolation => {
                driver.put_node(current.inode_number, 1).ok();
                return Err(Errno::from_i32(EINVAL));
            }
            Action::SearchFailed(error) => {
                // The current node is released with the loop exit handling
                // (lookup.c:198-200, lookup.c:329-330).
                driver.put_node(current.inode_number, 1).ok();
                return Err(error);
            }
            Action::Step {
                child_name_end,
                child_name_start,
                moving_up,
                at_end,
                tail_start,
                component_start,
            } => {
                position = tail_start;
                if moving_up {
                    // The process root is its own parent (lookup.c:214-217).
                    if current.inode_number == input.root_inode {
                        continue;
                    }
                    // Leaving through the file system root escapes to the
                    // mounting file system (lookup.c:224-229).
                    if current.inode_number == input.filesystem_root {
                        driver.put_node(current.inode_number, 1).ok();
                        return Ok(LookupOutcome::LeaveMount {
                            offset: component_start,
                            links_resolved,
                        });
                    }
                }
                let child_name =
                    core::str::from_utf8(&working_path[child_name_start..child_name_end])
                        .map_err(|_| Errno::from_i32(EINVAL))?;
                let (next, next_is_mount) =
                    match driver.lookup_child(current.inode_number, child_name) {
                        Ok(found) => found,
                        Err(error) => {
                            driver.put_node(current.inode_number, 1).ok();
                            return Err(error);
                        }
                    };

                // A parent must always be a directory (lookup.c:243-244).
                // The C code treats a violation as an internal panic; here
                // it is an invalid input error so the server survives a
                // confused implementation.
                if moving_up && !is_directory_mode(next.mode) {
                    driver.put_node(current.inode_number, 1).ok();
                    driver.put_node(next.inode_number, 1).ok();
                    return Err(Errno::from_i32(EINVAL));
                }

                let is_link = is_symlink_mode(next.mode);
                if is_link && (!at_end || !input.flags.return_symlink()) {
                    links_resolved += 1;
                    if links_resolved >= MAX_SYMLINK_DEPTH {
                        driver.put_node(current.inode_number, 1).ok();
                        driver.put_node(next.inode_number, 1).ok();
                        return Err(Errno::from_i32(ELOOP));
                    }
                    // Copy the unresolved tail aside before rewriting the
                    // buffer; this ends the last buffer borrow.
                    let mut tail = [0u8; PATH_MAX];
                    let tail_length = working_length - tail_start;
                    tail[..tail_length].copy_from_slice(&working_path[tail_start..working_length]);
                    // Read the link target and splice the tail after it
                    // (resolve_link, lookup.c:83-112). The driver delivers
                    // the target through the writer closure; no caller
                    // transport is involved.
                    let mut target = [0u8; PATH_MAX];
                    let mut target_length = 0usize;
                    let mut writer = |chunk: &[u8]| {
                        let end = target_length + chunk.len();
                        if end < PATH_MAX {
                            target[target_length..end].copy_from_slice(chunk);
                            target_length = end;
                        }
                    };
                    let moved = match driver.read_link(next.inode_number, PATH_MAX - 1, &mut writer)
                    {
                        Ok(moved) => moved,
                        Err(error) => {
                            // Mirror the C cleanup: release both the link
                            // node and the current node before reporting
                            // the failure (lookup.c:262-268,
                            // lookup.c:329-330).
                            driver.put_node(next.inode_number, 1).ok();
                            driver.put_node(current.inode_number, 1).ok();
                            return Err(error);
                        }
                    };
                    target_length = moved.min(PATH_MAX - 1);
                    driver.put_node(next.inode_number, 1).ok();
                    if target_length + tail_length >= PATH_MAX {
                        driver.put_node(current.inode_number, 1).ok();
                        return Err(Errno::from_i32(ENAMETOOLONG));
                    }
                    working_path[..target_length].copy_from_slice(&target[..target_length]);
                    working_path[target_length..target_length + tail_length]
                        .copy_from_slice(&tail[..tail_length]);
                    working_length = target_length + tail_length;
                    // An absolute link restarts from the virtual file system
                    // root: hand the rewritten path back (lookup.c:270-274).
                    if target_length > 0 && target[0] == b'/' {
                        let mut path = Box::new([0u8; PATH_MAX]);
                        path[..working_length].copy_from_slice(&working_path[..working_length]);
                        driver.put_node(current.inode_number, 1).ok();
                        return Ok(LookupOutcome::AbsoluteSymlink {
                            path,
                            path_length: working_length,
                            offset: component_start,
                            links_resolved,
                        });
                    }
                    position = 0;
                    continue;
                }

                driver.put_node(current.inode_number, 1).ok();
                current = next;
                is_mount_point = next_is_mount;

                if next_is_mount {
                    return Ok(LookupOutcome::EnterMount {
                        inode: current.inode_number,
                        offset: position,
                        links_resolved,
                    });
                }
            }
        }
    }

    Ok(LookupOutcome::Found(current))
}

/// Whether a mode word describes a directory (Unix file-type bits).
const fn is_directory_mode(mode: u32) -> bool {
    mode & 0o170000 == 0o040000
}

/// Whether a mode word describes a symbolic link.
const fn is_symlink_mode(mode: u32) -> bool {
    mode & 0o170000 == 0o120000
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::FileNode;
    use alloc::collections::BTreeMap;
    use alloc::vec::Vec;

    extern crate alloc;

    /// Tiny tree server: maps (directory, name) to (node, mount flag).
    struct TreeDriver {
        entries: BTreeMap<(u64, Vec<u8>), (FileNode, bool)>,
        links: BTreeMap<u64, Vec<u8>>,
        released: Vec<u64>,
    }

    impl TreeDriver {
        fn file(inode: u64) -> FileNode {
            FileNode::new(inode, 0o100644, 10, 100, 100, 0)
        }

        fn dir(inode: u64) -> FileNode {
            FileNode::new(inode, 0o040755, 0, 0, 0, 0)
        }

        fn link_node(inode: u64) -> FileNode {
            FileNode::new(inode, 0o120777, 0, 0, 0, 0)
        }
    }

    impl FsDriver for TreeDriver {
        fn mount(
            &mut self,
            _device: u64,
            _flags: crate::protocol::MountFlags,
            _capabilities: &mut crate::protocol::CapabilityFlags,
        ) -> Result<FileNode, Errno> {
            Ok(Self::dir(1))
        }

        fn lookup_child(&mut self, directory: u64, name: &str) -> Result<(FileNode, bool), Errno> {
            if name == "." {
                let node = match directory {
                    1 => Self::dir(1),
                    2 => Self::dir(2),
                    _ => Self::file(directory),
                };
                return Ok((node, false));
            }
            self.entries
                .get(&(directory, name.as_bytes().to_vec()))
                .cloned()
                .ok_or(Errno::from_i32(minix_types::ENOENT))
        }

        fn put_node(&mut self, inode: u64, _count: u32) -> Result<(), Errno> {
            self.released.push(inode);
            Ok(())
        }

        fn read_link(
            &mut self,
            inode: u64,
            capacity: usize,
            out: &mut dyn FnMut(&[u8]),
        ) -> Result<usize, Errno> {
            let target = self
                .links
                .get(&inode)
                .cloned()
                .ok_or(Errno::from_i32(minix_types::EINVAL))?;
            let take = target.len().min(capacity);
            out(&target[..take]);
            Ok(take)
        }
    }

    fn input<'a>(path: &'a str) -> LookupInput<'a> {
        LookupInput {
            start_directory: 1,
            root_inode: 1,
            filesystem_root: 1,
            path,
            flags: LookupFlags::EMPTY,
            credentials: Credentials::plain(100, 100),
        }
    }

    #[test]
    fn test_next_component_parsing() {
        assert_eq!(next_component("a/b").unwrap(), ("a", "/b"));
        assert_eq!(next_component("//a//b").unwrap(), ("a", "//b"));
        assert_eq!(next_component("").unwrap(), (".", ""));
        assert_eq!(next_component("/").unwrap(), (".", ""));
        let long = "x".repeat(NAME_MAX + 1);
        assert_eq!(next_component(&long).unwrap_err().to_i32(), ENAMETOOLONG);
    }

    #[test]
    fn test_search_check_rules() {
        let root = Credentials::plain(SUPERUSER_ID, 0);
        assert!(check_searchable(0, 1, 1, &root, true).is_ok());
        // Not a directory.
        assert_eq!(
            check_searchable(0o100644, 100, 100, &root, false)
                .unwrap_err()
                .to_i32(),
            ENOTDIR
        );
        let owner = Credentials::plain(100, 200);
        assert!(check_searchable(0o040700, 100, 200, &owner, true).is_ok());
        assert_eq!(
            check_searchable(0o040070, 100, 300, &owner, true)
                .unwrap_err()
                .to_i32(),
            EACCES
        );
        // Supplementary group grants group search.
        let member = Credentials {
            user: 100,
            group: 200,
            extra_groups: {
                let mut groups = [0u32; 16];
                groups[0] = 300;
                groups
            },
            extra_group_count: 1,
        };
        assert!(check_searchable(0o040050, 1, 300, &member, true).is_ok());
        // Other search bit serves strangers.
        let stranger = Credentials::plain(500, 500);
        assert!(check_searchable(0o040005, 1, 1, &stranger, true).is_ok());
        assert_eq!(
            check_searchable(0o040000, 1, 1, &stranger, true)
                .unwrap_err()
                .to_i32(),
            EACCES
        );
    }

    #[test]
    fn test_resolve_simple_path() {
        let mut driver = TreeDriver {
            entries: BTreeMap::from([
                ((1, b"etc".to_vec()), (TreeDriver::dir(2), false)),
                ((2, b"hosts".to_vec()), (TreeDriver::file(3), false)),
            ]),
            links: BTreeMap::new(),
            released: Vec::new(),
        };
        let outcome = resolve_path(&mut driver, &input("etc/hosts")).unwrap();
        assert_eq!(outcome, LookupOutcome::Found(TreeDriver::file(3)));
        // Every stepped-off node was released; the found node stays held.
        assert!(driver.released.contains(&1));
        assert!(driver.released.contains(&2));
        assert!(!driver.released.contains(&3));
    }

    #[test]
    fn test_resolve_dot_components() {
        let mut driver = TreeDriver {
            entries: BTreeMap::new(),
            links: BTreeMap::new(),
            released: Vec::new(),
        };
        let outcome = resolve_path(&mut driver, &input("./")).unwrap();
        assert_eq!(outcome, LookupOutcome::Found(TreeDriver::dir(1)));
    }

    #[test]
    fn test_resolve_enters_mount_point() {
        let mut driver = TreeDriver {
            entries: BTreeMap::from([((1, b"mnt".to_vec()), (TreeDriver::dir(9), true))]),
            links: BTreeMap::new(),
            released: Vec::new(),
        };
        let outcome = resolve_path(&mut driver, &input("mnt")).unwrap();
        match outcome {
            LookupOutcome::EnterMount { inode, .. } => assert_eq!(inode, 9),
            other => panic!("expected EnterMount, got {other:?}"),
        }
        assert_eq!(outcome.to_errno(), LookupRedirect::EnterMount.to_errno());
    }

    #[test]
    fn test_resolve_relative_symlink_inline() {
        let mut driver = TreeDriver {
            entries: BTreeMap::from([
                ((1, b"link".to_vec()), (TreeDriver::link_node(5), false)),
                ((1, b"target".to_vec()), (TreeDriver::file(6), false)),
            ]),
            links: BTreeMap::from([(5, b"target".to_vec())]),
            released: Vec::new(),
        };
        let outcome = resolve_path(&mut driver, &input("link")).unwrap();
        assert_eq!(outcome, LookupOutcome::Found(TreeDriver::file(6)));
    }

    #[test]
    fn test_resolve_absolute_symlink_redirects() {
        let mut driver = TreeDriver {
            entries: BTreeMap::from([((1, b"link".to_vec()), (TreeDriver::link_node(5), false))]),
            links: BTreeMap::from([(5, b"/etc/hosts".to_vec())]),
            released: Vec::new(),
        };
        let outcome = resolve_path(&mut driver, &input("link")).unwrap();
        match &outcome {
            LookupOutcome::AbsoluteSymlink {
                path, path_length, ..
            } => {
                assert_eq!(&path[..*path_length], b"/etc/hosts");
            }
            other => panic!("expected AbsoluteSymlink, got {other:?}"),
        }
        assert_eq!(
            outcome.to_errno(),
            LookupRedirect::AbsoluteSymlink.to_errno()
        );
    }

    #[test]
    fn test_symlink_loop_is_refused() {
        let mut driver = TreeDriver {
            entries: BTreeMap::from([((1, b"loop".to_vec()), (TreeDriver::link_node(5), false))]),
            links: BTreeMap::from([(5, b"loop".to_vec())]),
            released: Vec::new(),
        };
        assert_eq!(
            resolve_path(&mut driver, &input("loop"))
                .unwrap_err()
                .to_i32(),
            ELOOP
        );
    }

    #[test]
    fn test_return_symlink_flag_keeps_trailing_link() {
        let mut driver = TreeDriver {
            entries: BTreeMap::from([((1, b"link".to_vec()), (TreeDriver::link_node(5), false))]),
            links: BTreeMap::from([(5, b"target".to_vec())]),
            released: Vec::new(),
        };
        let flagged = LookupInput {
            flags: LookupFlags(LookupFlags::RETURN_SYMLINK.0),
            ..input("link")
        };
        let outcome = resolve_path(&mut driver, &flagged).unwrap();
        assert_eq!(outcome, LookupOutcome::Found(TreeDriver::link_node(5)));
    }

    #[test]
    fn test_missing_component_releases_current_node() {
        let mut driver = TreeDriver {
            entries: BTreeMap::new(),
            links: BTreeMap::new(),
            released: Vec::new(),
        };
        assert_eq!(
            resolve_path(&mut driver, &input("nope"))
                .unwrap_err()
                .to_i32(),
            minix_types::ENOENT
        );
        assert!(driver.released.contains(&1));
    }

    #[test]
    fn test_lookup_constants_match_c() {
        assert_eq!(PATH_MAX, 1024);
        assert_eq!(MAX_SYMLINK_DEPTH, 8);
        assert_eq!(SUPERUSER_ID, 0);
    }
}
