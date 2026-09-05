//! Superserver service table.
//!
//! Ground truth: `minix3/usr.sbin/inetd/inetd.c`. At most `OPEN_MAX 64`
//! sockets are served (near line 276), each server takes at most `MAXARGV 20`
//! arguments (near line 306). Built-in services handle echo, discard, daytime,
//! and character generation over both stream and datagram sockets (near lines
//! 339 to 348). The execution layer owns listening sockets and process
//! creation; this module owns the rows and the lookup.

use crate::ServiceError;

/// Largest number of served sockets.
pub const MAX_SERVED: usize = 64;

/// Largest number of server arguments (the program name counts as one).
pub const MAX_ARGUMENTS: usize = 20;

/// Socket kind of a service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketKind {
    /// Sequenced bidirectional byte stream.
    Stream,
    /// Unreliable datagram packets.
    Datagram,
}

/// Parse a socket kind word.
pub fn parse_socket_kind(word: &str) -> Result<SocketKind, ServiceError> {
    match word {
        "stream" => Ok(SocketKind::Stream),
        "dgram" => Ok(SocketKind::Datagram),
        _ => Err(ServiceError::InvalidArgument),
    }
}

/// Wait mode of a service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitMode {
    /// The server handles one datagram socket itself (single threaded).
    Wait,
    /// The superserver forks one process per connection.
    NoWait,
}

/// Parse a wait mode word.
pub fn parse_wait_mode(word: &str) -> Result<WaitMode, ServiceError> {
    match word {
        "wait" => Ok(WaitMode::Wait),
        "nowait" => Ok(WaitMode::NoWait),
        _ => Err(ServiceError::InvalidArgument),
    }
}

/// One service table row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceEntry<'a> {
    /// Service name (`ftp`, `telnet`, `echo`, and so on).
    pub name: &'a str,
    /// Socket kind.
    pub socket: SocketKind,
    /// Transport protocol (`tcp` or `udp`).
    pub protocol: &'a str,
    /// Wait mode.
    pub wait: WaitMode,
    /// User the server runs as.
    pub user: &'a str,
    /// Server program path, or `internal` for built-in services.
    pub server: &'a str,
}

/// Built-in services handled inside the superserver (no fork, no program).
pub const BUILT_IN_SERVICES: &[&str] = &["echo", "discard", "daytime", "chargen"];

/// True when `name` is handled inside the superserver.
pub fn is_built_in(name: &str) -> bool {
    BUILT_IN_SERVICES.contains(&name)
}

/// Service table behind the superserver.
pub trait ServiceTable<'a> {
    /// Row for `name` over `protocol`, or `None` when absent.
    fn find(&self, name: &str, protocol: &str) -> Option<ServiceEntry<'a>>;
    /// Number of stored rows.
    fn len(&self) -> usize;
    /// True when no row is stored.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Table backed by a borrowed slice (first match wins).
pub struct SliceServiceTable<'a> {
    rows: &'a [ServiceEntry<'a>],
}

impl<'a> SliceServiceTable<'a> {
    /// Build a table over borrowed rows.
    pub fn new(rows: &'a [ServiceEntry<'a>]) -> Self {
        SliceServiceTable { rows }
    }
}

impl<'a> ServiceTable<'a> for SliceServiceTable<'a> {
    fn find(&self, name: &str, protocol: &str) -> Option<ServiceEntry<'a>> {
        self.rows
            .iter()
            .find(|row| row.name == name && row.protocol == protocol)
            .copied()
    }

    fn len(&self) -> usize {
        self.rows.len()
    }
}

/// Empty table (every lookup misses).
pub struct EmptyServiceTable;

impl<'a> ServiceTable<'a> for EmptyServiceTable {
    fn find(&self, _name: &str, _protocol: &str) -> Option<ServiceEntry<'a>> {
        None
    }

    fn len(&self) -> usize {
        0
    }
}

/// Look up a service, reporting the Unix miss number on absence.
pub fn lookup_service<'a, T: ServiceTable<'a>>(
    table: &T,
    name: &str,
    protocol: &str,
) -> Result<ServiceEntry<'a>, ServiceError> {
    if name.is_empty() || protocol.is_empty() {
        return Err(ServiceError::InvalidArgument);
    }
    table.find(name, protocol).ok_or(ServiceError::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_rows() -> [ServiceEntry<'static>; 3] {
        [
            ServiceEntry {
                name: "ftp",
                socket: SocketKind::Stream,
                protocol: "tcp",
                wait: WaitMode::NoWait,
                user: "root",
                server: "/usr/libexec/ftpd",
            },
            ServiceEntry {
                name: "telnet",
                socket: SocketKind::Stream,
                protocol: "tcp",
                wait: WaitMode::NoWait,
                user: "root",
                server: "/usr/libexec/telnetd",
            },
            ServiceEntry {
                name: "echo",
                socket: SocketKind::Datagram,
                protocol: "udp",
                wait: WaitMode::Wait,
                user: "nobody",
                server: "internal",
            },
        ]
    }

    #[test]
    fn test_socket_kinds_parse() {
        assert_eq!(parse_socket_kind("stream"), Ok(SocketKind::Stream));
        assert_eq!(parse_socket_kind("dgram"), Ok(SocketKind::Datagram));
        assert_eq!(
            parse_socket_kind("carrier"),
            Err(ServiceError::InvalidArgument)
        );
    }

    #[test]
    fn test_wait_modes_parse() {
        assert_eq!(parse_wait_mode("wait"), Ok(WaitMode::Wait));
        assert_eq!(parse_wait_mode("nowait"), Ok(WaitMode::NoWait));
        assert_eq!(
            parse_wait_mode("sometime"),
            Err(ServiceError::InvalidArgument)
        );
    }

    #[test]
    fn test_built_in_detected() {
        assert!(is_built_in("echo"));
        assert!(is_built_in("discard"));
        assert!(is_built_in("daytime"));
        assert!(is_built_in("chargen"));
        assert!(!is_built_in("ftp"));
    }

    #[test]
    fn test_slice_table_finds() {
        let rows = sample_rows();
        let table = SliceServiceTable::new(&rows);
        assert_eq!(table.len(), 3);
        let found = lookup_service(&table, "ftp", "tcp").unwrap();
        assert_eq!(found.server, "/usr/libexec/ftpd");
        assert_eq!(
            lookup_service(&table, "ftp", "udp"),
            Err(ServiceError::NotFound)
        );
    }

    #[test]
    fn test_empty_table_misses() {
        let table = EmptyServiceTable;
        assert_eq!(table.len(), 0);
        assert_eq!(
            lookup_service(&table, "ftp", "tcp"),
            Err(ServiceError::NotFound)
        );
    }

    #[test]
    fn test_blank_names_rejected() {
        let rows = sample_rows();
        let table = SliceServiceTable::new(&rows);
        assert_eq!(
            lookup_service(&table, "", "tcp"),
            Err(ServiceError::InvalidArgument)
        );
    }

    #[test]
    fn test_limits_match_source() {
        assert_eq!(MAX_SERVED, 64);
        assert_eq!(MAX_ARGUMENTS, 20);
    }
}
