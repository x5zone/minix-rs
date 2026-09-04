//! DS data entries: what survives, in what shape.
//!
//! Mirrors `struct data_store` + the entry table (`minix3/minix/servers/ds/
//! store.h:12-29`). 03-ds-data-structures.md.
//!
//! The module owns the shape and nothing else: the entry, its body union,
//! the table, and the vacancy rule. Slot dynamics (alloc/lookup, 04),
//! identity (05), and heap verbs (malloc/free, 07/09) stay out.
//!
//! Single-threaded event loop: pure functions, no shared state.

use minix_types::{DS_MAX_KEYLEN, DsFlags};

/// Entries in the store (`store.h:12`: `2 * NR_SYS_PROCS` of 64).
pub const NR_DS_KEYS: usize = 128;

/// A stored byte range (`struct dsi_mem`, `store.h:23-27`).
///
/// ABI mirror: pointer, length, capacity — the union's wide arm (24
/// bytes on 64-bit). The pointer names ownership, not access: whoever
/// holds the entry allocates on publish and frees on delete/overwrite
/// (07/09); this module only fixes the lane width (A-3 weighs allocation,
/// the shape is decided here).
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MemBody {
    /// Owned bytes. C: `void *data` — store.h:24.
    pub data: *mut u8,
    /// Live length. C: `size_t length` — store.h:25.
    pub length: usize,
    /// Allocated room. C: `size_t reallen` — store.h:26.
    pub reallen: usize,
}

/// An entry's value (`union dsi_u`, `store.h:21-28`).
///
/// Numbers and endpoints ride the narrow arm; byte ranges ride the wide
/// arm. Labels have no arm of their own: a label's endpoint rides the
/// number lane (`store.c:241`, read back at `dmp_ds.c:41`) — the union
/// stays two-armed because C's is.
#[derive(Clone, Copy)]
#[repr(C)]
pub union DataBody {
    /// Plain value. C: `unsigned u32` — store.h:22.
    pub u32: u32,
    /// Byte range. C: `struct dsi_mem mem` — store.h:23-27.
    pub mem: MemBody,
}

/// One stored item (`struct data_store`, `store.h:16-29`).
///
/// Four lanes: flags, name (`key`), master (`owner`), body (`u`). Keys
/// and owners ride fixed 80-byte lanes (`DS_MAX_KEYLEN`, terminator
/// included); readers name them, this module only carries bytes (naming
/// lives in 05).
#[derive(Clone, Copy)]
#[repr(C)]
pub struct DataEntry {
    /// Occupancy and guards. C: `int flags` — store.h:17.
    pub flags: DsFlags,
    /// Lookup name. C: `char key[80]` — store.h:18.
    pub key: [u8; DS_MAX_KEYLEN],
    /// Owning name. C: `char owner[80]` — store.h:19.
    pub owner: [u8; DS_MAX_KEYLEN],
    /// Value. C: `union dsi_u u` — store.h:21-28.
    pub body: DataBody,
}

impl DataEntry {
    /// Vacancy rule (`store.c:15-17`: `!(flags & DSF_IN_USE)`).
    ///
    /// Empty is a shape, not a scan: `None` in the table means what the
    /// unset bit meant — the dynamics (first-fit walk, 04) read this.
    pub const fn is_vacant(&self) -> bool {
        !self.flags.contains(DsFlags::IN_USE)
    }
}

/// The entry table (`ds_store[NR_DS_KEYS]`, A-4).
///
/// Fixed at 128: the bound rides the type, so overrun is inexpressible.
/// `None` is vacancy (D3) — the table never holds a half-entry.
pub type DsStore = [Option<DataEntry>; NR_DS_KEYS];

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{align_of, size_of};

    const fn entry(flags: DsFlags) -> DataEntry {
        DataEntry {
            flags,
            key: [0u8; DS_MAX_KEYLEN],
            owner: [0u8; DS_MAX_KEYLEN],
            body: DataBody { u32: 0 },
        }
    }

    #[test]
    fn test_entry_layout() {
        // C x86-64: 4 + 80 + 80 + 24 = 188, aligned to 192 (A-10).
        // The union sits at 168 (4 pad bytes after the owner lane).
        assert_eq!(size_of::<MemBody>(), 24);
        assert_eq!(size_of::<DataBody>(), 24);
        assert_eq!(size_of::<DataEntry>(), 192);
        assert_eq!(align_of::<DataEntry>(), 8);
        let base = DataEntry {
            flags: DsFlags::empty(),
            key: [0u8; DS_MAX_KEYLEN],
            owner: [0u8; DS_MAX_KEYLEN],
            body: DataBody { u32: 0 },
        };
        let addr = &base as *const _ as usize;
        assert_eq!(&base.key as *const _ as usize - addr, 4);
        assert_eq!(&base.owner as *const _ as usize - addr, 84);
        assert_eq!(&base.body as *const _ as usize - addr, 168);
    }

    #[test]
    fn test_vacant_rule() {
        // Vacant is `None`; occupied entries judge by their flag
        // (`store.c:15-17`).
        let table: DsStore = [None; NR_DS_KEYS];
        assert_eq!(table.len(), NR_DS_KEYS);
        assert!(table.iter().all(|slot| slot.is_none()));
        assert!(entry(DsFlags::empty()).is_vacant());
        assert!(!entry(DsFlags::IN_USE).is_vacant());
        assert!(!entry(DsFlags::IN_USE | DsFlags::TYPE_U32).is_vacant());
    }

    #[test]
    fn test_label_lane() {
        // Labels ride the number lane (`store.c:241` writes it,
        // `dmp_ds.c:41` reads it back).
        let body = DataBody { u32: 6 };
        assert_eq!(unsafe { body.u32 }, 6);
    }

    #[test]
    fn test_flags_roundtrip() {
        // Flags round-trip through the entry (same source as the wire,
        // `com.rs` DsFlags — 02).
        let e = entry(DsFlags::IN_USE | DsFlags::TYPE_LABEL);
        assert!(e.flags.contains(DsFlags::IN_USE));
        assert!(e.flags.contains(DsFlags::TYPE_LABEL));
        assert!(!e.flags.contains(DsFlags::TYPE_U32));
    }

    #[test]
    fn test_key_owner_bytes() {
        // Names ride fixed lanes; the module carries bytes, 05 names them.
        let mut e = entry(DsFlags::IN_USE);
        e.key[..4].copy_from_slice(b"init");
        e.owner[..2].copy_from_slice(b"rs");
        assert_eq!(&e.key[..4], b"init");
        assert_eq!(&e.owner[..2], b"rs");
        assert_eq!(e.key.len(), DS_MAX_KEYLEN);
    }
}
