//! The VirtualBox backdoor interface model.
//!
//! C correspondence: `minix3/minix/include/minix/vbox.h` + `vboxtype.h` (the
//! ABI the shared-folder guest libraries program against) and the libsys
//! verb set `minix3/minix/lib/libsys/arch/i386/vbox.c` (`vbox_init`/
//! `vbox_open`/`vbox_close`/`vbox_call`, `:19-121`). The production
//! transport — DS label resolution, grant creation, and the `VBOX_OPEN`/
//! `VBOX_CALL` sendrecs against the vboxguest driver — is the seam; the
//! guest-library protocol layer above it is fully modeled and host-tested.

#![allow(dead_code)] // The dir/attr/link/file slices land one family at a time.

use minix_types::Errno;

/// Parameter transfer direction (`vboxtype.h:15-17`).
pub const VBOX_DIR_IN: u32 = 0x01;
pub const VBOX_DIR_OUT: u32 = 0x02;
pub const VBOX_DIR_INOUT: u32 = VBOX_DIR_IN | VBOX_DIR_OUT;

/// One HGCM call parameter — C `vbox_param_t` (`vboxtype.h:23-36`): a type
/// tag over `u32`/`u64`/granted-pointer. The pointer variant carries the
/// grant the seam created for the local buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Param {
    /// `VBOX_TYPE_INVALID` — C's zero value; `vbox_call` refuses it.
    Invalid,
    /// `VBOX_TYPE_U32`.
    U32(u32),
    /// `VBOX_TYPE_U64`.
    U64(u64),
    /// `VBOX_TYPE_PTR` — granted memory area.
    Ptr {
        grant: i32,
        off: usize,
        size: usize,
        dir: u32,
    },
}

/// The backdoor verb set — C's `vbox.h` declarations as implemented by
/// `libsys/arch/i386/vbox.c`. The production implementation rides the
/// vboxguest driver; tests script it.
pub trait Backdoor {
    /// C `vbox_init` (`vbox.c:19-31`): resolve the VBOX driver's endpoint
    /// from DS; `EINVAL` when the label is absent.
    fn init(&mut self) -> Result<(), Errno>;

    /// C `vbox_open` (`vbox.c:33-68`): open an HGCM connection to the named
    /// service; the answer is the connection id.
    fn open(&mut self, service: &str) -> Result<i32, Errno>;

    /// C `vbox_close` (`vbox.c:70-93`).
    fn close(&mut self, conn: i32) -> Result<(), Errno>;

    /// C `vbox_set_ptr` (`vbox.c:179-206`): grant the local buffer for the
    /// given direction (`DIR_IN` grants write, `DIR_OUT` read — the host
    /// reads what the guest produced). Grant failure yields `Param::Invalid`
    /// exactly as C sets the type tag back.
    fn grant_ptr(&mut self, addr: u64, size: usize, dir: u32) -> Param;

    /// C `vbox_call` (`vbox.c:74-121`): run one HGCM function.
    /// `Ok(result)` is `m.VBOX_RESULT` — the HGCM-level answer (`0` = OK,
    /// otherwise a `VBOXFS_*` code); `Err` is a transport errno. Grants are
    /// revoked on the seam side, C's `vbox_put` included.
    fn call(&mut self, conn: i32, function: u32, params: &mut [Param]) -> Result<i32, Errno>;
}
