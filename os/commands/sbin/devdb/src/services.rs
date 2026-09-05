//! Services file line format (`/etc/services`).
//!
//! Ground truth: `minix3/etc/services` (`name port/protocol aliases...`,
//! `#` starts a comment, blank lines skipped) read by the C library
//! `getservbyname` family and queried by `getent services`.

use crate::DevDbError;

/// Maximum aliases kept per service; more is a malformed line.
pub const MAX_ALIASES: usize = 8;

/// Transport protocol of a service port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    /// Transmission Control Protocol.
    Tcp,
    /// User Datagram Protocol.
    Udp,
}

/// One parsed services row, borrowed from the line it was parsed from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceEntry<'a> {
    /// Service name, for example `http`.
    pub name: &'a str,
    /// Port number in host order.
    pub port: u16,
    /// Transport protocol.
    pub protocol: Protocol,
    /// Alternative names.
    pub aliases: [&'a str; MAX_ALIASES],
    /// How many of `aliases` are used.
    pub alias_count: usize,
}

impl<'a> ServiceEntry<'a> {
    /// The aliases as a slice.
    pub fn alias_list(&self) -> &[&'a str] {
        &self.aliases[..self.alias_count]
    }
}

/// Parse one services line.
///
/// The second column has the shape `port/protocol`. Anything from `#`
/// onward is a comment and ignored. A line needs at least the name and the
/// port column.
pub fn parse_services_line<'a>(line: &'a str) -> Result<Option<ServiceEntry<'a>>, DevDbError> {
    let uncommented = match line.find('#') {
        Some(index) => &line[..index],
        None => line,
    };
    let trimmed = uncommented.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let mut words = trimmed.split_whitespace();
    let name = words.next().ok_or(DevDbError::InvalidArgument)?;
    let port_column = words.next().ok_or(DevDbError::InvalidArgument)?;
    let (port_text, protocol_text) = port_column
        .split_once('/')
        .ok_or(DevDbError::InvalidArgument)?;
    let mut entry = ServiceEntry {
        name: check_name(name)?,
        port: parse_port(port_text)?,
        protocol: parse_protocol(protocol_text)?,
        aliases: [""; MAX_ALIASES],
        alias_count: 0,
    };
    for alias in words {
        if entry.alias_count >= MAX_ALIASES {
            return Err(DevDbError::InvalidArgument);
        }
        entry.aliases[entry.alias_count] = check_name(alias)?;
        entry.alias_count += 1;
    }
    Ok(Some(entry))
}

fn check_name(name: &str) -> Result<&str, DevDbError> {
    if name.is_empty() {
        return Err(DevDbError::InvalidArgument);
    }
    let ok = name.bytes().all(|b| {
        b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.' || b == b'+'
    });
    if ok {
        Ok(name)
    } else {
        Err(DevDbError::InvalidArgument)
    }
}

fn parse_port(text: &str) -> Result<u16, DevDbError> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(DevDbError::InvalidArgument);
    }
    let mut value: u32 = 0;
    for byte in text.bytes() {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
            .ok_or(DevDbError::InvalidArgument)?;
    }
    if value > u16::MAX as u32 {
        return Err(DevDbError::InvalidArgument);
    }
    Ok(value as u16)
}

fn parse_protocol(text: &str) -> Result<Protocol, DevDbError> {
    match text {
        "tcp" => Ok(Protocol::Tcp),
        "udp" => Ok(Protocol::Udp),
        _ => Err(DevDbError::InvalidArgument),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_entry() {
        let entry = parse_services_line("http\t\t80/tcp\t\twww www-http #World Wide Web")
            .unwrap()
            .unwrap();
        assert_eq!(entry.name, "http");
        assert_eq!(entry.port, 80);
        assert_eq!(entry.protocol, Protocol::Tcp);
        assert_eq!(entry.alias_list(), &["www", "www-http"]);
    }

    #[test]
    fn test_udp_entry_without_aliases() {
        let entry = parse_services_line("domain 53/udp").unwrap().unwrap();
        assert_eq!(entry.port, 53);
        assert_eq!(entry.protocol, Protocol::Udp);
        assert_eq!(entry.alias_count, 0);
    }

    #[test]
    fn test_comment_and_blank_skipped() {
        assert_eq!(parse_services_line("# services"), Ok(None));
        assert_eq!(parse_services_line(""), Ok(None));
        assert_eq!(parse_services_line("   # indented"), Ok(None));
    }

    #[test]
    fn test_missing_protocol_rejected() {
        assert_eq!(
            parse_services_line("http 80"),
            Err(DevDbError::InvalidArgument)
        );
    }

    #[test]
    fn test_unknown_protocol_rejected() {
        assert_eq!(
            parse_services_line("http 80/sctp"),
            Err(DevDbError::InvalidArgument)
        );
    }

    #[test]
    fn test_oversized_port_rejected() {
        assert_eq!(
            parse_services_line("big 99999/tcp"),
            Err(DevDbError::InvalidArgument)
        );
    }

    #[test]
    fn test_missing_columns_rejected() {
        assert_eq!(parse_services_line("http"), Err(DevDbError::InvalidArgument));
    }
}
