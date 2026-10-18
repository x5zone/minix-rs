//! GPIO driver: pins as files, plus driver-side device registration.
//!
//! C correspondence: `minix3/minix/drivers/system/gpio/gpio.c` (290
//! lines, pin export through the virtual-tree filesystem) with
//! `minix3/minix/lib/libdevman/` (driver-side device add/delete plus USB
//! bind tracking, implemented by the `devman_client` and `usb_model`
//! modules of the `minix-sys` crate). This crate owns the pin numbers,
//! the export rules, and the tree hooks; the service binary runs the
//! virtual-tree filesystem loop. See document `12-gpio-devman.md` in
//! `rewrite-notes/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

extern crate alloc;

pub mod char_face;
pub mod files;
pub mod pins;
pub mod server;

use minix_vtreefs::driver::TreeServer;

use pins::NullBoard;
use server::GpioHooks;

/// Build the driver's service for the binary.
///
/// C exports pins only on BBXM and BB boards; the generic build has no
/// board table and serves an empty root, the same tree C builds when
/// neither board matches. The board tables and their pad configuration
/// ride the board-support seam.
pub fn init() -> TreeServer<GpioHooks<NullBoard>> {
    server::init_with(NullBoard, &[])
}

/// What the GPIO file server makes of one signal number.
///
/// C registers no handler of its own: the loop comes from the virtual tree
/// file system library, whose `got_signal` terminates on SIGTERM and
/// ignores everything else (`libvtreefs/vtreefs.c`).
pub const fn signal_action(signo: i32) -> minix_fs_rt::transport::SignalAction {
    if signo == minix_types::SIGNAL_TERMINATE {
        minix_fs_rt::transport::SignalAction::Terminate
    } else {
        minix_fs_rt::transport::SignalAction::Ignore
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use minix_fs_rt::transport::SignalAction;

    #[test]
    fn test_signal_action_follows_vtreefs_got_signal() {
        assert_eq!(
            signal_action(minix_types::SIGNAL_TERMINATE),
            SignalAction::Terminate
        );
        assert_eq!(signal_action(2), SignalAction::Ignore);
        assert_eq!(signal_action(0), SignalAction::Ignore);
    }
}
