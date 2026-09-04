//! MIB object tree: node shape verdicts.
//!
//! 03 owns the vocabulary (`flag`) and the shape invariants (`node`);
//! 04 owns the static wiring (`static_tree`, `init`); 05 the lookup;
//! 08 the dynamic lifecycle (`dynamic`, `version`). The tables themselves
//! (static arrays, dynamic lists, mount storage) land with 04's
//! follow-up/08/12 beside this module.
//!
//! 03-mib-node-model.md + 04/05/08 companions.

pub mod dispatch;
pub mod dynamic;
pub mod flag;
pub mod init;
pub mod lookup;
pub mod mount;
pub mod node;
pub mod static_tree;
pub mod version;

pub use dispatch::{
    EISDIR_EMPTY, LevelVerdict, MetaOp, RemoteOutcome, is_leaf_flags, is_remote_flags, judge_level,
    judge_meta, judge_remote_result, resolve_shape, terminal_code,
};
pub use dynamic::{
    DestroyRefusal, RemoveDelta, ScanOutcome, check_destroy, check_name, create_alloc_size,
    data_combo_ok, remove_delta, sanitize_rw, scan, type_size_ok, valid_create_flags,
};

pub use flag::{
    CTLFLAG_PARENT, CTLFLAG_REMOTE, CTLFLAG_VERIFY, NodeRole, NodeType, access_bits, any_number,
    has_verify, is_hex, is_hidden, is_immediate, is_permanent, is_private, is_unsigned,
    is_writable, owns_data, owns_desc,
};
pub use init::{
    ChildVerdict, InitPhase, PHASE_ORDER, WIRE_ORDER, WireStep, check_parent, fold_static,
    judge_child,
};
pub use lookup::{Lookup, find, find_static, scan_dynamic};
pub use mount::{
    MountHead, TargetVerdict, check_head, check_target, head_code, is_obscuring, path_id_ok,
    path_node_ok, recount_clen, temp_alloc_size, unmount_entry_ok,
};
pub use node::{
    ChildWindow, EID_BITS, MAX_DESC_LEN, MAX_ENDPOINTS, MAX_REMOTE_CHILDREN, RC_BITS, ROOT_VER,
    RemotePack, SCRATCH_ALIGN, SCRATCH_SIZE, TreeCounts, can_have_children, is_static_id,
    linked_ver, version_matches,
};
pub use static_tree::{RootSpec, TOP_SLOTS, TopSlot, slot_flags, top_slot};
pub use version::{create_ver_ok, next_root_ver};
