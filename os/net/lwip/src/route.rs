//! Routing table policy: prefix assumption, lookup order, miss handling.
//!
//! C correspondence: `minix3/minix/net/lwip/rttree.c` (744 lines) with the
//! route management in `minix3/minix/net/lwip/route.c` (1654 lines) and the
//! gateway hooks in the lightweight stack glue. Table storage, message
//! traffic, and stack overrides stay in the service binary. This module owns
//! the portion that can be decided from numbers alone: which prefix lengths
//! are valid, how bit positions map to bytes, and which lookup outcome
//! applies.
//!
//! The tree assumes every mask can be expressed as a prefix length: some
//! leading number of set bits followed by all clear bits. Entries live at
//! the node matching their bit count, and that node may still have children
//! that refine the prefix. There are no pure leaf-or-internal nodes, only
//! data nodes (with an entry, zero to two children) and link nodes (without
//! an entry, exactly two children).

use alloc::vec::Vec;

/// Largest version 4 prefix length (32 address bits).
pub const VERSION4_BITS: u8 = 32;

/// Largest version 6 prefix length (128 address bits).
pub const VERSION6_BITS: u8 = 128;

/// Byte index holding a bit position (`RTTREE_BITS_TO_BYTE`, `rttree.c:38`).
pub fn bit_to_byte(bit: u32) -> usize {
    (bit >> 3) as usize
}

/// Shift selecting a bit inside its byte (`RTTREE_BITS_TO_SHIFT`,
/// `rttree.c:39`: bit 0 is the most significant bit of byte 0).
pub fn bit_to_shift(bit: u32) -> u32 {
    7 - (bit & 7)
}

/// Bytes needed to hold a bit count (`RTTREE_BITS_TO_BYTES`, `rttree.c:40`).
pub fn bits_to_bytes(bits: u32) -> usize {
    ((bits + 7) >> 3) as usize
}

/// Whether a version 4 prefix length is valid.
pub fn version4_prefix_allowed(prefix: u8) -> bool {
    prefix <= VERSION4_BITS
}

/// Whether a version 6 prefix length is valid.
pub fn version6_prefix_allowed(prefix: u8) -> bool {
    prefix <= VERSION6_BITS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bit_mapping_matches_tree_macros() {
        assert_eq!(bit_to_byte(0), 0);
        assert_eq!(bit_to_byte(8), 1);
        assert_eq!(bit_to_shift(0), 7);
        assert_eq!(bit_to_shift(7), 0);
        assert_eq!(bit_to_shift(8), 7);
        assert_eq!(bits_to_bytes(0), 0);
        assert_eq!(bits_to_bytes(1), 1);
        assert_eq!(bits_to_bytes(8), 1);
        assert_eq!(bits_to_bytes(9), 2);
    }

    #[test]
    fn test_prefix_bounds_match_address_sizes() {
        assert_eq!(VERSION4_BITS, 32);
        assert_eq!(VERSION6_BITS, 128);
        assert!(version4_prefix_allowed(32));
        assert!(!version4_prefix_allowed(33));
        assert!(version6_prefix_allowed(128));
    }
}


// ---------------------------------------------------------------------------
// The routing table (the storage half of C's `rttree.c` plus the
// add/remove/lookup of `route.c:371-738`; the "decision separated from
// storage" registered in doc 19 §2.5 converges here: the tree implementation
// becomes a flat longest-match table with the same external contract — prefix
// validity, longest match wins, the default route being the prefix-0 entry).
// ---------------------------------------------------------------------------

/// Address family (route entries are grouped per family; an address takes a
/// fixed 4- or 16-byte representation per family).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpVersion {
    /// Version 4: the address takes the first 4 bytes.
    V4,
    /// Version 6: the address takes all 16 bytes.
    V6,
}

/// A route entry: network (normalized by prefix), next hop (`None` means
/// directly connected), outgoing interface line number. The interplay of the
/// default gateway and the default route is in doc 19 §1.3 (a prefix-0 entry
/// paired with a next-hop address makes the whole network reachable).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteEntry {
    /// Address family.
    pub version: IpVersion,
    /// Destination network (normalized by prefix on insertion, host bits cleared).
    pub dest: [u8; 16],
    /// Prefix length (0 = default route).
    pub prefix: u8,
    /// Next hop; `None` = directly connected (the destination is on the local link).
    pub gateway: Option<[u8; 16]>,
    /// Outgoing interface line number (the service-side interface table, doc 14).
    pub ifdev: u16,
}

/// The routing table: a flat entry set; lookup is longest-prefix-wins. C's
/// `rttree` uses a prefix tree as a storage strategy, but the only externally
/// observable semantics are "add, remove, longest-match lookup", and this
/// table carries them with a flat implementation of the same semantics (doc 19
/// §2.5 records the difference).
#[derive(Debug, Default)]
pub struct RouteTable {
    entries: Vec<RouteEntry>,
}

/// Build a mask from the family and prefix length (big-endian bit order: bit 0
/// is the most significant bit of byte 0, doc 19 §1.2; the mask slot of the
/// exported frame reuses the same function).
pub(crate) fn prefix_mask(version: IpVersion, prefix: u8) -> [u8; 16] {
    let mut mask = [0u8; 16];
    let bits = match version {
        IpVersion::V4 => prefix.min(32) as usize,
        IpVersion::V6 => prefix.min(128) as usize,
    };
    let whole = bits / 8;
    for byte in mask.iter_mut().take(whole) {
        *byte = 0xff;
    }
    if whole < 16 && bits % 8 != 0 {
        mask[whole] = 0xffu8 << (8 - bits % 8);
    }
    mask
}

fn masked(address: &[u8], mask: &[u8; 16]) -> [u8; 16] {
    let mut out = [0u8; 16];
    for (o, (a, m)) in out.iter_mut().zip(address.iter().zip(mask.iter())) {
        *o = a & m;
    }
    out
}

impl RouteTable {
    /// `route_init` (`route.c:248`): start from an empty table.
    pub fn new() -> Self {
        RouteTable { entries: Vec::new() }
    }

    /// Number of registered entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the table is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The registered entries (the rtsock read export face takes them one by
    /// one in insertion order).
    pub fn entries(&self) -> &[RouteEntry] {
        &self.entries
    }

    /// Add or update: an entry with the same key (family, network, prefix) is
    /// replaced (the "update" half of C's add/remove/update/query). The prefix
    /// is validated per family (32/128, the existing validity functions in
    /// `route.rs`), and the destination address is normalized by the prefix
    /// (host bits cleared, the premise of the tree's assumption).
    pub fn add(&mut self, mut entry: RouteEntry) -> Result<(), i32> {
        let allowed = match entry.version {
            IpVersion::V4 => version4_prefix_allowed(entry.prefix),
            IpVersion::V6 => version6_prefix_allowed(entry.prefix),
        };
        if !allowed {
            return Err(minix_types::EINVAL);
        }
        let mask = prefix_mask(entry.version, entry.prefix);
        entry.dest = masked(&entry.dest, &mask);
        self.entries.retain(|e| {
            e.version != entry.version || e.dest != entry.dest || e.prefix != entry.prefix
        });
        self.entries.push(entry);
        Ok(())
    }

    /// Remove: exact match on the key; returns whether it was actually
    /// registered.
    pub fn remove(&mut self, version: IpVersion, dest: [u8; 16], prefix: u8) -> bool {
        let mask = prefix_mask(version, prefix);
        let dest = masked(&dest, &mask);
        let before = self.entries.len();
        self.entries
            .retain(|e| !(e.version == version && e.dest == dest && e.prefix == prefix));
        self.entries.len() != before
    }

    /// Lookup: longest prefix wins; no match (including no default route)
    /// returns `None` — C's routing coverage returning "unreachable" is turned
    /// into an error by the caller.
    pub fn lookup(&self, version: IpVersion, address: &[u8]) -> Option<&RouteEntry> {
        let mut best: Option<&RouteEntry> = None;
        for entry in self.entries.iter().filter(|e| e.version == version) {
            let mask = prefix_mask(entry.version, entry.prefix);
            if masked(address, &mask) == entry.dest {
                let better = best
                    .map(|b| entry.prefix > b.prefix)
                    .unwrap_or(true);
                if better {
                    best = Some(entry);
                }
            }
        }
        best
    }
}

#[cfg(test)]
mod table_tests {
    use super::*;

    fn v4_entry(dest: [u8; 4], prefix: u8, gateway: Option<[u8; 4]>) -> RouteEntry {
        let mut d = [0u8; 16];
        d[..4].copy_from_slice(&dest);
        let g = gateway.map(|g| {
            let mut t = [0u8; 16];
            t[..4].copy_from_slice(&g);
            t
        });
        RouteEntry { version: IpVersion::V4, dest: d, prefix, gateway: g, ifdev: 0 }
    }

    fn v4_addr(dest: [u8; 4]) -> [u8; 16] {
        let mut d = [0u8; 16];
        d[..4].copy_from_slice(&dest);
        d
    }

    #[test]
    fn test_longest_prefix_wins() {
        let mut table = RouteTable::new();
        table.add(v4_entry([0, 0, 0, 0], 0, Some([10, 0, 0, 1]))).unwrap();
        table.add(v4_entry([192, 168, 0, 0], 16, None)).unwrap();
        let hit = table.lookup(IpVersion::V4, &[192, 168, 1, 5]).unwrap();
        assert_eq!(hit.prefix, 16, "更长前缀获胜");
        assert_eq!(hit.gateway, None, "直连条目无下一跳");
        // An address outside the 16-bit prefix goes through the default route.
        let miss = table.lookup(IpVersion::V4, &[8, 8, 8, 8]).unwrap();
        assert_eq!(miss.prefix, 0);
    }

    #[test]
    fn test_dest_normalized_on_add() {
        let mut table = RouteTable::new();
        // Non-zero host bits: after normalization by the prefix it has the
        // same key as the normalized form (covered by the update semantics).
        table.add(v4_entry([192, 168, 1, 1], 16, None)).unwrap();
        assert_eq!(table.len(), 1);
        table.add(v4_entry([192, 168, 9, 9], 16, Some([10, 0, 0, 1]))).unwrap();
        assert_eq!(table.len(), 1, "同键条目被替换（改半）");
        let hit = table.lookup(IpVersion::V4, &[192, 168, 7, 7]).unwrap();
        assert_eq!(hit.gateway.map(|g| g[..4].try_into().unwrap()), Some([10, 0, 0, 1]), "替换后的下一跳生效");
    }

    #[test]
    fn test_prefix_validation_and_remove() {
        let mut table = RouteTable::new();
        assert_eq!(
            table.add(v4_entry([10, 0, 0, 0], 33, None)).unwrap_err(),
            minix_types::EINVAL,
            "版本 4 前缀超 32 拒绝"
        );
        table.add(v4_entry([10, 0, 0, 0], 8, None)).unwrap();
        assert!(table.remove(IpVersion::V4, v4_addr([10, 0, 0, 0]), 8));
        assert!(!table.remove(IpVersion::V4, v4_addr([10, 0, 0, 0]), 8), "双删返回假");
        assert!(table.lookup(IpVersion::V4, &[10, 1, 2, 3]).is_none(), "删后无匹配");
    }

    #[test]
    fn test_v6_family_separate() {
        let mut table = RouteTable::new();
        let mut v6 = RouteEntry {
            version: IpVersion::V6,
            dest: [0u8; 16],
            prefix: 128,
            gateway: None,
            ifdev: 1,
        };
        // A version 6 prefix above 128 is rejected.
        v6.prefix = 129;
        assert_eq!(table.add(v6).unwrap_err(), minix_types::EINVAL);
        v6.prefix = 128;
        table.add(v6).unwrap();
        // Family isolation: a version 4 lookup does not hit version 6 entries.
        assert!(table.lookup(IpVersion::V4, &[0u8; 4]).is_none());
    }
}

