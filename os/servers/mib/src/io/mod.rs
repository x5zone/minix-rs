//! MIB copy and relay surface: what moves, who may touch it.
//!
//! 06 owns the verdicts (`copy`, `relay`); the moves themselves
//! (`sys_datacopy`, `cpf_grant_magic`) belong to the transport (A-12).
//!
//! 06-mib-copy-io.md.

pub mod copy;
pub mod relay;

pub use copy::{
    CopySpan, PAGE_SIZE, check_copyin, copyout_span, get_new_len, get_old_len, in_range,
    next_chunk, nul_size,
};
pub use relay::{GRANT_INVALID, RELAY_FAIL, RelayDir, RelayRegion, grant_valid};
