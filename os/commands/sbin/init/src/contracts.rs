//! External contracts: reboot/powerdown hooks and peer table.
//!
//! Covers `minix3/sbin/init/init.c:517-538`.
//! Design contract: `.design/14-design.v1.md §1.1-§1.2`.

/// What kind of shutdown a signal requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownRequest {
    Reboot,
    Powerdown,
}

/// Map the Minix-specific hooks: SIGABRT(6) → reboot, SIGUSR1(10/30) →
/// powerdown. Other signals yield `None` (owned by 02/03 handlers).
pub fn request_for(signum: i32) -> Option<ShutdownRequest> {
    match signum {
        6 => Some(ShutdownRequest::Reboot),
        10 | 30 => Some(ShutdownRequest::Powerdown),
        _ => None,
    }
}

/// Assemble `/sbin/shutdown` argv (C: init.c:521-522, 534-535).
pub fn shutdown_argv(request: ShutdownRequest) -> Vec<String> {
    let flag = match request {
        ShutdownRequest::Reboot => "-r",
        ShutdownRequest::Powerdown => "-p",
    };
    vec![
        "shutdown".to_string(),
        flag.to_string(),
        "now".to_string(),
        "CTRL-ALT_DEL".to_string(),
    ]
}

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
        assert_eq!(request_for(10), Some(ShutdownRequest::Powerdown));
    }

    #[test]
    fn test_other_signal_none() {
        assert_eq!(request_for(15), None);
    }

    #[test]
    fn test_reboot_argv() {
        let argv = shutdown_argv(ShutdownRequest::Reboot);
        assert_eq!(argv[1], "-r");
        assert_eq!(argv[3], "CTRL-ALT_DEL");
    }

    #[test]
    fn test_powerdown_argv() {
        let argv = shutdown_argv(ShutdownRequest::Powerdown);
        assert_eq!(argv[1], "-p");
    }

    #[test]
    fn test_boot_argv_contract() {
        assert_eq!(boot_argv(), vec!["init".to_string()]);
    }
}
