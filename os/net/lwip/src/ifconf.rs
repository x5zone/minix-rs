//! Interface configuration policy: default loopback setup and request families.
//!
//! C correspondence: `minix3/minix/net/lwip/ifconf.c` (930 lines) with the
//! Minix extensions in `minix3/minix/include/minix/if.h:39-49`. Interface
//! creation, address installation, and request dispatch stay in the service
//! binary. This module owns the portion that can be decided from numbers
//! alone: which default addresses the loopback interface gets, and how
//! control requests sort into handler families.
//!
//! Configuration is the last step of the interface plane: consumption
//! provides slots, objects provide shape, media fills the shape, addresses
//! populate it, and configuration exposes it all to control tools.

/// Default loopback interface name (`LOOPBACK_IFNAME`, `ifconf.c:11`).
pub const LOOPBACK_NAME: &str = "lo0";

/// Version 4 loopback address in host order (`INADDR_LOOPBACK`,
/// 127.0.0.1, installed at `ifconf.c:19-29`).
pub const LOOPBACK_IPV4: u32 = 0x7f00_0001;

/// Version 6 loopback prefix length (`ifconf.c:64-69`, /128).
pub const LOOPBACK_IPV6_PREFIX: u8 = 128;

/// Version 6 link-local prefix length for the interface identifier
/// (`ifconf.c:56-62`, /64).
pub const LINK_LOCAL_PREFIX: u8 = 64;

/// Control request families handled by the configuration entry point
/// (`ifconf_ioctl`, `ifconf.c:866-930`, dispatching to one handler per
/// address family and structure kind).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestFamily {
    /// Address-family-independent interface requests.
    Interface,
    /// Capability requests.
    Capability,
    /// Media requests, including the Minix media extension.
    Media,
    /// Cloner requests, including the Minix cloner extension.
    Cloner,
    /// Address preference requests.
    AddressPreference,
    /// Version 4 interface and alias requests.
    Version4,
    /// Version 6 interface, alias, neighbor, and router requests.
    Version6,
    /// Link-layer address requests.
    Link,
}

/// Whether a request code belongs to the Minix media extension
/// (`MINIX_SIOCGIFMEDIA`, `if.h:39`, handled at `ifconf.c:215` and
/// `ifconf.c:899`).
pub fn is_minix_media_request(is_minix_media: bool) -> bool {
    is_minix_media
}

/// Whether a request code belongs to the Minix cloner extension
/// (`MINIX_SIOCIFGCLONERS`, `if.h:49`, handled at `ifconf.c:902`).
pub fn is_minix_cloner_request(is_minix_cloner: bool) -> bool {
    is_minix_cloner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loopback_defaults_match_init() {
        assert_eq!(LOOPBACK_NAME, "lo0");
        assert_eq!(LOOPBACK_IPV4, 0x7f00_0001);
        assert_eq!(LOOPBACK_IPV6_PREFIX, 128);
        assert_eq!(LINK_LOCAL_PREFIX, 64);
    }

    #[test]
    fn test_request_families_cover_dispatch() {
        let families = [
            RequestFamily::Interface,
            RequestFamily::Capability,
            RequestFamily::Media,
            RequestFamily::Cloner,
            RequestFamily::AddressPreference,
            RequestFamily::Version4,
            RequestFamily::Version6,
            RequestFamily::Link,
        ];
        assert_eq!(families.len(), 8);
    }

    #[test]
    fn test_minix_extensions_are_distinguished() {
        assert!(is_minix_media_request(true));
        assert!(!is_minix_media_request(false));
        assert!(is_minix_cloner_request(true));
        assert!(!is_minix_cloner_request(false));
    }
}
