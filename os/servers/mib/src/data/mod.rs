//! MIB data access: read and write verdicts for plain leaves.
//!
//! 09 owns the verdicts (`readwrite`); handler bodies that reuse them
//! (13~20) call in, and the arena/transport effects stay out.
//!
//! 09-mib-data-access.md.

pub mod readwrite;

pub use readwrite::{
    PtrLane, Stage, apply_verify, finalize_string, getptr_lane, is_data_leaf, read_len,
    readwrite_combine, sanitize_bool, stage_for, write_size_ok,
};
