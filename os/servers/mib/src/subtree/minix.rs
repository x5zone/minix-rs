//! CTL_MINIX subtree: the test ground, the mirror, and the proc door.
//!
//! Mirrors `minix.c` (all 89 lines): the test87 proving ground (12
//! nodes, exact literals), the self-statistics mirror (live counters,
//! 03), and the proc door (two function nodes owned by 20). The test
//! subtree is compile-gated (`MINIX_TEST_SUBTREE`, mib.h:23); LWIP is
//! conspicuously absent (mounted via RMIB, 12/22).
//!
//! 15-mib-subtree-minix.md.

use minix_types::{
    MIB_NODES, MIB_OBJECTS, MIB_REMOTES, MINIX_LWIP, MINIX_MIB, MINIX_PROC, MINIX_TEST, PROC_DATA,
    PROC_LIST, SECRET_VALUE, TEST_ANYWRITE, TEST_BOOL, TEST_DESTROY1, TEST_DESTROY2, TEST_DYNAMIC,
    TEST_INT, TEST_PERM, TEST_PRIVATE, TEST_QUAD, TEST_SECRET, TEST_STRING, TEST_STRUCT,
};

/// Test-subtree gate: production disables, development enables (or
/// test87 fails). C: `MINIX_TEST_SUBTREE 1` — mib.h:23.
pub const MINIX_TEST_SUBTREE: bool = true;

/// Shape of one test node. Literals are the C table literals —
/// test87 asserts them, so they are pinned here, not described.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestKind {
    /// Read-only hex integer. C: `MIB_INT(_RO | HEX, 0x01020304, ...)`.
    HexInt(i32),
    /// Writable boolean. C: `MIB_BOOL(_RW, 0, ...)`.
    Bool,
    /// Writable quad. C: `MIB_QUAD(_RW, 0, ...)`.
    Quad,
    /// Writable string buffer (16 bytes). C: `MIB_STRING(_RW, ...)`.
    TestString,
    /// Writable struct buffer (12 bytes). C: `MIB_STRUCT(_RW, 12, ...)`.
    TestStruct,
    /// Writable private integer. C: `MIB_INT(_RW | PRIVATE, -5375, ...)`.
    PrivateInt(i32),
    /// Writable-by-any integer. C: `MIB_INT(_RW | ANYWRITE, 0, ...)`.
    AnywriteInt,
    /// Read-only integer, born to be destroyed. C: `MIB_INT(_RO, 0, ...)`.
    DoomedInt(i32),
    /// Private subtree. C: `MIB_NODE(_RO | PRIVATE, secret_table, ...)`.
    SecretTable,
    /// Permanent integer, no description. C: `MIB_INT(_P | _RO, 1, NULL)`.
    PermanentInt(i32),
}

/// One test87 node: id, name, shape, description presence.
///
/// C: `mib_minix_test_table[]` — minix.c:19-43. Descriptions are chosen
/// so the returned description-array alignment is itself tested
/// (`:14-18` — do not touch lightly); two nodes carry `NULL`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TestEntry {
    /// Slot id (`TEST_*`). C: table index.
    pub id: i32,
    /// Node name. C: `node_name`.
    pub name: &'static str,
    /// Node shape. C: macro + literal.
    pub kind: TestKind,
    /// Has a description (`None` = NULL desc). C: `node_desc`.
    pub described: bool,
}

/// The test87 proving ground, id-sorted (12 nodes).
///
/// C: minix.c:19-43. `deleteme`/`destroy1`/`destroy2` exist to be
/// destroyed (08's lifecycle under test); `secret` hides a subtree;
/// `permanent` cannot go; the `0x01020304` hex exercises byte order.
pub const TEST_ENTRIES: &[TestEntry] = &[
    TestEntry {
        id: TEST_INT,
        name: "int",
        kind: TestKind::HexInt(0x0102_0304),
        described: true,
    },
    TestEntry {
        id: TEST_BOOL,
        name: "bool",
        kind: TestKind::Bool,
        described: true,
    },
    TestEntry {
        id: TEST_QUAD,
        name: "quad",
        kind: TestKind::Quad,
        described: true,
    },
    TestEntry {
        id: TEST_STRING,
        name: "string",
        kind: TestKind::TestString,
        described: true,
    },
    TestEntry {
        id: TEST_STRUCT,
        name: "struct",
        kind: TestKind::TestStruct,
        described: true,
    },
    TestEntry {
        id: TEST_PRIVATE,
        name: "private",
        kind: TestKind::PrivateInt(-5375),
        described: true,
    },
    TestEntry {
        id: TEST_ANYWRITE,
        name: "anywrite",
        kind: TestKind::AnywriteInt,
        described: true,
    },
    TestEntry {
        id: TEST_DYNAMIC,
        name: "deleteme",
        kind: TestKind::DoomedInt(0),
        described: true,
    },
    TestEntry {
        id: TEST_SECRET,
        name: "secret",
        kind: TestKind::SecretTable,
        described: true,
    },
    TestEntry {
        id: TEST_PERM,
        name: "permanent",
        kind: TestKind::PermanentInt(1),
        described: false,
    },
    TestEntry {
        id: TEST_DESTROY1,
        name: "destroy1",
        kind: TestKind::DoomedInt(123),
        described: false,
    },
    TestEntry {
        id: TEST_DESTROY2,
        name: "destroy2",
        kind: TestKind::DoomedInt(456),
        described: true,
    },
];

/// The secret child: one integer under the private subtree.
///
/// C: `mib_minix_test_secret_table[]` — minix.c:9-12
/// (`SECRET_VALUE → "value"`, 12345, "The combination to my luggage").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecretEntry {
    /// Always `SECRET_VALUE` (0). C: table index.
    pub id: i32,
    /// Always `"value"`.
    pub name: &'static str,
    /// Always 12345.
    pub value: i32,
}

/// The secret table contents.
pub const SECRET_ENTRY: SecretEntry = SecretEntry {
    id: SECRET_VALUE,
    name: "value",
    value: 12345,
};

/// Self-statistics mirror: which counter each node reads.
///
/// C: `mib_minix_mib_table[]` — minix.c:47-57. All three are
/// `MIB_INTPTR(_P | _RO | UNSIGNED)` over the live globals (03's
/// `TreeCounts` fields): the tree reporting on itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MibStat {
    /// Live node count. C: `MIB_NODES → &mib_nodes`.
    Nodes,
    /// Allocated objects. C: `MIB_OBJECTS → &mib_objects`.
    Objects,
    /// Mounted remotes. C: `MIB_REMOTES → &mib_remotes`.
    Remotes,
}
pub const MIB_STAT_IDS: [(i32, &str); 3] = [
    (MIB_NODES, "nodes"),
    (MIB_OBJECTS, "objects"),
    (MIB_REMOTES, "remotes"),
];

/// Proc door: two function nodes owned by 20.
///
/// C: `mib_minix_proc_table[]` — minix.c:59-66. `list` is a STRUCT
/// func, `data` a NODE func; bodies are `mib_minix_proc_list` /
/// `mib_minix_proc_data` (20, ProcFS contract, A-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcDoor {
    /// Full-table snapshot. C: `PROC_LIST → mib_minix_proc_list`.
    List,
    /// Single-PID snapshot. C: `PROC_DATA → mib_minix_proc_data`.
    Data,
}

/// Proc door ids.
pub const PROC_DOOR_IDS: [(i32, &str); 2] = [(PROC_LIST, "list"), (PROC_DATA, "data")];

/// Top slots of the minix subtree.
///
/// C: `mib_minix_table[]` — minix.c:68-79. `test` is hidden + writable
/// (test87 creates under it); `mib`/`proc` read-only; LWIP is absent
/// *by design* (mounted via RMIB at run time, :78, 12/22).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MinixSlot {
    /// Test ground (gated). C: `MINIX_TEST`, HIDDEN + RW.
    Test,
    /// Self statistics. C: `MINIX_MIB`, RO.
    Mib,
    /// Proc door. C: `MINIX_PROC`, RO.
    Proc,
}

/// Minix slot ids (`MINIX_LWIP` intentionally absent — RMIB-mounted).
pub const MINIX_SLOT_IDS: [(i32, &str); 3] = [
    (MINIX_TEST, "test"),
    (MINIX_MIB, "mib"),
    (MINIX_PROC, "proc"),
];

/// LWIP id, for the absence assertion (mounted at run time, never tabled).
/// C: `MINIX_LWIP` + minix.c:78 comment.
pub const ABSENT_LWIP: i32 = MINIX_LWIP;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_test_ground() {
        // 12 nodes, exact literals (minix.c:19-43) — test87 asserts these.
        assert_eq!(TEST_ENTRIES.len(), 12);
        assert_eq!(MINIX_TEST_SUBTREE, true);
        let int = TEST_ENTRIES.iter().find(|e| e.id == TEST_INT).unwrap();
        assert_eq!(int.kind, TestKind::HexInt(0x0102_0304));
        let priv_ = TEST_ENTRIES.iter().find(|e| e.id == TEST_PRIVATE).unwrap();
        assert_eq!(priv_.kind, TestKind::PrivateInt(-5375));
        let d1 = TEST_ENTRIES.iter().find(|e| e.id == TEST_DESTROY1).unwrap();
        assert_eq!((d1.kind, d1.described), (TestKind::DoomedInt(123), false));
        let d2 = TEST_ENTRIES.iter().find(|e| e.id == TEST_DESTROY2).unwrap();
        assert_eq!((d2.kind, d2.described), (TestKind::DoomedInt(456), true));
        let perm = TEST_ENTRIES.iter().find(|e| e.id == TEST_PERM).unwrap();
        assert_eq!(
            (perm.kind, perm.described),
            (TestKind::PermanentInt(1), false)
        );
        // Secret child (minix.c:9-12).
        assert_eq!(
            (SECRET_ENTRY.id, SECRET_ENTRY.name, SECRET_ENTRY.value),
            (0, "value", 12345)
        );
        assert_eq!(SECRET_VALUE, 0);
    }

    #[test]
    fn test_mirror_proc_slots() {
        // Mirror ids (minix.c:47-57) — the tree reporting on itself (03).
        assert_eq!(MIB_STAT_IDS, [(1, "nodes"), (2, "objects"), (3, "remotes")]);
        // Proc door ids (minix.c:59-66) — bodies in 20.
        assert_eq!(PROC_DOOR_IDS, [(1, "list"), (2, "data")]);
        assert_eq!(PROC_LIST, 1);
        assert_eq!(PROC_DATA, 2);
        // Top slots (minix.c:68-79); LWIP absent by design (:78).
        assert_eq!(MINIX_SLOT_IDS, [(0, "test"), (1, "mib"), (2, "proc")]);
        assert_eq!(ABSENT_LWIP, 3);
        assert!(!MINIX_SLOT_IDS.iter().any(|(id, _)| *id == ABSENT_LWIP));
        let _ = (
            MibStat::Nodes,
            MibStat::Objects,
            MibStat::Remotes,
            ProcDoor::List,
            ProcDoor::Data,
            MinixSlot::Test,
            MinixSlot::Mib,
            MinixSlot::Proc,
        );
    }
}
