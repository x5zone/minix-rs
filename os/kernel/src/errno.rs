//! Minix3 error codes — re-exported from `minix-types` (single source).
//!
//! All values originate from `minix3/sys/sys/errno.h`. Previously the
//! kernel defined its own 115 `pub const` i32 constants; these are now
//! re-exported from `minix_types::types` which serves as the single
//! authority for the entire workspace (D1, todo §22 Phase 4).
//!
//! # Usage
//!
//! ```ignore
//! use crate::errno::*;
//! // ...
//! KcallResult::Ok(EBUSY)  // return EBUSY to caller
//! ```

// minix_types::types re-exports errno::* at its own module level, so all
// errno constants are available as `minix_types::types::EPERM` etc.
// This glob re-export preserves the kernel's `crate::errno::*` usage.
pub use minix_types::types::*;
