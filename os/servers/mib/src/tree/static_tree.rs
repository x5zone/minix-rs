//! Static wiring: the seven top slots and the root that holds them.
//!
//! Mirrors `mib_table[]` + `mib_root` (`main.c:46-62`). The per-subtree
//! *contents* (kern/vm/hw/minix tables) land in 13/14/15; this module
//! owns the seven slots themselves — which id, which name, which access —
//! so the init walk (init.rs) has something to link.
//!
//! 04-mib-static-tree-init.md.

use minix_types::{
    CTL_HW, CTL_KERN, CTL_MINIX, CTL_NET, CTL_USER, CTL_VENDOR, CTL_VM, CTLFLAG_PERMANENT,
    CTLFLAG_READONLY, CTLFLAG_READWRITE,
};

/// A top-level slot: id, name, description, access.
///
/// C: one `MIB_ENODE(_P | _RO/RW, name, desc)` row — main.c:47-53.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TopSlot {
    /// Top-level id (`CTL_*`). C: array index (`[CTL_KERN]` etc).
    pub id: i32,
    /// Short name. C: `node_name`.
    pub name: &'static str,
    /// Long description. C: `node_desc`.
    pub desc: &'static str,
    /// Writable by userspace. C: `_RW` vs `_RO` in the row.
    pub writable: bool,
}

/// The seven top slots in `mib_table` order.
///
/// C: main.c:46-54. kern/vm/net/hw/user/vendor/minix. `CTL_NET` stays
/// empty until remote services mount under it (12); `CTL_USER` is served
/// inside libc and only listed so sysctl(8) shows it (main.c:41-43);
/// `CTL_VENDOR` is the writable scratch slot for third parties.
pub const TOP_SLOTS: [TopSlot; 7] = [
    TopSlot {
        id: CTL_KERN,
        name: "kern",
        desc: "High kernel",
        writable: false,
    },
    TopSlot {
        id: CTL_VM,
        name: "vm",
        desc: "Virtual memory",
        writable: false,
    },
    TopSlot {
        id: CTL_NET,
        name: "net",
        desc: "Networking",
        writable: false,
    },
    TopSlot {
        id: CTL_HW,
        name: "hw",
        desc: "Generic CPU, I/O",
        writable: false,
    },
    TopSlot {
        id: CTL_USER,
        name: "user",
        desc: "User-level",
        writable: false,
    },
    TopSlot {
        id: CTL_VENDOR,
        name: "vendor",
        desc: "Vendor specific",
        writable: true,
    },
    TopSlot {
        id: CTL_MINIX,
        name: "minix",
        desc: "MINIX3 specific",
        writable: false,
    },
];

/// Access bits of a top slot, as the `MIB_ENODE` row sets them.
///
/// C: `MIB_ENODE(_P | _RO/RW, ...)` — mib.h:259-263 + main.c:47-53.
/// Every top slot is `PERMANENT` (never destroyable); only vendor is
/// writable.
pub const fn slot_flags(slot: TopSlot) -> u32 {
    let access = if slot.writable {
        CTLFLAG_READWRITE
    } else {
        CTLFLAG_READONLY
    };
    // NOTE: `CTLTYPE_NODE` is ORed by the table owner (04 wires the bit;
    // 03 classifies it) — see `MIB_ENODE`, mib.h:260.
    access | CTLFLAG_PERMANENT
}

/// Look a top slot up by id. `None` = no top slot (VFS/DEBUG/MACHDEP and
/// friends have no seat at this table — 02 §2.6).
pub const fn top_slot(id: i32) -> Option<TopSlot> {
    let mut i = 0;
    while i < TOP_SLOTS.len() {
        if TOP_SLOTS[i].id == id {
            return Some(TOP_SLOTS[i]);
        }
        i += 1;
    }
    None
}

/// The root: writable and nameless, internal-only.
///
/// C: `struct mib_node mib_root = MIB_NODE(_RW, mib_table, "", "")` —
/// main.c:62. Writable so init(8) may plant its own top-level entries
/// (:59-60); unreachable from userland by design (:57-58).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RootSpec {
    /// Root is writable. C: `_RW` in the `MIB_NODE` row.
    pub writable: bool,
}

impl RootSpec {
    /// The one root. C: main.c:62.
    pub const fn spec() -> Self {
        Self { writable: true }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seven_slots() {
        // C: main.c:46-54 — seven rows, sparse ids.
        assert_eq!(TOP_SLOTS.len(), 7);
        let ids = [
            CTL_KERN, CTL_VM, CTL_NET, CTL_HW, CTL_USER, CTL_VENDOR, CTL_MINIX,
        ];
        for (slot, id) in TOP_SLOTS.iter().zip(ids) {
            assert_eq!(slot.id, id);
        }
        assert_eq!(top_slot(CTL_KERN).unwrap().name, "kern");
        assert_eq!(top_slot(CTL_MINIX).unwrap().desc, "MINIX3 specific");
        // No seat: VFS et al (02 §2.6).
        assert_eq!(top_slot(3), None);
        assert_eq!(top_slot(14), None);
    }

    #[test]
    fn test_slot_access() {
        // Six read-only, one writable (main.c:47-53).
        for slot in TOP_SLOTS {
            let flags = slot_flags(slot);
            assert_ne!(flags & CTLFLAG_PERMANENT, 0);
            assert_eq!(flags & CTLFLAG_READWRITE != 0, slot.writable);
        }
        assert!(top_slot(CTL_VENDOR).unwrap().writable);
        assert!(!top_slot(CTL_NET).unwrap().writable);
        assert!(RootSpec::spec().writable);
    }
}
