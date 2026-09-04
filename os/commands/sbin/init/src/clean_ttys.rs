//! Clean-ttys state ('T'): re-read /etc/ttys.
//!
//! Covers `minix3/sbin/init/init.c:1569-1629` (`clean_ttys`).
//! Design contract: `.design/10-design.v1.md §1.1`.

/// What to do with one session line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineAction {
    Keep,
    ShutdownHup,
    CreateNew,
    RetireHup,
}

/// Diff one line (pure).
///
/// `known`: session exists; `in_file`: line still present;
/// `on`: TTY_ON and non-empty getty.
pub fn diff_line(known: bool, in_file: bool, on: bool) -> LineAction {
    if !known {
        return LineAction::CreateNew;
    }
    if !in_file {
        return LineAction::RetireHup;
    }
    if !on {
        return LineAction::ShutdownHup;
    }
    LineAction::Keep
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_on_keeps() {
        assert_eq!(diff_line(true, true, true), LineAction::Keep);
    }

    #[test]
    fn test_known_off_shutdowns() {
        assert_eq!(diff_line(true, true, false), LineAction::ShutdownHup);
    }

    #[test]
    fn test_unknown_creates() {
        assert_eq!(diff_line(false, true, true), LineAction::CreateNew);
    }

    #[test]
    fn test_missing_retires() {
        assert_eq!(diff_line(true, false, true), LineAction::RetireHup);
    }
}
