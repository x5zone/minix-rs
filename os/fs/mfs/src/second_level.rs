//! The server's second level: which page cache, and the wire to it.
//!
//! C correspondence: two pieces that live apart in C and meet in the server.
//! The wire is libsys (`minix3/minix/lib/libsys/vm_cache.c`, all four calls
//! plus `mmap`/`munmap` from `libc/sys/mmap.c`), and the decision whether to
//! use it is libminixfs's `may_use_vmcache` flag
//! (`minix3/minix/lib/libminixfs/cache.c:61,1236-1239`), set by the server's
//! start-up (`minix3/minix/fs/mfs/main.c:52`).
//!
//! Two types:
//! - [`VmWire`] wraps the message layer: one method per libminixfs call
//!   site, each running the matching wrapper from `minix-sys` (which owns
//!   the message layout).
//! - [`MfsSecondLevel`] is the runtime choice the boot configuration makes:
//!   [`MfsSecondLevel`] when `use_vmcache` is off, the real thing otherwise.
//!   It is the cache's second-level parameter everywhere inside the server,
//!   so every path — including the unmount that must clear the device in the
//!   page cache — reaches the same face.

use alloc::boxed::Box;

use minix_sys::ipc::IpcTransport;
use minix_sys::vm::{
    MAP_FLAG_ANON, MAP_FLAG_PREALLOC, MAP_PROTECTION_READ, MAP_PROTECTION_WRITE, MapRequest,
    clear_cache_via, forget_cacheblock_via, map_cacheblock_via, mmap_via, munmap_via,
    set_cacheblock_via,
};
use minix_types::{Endpoint, Errno, VMSF_ONCE, VirBytes};

use minix_fs::cache::{
    BlockMemory, BlockTag, BufferFlags, MappedPage, SecondLevelCache, VmCacheWire,
    VmSecondLevel, page_round_up,
};

/// The four page-cache calls plus anonymous block memory, over a transport.
///
/// Each method runs the same sequence as the C function it mirrors: the
/// block number is already a byte offset by the time it arrives (the pool's
/// [`VmSecondLevel`] does that conversion, as libminixfs does).
pub struct VmWire<T: IpcTransport> {
    transport: T,
}

/// A fresh production wire: this server's own channel to the memory server.
///
/// The transport is the process's trap channel, which carries no state of
/// its own; a mount builds one wire, the next mount builds another.
pub fn production_wire() -> Box<dyn VmCacheWire> {
    Box::new(VmWire::new(minix_sys::ipc::DirectTrapTransport))
}

impl<T: IpcTransport> VmWire<T> {
    /// A wire over one IPC channel to the memory server.
    pub const fn new(transport: T) -> Self {
        Self { transport }
    }

    /// The transport, for inspection.
    pub const fn transport(&self) -> &T {
        &self.transport
    }

    /// The transport, mutably (tests script further replies through it).
    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }
}

impl<T: IpcTransport> VmCacheWire for VmWire<T> {
    fn map_cacheblock(
        &mut self,
        dev: u64,
        dev_offset: u64,
        tag: Option<BlockTag>,
        flags: &mut u32,
        block_size: usize,
    ) -> Result<Option<MappedPage>, Errno> {
        // C: `vm_map_cacheblock` (vm_cache.c:47-57) — the mapped address
        // comes back in the reply; a failed call is the caller's MISS.
        let addr = map_cacheblock_via(
            &self.transport,
            dev,
            dev_offset as i64,
            BlockTag::inode_or_none(tag),
            BlockTag::offset_or_zero(tag) as i64,
            flags,
            page_round_up(block_size) as i32,
        )?;
        Ok(Some(mapped_page(addr.0, block_size)))
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
        // C: `vm_set_cacheblock` (vm_cache.c:59-66) — `setflags` is the
        // `VMSF_ONCE` lane; the block size travels rounded to whole pages.
        set_cacheblock_via(
            &self.transport,
            VirBytes(page.addr() as u64),
            dev,
            dev_offset as i64,
            BlockTag::inode_or_none(tag),
            BlockTag::offset_or_zero(tag) as i64,
            flags,
            page_round_up(block_size) as i32,
            if once { VMSF_ONCE as u8 } else { 0 },
        )
    }

    fn forget_cacheblock(
        &mut self,
        dev: u64,
        dev_offset: u64,
        block_size: usize,
    ) -> Result<(), Errno> {
        // C: `vm_forget_cacheblock` (vm_cache.c:68-75).
        forget_cacheblock_via(&self.transport, dev, dev_offset as i64, page_round_up(block_size) as i32)
    }

    fn clear_cache(&mut self, dev: u64) -> Result<(), Errno> {
        // C: `vm_clear_cache` (vm_cache.c:77-88).
        clear_cache_via(&self.transport, dev)
    }

    fn alloc_pages(&mut self, bytes: usize) -> Result<MappedPage, Errno> {
        // C: `lmfs_alloc_block`'s `mmap(0, block_size, PROT_READ|PROT_WRITE,
        // MAP_PREALLOC|MAP_ANON, -1, 0)` (cache.c:200-201). The beneficiary
        // is the server itself, which is what keeps the third-party flag off
        // (`mmap.c:36-38`).
        let request = MapRequest {
            beneficiary: Endpoint::SELF,
            address: VirBytes(0),
            length: VirBytes(bytes as u64),
            protection: MAP_PROTECTION_READ | MAP_PROTECTION_WRITE,
            flags: MAP_FLAG_PREALLOC | MAP_FLAG_ANON,
            file: -1,
            offset: 0,
        };
        let addr = mmap_via(&self.transport, Endpoint::SELF, request)?;
        Ok(mapped_page(addr.0, bytes))
    }

    fn free_pages(&mut self, page: MappedPage) {
        // C: `munmap_t` (cache.c:270). A failed unmap leaves the mapping to
        // the process's exit; the pool has already stopped using it.
        let _ = munmap_via(
            &self.transport,
            VirBytes(page.addr() as u64),
            VirBytes(page.bytes() as u64),
        );
    }
}

/// Describe a block's page.
///
/// The memory server returns page-aligned addresses for whole pages; the
/// pool's own invariant checks (`MappedPage::new`) fire here if that ever
/// stops being true, which is the right place to learn it.
fn mapped_page(addr: u64, block_size: usize) -> MappedPage {
    MappedPage::new(addr as usize, page_round_up(block_size))
}

/// The second level this server runs with.
///
/// The runtime choice C keeps in the `vmcache` flag, as one type: the server
/// records the boot configuration's decision here and every cache the
/// server builds uses it. Tests substitute their own wire through
/// [`MfsSecondLevel::vm`].
#[derive(Debug)]
pub enum MfsSecondLevel {
    /// No page cache: heap blocks, no wire calls (C with `vmcache` off).
    Off(minix_fs::cache::NoSecondLevel),
    /// The real thing, over a boxed wire.
    Vm(VmSecondLevel<Box<dyn VmCacheWire>>),
}

impl MfsSecondLevel {
    /// The page cache switched off.
    pub const fn off() -> Self {
        Self::Off(minix_fs::cache::NoSecondLevel)
    }

    /// The page cache over a wire.
    pub fn vm(wire: Box<dyn VmCacheWire>) -> Self {
        Self::Vm(VmSecondLevel::new(wire))
    }

    /// The page cache over a real transport.
    pub fn over<T: IpcTransport + 'static>(transport: T) -> Self {
        Self::vm(Box::new(VmWire::new(transport)))
    }

    /// A second level the file system opted out of, keeping its wire.
    pub fn opted_out(wire: Box<dyn VmCacheWire>) -> Self {
        Self::Vm(VmSecondLevel::opted_out(wire))
    }

    /// The real second level, when this is one.
    pub fn as_vm(&self) -> Option<&VmSecondLevel<Box<dyn VmCacheWire>>> {
        match self {
            Self::Vm(level) => Some(level),
            Self::Off(_) => None,
        }
    }
}

impl SecondLevelCache for MfsSecondLevel {
    fn is_enabled(&self) -> bool {
        match self {
            Self::Off(level) => level.is_enabled(),
            Self::Vm(level) => level.is_enabled(),
        }
    }

    fn set_block_size(&mut self, block_size: usize) {
        match self {
            Self::Off(level) => level.set_block_size(block_size),
            Self::Vm(level) => level.set_block_size(block_size),
        }
    }

    fn alloc_block(&mut self, block_size: usize) -> Result<BlockMemory, Errno> {
        match self {
            Self::Off(level) => level.alloc_block(block_size),
            Self::Vm(level) => level.alloc_block(block_size),
        }
    }

    fn free_block(&mut self, memory: BlockMemory) {
        match self {
            Self::Off(level) => level.free_block(memory),
            Self::Vm(level) => level.free_block(memory),
        }
    }

    fn map_block(
        &mut self,
        key: minix_fs::cache::BlockKey,
        tag: Option<BlockTag>,
        block_size: usize,
        flags: &mut BufferFlags,
    ) -> Result<Option<BlockMemory>, Errno> {
        match self {
            Self::Off(level) => level.map_block(key, tag, block_size, flags),
            Self::Vm(level) => level.map_block(key, tag, block_size, flags),
        }
    }

    fn set_block(
        &mut self,
        memory: &BlockMemory,
        key: minix_fs::cache::BlockKey,
        tag: Option<BlockTag>,
        block_size: usize,
        once: bool,
        flags: &mut BufferFlags,
    ) -> Result<(), Errno> {
        match self {
            Self::Off(level) => level.set_block(memory, key, tag, block_size, once, flags),
            Self::Vm(level) => level.set_block(memory, key, tag, block_size, once, flags),
        }
    }

    fn forget_block(&mut self, key: minix_fs::cache::BlockKey, block_size: usize) {
        match self {
            Self::Off(level) => level.forget_block(key, block_size),
            Self::Vm(level) => level.forget_block(key, block_size),
        }
    }

    fn clear_device(&mut self, device: u64) {
        match self {
            Self::Off(level) => level.clear_device(device),
            Self::Vm(level) => level.clear_device(device),
        }
    }
}

/// Test doubles: a page cache in a box, and a call log the test keeps.
///
/// The double plays the memory server's part in hosted tests. Its memory is
/// real and page-aligned (the pool slices it), and every wire call it
/// receives lands in a log the test can inspect after the level has been
/// consumed by a mount.
#[cfg(test)]
pub(crate) mod double {
    use super::*;
    use alloc::rc::Rc;
    use alloc::vec::Vec;
    use core::alloc::Layout;
    use core::cell::RefCell;

    /// One page-aligned allocation, freed when the double drops.
    #[derive(Debug)]
    struct OwnedPage {
        ptr: *mut u8,
        len: usize,
    }

    impl OwnedPage {
        fn zeroed(len: usize) -> Self {
            let layout = Layout::from_size_align(len, minix_fs::cache::PAGE_SIZE)
                .expect("valid page layout");
            // SAFETY: the layout has a non-zero size (a whole page); the
            // pointer is freed in `Drop`.
            let ptr = unsafe { alloc::alloc::alloc_zeroed(layout) };
            assert!(!ptr.is_null(), "test allocation failed");
            Self { ptr, len }
        }
    }

    impl Drop for OwnedPage {
        fn drop(&mut self) {
            let layout = Layout::from_size_align(self.len, minix_fs::cache::PAGE_SIZE)
                .expect("valid page layout");
            // SAFETY: the pointer came from `alloc_zeroed` with this layout
            // and is freed once.
            unsafe { alloc::alloc::dealloc(self.ptr, layout) };
        }
    }

    /// One recorded wire call.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum Call {
        /// A map request for a block (device, byte offset).
        Map { dev: u64, dev_offset: u64 },
        /// A hand-over of a block (device, byte offset, inode tag,
        /// one-shot flag).
        Set { dev: u64, dev_offset: u64, tag: Option<BlockTag>, once: bool },
        /// A single-block forget (device, byte offset).
        Forget { dev: u64, dev_offset: u64 },
        /// A whole-device clear.
        Clear { dev: u64 },
    }

    /// A shared call log: the double writes, the test reads.
    #[derive(Debug, Clone, Default)]
    pub(crate) struct CallLog(Rc<RefCell<Vec<Call>>>);

    impl CallLog {
        /// The calls recorded so far.
        pub(crate) fn calls(&self) -> Vec<Call> {
            self.0.borrow().clone()
        }

        /// Whether a call matching `pattern` was recorded.
        pub(crate) fn saw(&self, pattern: Call) -> bool {
            self.0.borrow().contains(&pattern)
        }

        fn push(&self, call: Call) {
            self.0.borrow_mut().push(call);
        }
    }

    /// A memory server in a box: real pages, no cached blocks, a call log.
    #[derive(Debug, Default)]
    pub(crate) struct MemoryVm {
        pages: Vec<OwnedPage>,
        log: CallLog,
    }

    impl MemoryVm {
        /// A double plus the handle its call log is read through.
        pub(crate) fn new() -> (Self, CallLog) {
            let double = Self::default();
            let log = double.log.clone();
            (double, log)
        }
    }

    impl VmCacheWire for MemoryVm {
        fn map_cacheblock(
            &mut self,
            dev: u64,
            dev_offset: u64,
            _tag: Option<BlockTag>,
            _flags: &mut u32,
            _block_size: usize,
        ) -> Result<Option<MappedPage>, Errno> {
            self.log.push(Call::Map { dev, dev_offset });
            // Nothing is in the page cache: the pool reads from storage.
            Ok(None)
        }

        fn set_cacheblock(
            &mut self,
            _page: MappedPage,
            dev: u64,
            dev_offset: u64,
            tag: Option<BlockTag>,
            _flags: &mut u32,
            _block_size: usize,
            once: bool,
        ) -> Result<(), Errno> {
            self.log.push(Call::Set { dev, dev_offset, tag, once });
            Ok(())
        }

        fn forget_cacheblock(
            &mut self,
            dev: u64,
            dev_offset: u64,
            _block_size: usize,
        ) -> Result<(), Errno> {
            self.log.push(Call::Forget { dev, dev_offset });
            Ok(())
        }

        fn clear_cache(&mut self, dev: u64) -> Result<(), Errno> {
            self.log.push(Call::Clear { dev });
            Ok(())
        }

        fn alloc_pages(&mut self, bytes: usize) -> Result<MappedPage, Errno> {
            let len = page_round_up(bytes.max(1));
            let page = OwnedPage::zeroed(len);
            let addr = page.ptr as usize;
            self.pages.push(page);
            Ok(MappedPage::new(addr, len))
        }

        fn free_pages(&mut self, _page: MappedPage) {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_fs::cache::VmCacheWire;
    use minix_sys::ipc::CannedTransport;
    use minix_sys::vm::{
        VM_CALL_CLEAR_CACHE, VM_CALL_FORGET_CACHE_PAGE, VM_CALL_MAP_CACHE_PAGE, VM_CALL_MMAP,
        VM_CALL_MUNMAP, VM_CALL_SET_CACHE_PAGE,
    };
    use minix_types::Message;

    /// A reply carrying the mapped address in the reply lane
    /// (`m_vmmcp_reply.addr` at byte zero of the payload).
    fn map_reply(address: u64) -> Message {
        let mut message = Message::zeroed();
        // SAFETY: the payload is a raw byte array; the layout is the one
        // minix-sys's wrapper documents for the reply.
        unsafe {
            message.m_u.raw[..8].copy_from_slice(&address.to_ne_bytes());
        }
        message
    }

    #[test]
    fn test_map_call_lanes() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(map_reply(0x9000_0000)));
        let mut wire = VmWire::new(transport);
        let mut flags = 0xABu32;
        let page = wire
            .map_cacheblock(0x5678, 0x10_0000, Some(BlockTag::new(42, 0x20_0000)), &mut flags, 4096)
            .unwrap()
            .expect("the reply carried an address");
        assert_eq!(page.addr(), 0x9000_0000);
        assert_eq!(page.bytes(), 4096);
        let sent = wire.transport().sent.borrow();
        assert_eq!(sent[0].1.m_type, VM_CALL_MAP_CACHE_PAGE);
        // SAFETY: the outgoing payload is the raw byte array the layout
        // documents (dev @0, dev_offset @8, ino_offset @16, ino @24).
        let raw = unsafe { &sent[0].1.m_u.raw };
        assert_eq!(&raw[..8], &0x5678u64.to_ne_bytes());
        assert_eq!(&raw[8..16], &0x10_0000u64.to_ne_bytes());
        assert_eq!(&raw[16..24], &0x20_0000u64.to_ne_bytes());
        assert_eq!(&raw[24..32], &42u64.to_ne_bytes());
        assert_eq!(raw[48], 1, "one page of 4096 bytes");
    }

    #[test]
    fn test_set_call_lanes_carry_the_page_and_the_once_flag() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(Message::zeroed()));
        let mut wire = VmWire::new(transport);
        let page = MappedPage::new(0x8000_0000, 8192);
        let mut flags = 0u32;
        wire.set_cacheblock(page, 7, 0x2000, Some(BlockTag::new(3, 0x1000)), &mut flags, 8192, true)
            .unwrap();
        let sent = wire.transport().sent.borrow();
        assert_eq!(sent[0].1.m_type, VM_CALL_SET_CACHE_PAGE);
        // SAFETY: as above; the block address travels at @32.
        let raw = unsafe { &sent[0].1.m_u.raw };
        assert_eq!(&raw[32..40], &0x8000_0000u64.to_ne_bytes());
        assert_eq!(&raw[24..32], &3u64.to_ne_bytes());
        assert_eq!(raw[48], 2, "8192 bytes is two pages");
        assert_eq!(raw[49], minix_types::VMSF_ONCE as u8);
    }

    #[test]
    fn test_forget_and_clear_call_lanes() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(Message::zeroed()));
        transport.reply_sendrec(Ok(Message::zeroed()));
        let mut wire = VmWire::new(transport);
        wire.forget_cacheblock(5, 0x3000, 4096).unwrap();
        wire.clear_cache(5).unwrap();
        let sent = wire.transport().sent.borrow();
        assert_eq!(sent[0].1.m_type, VM_CALL_FORGET_CACHE_PAGE);
        // SAFETY: the device and the byte offset are the first two lanes.
        let forgot = unsafe { &sent[0].1.m_u.raw };
        assert_eq!(&forgot[..8], &5u64.to_ne_bytes());
        assert_eq!(&forgot[8..16], &0x3000u64.to_ne_bytes());
        assert_eq!(sent[1].1.m_type, VM_CALL_CLEAR_CACHE);
        // SAFETY: clear's only argument is the device at lane zero.
        let cleared = unsafe { &sent[1].1.m_u.raw };
        assert_eq!(&cleared[..8], &5u64.to_ne_bytes());
    }

    #[test]
    fn test_block_memory_is_a_preallocated_anonymous_mapping() {
        let mut transport = CannedTransport::new();
        // `mmap`'s reply carries the chosen address in the return-address
        // lane (`MapPayload`'s last eight bytes before padding).
        let mut reply = Message::zeroed();
        // SAFETY: plain-value write into the documented reply lane.
        unsafe {
            reply.m_u.raw[40..48].copy_from_slice(&0x7000_0000u64.to_ne_bytes());
        }
        transport.reply_sendrec(Ok(reply));
        let mut wire = VmWire::new(transport);
        let page = wire.alloc_pages(4096).unwrap();
        assert_eq!(page.addr(), 0x7000_0000);
        {
            let sent = wire.transport().sent.borrow();
            assert_eq!(sent[0].1.m_type, VM_CALL_MMAP);
            // SAFETY: the mmap payload's field order is documented on
            // `MapPayload` (offset @0, address @8, length @16, protection
            // @24, flags @28, file @32, beneficiary @36).
            let raw = unsafe { &sent[0].1.m_u.raw };
            assert_eq!(&raw[16..24], &4096u64.to_ne_bytes());
            assert_eq!(&raw[24..28], &(MAP_PROTECTION_READ | MAP_PROTECTION_WRITE).to_ne_bytes());
            assert_eq!(&raw[28..32], &(MAP_FLAG_PREALLOC | MAP_FLAG_ANON).to_ne_bytes());
            assert_eq!(&raw[32..36], &(-1i32).to_ne_bytes(), "no backing file");
            assert_eq!(&raw[36..40], &Endpoint::SELF.0.to_ne_bytes());
        }

        // Unmapping goes to the same server with the same address.
        transport_reply(&mut wire, Message::zeroed());
        wire.free_pages(page);
        let sent = wire.transport().sent.borrow();
        assert_eq!(sent[1].1.m_type, VM_CALL_MUNMAP);
        // SAFETY: munmap's address and length are at lanes @8 and @16.
        let raw = unsafe { &sent[1].1.m_u.raw };
        assert_eq!(&raw[8..16], &0x7000_0000u64.to_ne_bytes());
        assert_eq!(&raw[16..24], &4096u64.to_ne_bytes());
    }

    /// Script one more reply on a wire whose transport is already borrowed
    /// by the test (the transport lives inside the wire).
    fn transport_reply(wire: &mut VmWire<CannedTransport>, reply: Message) {
        wire.transport_mut().reply_sendrec(Ok(reply));
    }
}
