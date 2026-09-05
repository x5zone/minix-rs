//! Host, service, and protocol database lookup.
//!
//! Ground truth: `minix3/etc/hosts` (name to address rows), `minix3/etc/services`
//! (service name to port and protocol rows), and `minix3/etc/protocols`
//! (protocol name to number rows). The network commands consume these files
//! but never parse them twice; this module owns the row shapes and the lookup
//! order (first match wins). File reading stays with the execution layer
//! behind the [`AddressDb`] trait.

use crate::NetconfigError;

/// One host row (name plus dotted decimal address).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostRow<'a> {
    /// Host name (`localhost`, `printer`, and so on).
    pub name: &'a str,
    /// Address in dotted decimal.
    pub address: &'a str,
}

/// One service row (name, port, transport protocol).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceRow<'a> {
    /// Service name (`http`, `ftp`, and so on).
    pub name: &'a str,
    /// Port number.
    pub port: u16,
    /// Transport protocol (`tcp` or `udp`).
    pub protocol: &'a str,
}

/// Parse one host row (`address name`, extra names ignored).
pub fn parse_host_row(line: &str) -> Result<HostRow<'_>, NetconfigError> {
    let mut words = line.split_ascii_whitespace();
    let address = words.next().ok_or(NetconfigError::InvalidArgument)?;
    let name = words.next().ok_or(NetconfigError::InvalidArgument)?;
    if address.starts_with('#') {
        return Err(NetconfigError::InvalidArgument);
    }
    crate::arp::check_dotted_decimal(address)?;
    Ok(HostRow { name, address })
}

/// Parse a decimal port number (1 through 65535).
pub fn parse_port(word: &str) -> Result<u16, NetconfigError> {
    if word.is_empty() {
        return Err(NetconfigError::InvalidArgument);
    }
    let mut value: u32 = 0;
    for byte in word.bytes() {
        if !byte.is_ascii_digit() {
            return Err(NetconfigError::InvalidArgument);
        }
        value = value
            .checked_mul(10)
            .and_then(|scaled| scaled.checked_add((byte - b'0') as u32))
            .ok_or(NetconfigError::InvalidArgument)?;
    }
    if value == 0 || value > 65535 {
        return Err(NetconfigError::InvalidArgument);
    }
    Ok(value as u16)
}

/// Database behind host and service lookups.
pub trait AddressDb<'a> {
    /// Address for `name`, or `None` when no row matches.
    fn address_of(&self, name: &str) -> Option<&'a str>;
    /// Port for (`service`, `protocol`), or `None` when no row matches.
    fn port_of(&self, service: &str, protocol: &str) -> Option<u16>;
}

/// Database backed by parallel slices (first match wins).
pub struct SliceDb<'a> {
    hosts: &'a [HostRow<'a>],
    services: &'a [ServiceRow<'a>],
}

impl<'a> SliceDb<'a> {
    /// Build a database over borrowed rows.
    pub fn new(hosts: &'a [HostRow<'a>], services: &'a [ServiceRow<'a>]) -> Self {
        SliceDb { hosts, services }
    }
}

impl<'a> AddressDb<'a> for SliceDb<'a> {
    fn address_of(&self, name: &str) -> Option<&'a str> {
        self.hosts
            .iter()
            .find(|row| row.name == name)
            .map(|row| row.address)
    }

    fn port_of(&self, service: &str, protocol: &str) -> Option<u16> {
        self.services
            .iter()
            .find(|row| row.name == service && row.protocol == protocol)
            .map(|row| row.port)
    }
}

/// Empty database (every lookup misses).
pub struct EmptyDb;

impl<'a> AddressDb<'a> for EmptyDb {
    fn address_of(&self, _name: &str) -> Option<&'a str> {
        None
    }

    fn port_of(&self, _service: &str, _protocol: &str) -> Option<u16> {
        None
    }
}

/// Look up a host through any database, reporting the Unix miss number.
pub fn lookup_host<'a, D: AddressDb<'a>>(db: &D, name: &str) -> Result<&'a str, NetconfigError> {
    if name.is_empty() {
        return Err(NetconfigError::InvalidArgument);
    }
    db.address_of(name).ok_or(NetconfigError::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_db() -> SliceDb<'static> {
        static HOSTS: &[HostRow<'static>] = &[
            HostRow { name: "localhost", address: "127.0.0.1" },
            HostRow { name: "printer", address: "192.168.1.20" },
        ];
        static SERVICES: &[ServiceRow<'static>] = &[
            ServiceRow { name: "http", port: 80, protocol: "tcp" },
            ServiceRow { name: "domain", port: 53, protocol: "udp" },
        ];
        SliceDb::new(HOSTS, SERVICES)
    }

    #[test]
    fn test_host_row_parses() {
        let row = parse_host_row("127.0.0.1 localhost").unwrap();
        assert_eq!(row.name, "localhost");
        assert_eq!(row.address, "127.0.0.1");
    }

    #[test]
    fn test_host_row_rejects_comments_and_blanks() {
        assert_eq!(
            parse_host_row("# comment"),
            Err(NetconfigError::InvalidArgument)
        );
        assert_eq!(parse_host_row(""), Err(NetconfigError::InvalidArgument));
        assert_eq!(
            parse_host_row("127.0.0.1"),
            Err(NetconfigError::InvalidArgument)
        );
    }

    #[test]
    fn test_port_bounds() {
        assert_eq!(parse_port("80"), Ok(80));
        assert_eq!(parse_port("0"), Err(NetconfigError::InvalidArgument));
        assert_eq!(parse_port("65536"), Err(NetconfigError::InvalidArgument));
        assert_eq!(parse_port("http"), Err(NetconfigError::InvalidArgument));
    }

    #[test]
    fn test_slice_db_finds_first_match() {
        let db = sample_db();
        assert_eq!(db.address_of("printer"), Some("192.168.1.20"));
        assert_eq!(db.port_of("http", "tcp"), Some(80));
        assert_eq!(db.port_of("http", "udp"), None);
        assert_eq!(db.address_of("unknown"), None);
    }

    #[test]
    fn test_empty_db_misses() {
        let db = EmptyDb;
        assert_eq!(db.address_of("localhost"), None);
        assert_eq!(db.port_of("http", "tcp"), None);
    }

    #[test]
    fn test_lookup_reports_not_found() {
        let db = sample_db();
        assert_eq!(lookup_host(&db, "printer"), Ok("192.168.1.20"));
        assert_eq!(
            lookup_host(&db, "ghost"),
            Err(NetconfigError::NotFound)
        );
        assert_eq!(
            lookup_host(&db, ""),
            Err(NetconfigError::InvalidArgument)
        );
    }

    #[test]
    fn test_error_numbers_match_unix() {
        assert_eq!(NetconfigError::InvalidArgument.as_errno(), 22);
        assert_eq!(NetconfigError::NotFound.as_errno(), 3);
        assert_eq!(NetconfigError::Unreachable.as_errno(), 51);
    }
}
