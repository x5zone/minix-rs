//! The virtual tree: storage, lookup, listing, and hook dispatch.
//!
//! The tree mirrors `inode.c` plus `inode.h`: a fixed-capacity node table
//! with the root at slot zero, parent and child links forming the tree,
//! two hash tables (by parent and name, by parent and index) accelerating
//! lookup, reference counts tracking open files, and a deletion flag for
//! nodes that are gone from the tree but still open. Node numbers shown to
//! callers are one-based (slot plus one, `inode.c:368-375`); slot zero is
//! the root, whose number is therefore one.

use alloc::vec::Vec;

use minix_types::{EACCES, EINVAL, ENAMETOOLONG, ENOENT, ENOMEM, ENOSYS, ENOTDIR, EIO, Errno};

use super::{NAME_MAX, NO_INDEX};

/// Why a tree operation failed. Each variant maps to the wire code the C
/// functions return at the same decision point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeError {
    /// Bad request (unknown number, corrupt state, overlong internal use).
    Invalid,
    /// Name too long (`ENAMETOOLONG`).
    NameTooLong,
    /// Expected a directory (`ENOTDIR`).
    NotDirectory,
    /// Name or slot not present (`ENOENT`).
    NotFound,
    /// Table full and nothing purgable (`ENOMEM`, via `purge_inode` miss).
    NoSpace,
    /// Missing hook (`ENOSYS`, link and status group).
    NotSupported,
    /// Refused (deleted target, missing write hook, `EACCES`).
    Access,
    /// Position counter overflow (`EIO`, `file.c:204-205`).
    Io,
}

impl TreeError {
    /// The wire error code.
    pub const fn to_errno(self) -> Errno {
        match self {
            Self::Invalid => Errno::from_i32(EINVAL),
            Self::NameTooLong => Errno::from_i32(ENAMETOOLONG),
            Self::NotDirectory => Errno::from_i32(ENOTDIR),
            Self::NotFound => Errno::from_i32(ENOENT),
            Self::NoSpace => Errno::from_i32(ENOMEM),
            Self::NotSupported => Errno::from_i32(ENOSYS),
            Self::Access => Errno::from_i32(EACCES),
            Self::Io => Errno::from_i32(EIO),
        }
    }
}

/// Hook failure: a hook reports its own error code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HookError(pub Errno);

/// File metadata carried per node (`struct inode_stat`, `vtreefs.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NodeStat {
    /// File mode (type plus permission bits).
    pub mode: u16,
    /// Owner user identifier.
    pub uid: u16,
    /// Owner group identifier.
    pub gid: u16,
    /// File size in bytes.
    pub size: i64,
    /// Device number (character and block files).
    pub device: u64,
}

/// Filesystem statistics (`fs_statvfs`, `stadir.c:115-122`): only the
/// truncation flag and the name limit; virtual trees have no blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreeStat {
    /// Whether names truncate (`ST_NOTRUNC` is always set: names never
    /// truncate, overlong names refuse instead).
    pub no_truncate: bool,
    /// Maximum name length.
    pub name_max: usize,
}

/// One listing entry: number, name, and raw file-type byte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    /// One-based node number.
    pub number: u64,
    /// Entry name.
    pub name: Vec<u8>,
    /// File-type byte (same encoding the directory encoder consumes).
    pub file_type: u8,
}

/// File status in framework-neutral fields (`fs_stat`, `stadir.c:9-43`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStat {
    /// File mode.
    pub mode: u16,
    /// Link count: one while linked, zero once deleted but still open.
    pub nlinks: u16,
    /// Owner user identifier.
    pub owner: u16,
    /// Owner group identifier.
    pub group: u16,
    /// Device number.
    pub device: u64,
    /// File size in bytes (symbolic links report the target length).
    pub size: i64,
}

/// Server hooks (`struct fs_hooks`, `vtreefs.h`). Every hook has a default
/// that reproduces the framework's no-hook behavior, so servers implement
/// only what they serve: mapping a missing hook to "not supported" or to
/// an empty result happens here, once, instead of at every call site.
pub trait FsHooks {
    /// Called after mounting (`init_hook`).
    fn init_hook(&mut self) {}
    /// Called before unmounting (`cleanup_hook`).
    fn cleanup_hook(&mut self) {}
    /// Refresh before resolving a name (`lookup_hook`).
    fn lookup_hook(&mut self, _node: NodeId, _name: &[u8]) -> Result<(), HookError> {
        Ok(())
    }
    /// Refresh before listing a directory (`getdents_hook`).
    fn getdents_hook(&mut self, _node: NodeId) -> Result<(), HookError> {
        Ok(())
    }
    /// Fill the buffer with file bytes (`read_hook`); returns bytes staged.
    fn read_hook(
        &mut self,
        _node: NodeId,
        _buffer: &mut [u8],
        _offset: u64,
    ) -> Result<usize, HookError> {
        Ok(0)
    }
    /// Consume file bytes (`write_hook`); returns bytes accepted.
    fn write_hook(
        &mut self,
        _node: NodeId,
        _data: &[u8],
        _offset: u64,
    ) -> Result<usize, HookError> {
        Err(HookError(Errno::from_i32(minix_types::ENOSYS)))
    }
    /// Truncate a file (`trunc_hook`).
    fn trunc_hook(&mut self, _node: NodeId, _offset: u64) -> Result<(), HookError> {
        Err(HookError(Errno::from_i32(minix_types::ENOSYS)))
    }
    /// Create a device node (`mknod_hook`).
    fn mknod_hook(&mut self, _parent: NodeId, _name: &[u8], _stat: NodeStat) -> Result<(), HookError> {
        Err(HookError(Errno::from_i32(minix_types::ENOSYS)))
    }
    /// Remove a node (`unlink_hook`).
    fn unlink_hook(&mut self, _node: NodeId) -> Result<(), HookError> {
        Err(HookError(Errno::from_i32(minix_types::ENOSYS)))
    }
    /// Create a symbolic link (`slink_hook`).
    fn slink_hook(
        &mut self,
        _parent: NodeId,
        _name: &[u8],
        _stat: NodeStat,
        _target: &[u8],
    ) -> Result<(), HookError> {
        Err(HookError(Errno::from_i32(minix_types::ENOSYS)))
    }
    /// Resolve a symbolic link (`rdlink_hook`); returns target length.
    fn rdlink_hook(&mut self, _node: NodeId, _buffer: &mut [u8]) -> Result<usize, HookError> {
        Err(HookError(Errno::from_i32(minix_types::ENOSYS)))
    }
    /// Apply a status change (`chstat_hook`).
    fn chstat_hook(&mut self, _node: NodeId, _stat: &NodeStat) -> Result<(), HookError> {
        Err(HookError(Errno::from_i32(minix_types::ENOSYS)))
    }
}

/// A node identifier: the slot index. Numbers shown to callers are the
/// slot plus one; the root lives at slot zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub usize);

/// One tree node (`struct inode`, `inode.h:25-49`).
struct Node {
    /// Parent slot, or `None` for the root and for fully unlinked nodes.
    parent: Option<usize>,
    /// Child slots, most-recently added first (`inode.c:236` inserts at
    /// the head, so listings walk newest first).
    children: Vec<usize>,
    /// Name within the parent.
    name: Vec<u8>,
    /// Open reference count.
    references: u32,
    /// Index within the parent, or `NO_INDEX`.
    index: i32,
    /// Indexed slots the directory offers (`i_indexed`).
    indexed: i32,
    /// Callback data (an opaque value the server owns).
    cbdata: u64,
    /// Metadata.
    stat: NodeStat,
    /// Whether the node is scheduled for deletion.
    deleted: bool,
    /// Whether the slot is in use at all.
    live: bool,
    /// Per-node extra bytes (`extra.c`).
    extra: Vec<u8>,
}

/// The virtual tree (`inode.c` state plus the two hash tables).
pub struct Tree {
    /// Node table; slot zero is the root.
    nodes: Vec<Node>,
    /// Unused slots, most-recently freed first.
    free: Vec<usize>,
    /// Name hash buckets over `(parent, name)`.
    by_name: Vec<Vec<usize>>,
    /// Index hash buckets over `(parent, index)`.
    by_index: Vec<Vec<usize>>,
    /// Round-robin cursor for purging (`purge_inode`, `inode.c:155`).
    purge_cursor: usize,
    /// Per-node extra byte count (`extra.c`).
    extra_size: usize,
}

impl Tree {
    /// Build a tree with room for `capacity` nodes (`init_inodes`,
    /// `inode.c:30-99`). The root takes slot zero with the given status
    /// and indexed-slot count; every other slot starts free. Capacity
    /// zero refuses with no space (an unusable tree is not worth
    /// building).
    pub fn new(capacity: usize, root: NodeStat, root_indexed: i32, extra_size: usize) -> Result<Self, TreeError> {
        if capacity == 0 {
            return Err(TreeError::NoSpace);
        }
        let mut nodes = Vec::with_capacity(capacity);
        nodes.push(Node {
            parent: None,
            children: Vec::new(),
            name: Vec::new(),
            references: 0,
            index: NO_INDEX,
            indexed: root_indexed,
            cbdata: 0,
            stat: root,
            deleted: false,
            live: true,
            extra: alloc::vec![0u8; extra_size],
        });
        for _ in 1..capacity {
            nodes.push(Node {
                parent: None,
                children: Vec::new(),
                name: Vec::new(),
                references: 0,
                index: NO_INDEX,
                indexed: 0,
                cbdata: 0,
                stat: NodeStat::default(),
                deleted: false,
                live: false,
                extra: Vec::new(),
            });
        }
        let mut free = Vec::with_capacity(capacity.saturating_sub(1));
        for slot in (1..capacity).rev() {
            free.push(slot);
        }
        let mut by_name = Vec::with_capacity(capacity);
        let mut by_index = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            by_name.push(Vec::new());
            by_index.push(Vec::new());
        }
        Ok(Self {
            nodes,
            free,
            by_name,
            by_index,
            purge_cursor: 0,
            extra_size,
        })
    }

    /// Number of slots (capacity, not live count).
    pub fn capacity(&self) -> usize {
        self.nodes.len()
    }

    /// The root node identifier.
    pub const fn root() -> NodeId {
        NodeId(0)
    }

    /// Caller-visible number of a node (slot plus one, `inode.c:374`).
    pub fn number(&self, node: NodeId) -> Result<u64, TreeError> {
        self.check(node)?;
        Ok(node.0 as u64 + 1)
    }

    /// Find a live node by caller-visible number (`find_inode`).
    pub fn find(&self, number: u64) -> Result<NodeId, TreeError> {
        if number == 0 || number > self.nodes.len() as u64 {
            return Err(TreeError::Invalid);
        }
        let node = NodeId(number as usize - 1);
        if !self.nodes[node.0].live {
            return Err(TreeError::Invalid);
        }
        Ok(node)
    }

    /// Open a node, counting one reference (`get_inode`).
    pub fn open(&mut self, number: u64) -> Result<NodeId, TreeError> {
        let node = self.find(number)?;
        self.nodes[node.0].references += 1;
        Ok(node)
    }

    /// Release one reference; a deleted node with no references left and
    /// no children is reclaimed now (`put_inode`, `inode.c:484-498`).
    pub fn release(&mut self, node: NodeId) -> Result<(), TreeError> {
        self.check(node)?;
        let references = &mut self.nodes[node.0].references;
        if *references == 0 {
            return Err(TreeError::Invalid);
        }
        *references -= 1;
        if self.nodes[node.0].deleted
            && self.nodes[node.0].references == 0
            && self.nodes[node.0].children.is_empty()
        {
            self.reclaim(node);
        }
        Ok(())
    }

    /// Count an extra reference without opening (`ref_inode`).
    pub fn retain(&mut self, node: NodeId) -> Result<(), TreeError> {
        self.check(node)?;
        self.nodes[node.0].references += 1;
        Ok(())
    }

    /// Release a batch counted by the caller (`fs_putnode`,
    /// `inode.c:611-626`): the count includes the reference being
    /// released, so a count of one releases once.
    pub fn release_batch(&mut self, number: u64, count: u32) -> Result<(), TreeError> {
        let node = self.find(number)?;
        if count == 0 || self.nodes[node.0].references < count {
            return Err(TreeError::Invalid);
        }
        self.nodes[node.0].references -= count - 1;
        self.release(node)
    }

    /// Look up a direct child by its index number
    /// (`get_inode_by_index`, without the bounds assert: out-of-range
    /// indexes report absent instead of aborting).
    pub fn child_by_index(&self, parent: NodeId, index: i32) -> Option<NodeId> {
        if self.check(parent).is_err() {
            return None;
        }
        self.by_index(index, parent)
    }

    /// Read a node's metadata (`get_inode_stat`).
    pub fn stat_of(&self, node: NodeId) -> Result<NodeStat, TreeError> {
        self.check(node)?;
        Ok(self.nodes[node.0].stat)
    }

    /// Replace a node's metadata (`set_inode_stat`).
    pub fn set_stat(&mut self, node: NodeId, stat: NodeStat) -> Result<(), TreeError> {
        self.check(node)?;
        self.nodes[node.0].stat = stat;
        Ok(())
    }

    /// Callback data of a node (`get_inode_cbdata`).
    pub fn cbdata(&self, node: NodeId) -> Result<u64, TreeError> {
        self.check(node)?;
        Ok(self.nodes[node.0].cbdata)
    }

    /// Parent of a node, or `None` for the root (`get_parent_inode`).
    pub fn parent(&self, node: NodeId) -> Result<Option<NodeId>, TreeError> {
        self.check(node)?;
        Ok(self.nodes[node.0].parent.map(NodeId))
    }

    /// Whether a node is scheduled for deletion.
    pub fn is_deleted(&self, node: NodeId) -> Result<bool, TreeError> {
        self.check(node)?;
        Ok(self.nodes[node.0].deleted)
    }

    /// Extra bytes of a node (`get_inode_extra`).
    pub fn extra(&self, node: NodeId) -> Result<&[u8], TreeError> {
        self.check(node)?;
        Ok(&self.nodes[node.0].extra)
    }

    /// Add a node below a directory (`add_inode`, `inode.c:184-249`).
    ///
    /// The parent must be a live directory; the name must fit and must be
    /// absent. A free slot is taken, or one is purged first when the table
    /// is full. The child lands at the head of the parent's children and
    /// in both hash tables (the index table only for indexed nodes).
    /// Extra bytes start zeroed (`clear_inode_extra`).
    pub fn add(
        &mut self,
        parent: NodeId,
        name: &[u8],
        index: i32,
        stat: NodeStat,
        indexed: i32,
        cbdata: u64,
    ) -> Result<NodeId, TreeError> {
        self.check(parent)?;
        if self.nodes[parent.0].deleted {
            return Err(TreeError::Invalid);
        }
        if !is_directory(self.nodes[parent.0].stat.mode) {
            return Err(TreeError::Invalid);
        }
        if name.len() > NAME_MAX {
            return Err(TreeError::NameTooLong);
        }
        if self.by_name(name, parent).is_some() {
            return Err(TreeError::Invalid);
        }
        if self.free.is_empty() {
            self.purge(parent)?;
        }
        let slot = self.free.pop().ok_or(TreeError::NoSpace)?;
        {
            let node = &mut self.nodes[slot];
            node.live = true;
            node.parent = Some(parent.0);
            node.name = name.to_vec();
            node.references = 0;
            node.index = index;
            node.indexed = indexed;
            node.cbdata = cbdata;
            node.stat = stat;
            node.deleted = false;
            node.children.clear();
            node.extra = alloc::vec![0u8; self.extra_size];
        }
        self.nodes[parent.0].children.insert(0, slot);
        let bucket = self.name_bucket(parent.0, name);
        self.by_name[bucket].insert(0, slot);
        if index != NO_INDEX {
            let bucket = self.index_bucket(parent.0, index);
            self.by_index[bucket].insert(0, slot);
        }
        Ok(NodeId(slot))
    }

    /// Delete a node (`delete_inode`, `inode.c:543-594`).
    ///
    /// Deletion runs in two phases. Marking removes children recursively,
    /// unhashes the node, and flags it; a non-directory also detaches from
    /// its parent at once, while a directory keeps its parent so callers
    /// can still walk out of it. Reclaiming returns the slot to the free
    /// list once no references and no children remain. Calling delete on
    /// an already-marked node retries the reclaim half, which is why the
    /// function may run several times before the slot actually frees.
    pub fn delete(&mut self, node: NodeId) -> Result<(), TreeError> {
        self.check(node)?;
        if node == Self::root() {
            return Err(TreeError::Invalid);
        }
        if !self.nodes[node.0].deleted {
            let children: Vec<usize> = self.nodes[node.0].children.clone();
            for child in children {
                self.delete(NodeId(child))?;
            }
            self.unhash(node);
            self.nodes[node.0].name.clear();
            self.nodes[node.0].deleted = true;
            if !is_directory(self.nodes[node.0].stat.mode) {
                self.detach(node);
            }
        }
        if self.nodes[node.0].references == 0 && self.nodes[node.0].children.is_empty() {
            if self.nodes[node.0].parent.is_some() {
                self.detach(node);
            }
            self.reclaim(node);
        }
        Ok(())
    }

    /// Resolve one name below a directory (`fs_lookup`, `path.c:8-59`).
    ///
    /// Dot stays, dot-dot moves to the parent (the root's parent is the
    /// root for listing, but lookup past the root reports missing, since
    /// a deleted root is impossible and anything else is corruption).
    /// Other names first run the server's refresh hook, then resolve
    /// through the name table; anything absent reports missing. The child
    /// opens with one extra reference on success.
    pub fn lookup<H: FsHooks>(
        &mut self,
        hooks: &mut H,
        dir: NodeId,
        name: &[u8],
    ) -> Result<NodeId, TreeError> {
        self.check(dir)?;
        if !is_directory(self.nodes[dir.0].stat.mode) {
            return Err(TreeError::NotDirectory);
        }
        if name.len() > NAME_MAX {
            return Err(TreeError::NameTooLong);
        }
        if name == b"." {
            self.nodes[dir.0].references += 1;
            return Ok(dir);
        }
        if name == b".." {
            let parent = self.nodes[dir.0].parent.map(NodeId).ok_or(TreeError::NotFound)?;
            self.nodes[parent.0].references += 1;
            return Ok(parent);
        }
        if !self.nodes[dir.0].deleted {
            hooks
                .lookup_hook(dir, name)
                .map_err(|error| errno_to_tree(error.0))?;
        }
        let child = self.by_name(name, dir).ok_or(TreeError::NotFound)?;
        self.nodes[child.0].references += 1;
        Ok(child)
    }

    /// List directory entries from a position (`fs_getdents`,
    /// `file.c:194-295`, without storage).
    ///
    /// Positions count every slot the walk considers, including indexed
    /// gaps that produce no output: dot is position zero, dot-dot is
    /// position one, indexed slots follow in index order, then plain
    /// children in table order (indexed children skipped, since they
    /// already had their slots). A full caller buffer stops the walk with
    /// the position rewound to the blocking entry; otherwise the position
    /// runs past the last child. The server's refresh hook runs first on
    /// live directories.
    pub fn list<H: FsHooks>(
        &mut self,
        hooks: &mut H,
        dir: NodeId,
        position: &mut u64,
        capacity: usize,
        out: &mut dyn FnMut(DirEntry),
    ) -> Result<usize, TreeError> {
        self.check(dir)?;
        if *position == u64::MAX {
            return Err(TreeError::Io);
        }
        if !self.nodes[dir.0].deleted {
            hooks
                .getdents_hook(dir)
                .map_err(|error| errno_to_tree(error.0))?;
        }
        let indexed = self.nodes[dir.0].indexed.max(0) as u64;
        let mut emitted = 0usize;
        loop {
            let pos = *position;
            *position += 1;
            let entry = self.entry_at(dir, pos, indexed)?;
            match entry {
                None => {
                    if pos < 2 + indexed {
                        continue;
                    }
                    break;
                }
                Some(entry) => {
                    if emitted >= capacity {
                        *position = pos;
                        break;
                    }
                    out(entry);
                    emitted += 1;
                }
            }
            if *position == u64::MAX {
                break;
            }
        }
        Ok(emitted)
    }

    /// Read a regular file through the server's hook (`fs_read`,
    /// `file.c:45-103`, without storage).
    ///
    /// Unknown numbers and non-files refuse; deleted files and missing
    /// hooks behave like empty files. Otherwise the hook fills the
    /// caller's buffer in staging-sized chunks until the request is done
    /// or a short chunk ends the stream. A hook error after some output
    /// reports the partial count; before any output it reports the error.
    pub fn read<H: FsHooks>(
        &mut self,
        hooks: &mut H,
        number: u64,
        buffer: &mut [u8],
        offset: u64,
        staging: usize,
    ) -> Result<usize, TreeError> {
        let node = self.find(number)?;
        if !is_regular(self.nodes[node.0].stat.mode) {
            return Err(TreeError::Invalid);
        }
        if self.nodes[node.0].deleted || staging == 0 {
            return Ok(0);
        }
        let mut moved = 0usize;
        let mut position = offset;
        let mut staging_buf = alloc::vec![0u8; staging];
        while moved < buffer.len() {
            let want = (buffer.len() - moved).min(staging);
            let got = hooks
                .read_hook(node, &mut staging_buf[..want], position)
                .map_err(|error| errno_to_tree(error.0))?;
            if got == 0 {
                break;
            }
            let got = got.min(want);
            buffer[moved..moved + got].copy_from_slice(&staging_buf[..got]);
            moved += got;
            position += got as u64;
            if got < staging {
                break;
            }
        }
        Ok(moved)
    }

    /// Report file status (`fs_stat`, `stadir.c:9-43`).
    ///
    /// Link count is one while linked and zero once deleted but still
    /// open. Symbolic links report their target length through the
    /// server's hook; when the hook is missing the stored size stands.
    /// All three times are the caller's clock: virtual trees keep no
    /// timestamps of their own.
    pub fn file_stat<H: FsHooks>(
        &mut self,
        hooks: &mut H,
        number: u64,
        now: i64,
    ) -> Result<(FileStat, i64, i64, i64), TreeError> {
        let node = self.find(number)?;
        let stat = self.nodes[node.0].stat;
        let mut size = stat.size;
        if is_symlink(stat.mode) {
            let mut target = alloc::vec![0u8; 256];
            if let Ok(length) = hooks.rdlink_hook(node, &mut target) {
                size = length as i64;
            }
        }
        Ok((
            FileStat {
                mode: stat.mode,
                nlinks: u16::from(!self.nodes[node.0].deleted),
                owner: stat.uid,
                group: stat.gid,
                device: stat.device,
                size,
            },
            now,
            now,
            now,
        ))
    }

    /// Filesystem statistics (`fs_statvfs`): names never truncate.
    pub const fn tree_stat() -> TreeStat {
        TreeStat {
            no_truncate: true,
            name_max: NAME_MAX,
        }
    }

    /// Entry the walk considers at a position, or `None` for gaps and the
    /// end of the table.
    fn entry_at(&self, dir: NodeId, pos: u64, indexed: u64) -> Result<Option<DirEntry>, TreeError> {
        if pos == 0 {
            return Ok(Some(self.describe(dir)?));
        }
        if pos == 1 {
            let parent = self.nodes[dir.0].parent.map(NodeId).unwrap_or(dir);
            return Ok(Some(self.describe(parent)?));
        }
        if pos - 2 < indexed {
            let index = (pos - 2) as i32;
            return Ok(self.by_index(index, dir).and_then(|child| self.describe(child).ok()));
        }
        let skip = pos - 2 - indexed;
        let mut seen = 0u64;
        for child in &self.nodes[dir.0].children.clone() {
            let node = NodeId(*child);
            if self.nodes[node.0].deleted || self.nodes[node.0].index != NO_INDEX {
                continue;
            }
            if seen == skip {
                return Ok(Some(self.describe(node)?));
            }
            seen += 1;
        }
        Ok(None)
    }

    /// Describe one node for listings.
    fn describe(&self, node: NodeId) -> Result<DirEntry, TreeError> {
        self.check(node)?;
        let inode = &self.nodes[node.0];
        let name = if node == Self::root() {
            alloc::vec![b'.']
        } else {
            inode.name.clone()
        };
        Ok(DirEntry {
            number: node.0 as u64 + 1,
            name,
            file_type: file_type_byte(inode.stat.mode),
        })
    }

    /// Check a node identifier names a live slot.
    fn check(&self, node: NodeId) -> Result<(), TreeError> {
        if node.0 < self.nodes.len() && self.nodes[node.0].live {
            Ok(())
        } else {
            Err(TreeError::Invalid)
        }
    }

    /// Hash bucket for a `(parent, name)` pair (`parent_name_hash`).
    fn name_bucket(&self, parent: usize, name: &[u8]) -> usize {
        let capacity = self.nodes.len();
        (parent ^ sdbm_hash(name) as usize) % capacity
    }

    /// Hash bucket for a `(parent, index)` pair (`parent_index_hash`).
    fn index_bucket(&self, parent: usize, index: i32) -> usize {
        let capacity = self.nodes.len();
        (parent ^ index as usize) % capacity
    }

    /// Look up a child by name through its bucket.
    fn by_name(&self, name: &[u8], parent: NodeId) -> Option<NodeId> {
        let bucket = self.name_bucket(parent.0, name);
        self.by_name[bucket].iter().copied().map(NodeId).find(|node| {
            let inode = &self.nodes[node.0];
            inode.live && !inode.deleted && inode.parent == Some(parent.0) && inode.name == name
        })
    }

    /// Look up a child by index through its bucket.
    fn by_index(&self, index: i32, parent: NodeId) -> Option<NodeId> {
        if index < 0 || index >= self.nodes[parent.0].indexed {
            return None;
        }
        let bucket = self.index_bucket(parent.0, index);
        self.by_index[bucket].iter().copied().map(NodeId).find(|node| {
            let inode = &self.nodes[node.0];
            inode.live && !inode.deleted && inode.parent == Some(parent.0) && inode.index == index
        })
    }

    /// Remove a node from both hash tables (`delete_inode` unhashing).
    fn unhash(&mut self, node: NodeId) {
        let inode = &self.nodes[node.0];
        if let Some(parent) = inode.parent {
            let bucket = self.name_bucket(parent, &inode.name.clone());
            self.by_name[bucket].retain(|slot| *slot != node.0);
            if inode.index != NO_INDEX {
                let bucket = self.index_bucket(parent, inode.index);
                self.by_index[bucket].retain(|slot| *slot != node.0);
            }
        }
    }

    /// Detach a node from its parent, retrying the parent's reclaim
    /// (`unlink_inode`, `inode.c:515-534`).
    fn detach(&mut self, node: NodeId) {
        let parent = self.nodes[node.0].parent.take();
        if let Some(parent) = parent {
            self.nodes[parent].children.retain(|slot| *slot != node.0);
            if self.nodes[parent].deleted {
                let grand = self.nodes[parent].children.is_empty()
                    && self.nodes[parent].references == 0;
                if grand {
                    self.reclaim(NodeId(parent));
                }
            }
        }
    }

    /// Return a slot to the free list.
    fn reclaim(&mut self, node: NodeId) {
        self.nodes[node.0].live = false;
        self.nodes[node.0].deleted = false;
        self.nodes[node.0].children.clear();
        self.nodes[node.0].name.clear();
        self.free.push(node.0);
    }

    /// Make room by deleting a purgable node (`purge_inode`,
    /// `inode.c:142-179`): indexed, unreferenced, childless, and not the
    /// given parent, scanned round-robin. Nothing purgable reports no
    /// space instead of looping forever.
    fn purge(&mut self, parent: NodeId) -> Result<(), TreeError> {
        let capacity = self.nodes.len();
        for _ in 0..capacity {
            let slot = self.purge_cursor;
            self.purge_cursor = (self.purge_cursor + 1) % capacity;
            if slot == parent.0 {
                continue;
            }
            let purgable = self.nodes[slot].live
                && !self.nodes[slot].deleted
                && self.nodes[slot].index != NO_INDEX
                && self.nodes[slot].references == 0
                && self.nodes[slot].children.is_empty();
            if purgable {
                self.delete(NodeId(slot))?;
                if !self.free.is_empty() {
                    return Ok(());
                }
            }
        }
        Err(TreeError::NoSpace)
    }
}

/// Hash a name with the sdbm polynomial (`sdbm_hash`, `sdbm.c:21-30`):
/// each byte folds in as `byte + (value shifted six) + (value shifted
/// sixteen) minus value`, overflows wrapping.
pub fn sdbm_hash(name: &[u8]) -> u32 {
    let mut value: u32 = 0;
    for byte in name {
        value = (*byte as u32)
            .wrapping_add(value << 6)
            .wrapping_add(value << 16)
            .wrapping_sub(value);
    }
    value
}

/// Whether a mode describes a directory (tested against the shared type
/// bits, so servers and trees agree on the encoding).
fn is_directory(mode: u16) -> bool {
    mode as u32 & 0o170000 == 0o040000
}

/// Whether a mode describes a regular file.
fn is_regular(mode: u16) -> bool {
    mode as u32 & 0o170000 == 0o100000
}

/// Whether a mode describes a symbolic link.
fn is_symlink(mode: u16) -> bool {
    mode as u32 & 0o170000 == 0o120000
}

/// File-type byte from a mode, using the shared directory-type encoding
/// (directory four, regular eight, link ten, matching the C `IFTODT`
/// mapping the readers already use).
fn file_type_byte(mode: u16) -> u8 {
    (((mode as u32) & 0o170000) >> 12) as u8
}

/// Map a hook error code onto the closest tree error.
fn errno_to_tree(code: Errno) -> TreeError {
    let raw = code.to_i32();
    if raw == EINVAL {
        TreeError::Invalid
    } else if raw == ENOENT {
        TreeError::NotFound
    } else if raw == ENAMETOOLONG {
        TreeError::NameTooLong
    } else if raw == ENOTDIR {
        TreeError::NotDirectory
    } else if raw == ENOMEM {
        TreeError::NoSpace
    } else if raw == ENOSYS {
        TreeError::NotSupported
    } else if raw == EACCES {
        TreeError::Access
    } else {
        TreeError::Io
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    extern crate alloc;

    struct ClosedHooks;

    impl FsHooks for ClosedHooks {}

    fn dir_stat() -> NodeStat {
        NodeStat {
            mode: 0o040555,
            uid: 0,
            gid: 0,
            size: 0,
            device: 0,
        }
    }

    fn file_stat() -> NodeStat {
        NodeStat {
            mode: 0o100444,
            uid: 0,
            gid: 0,
            size: 0,
            device: 0,
        }
    }

    fn tree() -> Tree {
        Tree::new(16, dir_stat(), 4, 0).unwrap()
    }

    #[test]
    fn test_root_number_is_one() {
        let tree = tree();
        assert_eq!(tree.number(Tree::root()).unwrap(), 1);
        assert_eq!(tree.find(1).unwrap(), Tree::root());
        assert_eq!(tree.find(0).unwrap_err(), TreeError::Invalid);
    }

    #[test]
    fn test_add_and_lookup_round_trip() {
        let mut tree = tree();
        let mut hooks = ClosedHooks;
        let child = tree.add(Tree::root(), b"proc", 0, dir_stat(), 2, 7).unwrap();
        assert_eq!(tree.number(child).unwrap(), 2);
        let found = tree.lookup(&mut hooks, Tree::root(), b"proc").unwrap();
        assert_eq!(found, child);
        assert_eq!(tree.cbdata(found).unwrap(), 7);
        // Lookup granted one reference; releasing once returns to zero.
        tree.release(found).unwrap();
        assert_eq!(tree.find(tree.number(child).unwrap()).unwrap(), child);
    }

    #[test]
    fn test_dot_and_dotdot() {
        let mut tree = tree();
        let mut hooks = ClosedHooks;
        let child = tree.add(Tree::root(), b"sub", NO_INDEX, dir_stat(), 0, 0).unwrap();
        assert_eq!(tree.lookup(&mut hooks, child, b".").unwrap(), child);
        assert_eq!(tree.lookup(&mut hooks, child, b"..").unwrap(), Tree::root());
        assert_eq!(
            tree.lookup(&mut hooks, Tree::root(), b"..").unwrap_err(),
            TreeError::NotFound
        );
        assert_eq!(
            tree.lookup(&mut hooks, Tree::root(), &[b'a'; 61]).unwrap_err(),
            TreeError::NameTooLong
        );
    }

    #[test]
    fn test_missing_name_reports_missing() {
        let mut tree = tree();
        let mut hooks = ClosedHooks;
        assert_eq!(
            tree.lookup(&mut hooks, Tree::root(), b"nope").unwrap_err(),
            TreeError::NotFound
        );
    }

    #[test]
    fn test_delete_detaches_and_frees() {
        let mut tree = tree();
        let child = tree.add(Tree::root(), b"gone", NO_INDEX, file_stat(), 0, 0).unwrap();
        tree.delete(child).unwrap();
        let mut hooks = ClosedHooks;
        assert_eq!(
            tree.lookup(&mut hooks, Tree::root(), b"gone").unwrap_err(),
            TreeError::NotFound
        );
        // The slot returns to the free list and is reused.
        let again = tree.add(Tree::root(), b"back", NO_INDEX, file_stat(), 0, 0).unwrap();
        assert_eq!(again, child);
    }

    #[test]
    fn test_delete_keeps_open_node_until_release() {
        let mut tree = tree();
        let child = tree.add(Tree::root(), b"open", NO_INDEX, file_stat(), 0, 0).unwrap();
        let number = tree.number(child).unwrap();
        let opened = tree.open(number).unwrap();
        tree.delete(child).unwrap();
        // Still findable while open, link count zero.
        let mut hooks = ClosedHooks;
        let (stat, _, _, _) = tree.file_stat(&mut hooks, number, 0).unwrap();
        assert_eq!(stat.nlinks, 0);
        tree.release(opened).unwrap();
        assert_eq!(tree.find(number).unwrap_err(), TreeError::Invalid);
    }

    #[test]
    fn test_release_batch_counts_like_c() {
        let mut tree = tree();
        let child = tree.add(Tree::root(), b"batch", NO_INDEX, file_stat(), 0, 0).unwrap();
        let number = tree.number(child).unwrap();
        tree.open(number).unwrap();
        tree.open(number).unwrap();
        // Two references held: releasing a batch of two drops to zero
        // (count minus one, then one release, like the C function).
        tree.release_batch(number, 2).unwrap();
        assert_eq!(tree.find(number).unwrap(), child);
    }

    #[test]
    fn test_list_walks_dot_indexed_plain() {
        let mut tree = tree();
        let mut hooks = ClosedHooks;
        tree.add(Tree::root(), b"indexed", 1, file_stat(), 0, 0).unwrap();
        tree.add(Tree::root(), b"plain", NO_INDEX, file_stat(), 0, 0).unwrap();
        let mut position = 0u64;
        let mut names = Vec::new();
        tree.list(&mut hooks, Tree::root(), &mut position, 16, &mut |entry| {
            names.push(entry.name.clone())
        })
        .unwrap();
        // Dot, dot-dot, indexed gap zero skipped silently, indexed one,
        // then the plain child.
        assert_eq!(names.len(), 4);
        assert_eq!(names[2], b"indexed".to_vec());
        assert_eq!(names[3], b"plain".to_vec());
    }

    #[test]
    fn test_read_empty_without_hook() {
        let mut tree = tree();
        let mut hooks = ClosedHooks;
        let child = tree.add(Tree::root(), b"empty", NO_INDEX, file_stat(), 0, 0).unwrap();
        let number = tree.number(child).unwrap();
        let mut buffer = [0u8; 16];
        assert_eq!(tree.read(&mut hooks, number, &mut buffer, 0, 64).unwrap(), 0);
    }

    #[test]
    fn test_sdbm_matches_reference_vector() {
        // Reference: the byte string "proc" folds to a fixed value; the
        // test locks the polynomial, not an implementation detail.
        assert_eq!(sdbm_hash(b""), 0);
        assert_eq!(sdbm_hash(b"proc"), 0x938d56b6);
    }

    #[test]
    fn test_error_codes_match_c() {
        use minix_types::{EACCES, EINVAL, ENAMETOOLONG, ENOENT, ENOMEM, ENOSYS, ENOTDIR, EIO};
        assert_eq!(TreeError::Invalid.to_errno().to_i32(), EINVAL);
        assert_eq!(TreeError::NameTooLong.to_errno().to_i32(), ENAMETOOLONG);
        assert_eq!(TreeError::NotDirectory.to_errno().to_i32(), ENOTDIR);
        assert_eq!(TreeError::NotFound.to_errno().to_i32(), ENOENT);
        assert_eq!(TreeError::NoSpace.to_errno().to_i32(), ENOMEM);
        assert_eq!(TreeError::NotSupported.to_errno().to_i32(), ENOSYS);
        assert_eq!(TreeError::Access.to_errno().to_i32(), EACCES);
        assert_eq!(TreeError::Io.to_errno().to_i32(), EIO);
    }
}
