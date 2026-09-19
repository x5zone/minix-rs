//! Second-level cache: the virtual-memory page cache behind the block pool.
//!
//! C correspondence: the `vmcache` machinery of
//! `minix3/minix/lib/libminixfs/cache.c` — the four call sites
//! (`vm_map_cacheblock`, cache.c:443-451; `vm_set_cacheblock`,
//! cache.c:562-587; `vm_forget_cacheblock`, cache.c:625-634;
//! `vm_clear_cache`, cache.c:807), the page-backed block memory
//! (`lmfs_alloc_block` → `mmap`, cache.c:195-211; `munmap_t` in
//! `freeblock`, cache.c:267-272), the per-buffer flags word
//! (`u32_t lmfs_flags`, `libminixfs.h:23`) and the block's inode tag
//! (`ino_t lmfs_inode` / `u64_t lmfs_inode_offset`, `libminixfs.h:26-30`).
//!
//! The idea in one paragraph: the file-system cache and the virtual-memory
//! page cache hold the *same pages*. When the pool misses, it first asks
//! virtual memory whether it already holds the block — a hit hands back a
//! mapping of that page, so the caller reads it without touching storage and
//! without copying. When a block leaves the pool, the pool hands its page over
//! so a later page fault finds the contents there. Both directions need block
//! memory that virtual memory manages, which is why the pool allocates blocks
//! through this same face instead of the Rust heap.
//!
//! Three pieces live here:
//! - [`BufferFlags`], the flags word C shares between the two sides, with the
//!   transitions the pool performs (`VMMC_DIRTY`, `VMMC_BLOCK_LOCKED`,
//!   `VMMC_EVICTED`).
//! - [`SecondLevelCache`], the face the pool talks to: memory in, memory out,
//!   plus the four wire calls. [`NoSecondLevel`] is the pool with no virtual
//!   memory behind it; [`VmSecondLevel`] is the real one over a
//!   [`VmCacheWire`].
//! - [`VmCacheWire`], the wire itself. The concrete implementation belongs to
//!   the server that owns an IPC channel (the MFS server's `vm_wire` module);
//!   tests implement it over memory that plays the part of virtual memory.

use alloc::vec::Vec;

use minix_types::{
    ENOMEM, ENOSYS, Errno, VMC_NO_INODE, VMMC_BLOCK_LOCKED, VMMC_DIRTY, VMMC_EVICTED,
};

use crate::cache::BlockKey;

/// Memory page size used for the alignment gate and size rounding
/// (`PAGE_SIZE`, `minix3/minix/include/machine/param.h`; the C cache rounds
/// every hand-over with `roundup(block_size, PAGE_SIZE)`, cache.c:445-446).
pub const PAGE_SIZE: usize = 4096;

/// Round a byte count up to a whole number of pages (`roundup`, the form the
/// C call sites use before every hand-over).
pub const fn page_round_up(bytes: usize) -> usize {
    bytes.div_ceil(PAGE_SIZE) * PAGE_SIZE
}

/// Whether a byte count needs no rounding.
pub const fn is_page_multiple(bytes: usize) -> bool {
    bytes != 0 && bytes.is_multiple_of(PAGE_SIZE)
}

/// The inode a cached block belongs to: number plus offset inside the file
/// (`lmfs_inode` / `lmfs_inode_offset`, `libminixfs.h:26-30`).
///
/// `None` in an `Option<BlockTag>` position is C's `VMC_NO_INODE`: a plain
/// disk block with no file behind it (metadata blocks, bitmaps).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockTag {
    /// Inode number.
    pub inode: u64,
    /// Byte offset inside the file.
    pub inode_offset: u64,
}

impl BlockTag {
    /// Tag a block with its inode and file offset.
    pub const fn new(inode: u64, inode_offset: u64) -> Self {
        Self { inode, inode_offset }
    }

    /// The wire value of the inode lane: `VMC_NO_INODE` when absent
    /// (cache.c:366-374 passes the tag straight through; VM treats zero as
    /// "no file", `minix3/minix/servers/vm/mem_cache.c:244`).
    pub const fn inode_or_none(tag: Option<BlockTag>) -> u64 {
        match tag {
            Some(tag) if tag.inode != VMC_NO_INODE => tag.inode,
            _ => VMC_NO_INODE,
        }
    }

    /// The wire value of the offset lane; zero when there is no tag.
    pub const fn offset_or_zero(tag: Option<BlockTag>) -> u64 {
        match tag {
            Some(tag) => tag.inode_offset,
            None => 0,
        }
    }
}

/// The shared flags word of one buffer (`u32_t lmfs_flags`,
/// `libminixfs.h:23`).
///
/// C hands the *address* of this word to virtual memory on every call
/// (`&bp->lmfs_flags`, cache.c:444) so the page cache can report eviction
/// back into it. [`BufferFlags::as_word_mut`] is that address's Rust
/// spelling; the word is the one place dirty state and lock state live.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BufferFlags(u32);

impl BufferFlags {
    /// No bits set.
    pub const fn empty() -> Self {
        Self(0)
    }

    /// The word as virtual memory sees it.
    pub const fn word(self) -> u32 {
        self.0
    }

    /// The word for handing to virtual memory as `u32_t *flags`.
    pub fn as_word_mut(&mut self) -> &mut u32 {
        &mut self.0
    }

    /// Whether the block holds changes not yet on storage (`VMMC_DIRTY`; C:
    /// `lmfs_isclean`, cache.c:174-177, inverted).
    pub const fn is_dirty(self) -> bool {
        self.0 & VMMC_DIRTY != 0
    }

    /// Mark the block as holding unwritten changes (`lmfs_markdirty`,
    /// cache.c:164-167).
    pub fn set_dirty(&mut self) {
        self.0 |= VMMC_DIRTY;
    }

    /// Mark the block as matching storage (`lmfs_markclean`, cache.c:169-172).
    pub fn clear_dirty(&mut self) {
        self.0 &= !VMMC_DIRTY;
    }

    /// Whether the page cache has dropped the block under us
    /// (`VMMC_EVICTED`, tested in `get_block_ino`, cache.c:345).
    pub const fn is_evicted(self) -> bool {
        self.0 & VMMC_EVICTED != 0
    }

    /// Note that the page cache dropped the block; its contents are stale.
    ///
    /// C tests this bit but never sets it — the page cache never writes the
    /// flags word back in this snapshot, so the branch is armed but silent.
    /// Rust keeps the same conduit (`BufferFlags::as_word_mut` travels in the
    /// `flags_ptr` lane) and offers this setter so the eviction path is
    /// testable before the page cache ever uses it.
    pub fn mark_evicted(&mut self) {
        self.0 |= VMMC_EVICTED;
    }

    /// Whether a caller currently pins the block (`VMMC_BLOCK_LOCKED`,
    /// asserted on release, cache.c:358-362).
    pub const fn is_locked(self) -> bool {
        self.0 & VMMC_BLOCK_LOCKED != 0
    }

    /// Pin the block against eviction (first user arrives; cache.c:359,
    /// cache.c:421).
    pub fn lock(&mut self) {
        self.0 |= VMMC_BLOCK_LOCKED;
    }

    /// Unpin the block (last user leaves; cache.c:558-559).
    pub fn unlock(&mut self) {
        self.0 &= !VMMC_BLOCK_LOCKED;
    }
}

/// A page-backed block: memory the second level owns.
///
/// Both C sources of block memory look like this — `mmap` on the
/// file-system side (`lmfs_alloc_block`, cache.c:200-201) and the mapping
/// `vm_map_cacheblock` returns (cache.c:443-447). Page-aligned by
/// construction; `bytes` is the mapping's length, a whole number of pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MappedPage {
    addr: usize,
    bytes: usize,
}

impl MappedPage {
    /// Describe a page mapping the second level has established.
    ///
    /// Callers are wire implementations and test doubles; the pool only
    /// receives these values. `addr` must be page-aligned and `bytes` a page
    /// multiple, because virtual memory refuses anything else (`do_setcache`
    /// rejects unaligned offsets, mem_cache.c:207-210).
    ///
    /// # Panics
    /// When the address or the length is not page-aligned.
    pub fn new(addr: usize, bytes: usize) -> Self {
        assert!(addr.is_multiple_of(PAGE_SIZE), "block memory must be page-aligned");
        assert!(is_page_multiple(bytes), "block memory must span whole pages");
        Self { addr, bytes }
    }

    /// Address of the first byte.
    pub const fn addr(self) -> usize {
        self.addr
    }

    /// Mapping length in bytes (a whole number of pages).
    pub const fn bytes(self) -> usize {
        self.bytes
    }

    /// The first `len` bytes of the mapping.
    ///
    /// # Panics
    /// When `len` exceeds the mapping.
    pub fn as_slice(&self, len: usize) -> &[u8] {
        assert!(len <= self.bytes, "block length exceeds its page mapping");
        // SAFETY: the caller that gave out this mapping keeps it alive at
        // least until the pool frees the block through
        // `SecondLevelCache::free_block`; `addr` is page-aligned and the
        // mapping spans `bytes >= len` readable bytes. The server that owns
        // the address space is single-threaded, so no other writer can
        // invalidate the range during this borrow.
        unsafe { core::slice::from_raw_parts(self.addr as *const u8, len) }
    }

    /// The first `len` bytes of the mapping, writable.
    ///
    /// # Panics
    /// When `len` exceeds the mapping.
    pub fn as_mut_slice(&mut self, len: usize) -> &mut [u8] {
        assert!(len <= self.bytes, "block length exceeds its page mapping");
        // SAFETY: as in `as_slice`; exclusive access follows from the
        // single-threaded server and from the pool's pin count (a block is
        // handed to one caller at a time).
        unsafe { core::slice::from_raw_parts_mut(self.addr as *mut u8, len) }
    }
}

/// A block's bytes and where they live.
///
/// C has two sources of block memory and hides the difference behind a
/// `void *data`; in practice every block comes from `mmap` or from
/// `vm_map_cacheblock`, so C may hand any block to virtual memory. The Rust
/// pool keeps the two cases apart in the type because only page-backed memory
/// *can* be handed over — C relies on the invariant being true, Rust states
/// it.
#[derive(Debug, PartialEq, Eq)]
pub enum BlockMemory {
    /// Heap bytes: the pool has no page cache behind it (the
    /// [`NoSecondLevel`] configuration).
    Owned(Vec<u8>),
    /// Page memory owned by the second level.
    Mapped(MappedPage),
}

impl BlockMemory {
    /// No bytes at all (a free slot).
    pub const fn empty() -> Self {
        Self::Owned(Vec::new())
    }

    /// Zeroed heap memory of `bytes` bytes.
    pub fn zeroed(bytes: usize) -> Self {
        Self::Owned(alloc::vec![0u8; bytes])
    }

    /// Bytes available in this memory (not the block length; a page mapping
    /// is longer than the block it carries).
    pub fn bytes(&self) -> usize {
        match self {
            Self::Owned(data) => data.len(),
            Self::Mapped(page) => page.bytes(),
        }
    }

    /// Whether this memory is page-backed (handable to virtual memory).
    pub const fn is_mapped(&self) -> bool {
        matches!(self, Self::Mapped(_))
    }

    /// The page mapping, when this block is page-backed.
    pub const fn mapped(&self) -> Option<MappedPage> {
        match self {
            Self::Mapped(page) => Some(*page),
            Self::Owned(_) => None,
        }
    }

    /// The first `len` bytes of the block.
    pub fn as_slice(&self, len: usize) -> &[u8] {
        match self {
            Self::Owned(data) => &data[..len],
            Self::Mapped(page) => page.as_slice(len),
        }
    }

    /// The first `len` bytes of the block, writable.
    pub fn as_mut_slice(&mut self, len: usize) -> &mut [u8] {
        match self {
            Self::Owned(data) => &mut data[..len],
            Self::Mapped(page) => page.as_mut_slice(len),
        }
    }
}

/// The four C calls plus the two memory operations, as a wire.
///
/// One method per C function on the file-system side of the boundary; byte
/// arguments are the ones C passes (`dev`, `dev_offset`), not the block
/// numbers the pool thinks in — the conversion `dev_offset = block *
/// block_size` belongs to [`VmSecondLevel`], exactly where libminixfs does
/// it (`dev_off = bp->lmfs_blocknr * fs_block_size`, cache.c:531).
pub trait VmCacheWire {
    /// Map the block's page into this address space (`vm_map_cacheblock`,
    /// `minix3/minix/lib/libsys/vm_cache.c:47-57`).
    ///
    /// `Ok(Some(page))` means virtual memory holds the block and the page is
    /// the block's bytes; `Ok(None)` means it does not (the C `MAP_FAILED`
    /// sentinel, which C's caller treats as a plain miss, cache.c:443-451).
    ///
    /// `flags` is the address channel of the buffer's flags word
    /// (`&bp->lmfs_flags` on the wire as `flags_ptr`); virtual memory may
    /// write eviction state into it.
    fn map_cacheblock(
        &mut self,
        dev: u64,
        dev_offset: u64,
        tag: Option<BlockTag>,
        flags: &mut u32,
        block_size: usize,
    ) -> Result<Option<MappedPage>, Errno>;

    /// Hand the block's page to virtual memory (`vm_set_cacheblock`,
    /// vm_cache.c:59-66). `once` is the `VMSF_ONCE` setflag.
    // The parameter list is the C call's: device, byte offset, inode tag,
    // flags address, block size, setflag. Splitting it into a struct would
    // hide which lane each value travels in.
    #[allow(clippy::too_many_arguments)]
    fn set_cacheblock(
        &mut self,
        page: MappedPage,
        dev: u64,
        dev_offset: u64,
        tag: Option<BlockTag>,
        flags: &mut u32,
        block_size: usize,
        once: bool,
    ) -> Result<(), Errno>;

    /// Drop the block from virtual memory (`vm_forget_cacheblock`,
    /// vm_cache.c:68-75).
    fn forget_cacheblock(&mut self, dev: u64, dev_offset: u64, block_size: usize)
    -> Result<(), Errno>;

    /// Drop every block of a device (`vm_clear_cache`, vm_cache.c:77-88).
    fn clear_cache(&mut self, dev: u64) -> Result<(), Errno>;

    /// Page-backed block memory (`mmap` with `MAP_ANON` in C,
    /// cache.c:200-201).
    fn alloc_pages(&mut self, bytes: usize) -> Result<MappedPage, Errno>;

    /// Give page-backed block memory back (`munmap_t`, cache.c:270).
    fn free_pages(&mut self, page: MappedPage);
}

/// A boxed wire is a wire: the forwarding impl that lets a server hold one
/// concrete second-level type while the transport behind it stays a choice
/// (`Box<dyn VmCacheWire>`) — the server picks its transport at mount time,
/// tests substitute a double without touching the server's types.
impl<W: VmCacheWire + ?Sized> VmCacheWire for alloc::boxed::Box<W> {
    fn map_cacheblock(
        &mut self,
        dev: u64,
        dev_offset: u64,
        tag: Option<BlockTag>,
        flags: &mut u32,
        block_size: usize,
    ) -> Result<Option<MappedPage>, Errno> {
        (**self).map_cacheblock(dev, dev_offset, tag, flags, block_size)
    }

    fn set_cacheblock(
        &mut self,
        page: MappedPage,
        dev: u64,
        dev_offset: u64,
        tag: Option<BlockTag>,
        flags: &mut u32,
        block_size: usize,
        once: bool,
    ) -> Result<(), Errno> {
        (**self).set_cacheblock(page, dev, dev_offset, tag, flags, block_size, once)
    }

    fn forget_cacheblock(
        &mut self,
        dev: u64,
        dev_offset: u64,
        block_size: usize,
    ) -> Result<(), Errno> {
        (**self).forget_cacheblock(dev, dev_offset, block_size)
    }

    fn clear_cache(&mut self, dev: u64) -> Result<(), Errno> {
        (**self).clear_cache(dev)
    }

    fn alloc_pages(&mut self, bytes: usize) -> Result<MappedPage, Errno> {
        (**self).alloc_pages(bytes)
    }

    fn free_pages(&mut self, page: MappedPage) {
        (**self).free_pages(page)
    }
}

/// The face the block pool talks to.
///
/// C's `vmcache` flag decides at run time whether the pool consults virtual
/// memory; the Rust pool always talks to a value of this type, and the
/// implementations decide how much of it is real. Every method's failure
/// policy follows the C call site it mirrors, and none of them can fail the
/// pool's own bookkeeping: a block stays valid in the pool's memory whether or
/// not virtual memory took it.
pub trait SecondLevelCache {
    /// Whether the second level participates (the C `vmcache` flag).
    fn is_enabled(&self) -> bool;

    /// Learn the pool's block size and settle the enable decision
    /// (`lmfs_set_blocksize`, cache.c:1226-1240).
    fn set_block_size(&mut self, block_size: usize);

    /// Memory for one block.
    fn alloc_block(&mut self, block_size: usize) -> Result<BlockMemory, Errno>;

    /// Give a block's memory back.
    fn free_block(&mut self, memory: BlockMemory);

    /// Ask for the block's page; `Ok(None)` is a plain miss.
    fn map_block(
        &mut self,
        key: BlockKey,
        tag: Option<BlockTag>,
        block_size: usize,
        flags: &mut BufferFlags,
    ) -> Result<Option<BlockMemory>, Errno>;

    /// Hand a block to the second level.
    fn set_block(
        &mut self,
        memory: &BlockMemory,
        key: BlockKey,
        tag: Option<BlockTag>,
        block_size: usize,
        once: bool,
        flags: &mut BufferFlags,
    ) -> Result<(), Errno>;

    /// Forget one block (`lmfs_free_block`, cache.c:625-634).
    fn forget_block(&mut self, key: BlockKey, block_size: usize);

    /// Forget every block of a device (`lmfs_invalidate`, cache.c:803-807).
    ///
    /// C calls this even when the cache is switched off — an error may have
    /// switched it off while blocks were still registered — so
    /// implementations must not gate it on [`SecondLevelCache::is_enabled`].
    fn clear_device(&mut self, device: u64);
}

/// The pool without virtual memory behind it: heap blocks, no wire calls.
///
/// This is the C configuration with `vmcache` off (`may_use_vmcache` never
/// called; cache.c:1318-1321); the pool behaves exactly as before the second
/// level existed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NoSecondLevel;

impl SecondLevelCache for NoSecondLevel {
    fn is_enabled(&self) -> bool {
        false
    }

    fn set_block_size(&mut self, _block_size: usize) {}

    fn alloc_block(&mut self, block_size: usize) -> Result<BlockMemory, Errno> {
        Ok(BlockMemory::zeroed(block_size))
    }

    fn free_block(&mut self, _memory: BlockMemory) {}

    fn map_block(
        &mut self,
        _key: BlockKey,
        _tag: Option<BlockTag>,
        _block_size: usize,
        _flags: &mut BufferFlags,
    ) -> Result<Option<BlockMemory>, Errno> {
        Ok(None)
    }

    fn set_block(
        &mut self,
        _memory: &BlockMemory,
        _key: BlockKey,
        _tag: Option<BlockTag>,
        _block_size: usize,
        _once: bool,
        _flags: &mut BufferFlags,
    ) -> Result<(), Errno> {
        Ok(())
    }

    fn forget_block(&mut self, _key: BlockKey, _block_size: usize) {}

    fn clear_device(&mut self, _device: u64) {}
}

/// Why the real second level stopped talking to virtual memory.
///
/// The pool keeps working after every one of these (C keeps a block's data
/// valid too); the server decides whether a fault deserves a log line or a
/// restart, which a library must not decide for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmCacheFault {
    /// The wire does not support the calls (`ENOSYS`): the level switches
    /// itself off for good (C: `vmcache = 0`, cache.c:573-575).
    TransportGone,
    /// Virtual memory had no room for the block (`ENOMEM`): reported and
    /// survived (C prints and carries on, cache.c:576-585).
    OutOfMemory,
    /// The wire refused the call for another reason. C aborts the server
    /// (cache.c:586-588); the library records the fault and keeps the block
    /// valid in its own memory instead.
    Rejected(Errno),
}

/// The real second level: the pool's view of the virtual-memory page cache.
///
/// Holds the enable decision C keeps in two globals (`may_use_vmcache` and
/// `vmcache`, cache.c:61,1238), so a pool built for an unaligned block size or
/// an opted-out file system behaves exactly like [`NoSecondLevel`] — except
/// that [`SecondLevelCache::clear_device`] still goes out, as C requires.
pub struct VmSecondLevel<W: VmCacheWire> {
    wire: W,
    /// C: `may_use_vmcache`, set by the file system's start-up
    /// (`lmfs_may_use_vmcache(1)`, `minix3/minix/fs/mfs/main.c:52`).
    may_use: bool,
    /// C: `vmcache`, derived from `may_use` and the block size.
    enabled: bool,
    /// Last failure, for the server's logs.
    fault: Option<VmCacheFault>,
}

impl<W: VmCacheWire> VmSecondLevel<W> {
    /// A second level that may be used, pending the block size
    /// (`lmfs_may_use_vmcache(1)`).
    pub fn new(wire: W) -> Self {
        Self { wire, may_use: true, enabled: false, fault: None }
    }

    /// A second level the file system opted out of (`lmfs_may_use_vmcache(0)`).
    pub fn opted_out(wire: W) -> Self {
        Self { wire, may_use: false, enabled: false, fault: None }
    }

    /// The wire, for inspection.
    pub fn wire(&self) -> &W {
        &self.wire
    }

    /// The wire, mutably (test doubles and the unmount path use this).
    pub fn wire_mut(&mut self) -> &mut W {
        &mut self.wire
    }

    /// The last failure, if any.
    pub fn fault(&self) -> Option<VmCacheFault> {
        self.fault
    }

    /// Record a wire failure and hand the error back to the caller.
    ///
    /// The three-way policy is C's (`put_block`, cache.c:570-588) with one
    /// deliberate change: the case C answers by aborting the server is
    /// recorded instead. Block data stays valid in the pool's own memory, so
    /// the file system keeps serving correct answers; the fault is visible
    /// through [`VmSecondLevel::fault`] for whoever owns the log.
    fn note_failure(&mut self, error: Errno) -> Errno {
        let code = error.to_i32();
        if code == ENOSYS {
            self.enabled = false;
            self.fault = Some(VmCacheFault::TransportGone);
        } else if code == ENOMEM {
            self.fault = Some(VmCacheFault::OutOfMemory);
        } else {
            self.fault = Some(VmCacheFault::Rejected(error));
        }
        error
    }
}

impl<W: VmCacheWire> core::fmt::Debug for VmSecondLevel<W> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // The wire is opaque (it may be a `dyn` transport); what matters for
        // debugging is the enable decision and the last fault.
        f.debug_struct("VmSecondLevel")
            .field("may_use", &self.may_use)
            .field("enabled", &self.enabled)
            .field("fault", &self.fault)
            .finish_non_exhaustive()
    }
}

impl<W: VmCacheWire> SecondLevelCache for VmSecondLevel<W> {
    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_block_size(&mut self, block_size: usize) {
        // C: `vmcache = may_use_vmcache && !(new_block_size % PAGE_SIZE)`
        // (cache.c:1236-1239): a block that does not fill whole pages cannot
        // be mapped into the page cache, so the second level stays out.
        self.enabled = self.may_use && is_page_multiple(block_size);
    }

    fn alloc_block(&mut self, block_size: usize) -> Result<BlockMemory, Errno> {
        // C always allocates block memory with `mmap` (`lmfs_alloc_block`,
        // cache.c:195-211), whether or not the page cache is switched on:
        // page-backed memory is what makes the hand-over possible later.
        match self.wire.alloc_pages(block_size) {
            Ok(page) => Ok(BlockMemory::Mapped(page)),
            Err(error) => Err(self.note_failure(error)),
        }
    }

    fn free_block(&mut self, memory: BlockMemory) {
        if let BlockMemory::Mapped(page) = memory {
            self.wire.free_pages(page);
        }
    }

    fn map_block(
        &mut self,
        key: BlockKey,
        tag: Option<BlockTag>,
        block_size: usize,
        flags: &mut BufferFlags,
    ) -> Result<Option<BlockMemory>, Errno> {
        if !self.enabled {
            return Ok(None);
        }
        let dev_offset = key.block.wrapping_mul(block_size as u64);
        match self.wire.map_cacheblock(key.device, dev_offset, tag, flags.as_word_mut(), block_size)
        {
            Ok(page) => Ok(page.map(BlockMemory::Mapped)),
            // C collapses every map failure into `MAP_FAILED` and reads from
            // storage instead (cache.c:443-451); a wire that is gone shows up
            // on the next hand-over, where C disables the cache.
            Err(_) => Ok(None),
        }
    }

    fn set_block(
        &mut self,
        memory: &BlockMemory,
        key: BlockKey,
        tag: Option<BlockTag>,
        block_size: usize,
        once: bool,
        flags: &mut BufferFlags,
    ) -> Result<(), Errno> {
        if !self.enabled {
            return Ok(());
        }
        let Some(page) = memory.mapped() else {
            // Heap memory has no page for virtual memory to map. C cannot
            // reach this state (all its block memory is mapped); the pool
            // refuses to hand over something the other side cannot use.
            return Err(Errno::from_i32(minix_types::EINVAL));
        };
        let dev_offset = key.block.wrapping_mul(block_size as u64);
        match self.wire.set_cacheblock(
            page,
            key.device,
            dev_offset,
            tag,
            flags.as_word_mut(),
            block_size,
            once,
        ) {
            Ok(()) => Ok(()),
            Err(error) => Err(self.note_failure(error)),
        }
    }

    fn forget_block(&mut self, key: BlockKey, block_size: usize) {
        // C prints and carries on when this fails (cache.c:629-633): the
        // block is gone from the pool either way.
        let dev_offset = key.block.wrapping_mul(block_size as u64);
        if let Err(error) = self.wire.forget_cacheblock(key.device, dev_offset, block_size) {
            let _ = self.note_failure(error);
        }
    }

    fn clear_device(&mut self, device: u64) {
        // C calls this even when the cache is switched off (cache.c:803-807)
        // and prints on failure (fsdriver_unmount, call.c:83-84).
        if let Err(error) = self.wire.clear_cache(device) {
            let _ = self.note_failure(error);
        }
    }
}

/// A page cache in a box for the crate's tests: the memory a wire hands out,
/// a cache directory keyed by (device, byte offset), and a call log.
///
/// The pages are real allocations, so the pool's slices are ordinary memory;
/// the double's directory stands in for virtual memory's own bookkeeping, and
/// a page seeded here is found by `map_cacheblock` just as a page the server
/// registered earlier would be.
#[cfg(test)]
pub(crate) mod mock {
    use super::*;
    use alloc::vec::Vec;
    use core::alloc::Layout;
    use minix_types::Errno;

    /// One page-aligned allocation, freed when the double drops.
    ///
    /// A `Vec<u8>` would only be word-aligned, while both C sources of block
    /// memory (the file system's `mmap`, the page cache's mapping) are page
    /// aligned — and the pool's hand-over relies on it, so the double must
    /// produce the real thing.
    #[derive(Debug)]
    struct OwnedPage {
        ptr: *mut u8,
        len: usize,
    }

    impl OwnedPage {
        /// A zeroed page-aligned allocation of `len` bytes.
        fn zeroed(len: usize) -> Self {
            let layout = Layout::from_size_align(len, PAGE_SIZE).expect("valid page layout");
            // SAFETY: `len` is a non-zero page multiple, so the layout has a
            // non-zero size; the pointer is freed in `Drop`.
            let ptr = unsafe { alloc::alloc::alloc_zeroed(layout) };
            assert!(!ptr.is_null(), "test allocation failed");
            Self { ptr, len }
        }
    }

    impl Drop for OwnedPage {
        fn drop(&mut self) {
            let layout = Layout::from_size_align(self.len, PAGE_SIZE).expect("valid page layout");
            // SAFETY: the pointer came from `alloc_zeroed` with this exact
            // layout and is freed once.
            unsafe { alloc::alloc::dealloc(self.ptr, layout) };
        }
    }

    /// One recorded wire call.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) enum MockCall {
        Map {
            dev: u64,
            dev_offset: u64,
            tag: Option<BlockTag>,
        },
        Set {
            dev: u64,
            dev_offset: u64,
            tag: Option<BlockTag>,
            once: bool,
            page: MappedPage,
        },
        Forget {
            dev: u64,
            dev_offset: u64,
        },
        Clear {
            dev: u64,
        },
    }

    /// A wire backed by ordinary memory.
    #[derive(Debug, Default)]
    pub(crate) struct MockVm {
        cache: Vec<((u64, u64), MappedPage)>,
        pages: Vec<OwnedPage>,
        /// Pages handed out and already given back again.
        freed: Vec<MappedPage>,
        /// Calls in order, for order-sensitive assertions.
        pub(crate) calls: Vec<MockCall>,
        /// When set, the next hand-over fails with this errno (and clears).
        pub(crate) set_fails: Option<i32>,
        /// When set, every mapping fails with this errno.
        pub(crate) map_fails: Option<i32>,
        /// When set, every page allocation fails with this errno.
        pub(crate) alloc_fails: Option<i32>,
    }

    impl MockVm {
        /// A page-aligned allocation of `bytes` bytes.
        fn page(&mut self, bytes: usize) -> MappedPage {
            let len = page_round_up(bytes.max(1));
            let owned = OwnedPage::zeroed(len);
            let addr = owned.ptr as usize;
            self.pages.push(owned);
            MappedPage::new(addr, len)
        }

        /// Fill a page of the mock's cache with `fill`, as if a file system
        /// had handed the block over earlier.
        pub(crate) fn seed(&mut self, dev: u64, dev_offset: u64, fill: u8, bytes: usize) {
            let mut page = self.page(bytes);
            page.as_mut_slice(bytes).fill(fill);
            self.cache.push(((dev, dev_offset), page));
        }

        /// Bytes of a cached block, as virtual memory sees them.
        pub(crate) fn cached(&self, dev: u64, dev_offset: u64, len: usize) -> Option<&[u8]> {
            self.cache
                .iter()
                .find(|((d, o), _)| *d == dev && *o == dev_offset)
                .map(|(_, page)| page.as_slice(len))
        }

        /// Drop one block the way an eviction would, without going through
        /// the file system: the page cache forgets the page while the file
        /// system still holds it, which is exactly the state the
        /// `VMMC_EVICTED` flag describes.
        pub(crate) fn evict(&mut self, dev: u64, dev_offset: u64) {
            self.cache.retain(|((d, o), _)| !(*d == dev && *o == dev_offset));
        }

        /// Pages this double handed out and has not been given back.
        ///
        /// A pool that forgets to return its memory shows up here as a
        /// ratchet; the C side's `munmap_t` has the same observable effect.
        pub(crate) fn live_pages(&self) -> usize {
            self.pages.len() - self.freed.len()
        }
    }

    impl VmCacheWire for MockVm {
        fn map_cacheblock(
            &mut self,
            dev: u64,
            dev_offset: u64,
            tag: Option<BlockTag>,
            _flags: &mut u32,
            _block_size: usize,
        ) -> Result<Option<MappedPage>, Errno> {
            self.calls.push(MockCall::Map { dev, dev_offset, tag });
            if let Some(code) = self.map_fails {
                return Err(Errno::from_i32(code));
            }
            Ok(self
                .cache
                .iter()
                .find(|((d, o), _)| *d == dev && *o == dev_offset)
                .map(|(_, page)| *page))
        }

        fn set_cacheblock(
            &mut self,
            page: MappedPage,
            dev: u64,
            dev_offset: u64,
            tag: Option<BlockTag>,
            _flags: &mut u32,
            _block_size: usize,
            once: bool,
        ) -> Result<(), Errno> {
            self.calls.push(MockCall::Set { dev, dev_offset, tag, once, page });
            if let Some(code) = self.set_fails.take() {
                return Err(Errno::from_i32(code));
            }
            // A one-shot block is used once and discarded, so it never enters
            // the directory (C: nothing is stored either).
            if !once {
                self.cache.push(((dev, dev_offset), page));
            }
            Ok(())
        }

        fn forget_cacheblock(
            &mut self,
            dev: u64,
            dev_offset: u64,
            _block_size: usize,
        ) -> Result<(), Errno> {
            self.calls.push(MockCall::Forget { dev, dev_offset });
            self.cache.retain(|((d, o), _)| !(*d == dev && *o == dev_offset));
            Ok(())
        }

        fn clear_cache(&mut self, dev: u64) -> Result<(), Errno> {
            self.calls.push(MockCall::Clear { dev });
            self.cache.retain(|((d, _), _)| *d != dev);
            Ok(())
        }

        fn alloc_pages(&mut self, bytes: usize) -> Result<MappedPage, Errno> {
            if let Some(code) = self.alloc_fails {
                return Err(Errno::from_i32(code));
            }
            Ok(self.page(bytes))
        }

        fn free_pages(&mut self, page: MappedPage) {
            self.freed.push(page);
        }
    }

    /// A wire that refuses every hand-over with a plain "not found" — an
    /// errno with no named policy in C's three-way split. Memory allocation
    /// works, so a test can reach the hand-over itself.
    #[derive(Debug, Default)]
    pub(crate) struct HostileWire(MockVm);

    impl VmCacheWire for HostileWire {
        fn map_cacheblock(
            &mut self,
            _dev: u64,
            _dev_offset: u64,
            _tag: Option<BlockTag>,
            _flags: &mut u32,
            _block_size: usize,
        ) -> Result<Option<MappedPage>, Errno> {
            Ok(None)
        }

        fn set_cacheblock(
            &mut self,
            _page: MappedPage,
            _dev: u64,
            _dev_offset: u64,
            _tag: Option<BlockTag>,
            _flags: &mut u32,
            _block_size: usize,
            _once: bool,
        ) -> Result<(), Errno> {
            Err(Errno::from_i32(minix_types::ENOENT))
        }

        fn forget_cacheblock(
            &mut self,
            _dev: u64,
            _dev_offset: u64,
            _block_size: usize,
        ) -> Result<(), Errno> {
            Ok(())
        }

        fn clear_cache(&mut self, _dev: u64) -> Result<(), Errno> {
            Ok(())
        }

        fn alloc_pages(&mut self, bytes: usize) -> Result<MappedPage, Errno> {
            self.0.alloc_pages(bytes)
        }

        fn free_pages(&mut self, page: MappedPage) {
            self.0.free_pages(page);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::mock::{HostileWire, MockCall, MockVm};
    use super::*;
    use crate::cache::BlockKey;
    use alloc::vec;
    use minix_types::ENOENT;

    #[test]
    fn test_page_rounding() {
        assert_eq!(PAGE_SIZE, 4096);
        assert_eq!(page_round_up(1), 4096);
        assert_eq!(page_round_up(4096), 4096);
        assert_eq!(page_round_up(4097), 8192);
        assert!(is_page_multiple(8192));
        assert!(!is_page_multiple(0));
        assert!(!is_page_multiple(1024));
    }

    #[test]
    fn test_tag_wire_values() {
        assert_eq!(BlockTag::inode_or_none(Some(BlockTag::new(7, 4096))), 7);
        assert_eq!(BlockTag::offset_or_zero(Some(BlockTag::new(7, 4096))), 4096);
        assert_eq!(BlockTag::inode_or_none(None), VMC_NO_INODE);
        assert_eq!(BlockTag::offset_or_zero(None), 0);
        // An explicit `VMC_NO_INODE` tag is C's "no file behind it".
        assert_eq!(
            BlockTag::inode_or_none(Some(BlockTag::new(VMC_NO_INODE, 8))),
            VMC_NO_INODE
        );
    }

    #[test]
    fn test_flags_word_round_trip() {
        let mut flags = BufferFlags::empty();
        assert!(!flags.is_dirty() && !flags.is_locked() && !flags.is_evicted());
        flags.set_dirty();
        flags.lock();
        assert!(flags.is_dirty() && flags.is_locked());
        assert_eq!(flags.word(), VMMC_DIRTY | VMMC_BLOCK_LOCKED);
        flags.clear_dirty();
        flags.mark_evicted();
        assert!(!flags.is_dirty() && flags.is_evicted());
        flags.unlock();
        assert_eq!(flags.word(), VMMC_EVICTED);
        assert_eq!(*flags.as_word_mut(), VMMC_EVICTED);
        // The wire's word is the same storage, not a copy.
        *flags.as_word_mut() |= VMMC_DIRTY;
        assert!(flags.is_dirty());
    }

    #[test]
    fn test_alignment_gate_decides_enablement() {
        let mut level = VmSecondLevel::new(MockVm::default());
        assert!(!level.is_enabled(), "no block size known yet");
        level.set_block_size(4096);
        assert!(level.is_enabled());
        level.set_block_size(1024);
        assert!(!level.is_enabled(), "a block that is not a whole page cannot be mapped");
        level.set_block_size(8192);
        assert!(level.is_enabled());
    }

    #[test]
    fn test_opt_out_never_enables() {
        let mut level = VmSecondLevel::opted_out(MockVm::default());
        level.set_block_size(4096);
        assert!(!level.is_enabled());
        let mut flags = BufferFlags::empty();
        assert_eq!(level.map_block(BlockKey::new(1, 0), None, 4096, &mut flags), Ok(None));
        assert_eq!(
            level.set_block(
                &BlockMemory::zeroed(4096),
                BlockKey::new(1, 0),
                None,
                4096,
                false,
                &mut flags,
            ),
            Ok(())
        );
        assert!(level.wire().calls.is_empty(), "an opted-out level never reaches the wire");
    }

    #[test]
    fn test_map_converts_block_number_to_byte_offset() {
        let mut level = VmSecondLevel::new(MockVm::default());
        level.set_block_size(4096);
        // Block 3 of a 4096-byte file system is byte offset 12288.
        level.wire_mut().seed(7, 3 * 4096, 0xAB, 4096);
        let mut flags = BufferFlags::empty();
        let block = level
            .map_block(BlockKey::new(7, 3), Some(BlockTag::new(42, 8192)), 4096, &mut flags)
            .unwrap()
            .expect("the block is in the page cache");
        assert_eq!(block.as_slice(4), &[0xAB; 4]);
        assert!(block.is_mapped());
        assert_eq!(
            level.wire().calls,
            vec![MockCall::Map { dev: 7, dev_offset: 12288, tag: Some(BlockTag::new(42, 8192)) }]
        );
    }

    #[test]
    fn test_map_miss_and_wire_failure_both_read_as_miss() {
        let mut level = VmSecondLevel::new(MockVm::default());
        level.set_block_size(4096);
        let mut flags = BufferFlags::empty();
        assert_eq!(level.map_block(BlockKey::new(1, 0), None, 4096, &mut flags), Ok(None));
        level.wire.map_fails = Some(ENOENT);
        assert_eq!(
            level.map_block(BlockKey::new(1, 0), None, 4096, &mut flags),
            Ok(None),
            "C collapses every map failure into MAP_FAILED (cache.c:443-451)"
        );
    }

    #[test]
    fn test_alloc_is_page_backed() {
        let mut level = VmSecondLevel::new(MockVm::default());
        level.set_block_size(4096);
        let memory = level.alloc_block(4096).unwrap();
        let page = memory.mapped().expect("the second level allocates page memory");
        assert!(page.addr() % PAGE_SIZE == 0);
        assert!(page.bytes() >= 4096);
        assert_eq!(memory.as_slice(2), &[0u8; 2]);
        level.free_block(memory);
    }

    #[test]
    fn test_set_hands_the_page_to_the_page_cache() {
        let mut level = VmSecondLevel::new(MockVm::default());
        level.set_block_size(4096);
        let mut memory = level.alloc_block(4096).unwrap();
        memory.as_mut_slice(4096).fill(0x5A);
        let mut flags = BufferFlags::empty();
        level
            .set_block(&memory, BlockKey::new(3, 9), Some(BlockTag::new(11, 4096)), 4096, false, &mut flags)
            .unwrap();
        // The page cache now holds the same bytes: byte offset 9 * 4096.
        assert_eq!(level.wire().cached(3, 9 * 4096, 4), Some(&[0x5A; 4][..]));
    }

    #[test]
    fn test_one_shot_hand_over_is_not_retained() {
        let mut level = VmSecondLevel::new(MockVm::default());
        level.set_block_size(4096);
        let memory = level.alloc_block(4096).unwrap();
        let mut flags = BufferFlags::empty();
        level
            .set_block(&memory, BlockKey::new(3, 9), None, 4096, true, &mut flags)
            .unwrap();
        assert!(level.wire().cached(3, 9 * 4096, 1).is_none());
        assert_eq!(
            level.wire().calls,
            vec![MockCall::Set {
                dev: 3,
                dev_offset: 9 * 4096,
                tag: None,
                once: true,
                page: memory.mapped().unwrap(),
            }]
        );
    }

    #[test]
    fn test_enosys_disables_the_level() {
        let mut level = VmSecondLevel::new(MockVm::default());
        level.set_block_size(4096);
        level.wire.set_fails = Some(ENOSYS);
        let mut flags = BufferFlags::empty();
        let memory = level.alloc_block(4096).unwrap();
        assert_eq!(
            level.set_block(&memory, BlockKey::new(1, 0), None, 4096, false, &mut flags),
            Err(Errno::from_i32(ENOSYS))
        );
        assert_eq!(level.fault(), Some(VmCacheFault::TransportGone));
        assert!(!level.is_enabled(), "C turns the cache off for good on ENOSYS");
    }

    #[test]
    fn test_enomem_is_survived_and_recorded() {
        let mut level = VmSecondLevel::new(MockVm::default());
        level.set_block_size(4096);
        level.wire.set_fails = Some(ENOMEM);
        let mut flags = BufferFlags::empty();
        let memory = level.alloc_block(4096).unwrap();
        assert_eq!(
            level.set_block(&memory, BlockKey::new(1, 0), None, 4096, false, &mut flags),
            Err(Errno::from_i32(ENOMEM))
        );
        assert_eq!(level.fault(), Some(VmCacheFault::OutOfMemory));
        assert!(level.is_enabled(), "C keeps the cache on and only prints");
    }

    #[test]
    fn test_rejection_is_recorded_and_the_block_memory_survives() {
        let mut level = VmSecondLevel::new(HostileWire::default());
        level.set_block_size(4096);
        let mut flags = BufferFlags::empty();
        let mut memory = level.alloc_block(4096).unwrap();
        memory.as_mut_slice(4).fill(0x7E);
        assert!(matches!(
            level.set_block(&memory, BlockKey::new(1, 0), None, 4096, false, &mut flags),
            Err(error) if error.to_i32() == ENOENT
        ));
        assert!(
            matches!(level.fault(), Some(VmCacheFault::Rejected(error)) if error.to_i32() == ENOENT)
        );
        assert_eq!(memory.as_slice(4), &[0x7E; 4], "the block is still readable");
    }

    #[test]
    fn test_heap_memory_is_refused_for_hand_over() {
        let mut level = VmSecondLevel::new(MockVm::default());
        level.set_block_size(4096);
        let mut flags = BufferFlags::empty();
        // An owned (heap) block has no page for virtual memory to map.
        assert_eq!(
            level.set_block(
                &BlockMemory::zeroed(4096),
                BlockKey::new(1, 0),
                None,
                4096,
                false,
                &mut flags,
            ),
            Err(Errno::from_i32(minix_types::EINVAL))
        );
        assert!(level.wire().calls.is_empty(), "nothing reached the wire");
    }

    #[test]
    fn test_clear_goes_out_even_when_disabled() {
        let mut level = VmSecondLevel::new(MockVm::default());
        level.set_block_size(1024);
        assert!(!level.is_enabled());
        level.wire_mut().seed(9, 0, 1, 4096);
        level.clear_device(9);
        assert_eq!(level.wire().calls, vec![MockCall::Clear { dev: 9 }]);
        assert!(level.wire().cached(9, 0, 1).is_none());
    }

    #[test]
    fn test_forget_drops_one_block() {
        let mut level = VmSecondLevel::new(MockVm::default());
        level.set_block_size(4096);
        level.wire_mut().seed(4, 2 * 4096, 0x11, 4096);
        level.wire_mut().seed(4, 5 * 4096, 0x22, 4096);
        level.forget_block(BlockKey::new(4, 2), 4096);
        assert_eq!(
            level.wire().calls,
            vec![MockCall::Forget { dev: 4, dev_offset: 2 * 4096 }]
        );
        assert!(level.wire().cached(4, 2 * 4096, 1).is_none());
        assert!(level.wire().cached(4, 5 * 4096, 1).is_some(), "the neighbour stays");
    }
}
