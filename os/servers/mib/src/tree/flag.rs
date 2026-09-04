//! Node flag vocabulary: types, access bits, and the internal matrix.
//!
//! Owns the *meaning* of the flag word. The wire *values* live in
//! `minix-types` (`CTLTYPE_*`/`CTLFLAG_*`, 02); this module classifies a
//! word into what the tree does with the node. The three internal
//! reassignments (`PARENT`/`VERIFY`/`REMOTE` over NetBSD's
//! `ROOT`/`ALIAS`/`MMAP`, mib.h:72-74) live here — never in `minix-types`,
//! because they are MIB-internal and explicitly changeable.
//!
//! 03-mib-node-model.md.

use minix_types::{
    CTLFLAG_ALIAS, CTLFLAG_ANYNUMBER, CTLFLAG_ANYWRITE, CTLFLAG_HEX, CTLFLAG_HIDDEN,
    CTLFLAG_IMMEDIATE, CTLFLAG_MMAP, CTLFLAG_OWNDATA, CTLFLAG_OWNDESC, CTLFLAG_PERMANENT,
    CTLFLAG_PRIVATE, CTLFLAG_READWRITE, CTLFLAG_ROOT, CTLFLAG_UNSIGNED, CTLTYPE_BOOL, CTLTYPE_INT,
    CTLTYPE_NODE, CTLTYPE_QUAD, CTLTYPE_STRING, CTLTYPE_STRUCT, SYSCTL_FLAGMASK, SYSCTL_TYPEMASK,
};

/// Internal flag aliases: NetBSD bits, MIB meanings.
///
/// C: `CTLFLAG_PARENT/VERIFY/REMOTE` — mib.h:72-74. Same bits as
/// `ROOT`/`ALIAS`/`MMAP` (02 keeps the NetBSD names); the tree only ever
/// reads them through these names, and they never reach userland.
pub const CTLFLAG_PARENT: u32 = CTLFLAG_ROOT;
/// Node carries a verify callback. C: `CTLFLAG_VERIFY` — mib.h:73.
pub const CTLFLAG_VERIFY: u32 = CTLFLAG_ALIAS;
/// Node is a remote mount point. C: `CTLFLAG_REMOTE` — mib.h:74.
pub const CTLFLAG_REMOTE: u32 = CTLFLAG_MMAP;

/// Data type of a node: the low nibble of the flag word.
///
/// C: `SYSCTL_TYPE(node->node_flags)` — sys/sys/sysctl.h:151.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeType {
    /// Subtree or function-driven node. C: `CTLTYPE_NODE`.
    Node,
    /// Integer leaf. C: `CTLTYPE_INT`.
    Int,
    /// String leaf. C: `CTLTYPE_STRING`.
    String,
    /// 64-bit leaf. C: `CTLTYPE_QUAD`.
    Quad,
    /// Struct leaf. C: `CTLTYPE_STRUCT`.
    Struct,
    /// Bool leaf. C: `CTLTYPE_BOOL`.
    Bool,
}

impl NodeType {
    /// Read the type nibble; unknown values refuse (`None`).
    ///
    /// C has no "unknown type" path — every static node is built by a
    /// `MIB_*` macro that sets a valid type — but dynamic create requests
    /// carry user-chosen sizes checked elsewhere (08), so the decoder
    /// stays total.
    pub const fn from_raw(flags: u32) -> Option<Self> {
        match flags & SYSCTL_TYPEMASK {
            CTLTYPE_NODE => Some(Self::Node),
            CTLTYPE_INT => Some(Self::Int),
            CTLTYPE_STRING => Some(Self::String),
            CTLTYPE_QUAD => Some(Self::Quad),
            CTLTYPE_STRUCT => Some(Self::Struct),
            CTLTYPE_BOOL => Some(Self::Bool),
            _ => None,
        }
    }

    /// Whether this type can have children (real or mounted).
    pub const fn is_node(self) -> bool {
        matches!(self, Self::Node)
    }
}

/// What a `CTLTYPE_NODE` node *is*: the `PARENT × REMOTE` matrix.
///
/// C: mib.h:111-125. Data nodes never take these roles — the matrix only
/// applies where `NodeType::is_node()` holds; `classify` enforces that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeRole {
    /// Local subtree with static and/or dynamic children.
    /// C: `PARENT` set, `REMOTE` clear — mib.h:116-117.
    RealParent,
    /// One handler owns the whole subtree below.
    /// C: neither flag; `node_func` serves — mib.h:114-115.
    FuncTree,
    /// Temporary mount point created for a remote tree; unmount destroys
    /// the node itself. C: `REMOTE` set, `PARENT` clear — mib.h:118-121.
    TempMount,
    /// Mount point hiding a real local subtree; unmount reveals it.
    /// C: both flags — mib.h:122-125.
    ObscuringMount,
}

impl NodeRole {
    /// Classify a node-type flag word into the matrix.
    ///
    /// Returns `None` for non-node types: asking "which mount is this
    /// integer" is a programming slip, not a fourth answer.
    pub const fn classify(flags: u32) -> Option<Self> {
        if flags & SYSCTL_TYPEMASK != CTLTYPE_NODE {
            return None;
        }
        match (flags & CTLFLAG_PARENT != 0, flags & CTLFLAG_REMOTE != 0) {
            (true, false) => Some(Self::RealParent),
            (false, false) => Some(Self::FuncTree),
            (false, true) => Some(Self::TempMount),
            (true, true) => Some(Self::ObscuringMount),
        }
    }
}

/// Read the access bits of a flag word (the `SYSCTL_FLAGS` view).
///
/// C: `SYSCTL_FLAGS(node->node_flags)` — sys/sys/sysctl.h:153.
#[inline(always)]
pub const fn access_bits(flags: u32) -> u32 {
    flags & SYSCTL_FLAGMASK
}

/// Whether the node is writable by anyone (any write bit set).
///
/// C: `node->node_flags & (CTLFLAG_READWRITE | CTLFLAG_ANYWRITE)`.
/// `READONLY` is zero, so "writable" is the only positive test.
pub const fn is_writable(flags: u32) -> bool {
    flags & (CTLFLAG_READWRITE | CTLFLAG_ANYWRITE) != 0
}

/// Whether the node demands superuser. C: `CTLFLAG_PRIVATE` — 07.
pub const fn is_private(flags: u32) -> bool {
    flags & CTLFLAG_PRIVATE != 0
}

/// Whether the node survives destroy requests. C: `CTLFLAG_PERMANENT`.
pub const fn is_permanent(flags: u32) -> bool {
    flags & CTLFLAG_PERMANENT != 0
}

/// Whether the value lives inside the node (vs behind a pointer).
/// C: `CTLFLAG_IMMEDIATE` — mib.h:93-99.
pub const fn is_immediate(flags: u32) -> bool {
    flags & CTLFLAG_IMMEDIATE != 0
}

/// Whether the node owns its data buffer. C: `CTLFLAG_OWNDATA`.
pub const fn owns_data(flags: u32) -> bool {
    flags & CTLFLAG_OWNDATA != 0
}

/// Whether the node owns its description. C: `CTLFLAG_OWNDESC`.
pub const fn owns_desc(flags: u32) -> bool {
    flags & CTLFLAG_OWNDESC != 0
}

/// Whether new values pass a verify callback. C: `CTLFLAG_VERIFY`.
pub const fn has_verify(flags: u32) -> bool {
    flags & CTLFLAG_VERIFY != 0
}

/// Whether children may be invented below at run time.
/// C: `CTLFLAG_ANYNUMBER`.
pub const fn any_number(flags: u32) -> bool {
    flags & CTLFLAG_ANYNUMBER != 0
}

/// Hex display. C: `CTLFLAG_HEX`.
pub const fn is_hex(flags: u32) -> bool {
    flags & CTLFLAG_HEX != 0
}

/// Hidden from enumeration. C: `CTLFLAG_HIDDEN`.
pub const fn is_hidden(flags: u32) -> bool {
    flags & CTLFLAG_HIDDEN != 0
}

/// Unsigned integer interpretation. C: `CTLFLAG_UNSIGNED`.
pub const fn is_unsigned(flags: u32) -> bool {
    flags & CTLFLAG_UNSIGNED != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_nibble() {
        // C: CTLTYPE_* 1..6 (sysctl.h:92-97).
        assert_eq!(NodeType::from_raw(CTLTYPE_NODE), Some(NodeType::Node));
        assert_eq!(NodeType::from_raw(CTLTYPE_INT), Some(NodeType::Int));
        assert_eq!(NodeType::from_raw(CTLTYPE_STRING), Some(NodeType::String));
        assert_eq!(NodeType::from_raw(CTLTYPE_QUAD), Some(NodeType::Quad));
        assert_eq!(NodeType::from_raw(CTLTYPE_STRUCT), Some(NodeType::Struct));
        assert_eq!(NodeType::from_raw(CTLTYPE_BOOL), Some(NodeType::Bool));
        // Nibble 0/7..15: no type (total decoder).
        assert_eq!(NodeType::from_raw(0), None);
        assert_eq!(NodeType::from_raw(7), None);
        // Flags ride above the nibble untouched.
        assert_eq!(
            NodeType::from_raw(CTLTYPE_INT | CTLFLAG_PRIVATE | CTLFLAG_IMMEDIATE),
            Some(NodeType::Int)
        );
    }

    #[test]
    fn test_role_matrix() {
        // C: mib.h:111-125, four cells.
        let node = CTLTYPE_NODE;
        assert_eq!(
            NodeRole::classify(node | CTLFLAG_PARENT),
            Some(NodeRole::RealParent)
        );
        assert_eq!(NodeRole::classify(node), Some(NodeRole::FuncTree));
        assert_eq!(
            NodeRole::classify(node | CTLFLAG_REMOTE),
            Some(NodeRole::TempMount)
        );
        assert_eq!(
            NodeRole::classify(node | CTLFLAG_PARENT | CTLFLAG_REMOTE),
            Some(NodeRole::ObscuringMount)
        );
        // Non-node types have no role (mib.h:110: matrix is node-only).
        assert_eq!(NodeRole::classify(CTLTYPE_INT | CTLFLAG_PARENT), None);
    }

    #[test]
    fn test_access_predicates() {
        // READONLY is zero: writable is the positive test.
        assert!(!is_writable(0));
        assert!(is_writable(CTLFLAG_READWRITE));
        assert!(is_writable(CTLFLAG_ANYWRITE));
        assert!(is_private(CTLFLAG_PRIVATE));
        assert!(!is_private(CTLFLAG_READWRITE));
        assert!(is_permanent(CTLFLAG_PERMANENT));
        assert!(is_immediate(CTLTYPE_INT | CTLFLAG_IMMEDIATE));
        assert!(owns_data(CTLFLAG_OWNDATA));
        assert!(owns_desc(CTLFLAG_OWNDESC));
        assert!(has_verify(CTLFLAG_VERIFY));
        assert!(any_number(CTLFLAG_ANYNUMBER));
        // Alias values are the same bits (mib.h:72-74).
        assert_eq!(CTLFLAG_PARENT, CTLFLAG_ROOT);
        assert_eq!(CTLFLAG_VERIFY, CTLFLAG_ALIAS);
        assert_eq!(CTLFLAG_REMOTE, CTLFLAG_MMAP);
    }
}
