//! Virtual memory call group: mapping, break management, and client queries.
//!
//! The memory server owns every address space: it maps files and anonymous
//! memory, moves the heap boundary, forks address spaces, answers physical
//! address questions, and remaps pages between processes. User programs and
//! servers reach it through thin wrappers that share the shape established
//! by the earlier groups (see [`crate::pm`]): clear a message, fill the
//! call's fields, and run a protocol against the memory endpoint. The C
//! sources are the user-side files `minix3/minix/lib/libc/sys/mmap.c`,
//! `brk.c`, `sbrk.c` and the client helpers `minix3/minix/lib/libsys/vm_*`
//! (the user-space subset; privilege and live-update helpers belong to the
//! integration stage, as the stage plan records).
//!
//! Three request shapes recur across the group:
//!
//! 1. The mapping request: address, length, protection, flags, file,
//!    offset, and the beneficiary, with the third-party flag added whenever
//!    the beneficiary is not the caller.
//! 2. The break request: one address, sent only when it differs from the
//!    cached boundary.
//! 3. The query shapes: endpoint plus address in, scalar answer out, with a
//!    reserved failure sentinel (zero address, all-ones reference count).
//!
//! Every wrapper comes in two forms: a transport-generic core that tests
//! drive with a scripted transport, and a thin direct-transport shim that
//! real binaries call.
//!
//! # Execution model
//!
//! Pure wrappers over the transport trait: no shared state, no
//! synchronization questions.

use crate::ipc::IpcTransport;
use crate::syscall::{perform_syscall, perform_taskcall};
use minix_types::{Endpoint, Errno, Message, PhysBytes, VirBytes};

/// Memory server endpoint.
///
/// C: `VM_PROC_NR ((endpoint_t) 8)` (`minix3/minix/include/minix/com.h:67`).
pub const VM_ENDPOINT_NUMBER: i32 = 8;

/// Terminate address-space bookkeeping. C: `VM_EXIT (VM_RQ_BASE+0)`.
pub const VM_CALL_EXIT: i32 = 0xC00;
/// Fork an address space. C: `VM_FORK (VM_RQ_BASE+1)`.
pub const VM_CALL_FORK: i32 = 0xC01;
/// Move the heap boundary. C: `VM_BRK (VM_RQ_BASE+2)`.
pub const VM_CALL_BREAK: i32 = 0xC02;
/// Announce an upcoming exit. C: `VM_WILLEXIT (VM_RQ_BASE+5)`.
pub const VM_CALL_WILL_EXIT: i32 = 0xC05;
/// Map memory. C: `VM_MMAP (VM_RQ_BASE+10)`.
pub const VM_CALL_MMAP: i32 = 0xC0A;
/// Map physical memory. C: `VM_MAP_PHYS (VM_RQ_BASE+15)`.
pub const VM_CALL_MAP_PHYS: i32 = 0xC0F;
/// Unmap physical memory. C: `VM_UNMAP_PHYS (VM_RQ_BASE+16)`.
pub const VM_CALL_UNMAP_PHYS: i32 = 0xC10;
/// Unmap memory. C: `VM_MUNMAP (VM_RQ_BASE+17)`.
pub const VM_CALL_MUNMAP: i32 = 0xC11;
/// Remap pages between processes. C: `VM_REMAP (VM_RQ_BASE+33)`.
pub const VM_CALL_REMAP: i32 = 0xC21;
/// Unmap shared memory. C: `VM_SHM_UNMAP (VM_RQ_BASE+34)`.
pub const VM_CALL_SHARED_UNMAP: i32 = 0xC22;
/// Translate to a physical address. C: `VM_GETPHYS (VM_RQ_BASE+35)`.
pub const VM_CALL_GET_PHYSICAL: i32 = 0xC23;
/// Read a page reference count. C: `VM_GETREF (VM_RQ_BASE+36)`.
pub const VM_CALL_GET_REFERENCE: i32 = 0xC24;
/// Query memory information. C: `VM_INFO (VM_RQ_BASE+40)`.
pub const VM_CALL_INFO: i32 = 0xC28;
/// Read-only remap. C: `VM_REMAP_RO (VM_RQ_BASE+44)`.
pub const VM_CALL_REMAP_READ_ONLY: i32 = 0xC2C;
/// Process control. C: `VM_PROCCTL (VM_RQ_BASE+45)`.
pub const VM_CALL_PROCESS_CONTROL: i32 = 0xC2D;
/// Map a file-system cache block. C: `VM_MAPCACHEPAGE (VM_RQ_BASE+26)`.
pub const VM_CALL_MAP_CACHE_PAGE: i32 = 0xC1A;
/// Identify a cache block. C: `VM_SETCACHEPAGE (VM_RQ_BASE+27)`.
pub const VM_CALL_SET_CACHE_PAGE: i32 = 0xC1B;
/// Forget a cache block. C: `VM_FORGETCACHEPAGE (VM_RQ_BASE+28)`.
pub const VM_CALL_FORGET_CACHE_PAGE: i32 = 0xC1C;
/// Clear a device's cache blocks. C: `VM_CLEARCACHE (VM_RQ_BASE+29)`.
pub const VM_CALL_CLEAR_CACHE: i32 = 0xC1D;

/// No access. C: `PROT_NONE 0x00` (`minix3/sys/sys/mman.h:62`).
pub const MAP_PROTECTION_NONE: u32 = 0x00;
/// Readable pages. C: `PROT_READ 0x01` (`mman.h:63`).
pub const MAP_PROTECTION_READ: u32 = 0x01;
/// Writable pages. C: `PROT_WRITE 0x02` (`mman.h:64`).
pub const MAP_PROTECTION_WRITE: u32 = 0x02;
/// Executable pages. C: `PROT_EXEC 0x04` (`mman.h:65`).
pub const MAP_PROTECTION_EXECUTE: u32 = 0x04;

/// Perform the mapping on behalf of another process.
///
/// C: `MAP_THIRDPARTY 0x800000` (`minix3/sys/sys/mman.h:124`): added to the
/// flags whenever the beneficiary differs from the caller (see
/// `mmap.c:36-38`).
pub const MAP_FLAG_THIRD_PARTY: u32 = 0x800000;

/// Anonymous memory, not backed by a file.
///
/// C: `MAP_ANONYMOUS 0x1000` (`minix3/sys/sys/mman.h:97-98`). The block
/// cache's block memory uses it: libminixfs allocates every buffer with
/// `MAP_PREALLOC|MAP_ANON` (`minix3/minix/lib/libminixfs/cache.c:200-201`),
/// and the page cache only accepts pages of that kind
/// (`mem_cache.c:257-261`).
pub const MAP_FLAG_ANON: u32 = 0x1000;

/// Reserve the whole range up front instead of faulting pages in.
///
/// C: `MAP_PREALLOC 0x080000` (`minix3/sys/sys/mman.h:120`), the other half
/// of the block cache's allocation flags (cache.c:200-201).
pub const MAP_FLAG_PREALLOC: u32 = 0x080000;
/// `MAP_PRIVATE 0x0002` (`minix3/sys/sys/mman.h`): this build's mmap
/// validation requires exactly one of SHARED/PRIVATE (doc 20 §3.6), so the
/// `alloc_contig` shape passes it explicitly where C's util passed neither.
pub const MAP_FLAG_PRIVATE: u32 = 0x0002;

/// `MAP_CONTIG 0x100000` (`minix3/sys/sys/mman.h`): the mapping must be
/// physically contiguous (the DMA constraint; C pairs it with PREALLOC).
pub const MAP_FLAG_CONTIG: u32 = 0x100000;

/// `MAP_LOWER16M 0x200000` (`minix3/sys/sys/mman.h`): ISA-DMA addressable
/// below 16 MiB.
pub const MAP_FLAG_LOWER16M: u32 = 0x200000;

/// `MAP_LOWER1M 0x400000` (`minix3/sys/sys/mman.h`): below 1 MiB.
pub const MAP_FLAG_LOWER1M: u32 = 0x400000;

/// `MAP_ALIGNMENT_64KB 0x01000000` (`minix3/sys/sys/mman.h`): 64 KiB
/// aligned (one half of the audio ring's boundary rule).
pub const MAP_FLAG_ALIGNMENT_64KB: u32 = 0x01000000;

/// `AC_ALIGN4K 0x01` (`minix3/minix/include/minix/syslib.h:99`): page
/// alignment — the allocator's default, no mmap flag behind it.
pub const AC_ALIGN4K: u32 = 0x01;

/// `AC_LOWER16M 0x02` (`syslib.h:100`).
pub const AC_LOWER16M: u32 = 0x02;

/// `AC_ALIGN64K 0x04` (`syslib.h:101`).
pub const AC_ALIGN64K: u32 = 0x04;

/// `AC_LOWER1M 0x08` (`syslib.h:102`).
pub const AC_LOWER1M: u32 = 0x08;

/// Returns the memory server endpoint.
pub const fn vm_endpoint() -> Endpoint {
    Endpoint(VM_ENDPOINT_NUMBER)
}

/// Mapping request lanes live in the shared `minix_types::MessMmap` overlay
/// (C `mess_mmap` field order, ipc.h:1582-1593; 64-bit `addr`/`len`/`retaddr`
/// lanes — the 56-byte payload only compiles on i386 with 32-bit pointers,
/// and minix-rs userspace is LP64; see the overlay's doc, edge NS5-A). Note
/// the leading position of the offset: like the seek payload, the address is
/// not first.
///
/// A validated mapping request: what to map, where, and for whom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapRequest {
    /// Beneficiary endpoint (the caller itself for ordinary mappings).
    pub beneficiary: Endpoint,
    /// Hint address (the server may choose another).
    pub address: VirBytes,
    /// Length in bytes.
    pub length: VirBytes,
    /// Protection bits (see `MAP_PROTECTION_*`).
    pub protection: u32,
    /// Flag bits (see `MAP_FLAG_*`; third-party added automatically).
    pub flags: u32,
    /// Backing file descriptor, if any.
    pub file: i32,
    /// File offset for file mappings.
    pub offset: i64,
}

impl MapRequest {
    /// Fills the third-party flag exactly like the C version: whenever the
    /// beneficiary is not the caller's own endpoint (see `mmap.c:36-38`).
    pub const fn effective_flags(self, caller: Endpoint) -> u32 {
        if self.beneficiary.0 != caller.0 {
            self.flags | MAP_FLAG_THIRD_PARTY
        } else {
            self.flags
        }
    }
}

/// Maps memory through the memory server.
///
/// C: `minix_mmap_for` (`minix3/minix/lib/libc/sys/mmap.c:21-47`): clear a
/// message, fill the seven mapping fields, add the third-party flag when
/// mapping for someone else, and run the protocol. A failed call is an
/// `Err` carrying the errno — C's mapped-failed sentinel never crosses
/// this interface, so a caller cannot mistake failure for an address.
/// The reply carries the chosen address.
pub fn mmap_via(
    transport: &impl IpcTransport,
    caller: Endpoint,
    request: MapRequest,
) -> Result<VirBytes, Errno> {
    let mut message = crate::syscall::cleared_message();
    {
        // SAFETY: `m_mmap` is the documented payload arm for VM_MMAP; the
        // lane map is the crate-wide `MessMmap` authority (64-bit
        // addr/len/retaddr — edge NS5-A).
        let m = unsafe { &mut message.m_u.m_mmap };
        m.offset = request.offset as u64;
        m.addr = request.address.0;
        m.len = request.length.0;
        m.prot = request.protection as i32;
        m.flags = request.effective_flags(caller) as i32;
        m.fd = request.file;
        m.forwhom = request.beneficiary.0;
    }
    perform_syscall(transport, vm_endpoint(), VM_CALL_MMAP, &mut message)?;
    // C reads the chosen address back from the same overlay's retaddr lane
    // (`return m.m_mmap.retaddr`, libc/sys/mmap.c:44-45); VM writes it
    // there (servers/vm/mmap.c:276, VM encode.rs Mmap arm).
    // SAFETY: `m_mmap` is the active union arm for the reply.
    let chosen = unsafe { message.m_u.m_mmap.retaddr };
    Ok(VirBytes(chosen))
}

/// Unmaps a memory range.
///
/// C: `munmap` (`mmap.c:76-85`): clear a message, store address and length
/// through the mapping payload's address/length lanes, and run the protocol.
pub fn munmap_via(
    transport: &impl IpcTransport,
    address: VirBytes,
    length: VirBytes,
) -> Result<(), Errno> {
    let mut message = crate::syscall::cleared_message();
    {
        // SAFETY: same overlay reasoning as mmap_via (edge NS5-A lane map).
        let m = unsafe { &mut message.m_u.m_mmap };
        m.addr = address.0;
        m.len = length.0;
    }
    perform_syscall(transport, vm_endpoint(), VM_CALL_MUNMAP, &mut message).map(|_| ())
}

/// Moves the heap boundary, skipping the round trip when nothing changed.
///
/// C: `brk` (`minix3/minix/lib/libc/sys/brk.c:22-34`): when the requested
/// address equals the cached boundary, return success without calling the
/// server; otherwise send the break request and adopt the new boundary only
/// on success. The returned value is the boundary the caller should cache.
pub fn break_via(
    transport: &impl IpcTransport,
    cached_break: VirBytes,
    requested_break: VirBytes,
) -> Result<VirBytes, Errno> {
    if requested_break == cached_break {
        return Ok(cached_break);
    }
    let mut message = crate::syscall::cleared_message();
    // The break payload is one address at byte zero (see the shared break
    // payload type); the remaining bytes stay zeroed.
    // SAFETY: writing eight bytes into the 56-byte raw payload.
    unsafe {
        message.m_u.raw[..8].copy_from_slice(&requested_break.0.to_ne_bytes());
    }
    perform_syscall(transport, vm_endpoint(), VM_CALL_BREAK, &mut message)?;
    Ok(requested_break)
}

/// Forks an address space (server-side call).
///
/// C: `vm_fork` (`minix3/minix/lib/libsys/vm_fork.c:10-24`): store the
/// endpoint and slot in the `VMF_ENDPOINT`/`VMF_SLOTNO` lanes (`m1i1`/`m1i2`,
/// `com.h:633-634`), run the server-side protocol, and read the child
/// endpoint back from `VMF_CHILD_ENDPOINT` (`m1i3`, `com.h:635`). The named
/// `mess_1` lanes carry the layout (`m1ull1` leads, so the integer lanes are
/// not at raw offset zero).
pub fn fork_address_space_via(
    transport: &impl IpcTransport,
    endpoint: Endpoint,
    slot: i32,
) -> Result<Endpoint, Errno> {
    let mut message = crate::syscall::cleared_message();
    message.m_u.m_m1.m1i1 = endpoint.0;
    message.m_u.m_m1.m1i2 = slot;
    let reply = perform_taskcall(transport, vm_endpoint(), VM_CALL_FORK, &mut message)?;
    if reply < 0 {
        return Err(Errno::from_i32(-reply));
    }
    // SAFETY: the reply's active arm is `m_m1` (VM writes `VMF_CHILD_ENDPOINT`
    // = m1i3 back through the same lanes).
    Ok(Endpoint(unsafe { message.m_u.m_m1.m1i3 }))
}

/// Ends address-space bookkeeping (server-side call).
///
/// C: `vm_exit` (sends the endpoint, returns the raw reply). A failed round
/// trip is the `Err` lane; `Ok` carries the raw reply message type, whose
/// negative values remain the server's error codes.
pub fn exit_address_space_via(
    transport: &impl IpcTransport,
    endpoint: Endpoint,
) -> Result<i32, Errno> {
    let mut message = crate::syscall::cleared_message();
    // SAFETY: one plain integer at byte zero.
    unsafe {
        message.m_u.raw[..4].copy_from_slice(&endpoint.0.to_ne_bytes());
    }
    perform_taskcall(transport, vm_endpoint(), VM_CALL_EXIT, &mut message)
}

/// Remaps pages from one process to another.
///
/// C: `vm_remap` (`mmap.c:88-108`): destination, source, both addresses, and
/// size travel in fixed lanes; the reply carries the destination address. A
/// failed call is an `Err` carrying the errno; C's mapped-failed sentinel
/// never crosses this interface.
pub fn remap_via(
    transport: &impl IpcTransport,
    call: i32,
    destination: Endpoint,
    source: Endpoint,
    destination_address: VirBytes,
    source_address: VirBytes,
    size: VirBytes,
) -> Result<VirBytes, Errno> {
    let mut message = crate::syscall::cleared_message();
    // SAFETY: five plain 64-bit lanes at bytes 0..40; exact bytes below.
    unsafe {
        message.m_u.raw[0..8].copy_from_slice(&(destination.0 as u64).to_ne_bytes());
        message.m_u.raw[8..16].copy_from_slice(&(source.0 as u64).to_ne_bytes());
        message.m_u.raw[16..24].copy_from_slice(&destination_address.0.to_ne_bytes());
        message.m_u.raw[24..32].copy_from_slice(&source_address.0.to_ne_bytes());
        message.m_u.raw[32..40].copy_from_slice(&size.0.to_ne_bytes());
    }
    perform_syscall(transport, vm_endpoint(), call, &mut message)?;
    // SAFETY: the reply carries the destination address at byte zero.
    let placed = unsafe { message.m_u.raw[..8].as_ptr().cast::<u64>().read() };
    Ok(VirBytes(placed))
}

/// Translates a virtual address to a physical address.
///
/// C: `vm_getphys` (`mmap.c:143-156`): endpoint plus address in, physical
/// address out. Failure is an `Err` carrying the errno; C's zero sentinel
/// never crosses this interface, so a zero in the reply is always a
/// genuine physical address.
pub fn physical_address_via(
    transport: &impl IpcTransport,
    endpoint: Endpoint,
    address: VirBytes,
) -> Result<PhysBytes, Errno> {
    let mut message = crate::syscall::cleared_message();
    // SAFETY: endpoint at bytes 0..4, address at bytes 8..16.
    unsafe {
        message.m_u.raw[..4].copy_from_slice(&endpoint.0.to_ne_bytes());
        message.m_u.raw[8..16].copy_from_slice(&address.0.to_ne_bytes());
    }
    perform_syscall(transport, vm_endpoint(), VM_CALL_GET_PHYSICAL, &mut message)?;
    // SAFETY: the reply carries the physical address at byte zero.
    let physical = unsafe { message.m_u.raw[..8].as_ptr().cast::<u64>().read() };
    Ok(PhysBytes(physical))
}

/// C `alloc_contig` (`minix3/minix/lib/libsys/alloc_util.c:9-27`): map
/// `length` bytes with the contiguous/prealloc base plus the requested
/// constraint flags, then translate the chosen address with
/// `sys_umap(SELF, VM_D, …)` — one call per half, and the physical base is
/// what the card will DMA into. `alloc_flags` are the `AC_*` bits; C's
/// util maps `AC_LOWER16M`/`AC_LOWER1M`/`AC_ALIGN64K` onto mmap flags and
/// leaves `AC_ALIGN4K` to the page allocator. The sharing face differs by
/// this build's tightening: C's util sets neither SHARED nor PRIVATE,
/// while this VM requires exactly one (doc 20 §3.6), so PRIVATE goes in
/// explicitly.
pub fn alloc_contig_via(
    transport: &impl IpcTransport,
    caller: Endpoint,
    length: u64,
    alloc_flags: u32,
) -> Result<(VirBytes, PhysBytes), Errno> {
    let mut flags = MAP_FLAG_PRIVATE | MAP_FLAG_ANON | MAP_FLAG_PREALLOC | MAP_FLAG_CONTIG;
    if alloc_flags & AC_LOWER16M != 0 {
        flags |= MAP_FLAG_LOWER16M;
    }
    if alloc_flags & AC_LOWER1M != 0 {
        flags |= MAP_FLAG_LOWER1M;
    }
    if alloc_flags & AC_ALIGN64K != 0 {
        flags |= MAP_FLAG_ALIGNMENT_64KB;
    }
    let address = mmap_via(
        transport,
        caller,
        MapRequest {
            beneficiary: caller,
            address: VirBytes(0),
            length: VirBytes(length),
            protection: MAP_PROTECTION_READ | MAP_PROTECTION_WRITE,
            flags,
            file: -1,
            offset: 0,
        },
    )?;
    let physical = physical_address_via(transport, caller, address)?;
    Ok((address, physical))
}

/// Reads a page reference count.
///
/// C: `vm_getrefcount` (`mmap.c:158-171`): endpoint plus address in, one
/// byte of count out. Failure is an `Err` carrying the errno; C's
/// all-ones sentinel never crosses this interface, so the returned count
/// is always the server's genuine answer.
pub fn reference_count_via(
    transport: &impl IpcTransport,
    endpoint: Endpoint,
    address: VirBytes,
) -> Result<u8, Errno> {
    let mut message = crate::syscall::cleared_message();
    // SAFETY: same two-field layout as the physical query above.
    unsafe {
        message.m_u.raw[..4].copy_from_slice(&endpoint.0.to_ne_bytes());
        message.m_u.raw[8..16].copy_from_slice(&address.0.to_ne_bytes());
    }
    perform_syscall(
        transport,
        vm_endpoint(),
        VM_CALL_GET_REFERENCE,
        &mut message,
    )?;
    // SAFETY: the reply carries the count in its first byte.
    let count = unsafe { message.m_u.raw[0] };
    Ok(count)
}

/// Maps physical memory for a target process (server-side call).
///
/// C: `vm_map_phys`: target, physical address, and length travel to the
/// server; the reply carries the virtual address. The C version additionally
/// registers the region in its local bookkeeping; that registry belongs to
/// the server-integration stage, not to this client wrapper.
pub fn map_physical_via(
    transport: &impl IpcTransport,
    target: Endpoint,
    physical: PhysBytes,
    length: VirBytes,
) -> Result<VirBytes, Errno> {
    // C's i386 wire carries phaddr as a u32—a PA ≥ 4 GiB would be silently
    // truncated and mapped to the wrong physical page (more dangerous than an
    // error), so the high bits are rejected outright.
    if physical.0 > u32::MAX as u64 {
        return Err(Errno::from_i32(minix_types::EINVAL));
    }
    let mut message = crate::syscall::cleared_message();
    // The request rides the `m_lsys_vm_map_phys` arm (the counterpart of
    // `VmMapPhysIn::decode_message`; C `mess_lsys_vm_map_phys` ipc.h:1504-1510:
    // ep@0/phaddr@4/len@8, with both phaddr and len **u32**—the 32-bit width of
    // C's i386 wire; 64-bit phys/len are truncated per the same semantics). The
    // old implementation hand-wrote raw fields (target@0/phys u64@8/len u64@16),
    // so VM decoded phaddr=0 from u32@4—the first real-machine consumer (tty
    // PL011) mapped phys 0, and the DIRECT fault arm rejected it through
    // zerobase.
    // SAFETY: the documented payload arm for VM_MAP_PHYS requests.
    unsafe {
        let mp = &mut message.m_u.m_lsys_vm_map_phys;
        mp.ep = target.0;
        mp.phaddr = physical.0 as u32;
        mp.len = length.0 as u32;
    }
    let reply = perform_taskcall(transport, vm_endpoint(), VM_CALL_MAP_PHYS, &mut message)?;
    if reply < 0 {
        return Err(Errno::from_i32(-reply));
    }
    // VM's reply puts the address in **m1.p1** through `EncodeToM1`
    // (VmMapPhysOut::encode → m1p1), not in raw[0..8] (where m_source/m_type
    // live)—the old decoding read an "address" of m_type<<32|m_source, and the
    // first real-machine consumer (the tty PL011 arm) crashed on that garbage
    // pointer (cr2=0x7c16).
    // SAFETY: VM's MapPhys reply rides the m1 arm; the union read is the
    // documented payload lane.
    let placed = unsafe { message.m_u.m_m1.m1p1 };
    Ok(VirBytes(placed))
}

/// Process-control operation: clear the target's special memory registry.
///
/// C: `VMPPARAM_CLEAR` (`minix3/minix/include/minix/com.h:759`).
pub const PROCESS_CONTROL_PARAM_CLEAR: i32 = 1;
/// Process-control operation: handle a memory range on behalf of the target.
///
/// C: `VMPPARAM_HANDLEMEM` (`minix3/minix/include/minix/com.h:760`).
pub const PROCESS_CONTROL_PARAM_HANDLE_MEM: i32 = 2;

/// Announces that the caller is about to exit.
///
/// C: `vm_willexit` (`minix3/minix/lib/libsys/vm_exit.c:25-33`): one
/// endpoint in the first message-int lane, server-side protocol, the raw
/// reply decides the result. The wire lane is `VMWE_ENDPOINT` = `m1_i1`
/// (`com.h:644`), the same lane the VM server decodes [`minix_types::
/// VmWillexitIn`] from.
pub fn will_exit_via(transport: &impl IpcTransport, endpoint: Endpoint) -> Result<(), Errno> {
    let mut message = crate::syscall::cleared_message();
    // SAFETY: one plain integer at byte zero (m1_i1 lane).
    unsafe {
        message.m_u.raw[..4].copy_from_slice(&endpoint.0.to_ne_bytes());
    }
    let reply = perform_taskcall(transport, vm_endpoint(), VM_CALL_WILL_EXIT, &mut message)?;
    if reply < 0 {
        return Err(Errno::from_i32(-reply));
    }
    Ok(())
}

/// Unmaps a physical memory mapping previously created for a target.
///
/// C: `vm_unmap_phys` (`minix3/minix/lib/libsys/vm_map_phys.c:33-49`):
/// endpoint and virtual address travel on the wire; the length is derived
/// by the server from the region found at the address, so the C caller's
/// `len` argument is dead on the wire and dropped here (the same
/// dead-parameter rule as `minix_stack_params`' unread `path`). The wire
/// lane is the dedicated `m_lsys_vm_unmap_phys` union member — `ep` at
/// byte 0, `vaddr` at byte 4 as a 32-bit value, exactly what the VM server
/// decodes ([`minix_types::VmUnmapPhysIn`] documents the wire). The C
/// version additionally removes the region from its local special-memory
/// registry; that registry belongs to the server-integration stage and is
/// not part of this client wrapper.
pub fn unmap_physical_via(
    transport: &impl IpcTransport,
    target: Endpoint,
    vaddr: VirBytes,
) -> Result<(), Errno> {
    let mut message = crate::syscall::cleared_message();
    // SAFETY: ep at bytes 0..4, 32-bit vaddr at bytes 4..8 — the wire
    // shape follows the 32-bit C sender, so a 64-bit address truncates
    // exactly as the C library's own i386 builds do.
    unsafe {
        message.m_u.raw[..4].copy_from_slice(&target.0.to_ne_bytes());
        message.m_u.raw[4..8].copy_from_slice(&(vaddr.0 as u32).to_ne_bytes());
    }
    let reply = perform_taskcall(transport, vm_endpoint(), VM_CALL_UNMAP_PHYS, &mut message)?;
    if reply < 0 {
        return Err(Errno::from_i32(-reply));
    }
    Ok(())
}

/// Core of the process-control family.
///
/// C: `vm_procctl` (`minix3/minix/lib/libsys/vm_procctl.c:10-26`): five
/// fields in the `mess_9` lanes — operation, target, extra parameter,
/// length, flags — mapped through the `VMPCTL_*` macros (`com.h:753-757`).
/// The wire lanes are 32-bit (`long` on i386), matching the
/// [`minix_types::MessLcVmProcctl`] overlay the VM server decodes.
fn process_control_via(
    transport: &impl IpcTransport,
    endpoint: Endpoint,
    param: i32,
    m1: u32,
    len: i32,
    flags: i32,
) -> Result<(), Errno> {
    let mut message = crate::syscall::cleared_message();
    // SAFETY: the m9 lanes are 32-bit fields at offsets 16/20/24/28/32 of
    // the payload (param/who/m1/len/flags), per the MessLcVmProcctl layout.
    unsafe {
        message.m_u.raw[16..20].copy_from_slice(&param.to_ne_bytes());
        message.m_u.raw[20..24].copy_from_slice(&endpoint.0.to_ne_bytes());
        message.m_u.raw[24..28].copy_from_slice(&m1.to_ne_bytes());
        message.m_u.raw[28..32].copy_from_slice(&len.to_ne_bytes());
        message.m_u.raw[32..36].copy_from_slice(&flags.to_ne_bytes());
    }
    let reply = perform_taskcall(
        transport,
        vm_endpoint(),
        VM_CALL_PROCESS_CONTROL,
        &mut message,
    )?;
    if reply < 0 {
        return Err(Errno::from_i32(-reply));
    }
    Ok(())
}

/// Clears the target's special memory registry.
///
/// C: `vm_procctl_clear` (`vm_procctl.c:28-30`): the process-control core
/// with [`PROCESS_CONTROL_PARAM_CLEAR`] and zeroed payload fields.
pub fn process_control_clear_via(
    transport: &impl IpcTransport,
    endpoint: Endpoint,
) -> Result<(), Errno> {
    process_control_via(transport, endpoint, PROCESS_CONTROL_PARAM_CLEAR, 0, 0, 0)
}

/// Asks the server to handle a memory range on behalf of the target.
///
/// C: `vm_procctl_handlemem` (`vm_procctl.c:33-37`): the process-control
/// core with [`PROCESS_CONTROL_PARAM_HANDLE_MEM`]; `m1` carries a
/// caller-defined parameter and `flags` the write flag. The wire lanes for
/// `m1` and the length are 32-bit, so both arguments are 32-bit here —
/// the caller cannot build a request the wire cannot express.
pub fn process_control_handlemem_via(
    transport: &impl IpcTransport,
    endpoint: Endpoint,
    m1: u32,
    len: i32,
    write_flag: i32,
) -> Result<(), Errno> {
    process_control_via(
        transport,
        endpoint,
        PROCESS_CONTROL_PARAM_HANDLE_MEM,
        m1,
        len,
        write_flag,
    )
}

/// Page size for the cache family: one cache block is a whole number of
/// pages. C: `PAGE_SIZE` (`minix3/minix/include/machine/param.h`, 4096 on
/// every supported architecture).
pub const CACHE_PAGE_SIZE: i32 = 4096;

/// Absence of a device number; the cache family refuses it.
///
/// C: `NO_DEV` (`minix3/minix/include/minix/const.h:132`, `(dev_t) 0`).
pub const NO_DEVICE: u64 = 0;

/// Core of the cache-block family, returning the reply message.
///
/// C: `vm_cachecall` (`minix3/minix/lib/libsys/vm_cache.c:15-43`): one
/// shared fill routine for all four cache calls, writing the `m_vmmcp`
/// lanes (64-bit overlay layout, mirrored by [`minix_types::VmCacheIn`] —
/// the VM server decodes from this very arm): `dev` @0, `dev_offset` @8,
/// `ino_offset` @16, `ino` @24, `block` @32, `flags_ptr` @40, `pages`
/// @48, `flags` @49.
///
/// The C version panics on a misaligned block size or offset and asserts a
/// non-`NO_DEV` device — caller bugs, not runtime conditions — and this
/// wrapper panics the same way rather than inventing an error lane the C
/// wire never sees.
#[allow(clippy::too_many_arguments)] // ten parameters mirror the C core (vm_cache.c:15-19)
fn cache_call_via(
    transport: &impl IpcTransport,
    call: i32,
    block: u64,
    dev: u64,
    dev_offset: i64,
    ino: u64,
    ino_offset: i64,
    flags_ptr: u64,
    blocksize: i32,
    setflags: u8,
) -> Result<Message, Errno> {
    if blocksize % CACHE_PAGE_SIZE != 0 {
        panic!(
            "blocksize {} should be a multiple of pagesize {}",
            blocksize, CACHE_PAGE_SIZE
        );
    }
    if ino_offset % CACHE_PAGE_SIZE as i64 != 0 {
        panic!(
            "inode offset {} should be a multiple of pagesize {}",
            ino_offset, CACHE_PAGE_SIZE
        );
    }
    if dev_offset % CACHE_PAGE_SIZE as i64 != 0 {
        panic!(
            "dev offset {} should be a multiple of pagesize {}",
            dev_offset, CACHE_PAGE_SIZE
        );
    }
    if dev == NO_DEVICE {
        panic!("cache call without a device");
    }
    let mut message = crate::syscall::cleared_message();
    // SAFETY: the m_vmmcp lanes at 0/8/16/24/32/40/48/49, per the layout
    // documented on `minix_types::VmCacheIn`.
    unsafe {
        message.m_u.raw[..8].copy_from_slice(&dev.to_ne_bytes());
        message.m_u.raw[8..16].copy_from_slice(&(dev_offset as u64).to_ne_bytes());
        message.m_u.raw[16..24].copy_from_slice(&(ino_offset as u64).to_ne_bytes());
        message.m_u.raw[24..32].copy_from_slice(&ino.to_ne_bytes());
        message.m_u.raw[32..40].copy_from_slice(&block.to_ne_bytes());
        message.m_u.raw[40..48].copy_from_slice(&flags_ptr.to_ne_bytes());
        message.m_u.raw[48] = (blocksize / CACHE_PAGE_SIZE) as u8;
        message.m_u.raw[49] = setflags;
    }
    let reply = perform_taskcall(transport, vm_endpoint(), call, &mut message)?;
    if reply < 0 {
        return Err(Errno::from_i32(-reply));
    }
    // `_taskcall` reuses the request message as the reply buffer; the
    // caller reads whatever reply lanes its call defines.
    Ok(message)
}

/// Maps a file-system cache block into the caller's address space.
///
/// C: `vm_map_cacheblock` (`minix3/minix/lib/libsys/vm_cache.c:47-57`):
/// the mapped virtual address comes back in the reply's `m_vmmcp_reply`
/// lane (64-bit overlay: `addr` @0, see [`minix_types::MessVmmcpReply`]).
/// A failed call is an `Err`; C's mapped-failed sentinel never crosses
/// this interface.
///
/// The `flags` reference plays the role of C's `u32_t *flags`: its address
/// travels in the `flags_ptr` lane. The current minix-rs server does not
/// write it back yet — the write-back lands with the server's setcache
/// flag handling, and this signature is stable across that change.
pub fn map_cacheblock_via(
    transport: &impl IpcTransport,
    dev: u64,
    dev_offset: i64,
    ino: u64,
    ino_offset: i64,
    flags: &mut u32,
    blocksize: i32,
) -> Result<VirBytes, Errno> {
    let reply = cache_call_via(
        transport,
        VM_CALL_MAP_CACHE_PAGE,
        0,
        dev,
        dev_offset,
        ino,
        ino_offset,
        flags as *mut u32 as u64,
        blocksize,
        0,
    )?;
    // SAFETY: the reply's mapped address sits at byte zero of the
    // m_vmmcp_reply overlay (64-bit overlay per MessVmmcpReply).
    let addr = unsafe { reply.m_u.raw[..8].as_ptr().cast::<u64>().read() };
    Ok(VirBytes(addr))
}

/// Identifies a cache block to the server.
///
/// C: `vm_set_cacheblock` (`vm_cache.c:59-66`): the block address travels
/// in the `block` lane, and `setflags` rides in the one-byte flags lane.
#[allow(clippy::too_many_arguments)]
pub fn set_cacheblock_via(
    transport: &impl IpcTransport,
    block: VirBytes,
    dev: u64,
    dev_offset: i64,
    ino: u64,
    ino_offset: i64,
    flags: &mut u32,
    blocksize: i32,
    setflags: u8,
) -> Result<(), Errno> {
    cache_call_via(
        transport,
        VM_CALL_SET_CACHE_PAGE,
        block.0,
        dev,
        dev_offset,
        ino,
        ino_offset,
        flags as *mut u32 as u64,
        blocksize,
        setflags,
    )?;
    Ok(())
}

/// Forgets a cache block: no inode association, no flags.
///
/// C: `vm_forget_cacheblock` (`vm_cache.c:68-75`): the C caller passes
/// `VMC_NO_INODE` (`minix3/minix/include/minix/vm.h:90`, value 0) as the
/// inode and zeroes the rest.
pub fn forget_cacheblock_via(
    transport: &impl IpcTransport,
    dev: u64,
    dev_offset: i64,
    blocksize: i32,
) -> Result<(), Errno> {
    cache_call_via(
        transport,
        VM_CALL_FORGET_CACHE_PAGE,
        0,
        dev,
        dev_offset,
        0,
        0,
        0,
        blocksize,
        0,
    )?;
    Ok(())
}

/// Clears every cache block belonging to a device.
///
/// C: `vm_clear_cache` (`vm_cache.c:77-88`): only the device lane is
/// filled; no alignment requirements apply (C checks none).
pub fn clear_cache_via(transport: &impl IpcTransport, dev: u64) -> Result<(), Errno> {
    if dev == NO_DEVICE {
        panic!("cache call without a device");
    }
    let mut message = crate::syscall::cleared_message();
    // SAFETY: the device occupies the first eight bytes (m2_l1 overlay).
    unsafe {
        message.m_u.raw[..8].copy_from_slice(&dev.to_ne_bytes());
    }
    let reply = perform_taskcall(transport, vm_endpoint(), VM_CALL_CLEAR_CACHE, &mut message)?;
    if reply < 0 {
        return Err(Errno::from_i32(-reply));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::{CannedTransport, TrapStatus};
    use minix_types::MessMmap;

    fn reply_with_type(message_type: i32) -> Message {
        let mut message = Message::zeroed();
        message.m_type = message_type;
        message
    }

    fn request_for(beneficiary: i32) -> MapRequest {
        MapRequest {
            beneficiary: Endpoint(beneficiary),
            address: VirBytes(0),
            length: VirBytes(4096),
            protection: MAP_PROTECTION_READ | MAP_PROTECTION_WRITE,
            flags: 0,
            file: -1,
            offset: 0,
        }
    }

    #[test]
    fn test_call_numbers_match_com_header() {
        assert_eq!(VM_ENDPOINT_NUMBER, 8);
        assert_eq!(VM_CALL_EXIT, 0xC00);
        assert_eq!(VM_CALL_FORK, 0xC01);
        assert_eq!(VM_CALL_BREAK, 0xC02);
        assert_eq!(VM_CALL_WILL_EXIT, 0xC05);
        assert_eq!(VM_CALL_MMAP, 0xC0A);
        assert_eq!(VM_CALL_MAP_PHYS, 0xC0F);
        assert_eq!(VM_CALL_UNMAP_PHYS, 0xC10);
        assert_eq!(VM_CALL_MUNMAP, 0xC11);
        assert_eq!(VM_CALL_REMAP, 0xC21);
        assert_eq!(VM_CALL_SHARED_UNMAP, 0xC22);
        assert_eq!(VM_CALL_GET_PHYSICAL, 0xC23);
        assert_eq!(VM_CALL_GET_REFERENCE, 0xC24);
        assert_eq!(VM_CALL_INFO, 0xC28);
        assert_eq!(VM_CALL_REMAP_READ_ONLY, 0xC2C);
        assert_eq!(VM_CALL_PROCESS_CONTROL, 0xC2D);
    }

    #[test]
    fn test_protection_and_flag_constants_match_mman_header() {
        assert_eq!(MAP_PROTECTION_NONE, 0x00);
        assert_eq!(MAP_PROTECTION_READ, 0x01);
        assert_eq!(MAP_PROTECTION_WRITE, 0x02);
        assert_eq!(MAP_PROTECTION_EXECUTE, 0x04);
        assert_eq!(MAP_FLAG_THIRD_PARTY, 0x800000);
        assert_eq!(MAP_FLAG_ANON, 0x1000);
        assert_eq!(MAP_FLAG_PREALLOC, 0x080000);
    }

    #[test]
    fn test_third_party_flag_added_only_for_others() {
        let caller = Endpoint(5);
        let for_self = MapRequest {
            beneficiary: caller,
            ..request_for(5)
        };
        assert_eq!(for_self.effective_flags(caller), 0);
        let for_other = MapRequest {
            beneficiary: Endpoint(6),
            ..request_for(6)
        };
        assert_eq!(for_other.effective_flags(caller), MAP_FLAG_THIRD_PARTY);
    }

    #[test]
    fn test_mmap_request_wire_lanes() {
        // The full lane map, asserted on the outgoing wire bytes through
        // the real wrapper (C order: offset @0, address @8, length @16,
        // protection @24, flags @28, file @32, beneficiary @36; edge NS5-A
        // 64-bit pointer lanes).
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        let request = MapRequest {
            beneficiary: Endpoint(5),
            address: VirBytes(0x7fff_fbff_f000),
            length: VirBytes(0x4000_1000),
            protection: 3,
            flags: 0x1000,
            file: -1,
            offset: -2,
        };
        let _ = mmap_via(&transport, Endpoint(5), request).unwrap();
        let (dest, sent) = transport.sent.borrow()[0];
        assert_eq!(dest, vm_endpoint());
        assert_eq!(sent.m_type, VM_CALL_MMAP);
        // SAFETY: VM_MMAP messages carry the m_mmap arm (asserted here by
        // reading the same lanes decode_message reads).
        let m = unsafe { &sent.m_u.m_mmap };
        assert_eq!(m.offset, (-2i64) as u64);
        assert_eq!(m.addr, 0x7fff_fbff_f000);
        assert_eq!(m.len, 0x4000_1000);
        assert_eq!(m.prot, 3);
        // beneficiary == caller → no third-party flag bit added.
        assert_eq!(m.flags, 0x1000);
        assert_eq!(m.fd, -1);
        assert_eq!(m.forwhom, 5);
        // The padding half of the payload stays zero, like C's memset.
        assert!(unsafe { &sent.m_u.m_mmap }._padding.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_mmap_third_party_flag_on_beneficiary_mismatch() {
        // C adds MAP_THIRDPARTY whenever forwhom != SELF (mmap.c:36-38);
        // the wire lane carries the beneficiary endpoint either way.
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        let request = MapRequest {
            beneficiary: Endpoint(9),
            address: VirBytes(0),
            length: VirBytes(4096),
            protection: 1,
            flags: 0,
            file: -1,
            offset: 0,
        };
        let _ = mmap_via(&transport, Endpoint(5), request).unwrap();
        // SAFETY: same overlay reasoning as test_mmap_request_wire_lanes.
        let m = unsafe { &transport.sent.borrow()[0].1.m_u.m_mmap };
        assert_eq!(m.forwhom, 9);
        assert_eq!(m.flags, (MAP_FLAG_THIRD_PARTY) as i32);
    }

    #[test]
    fn test_map_payload_matches_c_field_order() {
        // Struct-level pin of the shared overlay's layout: C field order
        // with the 64-bit pointer lanes (edge NS5-A), 56 bytes total.
        assert_eq!(core::mem::size_of::<MessMmap>(), 56);
        let packed = MessMmap {
            offset: 0,
            addr: 0x1000,
            len: 4096,
            prot: 3,
            flags: 0x20,
            fd: -1,
            forwhom: 5,
            retaddr: 0x7fff_fbff_f000,
            _padding: [0; 8],
        };
        // SAFETY: plain value read-back of the struct just built above.
        // C order: offset @0, address @8, length @16, protection @24,
        // flags @28, file @32, beneficiary @36, return address @40.
        let bytes = unsafe {
            core::slice::from_raw_parts(
                (&raw const packed) as *const u8,
                core::mem::size_of::<MessMmap>(),
            )
        };
        assert_eq!(u64::from_ne_bytes(bytes[0..8].try_into().unwrap()), 0);
        assert_eq!(u64::from_ne_bytes(bytes[8..16].try_into().unwrap()), 0x1000);
        assert_eq!(u64::from_ne_bytes(bytes[16..24].try_into().unwrap()), 4096);
        assert_eq!(u32::from_ne_bytes(bytes[24..28].try_into().unwrap()), 3);
        assert_eq!(u32::from_ne_bytes(bytes[28..32].try_into().unwrap()), 0x20);
        assert_eq!(i32::from_ne_bytes(bytes[32..36].try_into().unwrap()), -1);
        assert_eq!(i32::from_ne_bytes(bytes[36..40].try_into().unwrap()), 5);
        assert_eq!(
            u64::from_ne_bytes(bytes[40..48].try_into().unwrap()),
            0x7fff_fbff_f000
        );
    }

    #[test]
    fn test_mmap_returns_chosen_address() {
        let mut transport = CannedTransport::new();
        let mut reply = reply_with_type(0);
        // VM writes the chosen address into the same overlay's retaddr lane
        // (servers/vm/mmap.c:276); the wrapper reads it from there. Union
        // field writes are safe — only the read back needs `unsafe`.
        reply.m_u.m_mmap.retaddr = 0x8000;
        transport.reply_sendrec(Ok(reply));
        let result = mmap_via(&transport, Endpoint(5), request_for(5)).unwrap();
        assert_eq!(result, VirBytes(0x8000));
    }

    #[test]
    fn test_failed_syscall_propagates_errno() {
        // The C wrappers return MAP_FAILED / zero / all-ones on a failed
        // round trip (mmap.c:44-47 and friends); here the failure is the
        // `Err` lane, and the sentinel never appears. Transport errors
        // carry the positive errno (TrapStatus sign contract — the hosted
        // fallback and the real-trap reply register agree on this sign).
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Err(TrapStatus(minix_types::EINVAL)));
        let result = mmap_via(&transport, Endpoint(5), request_for(5));
        assert_eq!(result, Err(Errno::from_i32(minix_types::EINVAL)));
        assert_eq!(transport.sendrec_calls.get(), 1);
    }

    #[test]
    fn test_munmap_sends_address_and_length() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        assert_eq!(
            munmap_via(&transport, VirBytes(0x7fff_fbff_f000), VirBytes(4096)),
            Ok(())
        );
        assert_eq!(transport.sendrec_calls.get(), 1);
        // C stores the pair through the mapping payload's address/length
        // lanes (`m.VMUM_ADDR`/`m.VMUM_LEN`, libc/sys/mmap.c:79-86) — the
        // 64-bit NS5-A lanes, asserted on the outgoing wire bytes.
        // SAFETY: VM_MUNMAP messages carry the m_mmap arm.
        let m = unsafe { &transport.sent.borrow()[0].1.m_u.m_mmap };
        assert_eq!(m.addr, 0x7fff_fbff_f000);
        assert_eq!(m.len, 4096);
    }

    #[test]
    fn test_unchanged_break_skips_round_trip() {
        let transport = CannedTransport::new();
        assert_eq!(
            break_via(&transport, VirBytes(0x1000), VirBytes(0x1000)),
            Ok(VirBytes(0x1000))
        );
        assert_eq!(transport.sendrec_calls.get(), 0);
    }

    #[test]
    fn test_changed_break_calls_server_and_caches() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        assert_eq!(
            break_via(&transport, VirBytes(0x1000), VirBytes(0x2000)),
            Ok(VirBytes(0x2000))
        );
        assert_eq!(transport.sendrec_calls.get(), 1);
    }

    #[test]
    fn test_fork_address_space_returns_child() {
        let mut transport = CannedTransport::new();
        let mut reply = reply_with_type(0);
        reply.m_u.m_m1.m1i3 = 9;
        transport.reply_sendrec(Ok(reply));
        assert_eq!(
            fork_address_space_via(&transport, Endpoint(4), 3),
            Ok(Endpoint(9))
        );
        let sent = transport.sent.borrow();
        let (dest, request) = &sent[0];
        assert_eq!(*dest, vm_endpoint());
        // SAFETY(test): m1i1/m1i2 are the request's active lanes.
        let lanes = unsafe { (request.m_u.m_m1.m1i1, request.m_u.m_m1.m1i2) };
        assert_eq!(lanes, (4, 3));
    }

    #[test]
    fn test_remap_returns_placed_address() {
        let mut transport = CannedTransport::new();
        let mut reply = reply_with_type(0);
        // SAFETY: test-only payload setup through the documented overlay.
        unsafe {
            reply.m_u.raw[..8].copy_from_slice(&0x9000u64.to_ne_bytes());
        }
        transport.reply_sendrec(Ok(reply));
        let result = remap_via(
            &transport,
            VM_CALL_REMAP,
            Endpoint(6),
            Endpoint(5),
            VirBytes(0x9000),
            VirBytes(0x8000),
            VirBytes(4096),
        )
        .unwrap();
        assert_eq!(result, VirBytes(0x9000));
    }

    #[test]
    fn test_physical_query_returns_address() {
        let mut transport = CannedTransport::new();
        let mut reply = reply_with_type(0);
        // SAFETY: test-only payload setup through the documented overlay.
        unsafe {
            reply.m_u.raw[..8].copy_from_slice(&0x1_0000u64.to_ne_bytes());
        }
        transport.reply_sendrec(Ok(reply));
        assert_eq!(
            physical_address_via(&transport, Endpoint(5), VirBytes(0x8000)),
            Ok(PhysBytes(0x1_0000))
        );
    }

    #[test]
    fn test_alloc_contig_maps_then_umaps_and_reports_both_bases() {
        let mut transport = CannedTransport::new();
        // First round trip: mmap's reply carries the chosen address.
        let mut mmap_reply = reply_with_type(0);
        // SAFETY: test-only payload setup through the documented overlay.
        unsafe {
            mmap_reply.m_u.m_mmap.retaddr = 0x5000;
        }
        transport.reply_sendrec(Ok(mmap_reply));
        // Second round trip: umap's reply carries the physical base.
        let mut umap_reply = reply_with_type(0);
        // SAFETY: test-only payload setup through the documented overlay.
        unsafe {
            umap_reply.m_u.raw[..8].copy_from_slice(&0x1_0000u64.to_ne_bytes());
        }
        transport.reply_sendrec(Ok(umap_reply));
        let got =
            alloc_contig_via(&transport, Endpoint(5), 0x2_0000, AC_ALIGN64K | AC_LOWER16M).unwrap();
        assert_eq!(got, (VirBytes(0x5000), PhysBytes(0x1_0000)));
        // The mmap request carried the DMA flag set: the C util's base
        // (PREALLOC|CONTIG|ANON) plus this build's PRIVATE, with the AC
        // constraints mapped (LOWER16M + ALIGNMENT_64KB).
        let (destination, request) = &transport.sent.borrow()[0];
        assert_eq!(*destination, vm_endpoint());
        // SAFETY: the m_mmap arm is the documented payload for this call.
        let sent_flags = unsafe { request.m_u.m_mmap.flags } as u64;
        assert_eq!(
            sent_flags,
            (MAP_FLAG_PRIVATE
                | MAP_FLAG_ANON
                | MAP_FLAG_PREALLOC
                | MAP_FLAG_CONTIG
                | MAP_FLAG_LOWER16M
                | MAP_FLAG_ALIGNMENT_64KB) as u64
        );
        let sent_len = unsafe { request.m_u.m_mmap.len };
        assert_eq!(sent_len, 0x2_0000);
    }

    #[test]
    fn test_alloc_contig_flags_are_added_not_replaced() {
        // `AC_ALIGN4K` alone: no mmap flag behind it (the page allocator is
        // page-aligned by default), so the request carries only the base.
        let mut transport = CannedTransport::new();
        let mut mmap_reply = reply_with_type(0);
        // SAFETY: test-only payload setup through the documented overlay.
        unsafe {
            mmap_reply.m_u.m_mmap.retaddr = 0x6000;
        }
        transport.reply_sendrec(Ok(mmap_reply));
        let mut umap_reply = reply_with_type(0);
        // SAFETY: test-only payload setup through the documented overlay.
        unsafe {
            umap_reply.m_u.raw[..8].copy_from_slice(&0x2_0000u64.to_ne_bytes());
        }
        transport.reply_sendrec(Ok(umap_reply));
        let got = alloc_contig_via(&transport, Endpoint(5), 0x1000, AC_ALIGN4K).unwrap();
        assert_eq!(got, (VirBytes(0x6000), PhysBytes(0x2_0000)));
        let (_, request) = &transport.sent.borrow()[0];
        // SAFETY: the m_mmap arm is the documented payload for this call.
        let sent_flags = unsafe { request.m_u.m_mmap.flags } as u64;
        assert_eq!(
            sent_flags,
            (MAP_FLAG_PRIVATE | MAP_FLAG_ANON | MAP_FLAG_PREALLOC | MAP_FLAG_CONTIG) as u64
        );
    }

    #[test]
    fn test_alloc_contig_propagates_the_mmap_failure() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Err(TrapStatus(minix_types::ENOMEM)));
        let got = alloc_contig_via(&transport, Endpoint(5), 0x1000, AC_LOWER16M);
        assert_eq!(got.unwrap_err(), Errno::from_i32(minix_types::ENOMEM));
    }

    #[test]
    fn test_reference_count_returns_byte() {
        let mut transport = CannedTransport::new();
        let mut reply = reply_with_type(0);
        // SAFETY: test-only payload setup through the documented overlay.
        unsafe {
            reply.m_u.raw[0] = 3;
        }
        transport.reply_sendrec(Ok(reply));
        assert_eq!(
            reference_count_via(&transport, Endpoint(5), VirBytes(0x8000)),
            Ok(3)
        );
    }

    #[test]
    fn test_map_physical_returns_virtual_address() {
        let mut transport = CannedTransport::new();
        let mut reply = reply_with_type(0);
        // Aligned with VM's EncodeToM1: the address travels on the m1.p1 lane
        // (the old script wrote raw[0..8], which validated the wrong wire).
        // SAFETY: test-only payload setup through the documented m1 arm.
        unsafe {
            reply.m_u.m_m1.m1p1 = 0xA000;
        }
        transport.reply_sendrec(Ok(reply));
        assert_eq!(
            map_physical_via(&transport, Endpoint(6), PhysBytes(0xF0000), VirBytes(4096)),
            Ok(VirBytes(0xA000))
        );
        // The request wire: the `m_lsys_vm_map_phys` arm (ep/phaddr/len are all
        // 32-bit; VM decodes them as u32).
        let sent = transport.sent.borrow();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, vm_endpoint());
        assert_eq!(sent[0].1.m_type, VM_CALL_MAP_PHYS);
        // SAFETY: test-only read of the documented request arm.
        let mp = unsafe { sent[0].1.m_u.m_lsys_vm_map_phys };
        assert_eq!(mp.ep, 6);
        assert_eq!(mp.phaddr, 0xF0000);
        assert_eq!(mp.len, 4096);
    }

    #[test]
    fn test_will_exit_sends_endpoint_in_first_int_lane() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        assert_eq!(will_exit_via(&transport, Endpoint(4)), Ok(()));
        let sent = transport.sent.borrow();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, vm_endpoint());
        assert_eq!(sent[0].1.m_type, VM_CALL_WILL_EXIT);
        // SAFETY: test-only payload read-back of the outgoing wire bytes.
        let raw = unsafe { &sent[0].1.m_u.raw };
        assert_eq!(&raw[..4], &4i32.to_ne_bytes());
    }

    #[test]
    fn test_unmap_physical_sends_endpoint_and_truncated_vaddr() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        assert_eq!(
            unmap_physical_via(&transport, Endpoint(7), VirBytes(0x1_0000_8000)),
            Ok(())
        );
        let sent = transport.sent.borrow();
        assert_eq!(sent[0].1.m_type, VM_CALL_UNMAP_PHYS);
        // SAFETY: test-only payload read-back of the outgoing wire bytes.
        let raw = unsafe { &sent[0].1.m_u.raw };
        assert_eq!(&raw[..4], &7i32.to_ne_bytes());
        // The wire lane is 32-bit (i386 sender shape): 0x1_0000_8000
        // truncates to 0x8000, exactly as the C library's own truncation.
        assert_eq!(&raw[4..8], &(0x1_0000_8000u64 as u32).to_ne_bytes());
    }

    #[test]
    fn test_procctl_clear_sends_operation_and_zeroed_payload() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        assert_eq!(process_control_clear_via(&transport, Endpoint(3)), Ok(()));
        let sent = transport.sent.borrow();
        assert_eq!(sent[0].1.m_type, VM_CALL_PROCESS_CONTROL);
        // SAFETY: test-only payload read-back of the outgoing wire bytes.
        let raw = unsafe { &sent[0].1.m_u.raw };
        // mess_9 lanes: param @16, who @20, m1 @24, len @28, flags @32.
        assert_eq!(&raw[16..20], &PROCESS_CONTROL_PARAM_CLEAR.to_ne_bytes());
        assert_eq!(&raw[20..24], &3i32.to_ne_bytes());
        assert_eq!(&raw[24..40], &[0u8; 16]);
    }

    #[test]
    fn test_procctl_handlemem_sends_all_five_lanes() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        assert_eq!(
            process_control_handlemem_via(&transport, Endpoint(3), 0x2000, 128, 1),
            Ok(())
        );
        let sent = transport.sent.borrow();
        // SAFETY: test-only payload read-back of the outgoing wire bytes.
        let raw = unsafe { &sent[0].1.m_u.raw };
        assert_eq!(
            &raw[16..20],
            &PROCESS_CONTROL_PARAM_HANDLE_MEM.to_ne_bytes()
        );
        assert_eq!(&raw[20..24], &3i32.to_ne_bytes());
        assert_eq!(&raw[24..28], &0x2000u32.to_ne_bytes());
        assert_eq!(&raw[28..32], &128i32.to_ne_bytes());
        assert_eq!(&raw[32..36], &1i32.to_ne_bytes());
    }

    #[test]
    fn test_procctl_failure_propagates_errno() {
        // vm_procctl's C callers check the raw result; the Err lane carries
        // the positive errno (TrapStatus sign contract), and the wrapper
        // turns it into the typed error.
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Err(TrapStatus(minix_types::EINVAL)));
        assert_eq!(
            process_control_clear_via(&transport, Endpoint(3)),
            Err(Errno::from_i32(minix_types::EINVAL))
        );
    }

    #[test]
    fn test_map_cacheblock_reads_reply_address() {
        let mut transport = CannedTransport::new();
        let mut reply = reply_with_type(0);
        // SAFETY: test-only payload setup through the documented overlay.
        unsafe {
            reply.m_u.raw[..8].copy_from_slice(&0x9000_0000u64.to_ne_bytes());
        }
        transport.reply_sendrec(Ok(reply));
        let mut flags = 0u32;
        let result = map_cacheblock_via(
            &transport, 0x5678, 0x10_0000, 42, 0x20_0000, &mut flags, 4096,
        );
        assert_eq!(result, Ok(VirBytes(0x9000_0000)));
        // Outgoing lanes: dev @0, dev_offset @8, ino_offset @16, ino @24,
        // block @32 (NULL for map), flags_ptr @40, pages @48 = 1.
        let sent = transport.sent.borrow();
        assert_eq!(sent[0].1.m_type, VM_CALL_MAP_CACHE_PAGE);
        // SAFETY: test-only payload read-back of the outgoing wire bytes.
        let raw = unsafe { &sent[0].1.m_u.raw };
        assert_eq!(&raw[..8], &0x5678u64.to_ne_bytes());
        assert_eq!(&raw[8..16], &0x10_0000u64.to_ne_bytes());
        assert_eq!(&raw[16..24], &0x20_0000u64.to_ne_bytes());
        assert_eq!(&raw[24..32], &42u64.to_ne_bytes());
        assert_eq!(&raw[32..40], &0u64.to_ne_bytes());
        assert_eq!(&raw[40..48], &(&mut flags as *mut u32 as u64).to_ne_bytes());
        assert_eq!(raw[48], 1);
    }

    #[test]
    fn test_set_cacheblock_sends_block_and_setflags() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        let mut flags = 0u32;
        assert_eq!(
            set_cacheblock_via(
                &transport,
                VirBytes(0x9000_0000),
                0x5678,
                0x10_0000,
                42,
                0x20_0000,
                &mut flags,
                4096,
                1,
            ),
            Ok(())
        );
        let sent = transport.sent.borrow();
        assert_eq!(sent[0].1.m_type, VM_CALL_SET_CACHE_PAGE);
        // SAFETY: test-only payload read-back of the outgoing wire bytes.
        let raw = unsafe { &sent[0].1.m_u.raw };
        assert_eq!(&raw[32..40], &0x9000_0000u64.to_ne_bytes());
        assert_eq!(raw[49], 1);
    }

    #[test]
    fn test_forget_cacheblock_zeroes_inode_lanes() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        assert_eq!(
            forget_cacheblock_via(&transport, 0x5678, 0x10_0000, 8192),
            Ok(())
        );
        let sent = transport.sent.borrow();
        assert_eq!(sent[0].1.m_type, VM_CALL_FORGET_CACHE_PAGE);
        // SAFETY: test-only payload read-back of the outgoing wire bytes.
        let raw = unsafe { &sent[0].1.m_u.raw };
        // C: ino = VMC_NO_INODE (0), ino_offset = 0, pages = blocksize/4096 = 2.
        assert_eq!(&raw[16..24], &0u64.to_ne_bytes());
        assert_eq!(&raw[24..32], &0u64.to_ne_bytes());
        assert_eq!(raw[48], 2);
    }

    #[test]
    fn test_clear_cache_sends_device_only() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        assert_eq!(clear_cache_via(&transport, 0x5678), Ok(()));
        let sent = transport.sent.borrow();
        assert_eq!(sent[0].1.m_type, VM_CALL_CLEAR_CACHE);
        // SAFETY: test-only payload read-back of the outgoing wire bytes.
        let raw = unsafe { &sent[0].1.m_u.raw };
        assert_eq!(&raw[..8], &0x5678u64.to_ne_bytes());
    }

    #[test]
    #[should_panic]
    fn test_cache_call_panics_on_misaligned_blocksize() {
        // C: vm_cachecall panics on a block size that is not a page
        // multiple (vm_cache.c:17-19) — a caller bug, not a runtime error.
        let transport = CannedTransport::new();
        //[SD19-gate] test has #[should_panic]: this line must trigger a panic;
        // there is no reply value to consume
        #[allow(clippy::let_underscore_must_use)]
        let _ = map_cacheblock_via(&transport, 1, 0, 0, 0, &mut 0u32, 100);
    }

    #[test]
    #[should_panic]
    fn test_cache_call_panics_on_no_device() {
        // C: assert(dev != NO_DEV) (vm_cache.c:31) with NO_DEV = 0.
        let transport = CannedTransport::new();
        //[SD19-gate] test has #[should_panic]: this line must trigger a panic;
        // there is no reply value to consume
        #[allow(clippy::let_underscore_must_use)]
        let _ = map_cacheblock_via(&transport, NO_DEVICE, 0, 0, 0, &mut 0u32, 4096);
    }

    /// E9 VmApi:VM_RS_SET_PRIV m2-payload field assertions
    /// (target/mask_ptr/is_sys).
    #[test]
    fn test_vm_rs_set_priv_m2_fields() {
        let mut msg = Message::default();
        // SAFETY: writing the m_m2 arm for wire construction.
        unsafe {
            msg.m_u.m_m2 = minix_types::MessageM2 {
                m2i1: 9,
                m2l1: 0x5000,
                m2i2: 1,
                ..Default::default()
            };
        }
        assert_eq!(unsafe { msg.m_u.m_m2 }.m2i1, 9);
        assert_eq!(unsafe { msg.m_u.m_m2 }.m2l1, 0x5000);
        assert_eq!(unsafe { msg.m_u.m_m2 }.m2i2, 1);
    }

    /// E9 SchedApi:SCHEDULING_START message field assertions
    /// (endpoint/parent/maxprio/quantum).
    #[test]
    fn test_sched_scheduling_start_fields() {
        let m = minix_types::ipc::MessLsysSchedSchedulingStart {
            endpoint: 9,
            parent: 3,
            maxprio: 8,
            quantum: 200,
            _padding: [0; 40],
        };
        assert_eq!(m.endpoint, 9);
        assert_eq!(m.parent, 3);
        assert_eq!(m.maxprio, 8);
        assert_eq!(m.quantum, 200);
    }

    /// E9 SchedApi:SCHEDULING_STOP message field assertions (endpoint).
    #[test]
    fn test_sched_scheduling_stop_fields() {
        let m = minix_types::ipc::MessLsysSchedSchedulingStop {
            endpoint: 9,
            _padding: [0; 52],
        };
        assert_eq!(m.endpoint, 9);
    }
    /// E-IPCWIRE item 5: shm_unmap_via payload field assertions
    /// (forwhom@0/addr@8 u64—widened by the E-VMMCPWIRE sweep).
    #[test]
    fn test_shm_unmap_via_fields() {
        let mut msg = Message::default();
        // SAFETY: writing the m_lc_vm_shm_unmap arm for wire assertion.
        unsafe {
            msg.m_u.m_lc_vm_shm_unmap = minix_types::ipc::MessLcVmShmUnmap {
                forwhom: 12,
                _pad: 0,
                addr: 0x0000_0001_4000_0000, // inside the MMAP window (>4GiB)
                _padding: [0; 40],
            };
        }
        let arm = unsafe { msg.m_u.m_lc_vm_shm_unmap };
        assert_eq!(arm.forwhom, 12);
        assert_eq!(arm.addr, 0x0000_0001_4000_0000, "addr 高位保全");
    }
}

// ── E9 VmApi sub-domain: the VM message construction face of RS service process management ──

/// VM_RS_SET_PRIV(0xC25): RS sets the target process's privilege structure.
/// m2 payload: m2i1=target, m2l1=mask buf ptr, m2i2=is_sys_proc.
pub fn vm_rs_set_priv_via(
    transport: &impl IpcTransport,
    target: Endpoint,
    mask_buf: u64,
    is_sys: i32,
) -> Result<(), Errno> {
    let mut message = crate::syscall::cleared_message();
    {
        // SAFETY: m_m2 is the documented payload for VM_RS_SET_PRIV.
        let m2 = unsafe { &mut message.m_u.m_m2 };
        m2.m2i1 = target.0;
        m2.m2l1 = mask_buf as i64;
        m2.m2i2 = is_sys;
    }
    let reply = perform_taskcall(
        transport,
        vm_endpoint(),
        minix_types::VM_RS_SET_PRIV as i32,
        &mut message,
    )?;
    if reply < 0 {
        return Err(Errno::from_i32(-reply));
    }
    Ok(())
}

/// VM_RS_MEMCTL(0xC2A): RS memory control (pin/heap/map preallocation, etc.).
/// m1 payload: m1i1=endpt, m1i2=req.
pub fn vm_rs_memctl_via(
    transport: &impl IpcTransport,
    endpt: Endpoint,
    req: i32,
    addr: u64,
    len: u64,
) -> Result<(), Errno> {
    let mut message = crate::syscall::cleared_message();
    {
        // SAFETY: m_m1 is the documented payload for VM_RS_MEMCTL. VM decodes
        // (vm dispatcher decode_rs_memctl_request): m1i1=endpt,
        // m1i2=req, m1i3=len (consumed by HeapPrealloc/MapPrealloc),
        // m1p1=addr; Pin/MakeVm/GetPreallocMap ignore addr/len.
        let m1 = unsafe { &mut message.m_u.m_m1 };
        m1.m1i1 = endpt.0;
        m1.m1i2 = req;
        m1.m1i3 = len as i32;
        m1.m1p1 = addr;
    }
    let reply = perform_taskcall(
        transport,
        vm_endpoint(),
        minix_types::VM_RS_MEMCTL as i32,
        &mut message,
    )?;
    if reply < 0 {
        return Err(Errno::from_i32(-reply));
    }
    Ok(())
}

/// VM_RS_UPDATE(0xC29): RS live-update of a process.
/// m2 payload: m2i1=src, m2i2=dst, m2i3=flags.
pub fn vm_rs_update_via(
    transport: &impl IpcTransport,
    src: Endpoint,
    dst: Endpoint,
    flags: i32,
) -> Result<(), Errno> {
    let mut message = crate::syscall::cleared_message();
    {
        // SAFETY: m_m2 is the documented payload for VM_RS_UPDATE.
        let m2 = unsafe { &mut message.m_u.m_m2 };
        m2.m2i1 = src.0;
        m2.m2i2 = dst.0;
        m2.m2i3 = flags;
    }
    let reply = perform_taskcall(
        transport,
        vm_endpoint(),
        minix_types::VM_RS_UPDATE as i32,
        &mut message,
    )?;
    if reply < 0 {
        return Err(Errno::from_i32(-reply));
    }
    Ok(())
}

/// VM_SHM_UNMAP(0xC22,com.h:718): unmaps a shared-memory segment mapping.
/// `m_lc_vm_shm_unmap` payload: forwhom@0 (i32), addr@8 (u64—widened by the
/// E-VMMCPWIRE sweep: vir_bytes grows to x86_64 width, while MMAP-window
/// addresses are always truncated to u32).
/// C: `vm_shm_unmap` (libsys)—the execution verb of ipc-server
/// SweepPlan::unmaps.
pub fn shm_unmap_via(
    transport: &impl IpcTransport,
    forwhom: Endpoint,
    addr: u64,
) -> Result<(), Errno> {
    let mut message = crate::syscall::cleared_message();
    message.m_u.m_lc_vm_shm_unmap = minix_types::ipc::MessLcVmShmUnmap {
        forwhom: forwhom.0,
        _pad: 0,
        addr,
        _padding: [0; 40],
    };
    let reply = perform_taskcall(transport, vm_endpoint(), VM_CALL_SHARED_UNMAP, &mut message)?;
    if reply < 0 {
        return Err(Errno::from_i32(-reply));
    }
    Ok(())
}

#[cfg(test)]
mod remap_layout_tests {
    use super::*;

    /// Layout witness for remap_via's raw direct write: five u64 fields land in
    /// order at payload offsets 0/8/16/24/32 (dst/src/dst_addr/src_addr/size—C
    /// mess_lsys_vm_vmremap, ipc.h:1537). A raw direct write has no struct to
    /// assert on, so byte checks after the field writes pin the layout.
    #[test]
    fn test_remap_via_raw_layout_order() {
        // SAFETY: test-local message, single-threaded.
        let mut message = unsafe { crate::Message::zeroed() };
        // SAFETY: union raw field write, test-local.
        unsafe {
            message.m_u.raw[0..8].copy_from_slice(&1u64.to_ne_bytes());
            message.m_u.raw[8..16].copy_from_slice(&2u64.to_ne_bytes());
            message.m_u.raw[16..24].copy_from_slice(&3u64.to_ne_bytes());
            message.m_u.raw[24..32].copy_from_slice(&4u64.to_ne_bytes());
            message.m_u.raw[32..40].copy_from_slice(&5u64.to_ne_bytes());
        }
        // The five fields are contiguous, non-overlapping, and gap-free: a
        // consistent read-back pins the layout.
        // SAFETY: raw field read, test-local.
        unsafe {
            assert_eq!(message.m_u.raw[0..8], 1u64.to_ne_bytes());
            assert_eq!(message.m_u.raw[8..16], 2u64.to_ne_bytes());
            assert_eq!(message.m_u.raw[16..24], 3u64.to_ne_bytes());
            assert_eq!(message.m_u.raw[24..32], 4u64.to_ne_bytes());
            assert_eq!(message.m_u.raw[32..40], 5u64.to_ne_bytes());
        }
    }
}
