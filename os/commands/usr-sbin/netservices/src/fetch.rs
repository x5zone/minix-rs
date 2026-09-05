//! File fetching locator parsing.
//!
//! Ground truth: `minix3/minix/commands/fetch/fetch.c`, usage with `-o`
//! output file, `-T` timeout, `-h` host, `-f` file, and trailing uniform
//! resource locators near line 859. The execution layer owns the network
//! transfer; this module owns the locator words.

use crate::ServiceError;

/// Transfer scheme selected by the locator prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchScheme {
    /// Hypertext transfer.
    Http,
    /// File transfer.
    Ftp,
}

/// Parse a scheme word (`http` or `ftp`).
pub fn parse_scheme(word: &str) -> Result<FetchScheme, ServiceError> {
    match word {
        "http" => Ok(FetchScheme::Http),
        "ftp" => Ok(FetchScheme::Ftp),
        _ => Err(ServiceError::InvalidArgument),
    }
}

/// One parsed locator (scheme, host, port, path).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Locator<'a> {
    /// Transfer scheme.
    pub scheme: FetchScheme,
    /// Host name or address.
    pub host: &'a str,
    /// Port number (defaulted when the locator carries none).
    pub port: u16,
    /// Path on the host (always starts with `/`).
    pub path: &'a str,
}

/// Default port per scheme.
pub fn default_port(scheme: FetchScheme) -> u16 {
    match scheme {
        FetchScheme::Http => 80,
        FetchScheme::Ftp => 21,
    }
}

/// Parse a locator of the shape `scheme://host[:port]/path`.
pub fn parse_locator(text: &str) -> Result<Locator<'_>, ServiceError> {
    let (scheme_word, rest) = text.split_once("://").ok_or(ServiceError::InvalidArgument)?;
    let scheme = parse_scheme(scheme_word)?;
    let (authority, path) = match rest.find('/') {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, "/"),
    };
    if authority.is_empty() || path.is_empty() || !path.starts_with('/') {
        return Err(ServiceError::InvalidArgument);
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port_word)) if !host.is_empty() => {
            let port = fetch_port(port_word)?;
            (host, port)
        }
        _ => (authority, default_port(scheme)),
    };
    if host.is_empty() {
        return Err(ServiceError::InvalidArgument);
    }
    Ok(Locator {
        scheme,
        host,
        port,
        path,
    })
}

/// Parse a decimal port for a locator suffix.
pub fn fetch_port(word: &str) -> Result<u16, ServiceError> {
    if word.is_empty() {
        return Err(ServiceError::InvalidArgument);
    }
    let mut value: u32 = 0;
    for byte in word.bytes() {
        if !byte.is_ascii_digit() {
            return Err(ServiceError::InvalidArgument);
        }
        value = value
            .checked_mul(10)
            .and_then(|scaled| scaled.checked_add((byte - b'0') as u32))
            .ok_or(ServiceError::InvalidArgument)?;
    }
    if value == 0 || !(1..=65535).contains(&value) {
        return Err(ServiceError::InvalidArgument);
    }
    Ok(value as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schemes_parse() {
        assert_eq!(parse_scheme("http"), Ok(FetchScheme::Http));
        assert_eq!(parse_scheme("ftp"), Ok(FetchScheme::Ftp));
        assert_eq!(
            parse_scheme("gopher"),
            Err(ServiceError::InvalidArgument)
        );
    }

    #[test]
    fn test_locator_with_explicit_port() {
        let locator = parse_locator("http://example.com:8080/index.html").unwrap();
        assert_eq!(locator.scheme, FetchScheme::Http);
        assert_eq!(locator.host, "example.com");
        assert_eq!(locator.port, 8080);
        assert_eq!(locator.path, "/index.html");
    }

    #[test]
    fn test_locator_defaults_port_and_path() {
        let locator = parse_locator("ftp://files.example.com").unwrap();
        assert_eq!(locator.port, 21);
        assert_eq!(locator.path, "/");
    }

    #[test]
    fn test_locator_rejects_shapes() {
        assert_eq!(
            parse_locator("example.com/index"),
            Err(ServiceError::InvalidArgument)
        );
        assert_eq!(
            parse_locator("http:///index"),
            Err(ServiceError::InvalidArgument)
        );
        assert_eq!(
            parse_locator("http://host:0/index"),
            Err(ServiceError::InvalidArgument)
        );
    }
}
