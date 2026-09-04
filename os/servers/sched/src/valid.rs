//! SCHED sender names: who may knock.
//!
//! Mirrors `accept_message()` (`minix3/minix/servers/sched/utility.c:61-74`).
//! 04-schedproc-table.md.
//!
//! The module owns the role split and nothing else: which senders exist
//! and whether each may enter. Dispatch (02) and handlers (06~08) call
//! in; nobody else needs names for endpoints.

use minix_types::Endpoint;

/// Who knocks (`accept_message`, `utility.c:64-73`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sender {
    /// The process manager. C: `PM_PROC_NR` — utility.c:66.
    Pm,
    /// The reincarnation server. C: `RS_PROC_NR` — utility.c:67.
    Rs,
    /// Anyone else: not allowable (`72-73`).
    Other,
}

/// Name the sender.
///
/// `Endpoint::PM` is 0 and `Endpoint::RS` is 2 (`com.h:59-61`); every
/// other endpoint — kernel tasks, drivers, user processes — reads
/// `Other`. The name is the identity: no lookup, no table.
pub fn sender_from(source: Endpoint) -> Sender {
    if source == Endpoint::PM {
        Sender::Pm
    } else if source == Endpoint::RS {
        Sender::Rs
    } else {
        Sender::Other
    }
}

/// Whether the sender may enter (`utility.c:66-73`).
///
/// PM and RS pass; the list is closed — two names, forever. A table
/// would pretend the list varies; it does not.
pub fn accept(sender: Sender) -> bool {
    matches!(sender, Sender::Pm | Sender::Rs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_names() {
        // PM is 0, RS is 2 (`com.h:59-61`); the rest are Other.
        assert_eq!(sender_from(Endpoint::PM), Sender::Pm);
        assert_eq!(sender_from(Endpoint(0)), Sender::Pm);
        assert_eq!(sender_from(Endpoint::RS), Sender::Rs);
        assert_eq!(sender_from(Endpoint(2)), Sender::Rs);
        assert_eq!(sender_from(Endpoint::CLOCK), Sender::Other);
        assert_eq!(sender_from(Endpoint(100)), Sender::Other);
        assert_eq!(sender_from(Endpoint(-1)), Sender::Other);
    }

    #[test]
    fn test_closed_list() {
        // Two names pass, all others refuse (`utility.c:64-73`).
        assert!(accept(Sender::Pm));
        assert!(accept(Sender::Rs));
        assert!(!accept(Sender::Other));
    }
}
