//! Multi-user steady state ('m').
//!
//! Covers `minix3/sbin/init/init.c:1528-1564` (`multi_user`),
//! `1321-1370` (`start_getty`), `1290-1316` (`start_window_system`),
//! `669-689` (`setctty`), `1460-1497` (`collect_child`).
//! Design contract: `.design/09-design.v1.md §1.1-§1.3`.

/// Minimum getty spacing in seconds (C: `GETTY_SPACING`, init.c:92).
pub const GETTY_SPACING_SECS: i64 = 5;
/// Sleep after spacing violation (C: `GETTY_SLEEP`, init.c:93).
pub const GETTY_SLEEP_SECS: u64 = 30;
/// Wait after starting window system (C: `WINDOW_WAIT`, init.c:94).
pub const WINDOW_WAIT_SECS: u64 = 3;

/// How long to delay a getty start (C: init.c:1350-1355).
pub fn getty_delay_secs(now_secs: i64, started_secs: i64) -> u64 {
    if now_secs > started_secs && now_secs - started_secs < GETTY_SPACING_SECS {
        GETTY_SLEEP_SECS
    } else {
        0
    }
}

/// What `collect_child` should do (C: init.c:1466-1495).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectAction {
    Ignore,
    RemoveSession,
    RestartSession,
    RequestCleanTtys,
}

/// Classify one reaped child.
pub fn classify_collect(known: bool, shutdown: bool, spawn_ok: bool) -> CollectAction {
    if !known {
        return CollectAction::Ignore;
    }
    if shutdown {
        return CollectAction::RemoveSession;
    }
    if !spawn_ok {
        return CollectAction::RequestCleanTtys;
    }
    CollectAction::RestartSession
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spacing_triggers_sleep() {
        assert_eq!(getty_delay_secs(100, 98), GETTY_SLEEP_SECS);
    }

    #[test]
    fn test_spacing_no_sleep() {
        assert_eq!(getty_delay_secs(100, 90), 0);
        assert_eq!(getty_delay_secs(100, 100), 0);
    }

    #[test]
    fn test_collect_restarts() {
        assert_eq!(
            classify_collect(true, false, true),
            CollectAction::RestartSession
        );
    }

    #[test]
    fn test_collect_removes_shutdown() {
        assert_eq!(
            classify_collect(true, true, true),
            CollectAction::RemoveSession
        );
    }

    #[test]
    fn test_collect_unknown_ignores() {
        assert_eq!(
            classify_collect(false, false, true),
            CollectAction::Ignore
        );
    }

    #[test]
    fn test_collect_spawn_failure_requests_clean() {
        assert_eq!(
            classify_collect(true, false, false),
            CollectAction::RequestCleanTtys
        );
    }
}
