//! CTL_VM subtree: load math, page-shift lore, and a small table.
//!
//! Mirrors `vm.c` (all 154 lines): two function handlers and four
//! populated slots of thirteen. VM statistics reads (`sys_getloadinfo`,
//! `vm_info_stats`) are transport effects (A-12, shared with 16's table
//! pulls); the averaging math, the power-of-two hunt, and the table
//! shape are judged here.
//!
//! 14-mib-subtree-vm-hw.md.

use minix_types::{
    VM_ANONMAX, VM_ANONMIN, VM_EXECMAX, VM_EXECMIN, VM_FILEMAX, VM_FILEMIN, VM_LOADAVG, VM_MAXSLP,
    VM_METER, VM_NKMEMPAGES, VM_USPACE, VM_UVMEXP, VM_UVMEXP2,
};

/// Handler behind a vm function node.
///
/// C: `mib_vm_loadavg` (vm.c:12-73), `mib_vm_uvmexp2` (:79-117).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmFunc {
    /// Load averages over 1/5/15 minutes. C: `mib_vm_loadavg`.
    Loadavg,
    /// UVM statistics for top(1). C: `mib_vm_uvmexp2` (partially filled).
    Uvmexp2,
}

/// Shape of one populated vm slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmKind {
    /// Function node. C: `MIB_FUNC(...)`.
    Func(VmFunc),
    /// Constant integer leaf. C: `MIB_INT(_P | _RO, value, ...)`.
    ConstInt(i32),
}

/// One populated vm slot: id, name, shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmEntry {
    /// Slot id (`VM_*`). C: table index.
    pub id: i32,
    /// Node name. C: `node_name`.
    pub name: &'static str,
    /// Node shape. C: macro + handler.
    pub kind: VmKind,
}

/// The populated vm slots, id-sorted (4 of 13).
///
/// C: `mib_vm_table[]` — vm.c:120-144. `uspace` is 0 with a comment
/// ("MINIX3 processes don't have k-stacks", :138-140) — a zero with a
/// reason, not a missing value.
pub const VM_ENTRIES: &[VmEntry] = &[
    VmEntry {
        id: VM_LOADAVG,
        name: "loadavg",
        kind: VmKind::Func(VmFunc::Loadavg),
    },
    VmEntry {
        id: VM_UVMEXP2,
        name: "uvmexp2",
        kind: VmKind::Func(VmFunc::Uvmexp2),
    },
    VmEntry {
        id: VM_MAXSLP,
        name: "maxslp",
        kind: VmKind::ConstInt(20),
    },
    VmEntry {
        id: VM_USPACE,
        name: "uspace",
        kind: VmKind::ConstInt(0),
    },
];

/// Unpopulated vm slots (A-9, 9 of 13).
///
/// C: the `/* ... not yet supported */` rows — vm.c:121,125-126,128,
/// 132-134,141-143.
pub const VM_UNIMPLEMENTED: &[i32] = &[
    VM_METER,
    VM_UVMEXP,
    VM_NKMEMPAGES,
    VM_ANONMIN,
    VM_EXECMIN,
    VM_FILEMIN,
    VM_ANONMAX,
    VM_EXECMAX,
    VM_FILEMAX,
];

/// Find a populated entry by id.
pub fn find_entry(id: i32) -> Option<&'static VmEntry> {
    VM_ENTRIES.iter().find(|e| e.id == id)
}

/// Load-average window in minutes: 1, 5, 15.
/// C: `minutes[3] = { 1, 5, 15 }` — vm.c:23.
pub const LOAD_MINUTES: [u32; 3] = [1, 5, 15];

/// History slot length in seconds (ABI — changing breaks consumers).
/// C: `_LOAD_UNIT_SECS 6` — minix/type.h:88.
pub const LOAD_UNIT_SECS: u32 = 6;

/// History depth in slots (15 min × 60 / 6).
/// C: `_LOAD_HISTORY` — minix/type.h:95.
pub const LOAD_HISTORY: u32 = 150;

/// Fixed-point scale of the reported averages.
/// C: `loadavg.fscale = 100L` — vm.c:70.
pub const LOAD_FSCALE: i32 = 100;

/// Slots covering `minutes` of history: `minutes * 60 / UNIT`.
///
/// C: `slots = minutes[p] * 60 / _LOAD_UNIT_SECS` — vm.c:43.
pub const fn slots_for(minutes: u32) -> u32 {
    minutes * 60 / LOAD_UNIT_SECS
}

/// Ticks per history slot at this clock rate.
///
/// C: `ticks_per_slot = _LOAD_UNIT_SECS * sys_hz()` — vm.c:37.
pub const fn ticks_per_slot(hz: u32) -> u32 {
    LOAD_UNIT_SECS * hz
}

/// Ticks missing from the newest (still filling) slot.
///
/// C: `ticks_per_slot - (last_clock % ticks_per_slot)` — vm.c:38-39.
pub const fn unfilled_ticks(last_clock: u32, per_slot: u32) -> u32 {
    per_slot - (last_clock % per_slot)
}

/// One average in fscale units: percent of one CPU, roughly.
///
/// C: `loadavg.ldavg[p] = 100UL * proc_load / ticks` — vm.c:67, where
/// `ticks = slots * ticks_per_slot - unfilled` (:65). Returns `None` on
/// a zero divisor (a clock that never ticks has no average).
pub const fn avg_x100(proc_load: u64, ticks: u64) -> Option<u64> {
    if ticks == 0 {
        return None;
    }
    Some(100 * proc_load / ticks)
}

/// Page-shift hunt: first `shift` with `1 << shift == pagesize`.
///
/// C: vm.c:100-104. Note the 32-bit `1U`: the hunt caps at the pointer
/// width (`CHAR_BIT * sizeof(void *)`); non-powers-of-two match nothing
/// and leave the field zeroed (the `memset`, :90). `ptr_bits` is the
/// caller's pointer width (32 on MINIX3 C, 64 on minix-rs — an ARCH
/// widening that only *extends* the hunt, never changes a match).
pub const fn page_shift(pagesize: u64, ptr_bits: u32) -> Option<u32> {
    let mut shift = 0;
    while shift < ptr_bits {
        if (1u64 << shift) == pagesize {
            return Some(shift);
        }
        shift += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_shape() {
        // 4 populated of 13 (vm.c:120-144); sorted; disjoint from A-9.
        assert_eq!(VM_ENTRIES.len(), 4);
        assert_eq!(VM_UNIMPLEMENTED.len(), 9);
        assert_eq!(VM_ENTRIES.len() + VM_UNIMPLEMENTED.len(), 13);
        let mut prev = 0;
        for e in VM_ENTRIES {
            assert!(e.id > prev);
            assert!(!VM_UNIMPLEMENTED.contains(&e.id));
            prev = e.id;
        }
        assert_eq!(find_entry(VM_LOADAVG).unwrap().name, "loadavg");
        assert_eq!(
            find_entry(VM_UVMEXP2).unwrap().kind,
            VmKind::Func(VmFunc::Uvmexp2)
        );
        assert_eq!(find_entry(VM_METER), None);
    }

    #[test]
    fn test_loadavg_math() {
        // Windows: 10/50/150 slots (vm.c:43, UNIT 6).
        assert_eq!((LOAD_UNIT_SECS, LOAD_HISTORY, LOAD_FSCALE), (6, 150, 100));
        assert_eq!(slots_for(1), 10);
        assert_eq!(slots_for(5), 50);
        assert_eq!(slots_for(15), 150);
        // Ticks per slot at hz 100: 600 (vm.c:37).
        assert_eq!(ticks_per_slot(100), 600);
        // Unfilled newest slot (:38-39).
        assert_eq!(unfilled_ticks(0, 600), 600);
        assert_eq!(unfilled_ticks(599, 600), 1);
        assert_eq!(unfilled_ticks(600, 600), 600);
        // Average: 100 * load / ticks (:65-67); zero clock → None.
        assert_eq!(avg_x100(300, 600), Some(50));
        assert_eq!(avg_x100(0, 600), Some(0));
        assert_eq!(avg_x100(300, 0), None);
    }

    #[test]
    fn test_page_shift() {
        // First matching shift wins (vm.c:100-104).
        assert_eq!(page_shift(4096, 32), Some(12));
        assert_eq!(page_shift(4096, 64), Some(12));
        assert_eq!(page_shift(1, 32), Some(0));
        // Non-powers-of-two match nothing (field stays zeroed).
        assert_eq!(page_shift(1000, 32), None);
        assert_eq!(page_shift(0, 32), None);
    }

    #[test]
    fn test_vm_struct_sizes_match_c_layout() {
        // MIB 把这两个 struct 的宽度写进 sysctlnode 的 size lane（
        // walker.rs 的 report_size），客户端枚举 CTL_VM 时读到的就是它，
        // 必须在函数腿接线之前就与 C 对齐。从真实装配的树里回读 size。
        let t = crate::tree::arena::MibTree::init();
        let vm = t
            .child(t.root(), minix_types::CTL_VM)
            .expect("CTL_VM mounted");
        // loadavg：C `sizeof(struct loadavg)` — 3×uint32 + 4 pad + long(8) = 24。
        let loadavg = t.child(vm, VM_LOADAVG).expect("loadavg node");
        assert_eq!(t.slot(loadavg).size, 24);
        // uvmexp2：C `sizeof(struct uvmexp_sysctl)` — 77 个 int64_t = 616。
        // 旧形状此处报 0，客户端会误以为该节点不返回任何字节。
        let uvmexp2 = t.child(vm, VM_UVMEXP2).expect("uvmexp2 node");
        assert_eq!(t.slot(uvmexp2).size, 616);
        // maxslp/uspace 两个常量叶仍为 4 字节 int。
        let maxslp = t.child(vm, VM_MAXSLP).expect("maxslp node");
        assert_eq!(t.slot(maxslp).size, 4);
    }
}

// ── Arena wiring (P1-2) ──
//
// C 的 `mib_vm_init` 等价物：loadavg/uvmexp2 两个函数门 + maxslp/uspace。
// 这两个 struct 的宽度写进 sysctlnode 的 size lane，客户端枚举 CTL_VM 时
// 直接读到——所以宽度必须在函数腿接线（P1-5 的 `sys_getloadinfo` /
// `vm_info_stats` 跨服务拉取）之前就与 C 对齐，否则枚举面报出的记录尺寸
// 是错的（旧值把 uvmexp2 记 0，客户端会以为该节点不返回任何字节）。

/// `sizeof(struct loadavg)` — C `sys/sys/resource.h:118-121`
/// `{ fixpt_t ldavg[3]; long fscale; }`，`fixpt_t = uint32_t`
/// (`sys/sys/types.h:188`)。LP64 self-consistent 布局：`3×4` 定长数组 +
/// 4 字节对齐填充 + `long`（8）= 24。与 vm.c:122 `sizeof(struct loadavg)` 同值。
pub(crate) const SIZEOF_STRUCT_LOADAVG: u64 = 24;

/// `struct uvmexp_sysctl` 的字段数——C `sys/uvm/uvm_extern.h:373-451`，
/// 从 `pagesize` 到 `ncolors` 逐行数得 77 个 `int64_t` 字段，无 padding。
const UVMEXP_SYSCTL_FIELDS: u64 = 77;

/// `sizeof(struct uvmexp_sysctl)` — 77 个 `int64_t`（8 字节、对齐 8、无填充），
/// 两数据模型（ILP32 参考构建 / LP64 minix-rs）同宽：77 × 8 = 616。与 vm.c:127
/// `sizeof(struct uvmexp_sysctl)` 同值。此前记「minix3 头树里没有可引用的定义」
/// 不成立——定义就在 `uvm_extern.h:373`，`mib/vm.c` 经 `#include <uvm/uvm_extern.h>`
/// 引用它，只是当时未去查。
pub(crate) const SIZEOF_STRUCT_UVMEXP_SYSCTL: u64 = UVMEXP_SYSCTL_FIELDS * 8;

pub(crate) fn build(t: &mut crate::tree::arena::MibTree, parent: crate::tree::arena::NodeId) {
    use crate::tree::arena::{FuncKey, NodeId};
    use minix_types::{CTLFLAG_PERMANENT, CTLFLAG_READONLY, CTLTYPE_INT, CTLTYPE_STRUCT};
    const RO: u32 = CTLFLAG_READONLY | CTLFLAG_PERMANENT;
    let csize = VM_ENTRIES.iter().map(|e| e.id).max().unwrap() as u32 + 1;
    t.reserve(parent, csize);
    for e in VM_ENTRIES.iter() {
        match &e.kind {
            VmKind::Func(f) => {
                let (flags, size) = match f {
                    // C vm.c:122 `sizeof(struct loadavg)`；见 SIZEOF_STRUCT_LOADAVG 的推导。
                    VmFunc::Loadavg => (CTLTYPE_STRUCT | RO, SIZEOF_STRUCT_LOADAVG),
                    // C vm.c:127 `sizeof(struct uvmexp_sysctl)`；见 SIZEOF_STRUCT_UVMEXP_SYSCTL 的推导。
                    VmFunc::Uvmexp2 => (CTLTYPE_STRUCT | RO, SIZEOF_STRUCT_UVMEXP_SYSCTL),
                };
                let n = t.place(parent, e.name, e.id, flags);
                t.slot_mut(n).func = Some(FuncKey::Vm(*f));
                t.slot_mut(n).size = size;
            }
            VmKind::ConstInt(v) => {
                let n = t.place(
                    parent,
                    e.name,
                    e.id,
                    CTLTYPE_INT | minix_types::CTLFLAG_IMMEDIATE | RO,
                );
                t.slot_mut(n).imm = Some(*v as i64);
                t.slot_mut(n).size = 4;
            }
        }
    }
    let _ = NodeId(0);
}
