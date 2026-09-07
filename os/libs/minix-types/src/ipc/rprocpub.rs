//! `rprocpub` wire face — byte-ABI pinning and decode of C `struct rprocpub`.
//!
//! C: `<minix/rs.h>`:165-183. Two consumers share this layout: RS copies the
//! table out to requesters (`do_getsysinfo`'s `SI_PROCPUB_TAB`,
//! request.c:1119-1121), and VM fetches it through the RS_INIT grant
//! (`ipc_call_rs_init` — the VM-side half of edge E-RSWIRE). Data model:
//! **x86-64 LP64** — same decision as `rs_start` (see `rs_start.rs`); every
//! field here is fixed-width (`short`/`unsigned`/`endpoint_t`/`devmajor_t`
//! = 2/4/4/4 bytes, `devmajor_t` = `int32_t`, sys/sys/types.h:286-288), so
//! the layout carries no LP64-dependent field at all.
//!
//! The offset table is asserted against a `#[repr(C)]` witness with the C
//! field order (`offset_of!`/`size_of`), exactly like `rs_start.rs`.

use crate::Errno;

/// Field offsets within `struct rprocpub` (single authority).
pub mod rprocpub_off {
    /// `short in_use` — rs.h:166.
    pub const IN_USE: usize = 0;
    /// `unsigned sys_flags` — rs.h:167.
    pub const SYS_FLAGS: usize = 4;
    /// `endpoint_t endpoint` — rs.h:168.
    pub const ENDPOINT: usize = 8;
    /// `endpoint_t old_endpoint` — rs.h:169.
    pub const OLD_ENDPOINT: usize = 12;
    /// `endpoint_t new_endpoint` — rs.h:170.
    pub const NEW_ENDPOINT: usize = 16;
    /// `devmajor_t dev_nr` — rs.h:172.
    pub const DEV_NR: usize = 20;
    /// `int nr_domain` — rs.h:173.
    pub const NR_DOMAIN: usize = 24;
    /// `int domain[NR_DOMAIN]` — rs.h:174 (8 × 4 bytes).
    pub const DOMAIN: usize = 28;
    /// `char label[RS_MAX_LABEL_LEN]` — rs.h:176.
    pub const LABEL: usize = 60;
    /// `char proc_name[RS_MAX_LABEL_LEN]` — rs.h:177.
    pub const PROC_NAME: usize = 76;
    /// `bitchunk_t vm_call_mask[VM_CALL_MASK_SIZE]` — rs.h:179 (2 × 4 bytes).
    pub const VM_CALL_MASK: usize = 92;
    /// `struct rs_pci pci_acl` — rs.h:181 (rs.h:154-162).
    pub const PCI_ACL: usize = 100;
    /// `int devman_id` — rs.h:182.
    pub const DEVMAN_ID: usize = 416;
    /// `sizeof(struct rprocpub)` — 420 bytes.
    pub const SIZE: usize = 420;

    /// `struct rs_pci` internals (relative to `PCI_ACL`).
    /// `char rsp_label[RS_MAX_LABEL_LEN]` — rs.h:155.
    pub const PCI_LABEL: usize = 0;
    /// `endpoint_t rsp_endpoint` — rs.h:156.
    pub const PCI_ENDPOINT: usize = 16;
    /// `int rsp_nr_device` — rs.h:157.
    pub const PCI_NR_DEVICE: usize = 20;
    /// `struct rs_pci_id rsp_device[RS_NR_PCI_DEVICE]` — rs.h:158 (32 × 8).
    pub const PCI_DEVICE: usize = 24;
    /// `int rsp_nr_class` — rs.h:159. `rs_pci_id` is four `u16_t` fields
    /// (alignment 2), so the 32-entry device table spans 24..280 exactly.
    pub const PCI_NR_CLASS: usize = 280;
    /// `struct rs_pci_class rsp_class[RS_NR_PCI_CLASS]` — rs.h:160 (4 × 8).
    pub const PCI_CLASS: usize = 284;
}

/// Wire-shaped decode of one `struct rprocpub` (the public half of an RS
/// service slot as C lays it out on the wire).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RprocPubWire {
    /// `in_use` — nonzero when the row is live.
    pub in_use: i16,
    /// `sys_flags` — raw `SF_*` bits.
    pub sys_flags: u32,
    /// `endpoint`.
    pub endpoint: i32,
    /// `old_endpoint`.
    pub old_endpoint: i32,
    /// `new_endpoint`.
    pub new_endpoint: i32,
    /// `dev_nr`.
    pub dev_nr: i32,
    /// `nr_domain`.
    pub nr_domain: i32,
    /// `domain`.
    pub domain: [i32; 8],
    /// `label` — raw 16 bytes.
    pub label: [u8; 16],
    /// `proc_name` — raw 16 bytes.
    pub proc_name: [u8; 16],
    /// `vm_call_mask` — chunks combined little-endian (bit *i* = call *i*).
    pub vm_call_mask: u64,
    /// `pci_acl`.
    pub pci_acl: RprocPciWire,
    /// `devman_id`.
    pub devman_id: i32,
}

/// `struct rs_pci` — rs.h:154-162.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RprocPciWire {
    /// `rsp_label` — raw 16 bytes.
    pub label: [u8; 16],
    /// `rsp_endpoint`.
    pub endpoint: i32,
    /// `rsp_nr_device`.
    pub nr_device: i32,
    /// `rsp_device`.
    pub device: [RprocPciIdWire; 32],
    /// `rsp_nr_class`.
    pub nr_class: i32,
    /// `rsp_class`.
    pub class: [RprocPciClassWire; 4],
}

/// `struct rs_pci_id` — rs.h:73-78.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RprocPciIdWire {
    /// `vid`.
    pub vid: u16,
    /// `did`.
    pub did: u16,
    /// `sub_vid`.
    pub sub_vid: u16,
    /// `sub_did`.
    pub sub_did: u16,
}

/// `struct rs_pci_class` — rs.h:82-85.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RprocPciClassWire {
    /// `pciclass`.
    pub pciclass: u32,
    /// `mask`.
    pub mask: u32,
}

/// Decodes `struct rprocpub` bytes into the wire view. The buffer must
/// carry the whole struct (`rprocpub_off::SIZE`; a larger scratch buffer is
/// accepted — the table copy reads row-sized slices).
pub fn decode_rproc_pub(bytes: &[u8]) -> Result<RprocPubWire, Errno> {
    if bytes.len() < rprocpub_off::SIZE {
        return Err(Errno::EINVAL);
    }
    let u32at = |o: usize| u32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
    let i32at = |o: usize| i32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
    let u16at = |o: usize| u16::from_le_bytes([bytes[o], bytes[o + 1]]);
    let bytes16 = |o: usize| {
        [
            bytes[o],
            bytes[o + 1],
            bytes[o + 2],
            bytes[o + 3],
            bytes[o + 4],
            bytes[o + 5],
            bytes[o + 6],
            bytes[o + 7],
            bytes[o + 8],
            bytes[o + 9],
            bytes[o + 10],
            bytes[o + 11],
            bytes[o + 12],
            bytes[o + 13],
            bytes[o + 14],
            bytes[o + 15],
        ]
    };

    let mut domain = [0i32; 8];
    for (i, v) in domain.iter_mut().enumerate() {
        *v = i32at(rprocpub_off::DOMAIN + i * 4);
    }
    let mask = u32at(rprocpub_off::VM_CALL_MASK) as u64
        | (u32at(rprocpub_off::VM_CALL_MASK + 4) as u64) << 32;

    let pci = &rprocpub_off::PCI_ACL;
    let mut device = [RprocPciIdWire {
        vid: 0,
        did: 0,
        sub_vid: 0,
        sub_did: 0,
    }; 32];
    for (i, d) in device.iter_mut().enumerate() {
        let o = pci + rprocpub_off::PCI_DEVICE + i * 8;
        *d = RprocPciIdWire {
            vid: u16at(o),
            did: u16at(o + 2),
            sub_vid: u16at(o + 4),
            sub_did: u16at(o + 6),
        };
    }
    let mut class = [RprocPciClassWire {
        pciclass: 0,
        mask: 0,
    }; 4];
    for (i, c) in class.iter_mut().enumerate() {
        *c = RprocPciClassWire {
            pciclass: u32at(pci + rprocpub_off::PCI_CLASS + i * 8),
            mask: u32at(pci + rprocpub_off::PCI_CLASS + i * 8 + 4),
        };
    }

    Ok(RprocPubWire {
        in_use: i16::from_le_bytes([bytes[0], bytes[1]]),
        sys_flags: u32at(rprocpub_off::SYS_FLAGS),
        endpoint: i32at(rprocpub_off::ENDPOINT),
        old_endpoint: i32at(rprocpub_off::OLD_ENDPOINT),
        new_endpoint: i32at(rprocpub_off::NEW_ENDPOINT),
        dev_nr: i32at(rprocpub_off::DEV_NR),
        nr_domain: i32at(rprocpub_off::NR_DOMAIN),
        domain,
        label: bytes16(rprocpub_off::LABEL),
        proc_name: bytes16(rprocpub_off::PROC_NAME),
        vm_call_mask: mask,
        pci_acl: RprocPciWire {
            label: bytes16(pci + rprocpub_off::PCI_LABEL),
            endpoint: i32at(pci + rprocpub_off::PCI_ENDPOINT),
            nr_device: i32at(pci + rprocpub_off::PCI_NR_DEVICE),
            device,
            nr_class: i32at(pci + rprocpub_off::PCI_NR_CLASS),
            class,
        },
        devman_id: i32at(rprocpub_off::DEVMAN_ID),
    })
}

/// Compile-time layout witness: the C field order with C-equivalent types.
/// `repr(C)` gives it the ABI layout; the test build asserts every
/// [`rprocpub_off`] constant via `offset_of!`. Never instantiated at
/// runtime.
#[repr(C)]
#[cfg(test)]
struct RprocPubLayout {
    in_use: i16,
    sys_flags: u32,
    endpoint: i32,
    old_endpoint: i32,
    new_endpoint: i32,
    dev_nr: i32,
    nr_domain: i32,
    domain: [i32; 8],
    label: [u8; 16],
    proc_name: [u8; 16],
    vm_call_mask: [u32; 2],
    pci_acl: RprocPciLayout,
    devman_id: i32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
#[cfg(test)]
struct RprocPciLayout {
    label: [u8; 16],
    endpoint: i32,
    nr_device: i32,
    device: [RprocPciIdLayout; 32],
    nr_class: i32,
    class: [RprocPciClassLayout; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
#[cfg(test)]
struct RprocPciIdLayout {
    vid: u16,
    did: u16,
    sub_vid: u16,
    sub_did: u16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
#[cfg(test)]
struct RprocPciClassLayout {
    pciclass: u32,
    mask: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    const _: () = assert!(offset_of!(RprocPubLayout, in_use) == rprocpub_off::IN_USE);
    const _: () = assert!(offset_of!(RprocPubLayout, sys_flags) == rprocpub_off::SYS_FLAGS);
    const _: () = assert!(offset_of!(RprocPubLayout, endpoint) == rprocpub_off::ENDPOINT);
    const _: () = assert!(offset_of!(RprocPubLayout, old_endpoint) == rprocpub_off::OLD_ENDPOINT);
    const _: () = assert!(offset_of!(RprocPubLayout, new_endpoint) == rprocpub_off::NEW_ENDPOINT);
    const _: () = assert!(offset_of!(RprocPubLayout, dev_nr) == rprocpub_off::DEV_NR);
    const _: () = assert!(offset_of!(RprocPubLayout, nr_domain) == rprocpub_off::NR_DOMAIN);
    const _: () = assert!(offset_of!(RprocPubLayout, domain) == rprocpub_off::DOMAIN);
    const _: () = assert!(offset_of!(RprocPubLayout, label) == rprocpub_off::LABEL);
    const _: () = assert!(offset_of!(RprocPubLayout, proc_name) == rprocpub_off::PROC_NAME);
    const _: () = assert!(offset_of!(RprocPubLayout, vm_call_mask) == rprocpub_off::VM_CALL_MASK);
    const _: () = assert!(offset_of!(RprocPubLayout, pci_acl) == rprocpub_off::PCI_ACL);
    const _: () = assert!(offset_of!(RprocPubLayout, devman_id) == rprocpub_off::DEVMAN_ID);
    const _: () = assert!(size_of::<RprocPubLayout>() == rprocpub_off::SIZE);
    const _: () = assert!(offset_of!(RprocPciLayout, device) == rprocpub_off::PCI_DEVICE);
    const _: () = assert!(offset_of!(RprocPciLayout, nr_class) == rprocpub_off::PCI_NR_CLASS);
    const _: () = assert!(size_of::<RprocPciLayout>() == 316);

    #[test]
    fn test_decode_rproc_pub_fields() {
        // Hand-assembled bytes at the pinned offsets (the witness asserts
        // the offsets themselves; here the decoder's field extraction is
        // checked against a constructed image).
        let mut img = [0u8; rprocpub_off::SIZE];
        let put32 =
            |img: &mut [u8], o: usize, v: u32| img[o..o + 4].copy_from_slice(&v.to_le_bytes());
        img[0..2].copy_from_slice(&1i16.to_le_bytes());
        put32(&mut img, rprocpub_off::SYS_FLAGS, 0x0000_1000);
        put32(&mut img, rprocpub_off::ENDPOINT, 1); // VFS
        put32(&mut img, rprocpub_off::OLD_ENDPOINT, (-1i32) as u32);
        put32(&mut img, rprocpub_off::DEV_NR, 3);
        put32(&mut img, rprocpub_off::NR_DOMAIN, 1);
        put32(&mut img, rprocpub_off::DOMAIN, 7);
        img[rprocpub_off::LABEL..rprocpub_off::LABEL + 4].copy_from_slice(b"vfs\0");
        img[rprocpub_off::PROC_NAME..rprocpub_off::PROC_NAME + 4].copy_from_slice(b"vfs\0");
        put32(&mut img, rprocpub_off::VM_CALL_MASK, 0x0000_00ff);
        put32(&mut img, rprocpub_off::VM_CALL_MASK + 4, 0x0000_0300);
        put32(
            &mut img,
            rprocpub_off::PCI_ACL + rprocpub_off::PCI_NR_DEVICE,
            1,
        );
        let d0 = rprocpub_off::PCI_ACL + rprocpub_off::PCI_DEVICE;
        img[d0..d0 + 2].copy_from_slice(&0x8086u16.to_le_bytes());
        img[d0 + 2..d0 + 4].copy_from_slice(&0x100eu16.to_le_bytes());
        put32(&mut img, rprocpub_off::DEVMAN_ID, 5);

        let w = decode_rproc_pub(&img).expect("decode");
        assert_eq!(w.in_use, 1);
        assert_eq!(w.sys_flags, 0x0000_1000);
        assert_eq!(w.endpoint, 1);
        assert_eq!(w.old_endpoint, -1);
        assert_eq!(w.dev_nr, 3);
        assert_eq!(w.nr_domain, 1);
        assert_eq!(w.domain[0], 7);
        assert_eq!(&w.label[..4], b"vfs\0");
        assert_eq!(w.vm_call_mask, 0x0000_0300_0000_00ff);
        assert_eq!(w.pci_acl.nr_device, 1);
        assert_eq!(w.pci_acl.device[0].vid, 0x8086);
        assert_eq!(w.pci_acl.device[0].did, 0x100e);
        assert_eq!(w.devman_id, 5);
    }

    #[test]
    fn test_decode_rproc_pub_short_buffer_rejected() {
        assert_eq!(
            decode_rproc_pub(&[0u8; rprocpub_off::SIZE - 1]),
            Err(Errno::EINVAL)
        );
    }
}
