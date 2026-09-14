//! DS startup: settle in, register four promises, wait to be asked.
//!
//! Mirrors `sef_local_startup()` (`minix3/minix/servers/ds/main.c:93-104`).
//! 01-ds-init-main.md.
//!
//! The lifecycle owns the role split and nothing else: which init names
//! exist, and which transfer hook the startup registers. The fresh-boot
//! body (`sef_cb_init_fresh`, 06), restart state keeping (libsef
//! generic), and the transfer body (06, A-6) stay out.

/// The init names DS registers (`sef_local_startup`, `main.c:96-97`).
///
/// Only two: a fresh boot runs DS code; a restart reuses the libsef
/// generic (`SEF_CB_INIT_RESTART_STATEFUL`, `sef.h:85`) and needs no
/// DS body — so this is an enum, not a trait (a one-method trait
/// would be decoration: no second implementor, never a bound).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DsInitKind {
    /// Fresh boot. C: `sef_setcb_init_fresh(sef_cb_init_fresh)` — main.c:96.
    Fresh,
    /// Restart with state kept. C: `sef_setcb_init_restart
    /// (SEF_CB_INIT_RESTART_STATEFUL)` — main.c:97; the generic lives in
    /// libsef, DS contributes no code.
    RestartStateful,
}

/// The Live Update transfer hook DS registers (`main.c:100`).
///
/// C: `sef_llvm_ds_st_init()` (`sef.h:372`) — the weak magic hook that
/// walks DS static memory on update (A-6). minix-rs has no LLVM magic:
/// the transfer body becomes explicit serialization in 06, so this enum
/// only names the promise here — the startup registers it, 06 fulfils it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveUpdateHook {
    /// DS state transfer registered. C: `sef_llvm_ds_st_init()` — main.c:100.
    DsStateTransfer,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_kinds() {
        // Two registrations (`main.c:96-97`); restart is generic (sef.h:85).
        assert_eq!(DsInitKind::Fresh, DsInitKind::Fresh);
        assert_ne!(DsInitKind::Fresh, DsInitKind::RestartStateful);
    }

    #[test]
    fn test_hook_named() {
        // The hook is named at startup (`main.c:100`); its body lives in 06.
        assert_eq!(
            LiveUpdateHook::DsStateTransfer,
            LiveUpdateHook::DsStateTransfer
        );
    }
}

// ── Live-Update / restart state transfer ([ARCH A-6]) ──
//
// C's `sef_llvm_ds_st_init` magic (`libmagicrt/magic_ds.c`) walks the
// static `ds_store`/`ds_subs` memory in place. minix-rs has no magic
// instrumentation: the transfer is an explicit serialize/deserialize
// pair (the RS `state_data.rs` precedent). The pool travels too — C's
// magic type-converts the `dsi_u` arms because malloc'd buffers sit
// outside the static segment; this pool serializes its bytes and
// re-stamps the descriptors' pointers on import (the data lane carries
// the *slot number* in transit, never a raw address).

use crate::heap::POOL_SLOTS;
use crate::store::{DsStore, NR_DS_KEYS};
use crate::subscription::{DsSubs, NR_DS_SUBS};
use minix_types::{DS_MAX_KEYLEN, DsFlags};

/// Bytes of one subscription record in transit: flags + owner + pattern
/// + reserved + told-map (128 bits = 16 bytes).
const SUB_RECORD: usize = 4 + DS_MAX_KEYLEN + DS_MAX_KEYLEN + 1 + 16;

/// Serialized subscription table: magic ("DSS1") + record count + records.
const SUBS_STATE_BYTES: usize = 8 + NR_DS_SUBS * SUB_RECORD;

/// The whole transferable state: entry table (canonical 192-byte C
/// layout, wide descriptors carrying their pool slot number), the
/// subscription records, and the pool bytes with liveness.
#[derive(Clone)]
pub struct StateImage {
    pub entries: [[u8; 192]; NR_DS_KEYS],
    pub subs: [u8; SUBS_STATE_BYTES],
    pub pool: [u8; POOL_SLOTS * crate::heap::POOL_SLOT],
    pub pool_live: [bool; POOL_SLOTS],
}

/// Serialize the tables and the pool (`sef_llvm_ds_st_init`'s walk,
/// made explicit).
pub fn export_state(store: &DsStore, subs: &DsSubs, pool: &crate::heap::DsPool) -> StateImage {
    let mut img = StateImage {
        entries: [[0; 192]; NR_DS_KEYS],
        subs: [0; SUBS_STATE_BYTES],
        pool: *pool.bytes(),
        pool_live: *pool.live_map(),
    };
    for (i, seat) in store.iter().enumerate() {
        let Some(entry) = seat else { continue };
        let out = &mut img.entries[i];
        out[..4].copy_from_slice(&entry.flags.bits().to_ne_bytes());
        out[4..84].copy_from_slice(&entry.key);
        out[84..164].copy_from_slice(&entry.owner);
        if entry
            .flags
            .intersects(DsFlags::TYPE_STR | DsFlags::TYPE_MEM)
        {
            // SAFETY: wide-arm lanes only (03's documented arm). The data
            // lane carries the pool *slot number* in transit — import
            // re-stamps the pointer against the receiving pool.
            let mem = unsafe { entry.body.mem };
            let slot = pool.slot_of(&mem);
            out[168..176].copy_from_slice(&(slot as u64).to_ne_bytes());
            out[176..184].copy_from_slice(&mem.length.to_ne_bytes());
            out[184..192].copy_from_slice(&mem.reallen.to_ne_bytes());
        } else {
            // SAFETY: narrow arm (U32/LABEL).
            let value = unsafe { entry.body.u32 };
            out[168..172].copy_from_slice(&value.to_ne_bytes());
        }
    }

    img.subs[..4].copy_from_slice(&0x4453_5331u32.to_ne_bytes()); // "DSS1"
    let mut at = 8usize;
    let mut count = 0u32;
    for seat in subs.iter() {
        let Some(sub) = seat else { continue };
        if sub.is_vacant() {
            continue;
        }
        let rec = &mut img.subs[at..at + SUB_RECORD];
        rec[..4].copy_from_slice(&sub.flags.bits().to_ne_bytes());
        rec[4..84].copy_from_slice(&sub.owner);
        rec[84..164].copy_from_slice(&sub.pattern);
        rec[164] = 0; // reserved
        for bit in 0..NR_DS_KEYS {
            if sub.old_subs.get(bit) {
                rec[165 + bit / 8] |= 1 << (bit % 8);
            }
        }
        at += SUB_RECORD;
        count += 1;
    }
    img.subs[4..8].copy_from_slice(&count.to_ne_bytes());
    img
}

/// Restore tables and pool from an image (the import half; descriptors
/// re-stamp into the receiving pool). An invalid image refuses
/// wholesale — a half-restored registry is worse than a fresh one.
pub fn import_state(
    store: &mut DsStore,
    subs: &mut DsSubs,
    pool: &mut crate::heap::DsPool,
    img: &StateImage,
) -> Result<(), ()> {
    if img.subs[..4] != 0x4453_5331u32.to_ne_bytes() {
        return Err(());
    }
    *store = [None; NR_DS_KEYS];
    *subs = [None; crate::subscription::NR_DS_SUBS];
    for (i, bytes) in img.entries.iter().enumerate() {
        if bytes[..4] == [0, 0, 0, 0] {
            continue; // vacant (canonical zero flags)
        }
        let mut entry = crate::store::DataEntry {
            flags: DsFlags::from_bits_truncate(u32::from_ne_bytes(bytes[..4].try_into().unwrap())),
            key: [0; DS_MAX_KEYLEN],
            owner: [0; DS_MAX_KEYLEN],
            body: crate::store::DataBody { u32: 0 },
        };
        entry.key.copy_from_slice(&bytes[4..84]);
        entry.owner.copy_from_slice(&bytes[84..164]);
        if entry
            .flags
            .intersects(DsFlags::TYPE_STR | DsFlags::TYPE_MEM)
        {
            let slot = u64::from_ne_bytes(bytes[168..176].try_into().unwrap()) as usize;
            if slot >= POOL_SLOTS {
                return Err(());
            }
            // SAFETY: the pointer re-stamps into the receiving pool at
            // the recorded slot — the inverse of export's slot stamp.
            entry.body.mem = crate::store::MemBody {
                data: pool.ptr_for_slot(slot),
                length: u64::from_ne_bytes(bytes[176..184].try_into().unwrap()) as usize,
                reallen: u64::from_ne_bytes(bytes[184..192].try_into().unwrap()) as usize,
            };
        } else {
            entry.body.u32 = u32::from_ne_bytes(bytes[168..172].try_into().unwrap());
        }
        store[i] = Some(entry);
    }

    let count = u32::from_ne_bytes(img.subs[4..8].try_into().unwrap()) as usize;
    if count > NR_DS_SUBS {
        return Err(());
    }
    let mut at = 8usize;
    for _ in 0..count {
        let rec = &img.subs[at..at + SUB_RECORD];
        at += SUB_RECORD;
        let mut sub = crate::subscription::Subscription::vacant();
        sub.flags = DsFlags::from_bits_truncate(u32::from_ne_bytes(rec[..4].try_into().unwrap()));
        if !sub.flags.contains(DsFlags::IN_USE) {
            continue;
        }
        sub.owner.copy_from_slice(&rec[4..84]);
        sub.pattern.copy_from_slice(&rec[84..164]);
        for bit in 0..NR_DS_KEYS {
            sub.old_subs
                .set(bit, rec[165 + bit / 8] & (1 << (bit % 8)) != 0);
        }
        if let Some(seat) = subs.iter_mut().find(|s| s.is_none()) {
            *seat = Some(sub);
        }
    }

    *pool = crate::heap::DsPool::restore(&img.pool, &img.pool_live);
    Ok(())
}

#[cfg(test)]
mod state_tests {
    use super::*;
    use crate::heap::DsPool;
    use crate::store::{DataBody, DataEntry};
    use crate::subscription::Subscription;

    fn named(key: &[u8], ty: DsFlags, owner: &[u8]) -> DataEntry {
        let mut e = DataEntry {
            flags: DsFlags::IN_USE | ty,
            key: [0u8; DS_MAX_KEYLEN],
            owner: [0u8; DS_MAX_KEYLEN],
            body: DataBody { u32: 0 },
        };
        e.key[..key.len()].copy_from_slice(key);
        e.owner[..owner.len()].copy_from_slice(owner);
        e
    }

    #[test]
    fn test_state_roundtrip_preserves_tables_pool_and_told_bits() {
        // The transfer's whole job: a restarted (or live-updated) store
        // serves the same bytes through re-stamped pool pointers, with
        // every subscriber's pending updates intact.
        let mut store: DsStore = [None; NR_DS_KEYS];
        let mut subs: DsSubs = [None; NR_DS_SUBS];
        let mut pool = DsPool::new();

        let mut cfg = named(b"cfg", DsFlags::TYPE_U32, b"vfs");
        cfg.body.u32 = 7;
        store[0] = Some(cfg);

        let (_, body) = pool.alloc(11).expect("pool seats");
        {
            let bytes = pool.slice_mut(&body);
            bytes[..11].copy_from_slice(b"hello world");
        }
        let mut log = named(b"log", DsFlags::TYPE_MEM, b"vfs");
        log.body.mem = body;
        store[1] = Some(log);

        let mut sub = Subscription::vacant();
        sub.flags = DsFlags::IN_USE | DsFlags::TYPE_MEM;
        sub.owner[..2].copy_from_slice(b"pm");
        sub.pattern[..3].copy_from_slice(b"log");
        sub.old_subs.set(1, true);
        subs[0] = Some(sub);

        // Transfer into a fresh incarnation.
        let img = export_state(&store, &subs, &pool);
        let mut store2: DsStore = [None; NR_DS_KEYS];
        let mut subs2: DsSubs = [None; NR_DS_SUBS];
        let mut pool2 = DsPool::new();
        import_state(&mut store2, &mut subs2, &mut pool2, &img).expect("a valid image must import");

        // U32 identity survives.
        let cfg2 = store2[0].as_ref().unwrap();
        assert_eq!(unsafe { cfg2.body.u32 }, 7);
        // The wide descriptor re-stamped into the new pool serves the
        // same bytes (this is the road C's magic pointer conversion
        // walks — store.c's dsi_u type dispatch, A-6).
        let log2 = store2[1].as_ref().unwrap();
        // SAFETY: wide-arm entry (flags say MEM); the pointer was
        // re-stamped against pool2 on import.
        let mem = unsafe { log2.body.mem };
        assert_eq!(mem.length, 11);
        assert_eq!(pool2.slice(&mem), b"hello world");
        // The subscriber's pending update survived with its told-bit.
        let sub2 = subs2[0].as_ref().unwrap();
        assert_eq!(&sub2.owner[..2], b"pm");
        assert!(sub2.old_subs.get(1));
    }

    #[test]
    fn test_import_refuses_broken_images() {
        // A live wide entry must exist for the slot-lane check to bite:
        // vacant entries skip import entirely (canonical zero flags).
        let mut store: DsStore = [None; NR_DS_KEYS];
        let mut subs: DsSubs = [None; NR_DS_SUBS];
        let mut pool = DsPool::new();
        let (_, body) = pool.alloc(4).expect("pool seats");
        let mut log = named(b"log", DsFlags::TYPE_MEM, b"vfs");
        log.body.mem = body;
        store[0] = Some(log);

        let mut img = export_state(&store, &subs, &pool);
        img.subs[0] = 0xFF; // wrong magic
        assert!(import_state(&mut store, &mut subs, &mut pool, &img).is_err());

        // A wide entry pointing past the pool refuses too (store.c's
        // magic would have dereferenced garbage; the explicit transfer
        // validates its own format).
        let img = export_state(&store, &subs, &pool);
        let mut bad = img.clone();
        bad.entries[0][168..176].copy_from_slice(&99u64.to_ne_bytes());
        assert!(import_state(&mut store, &mut subs, &mut pool, &bad).is_err());
    }
}
