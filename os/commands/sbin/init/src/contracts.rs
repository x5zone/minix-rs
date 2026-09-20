//! External contracts: reboot/powerdown hooks and peer table.
//!
//! Covers `minix3/sbin/init/init.c:517-538`.
//! Design contract: `.design/14-design.v1.md §1.1-§1.2`.

use alloc::{string::String, string::ToString, vec::Vec};
use crate::session::ParsedCommand;
use crate::state_machine::sig;
use sig::{SIGNAL_ABORT, SIGNAL_USER_1};

/// What kind of shutdown a signal requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownRequest {
    Reboot,
    Powerdown,
}

/// Map the Minix-specific hooks: SIGABRT → reboot, SIGUSR1 → powerdown.
///
/// Signums come from the Minix3 numbering authority (where SIGUSR1 = 30
/// and 10 is SIGBUS — the value the old `10` arm used to match powers
/// nothing). Other signals yield `None` (owned by 02/03 handlers).
pub fn request_for(signum: i32) -> Option<ShutdownRequest> {
    match signum {
        SIGNAL_ABORT => Some(ShutdownRequest::Reboot),
        SIGNAL_USER_1 => Some(ShutdownRequest::Powerdown),
        _ => None,
    }
}

/// Assemble the `/sbin/shutdown` spawn request (C: the literal path and
/// argv at init.c:521-522, 534-535).
///
/// The exec path is the absolute binary location the C code spells out;
/// argv[0] is the bare "shutdown" the child sees.
pub fn shutdown_argv(request: ShutdownRequest) -> ParsedCommand {
    let flag = match request {
        ShutdownRequest::Reboot => "-r",
        ShutdownRequest::Powerdown => "-p",
    };
    ParsedCommand {
        exec_path: SHUTDOWN_PATH.to_string(),
        argv: vec![
            "shutdown".to_string(),
            flag.to_string(),
            "now".to_string(),
            "CTRL-ALT_DEL".to_string(),
        ],
    }
}

/// Absolute path of the shutdown binary (C: init.c:521 hard-codes
/// `"/sbin/shutdown"` inline).
pub const SHUTDOWN_PATH: &str = "/sbin/shutdown";

/// Fixed boot argv from VM (C: `vm/main.c:345`).
pub fn boot_argv() -> Vec<String> {
    vec!["init".to_string()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sigabrt_requests_reboot() {
        assert_eq!(request_for(6), Some(ShutdownRequest::Reboot));
    }

    #[test]
    fn test_sigusr1_requests_powerdown() {
        // Minix3 SIGUSR1 = 30; 10 is SIGBUS there and must not match.
        assert_eq!(request_for(30), Some(ShutdownRequest::Powerdown));
    }

    #[test]
    fn test_sigbus_is_not_a_hook() {
        assert_eq!(request_for(10), None);
    }

    #[test]
    fn test_other_signal_none() {
        assert_eq!(request_for(15), None);
    }

    #[test]
    fn test_reboot_argv() {
        let cmd = shutdown_argv(ShutdownRequest::Reboot);
        assert_eq!(cmd.argv[1], "-r");
        assert_eq!(cmd.argv[3], "CTRL-ALT_DEL");
    }

    #[test]
    fn test_powerdown_argv() {
        let cmd = shutdown_argv(ShutdownRequest::Powerdown);
        assert_eq!(cmd.argv[1], "-p");
    }

    #[test]
    fn test_shutdown_exec_path_differs_from_argv0() {
        // C: execl("/sbin/shutdown", "shutdown", ...) (init.c:521-522) —
        // the absolute path and argv[0] are not the same string.
        let cmd = shutdown_argv(ShutdownRequest::Reboot);
        assert_eq!(cmd.exec_path, "/sbin/shutdown");
        assert_eq!(cmd.argv[0], "shutdown");
    }

    #[test]
    fn test_boot_argv_contract() {
        assert_eq!(boot_argv(), vec!["init".to_string()]);
    }
}
