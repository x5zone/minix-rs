//! Routing table vocabulary and lookup.
//!
//! Ground truth: `minix3/sbin/route/route.c` (routing socket verbs) and
//! `minix3/usr.sbin/arp/arp.c` (neighbor verbs reuse the same routing socket:
//! entry lookup with `RTM_GET` near line 315, entry creation with `RTM_ADD`
//! near line 350, entry removal with `RTM_DELETE` near line 434, the version
//! stamp `RTM_VERSION` near line 642). The execution layer owns the routing
//! socket; this module owns the verbs and the table.

use crate::NetconfigError;

/// Routing verbs understood by the route command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteVerb {
    /// Add a route.
    Add,
    /// Delete a route.
    Delete,
    /// Change a route.
    Change,
    /// Look up a route.
    Get,
    /// Remove all routes.
    Flush,
    /// Watch routing socket announcements.
    Monitor,
}

/// Parse a routing verb word.
pub fn parse_route_verb(word: &str) -> Result<RouteVerb, NetconfigError> {
    match word {
        "add" => Ok(RouteVerb::Add),
        "delete" => Ok(RouteVerb::Delete),
        "change" => Ok(RouteVerb::Change),
        "get" => Ok(RouteVerb::Get),
        "flush" => Ok(RouteVerb::Flush),
        "monitor" => Ok(RouteVerb::Monitor),
        _ => Err(NetconfigError::InvalidArgument),
    }
}

/// One route entry (destination, gateway, outgoing interface).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteEntry<'a> {
    /// Destination network in dotted decimal (`192.168.1.0`).
    pub destination: &'a str,
    /// Gateway in dotted decimal, or `None` for a direct route.
    pub gateway: Option<&'a str>,
    /// Outgoing interface name, or `None` when the kernel may choose.
    pub interface: Option<&'a str>,
}

/// Routing table behind the verbs.
pub trait RouteTable<'a> {
    /// Look up the route for `destination`.
    fn lookup(&self, destination: &str) -> Option<RouteEntry<'a>>;
    /// Insert or replace a route.
    fn insert(&mut self, entry: RouteEntry<'a>) -> Result<(), NetconfigError>;
    /// Remove the route for `destination`.
    fn remove(&mut self, destination: &str) -> Result<(), NetconfigError>;
}

/// In-memory routing table backed by parallel slices with a fixed capacity.
pub struct MemoryRouteTable<'a> {
    destinations: [&'a str; 16],
    gateways: [Option<&'a str>; 16],
    interfaces: [Option<&'a str>; 16],
    count: usize,
}

impl<'a> MemoryRouteTable<'a> {
    /// An empty table.
    pub fn new() -> Self {
        MemoryRouteTable {
            destinations: [""; 16],
            gateways: [None; 16],
            interfaces: [None; 16],
            count: 0,
        }
    }

    /// Number of stored routes.
    pub fn len(&self) -> usize {
        self.count
    }

    /// True when no route is stored.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

impl<'a> Default for MemoryRouteTable<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> RouteTable<'a> for MemoryRouteTable<'a> {
    fn lookup(&self, destination: &str) -> Option<RouteEntry<'a>> {
        for index in 0..self.count {
            if self.destinations[index] == destination {
                return Some(RouteEntry {
                    destination: self.destinations[index],
                    gateway: self.gateways[index],
                    interface: self.interfaces[index],
                });
            }
        }
        None
    }

    fn insert(&mut self, entry: RouteEntry<'a>) -> Result<(), NetconfigError> {
        if entry.destination.is_empty() {
            return Err(NetconfigError::InvalidArgument);
        }
        for index in 0..self.count {
            if self.destinations[index] == entry.destination {
                self.gateways[index] = entry.gateway;
                self.interfaces[index] = entry.interface;
                return Ok(());
            }
        }
        if self.count >= self.destinations.len() {
            return Err(NetconfigError::InvalidArgument);
        }
        self.destinations[self.count] = entry.destination;
        self.gateways[self.count] = entry.gateway;
        self.interfaces[self.count] = entry.interface;
        self.count += 1;
        Ok(())
    }

    fn remove(&mut self, destination: &str) -> Result<(), NetconfigError> {
        for index in 0..self.count {
            if self.destinations[index] == destination {
                self.count -= 1;
                self.destinations[index] = self.destinations[self.count];
                self.gateways[index] = self.gateways[self.count];
                self.interfaces[index] = self.interfaces[self.count];
                return Ok(());
            }
        }
        Err(NetconfigError::NotFound)
    }
}

/// Empty routing table (no route behind any destination).
pub struct EmptyRouteTable;

impl<'a> RouteTable<'a> for EmptyRouteTable {
    fn lookup(&self, _destination: &str) -> Option<RouteEntry<'a>> {
        None
    }

    fn insert(&mut self, _entry: RouteEntry<'a>) -> Result<(), NetconfigError> {
        Err(NetconfigError::Unreachable)
    }

    fn remove(&mut self, _destination: &str) -> Result<(), NetconfigError> {
        Err(NetconfigError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verbs_parse() {
        assert_eq!(parse_route_verb("add"), Ok(RouteVerb::Add));
        assert_eq!(parse_route_verb("delete"), Ok(RouteVerb::Delete));
        assert_eq!(parse_route_verb("change"), Ok(RouteVerb::Change));
        assert_eq!(parse_route_verb("get"), Ok(RouteVerb::Get));
        assert_eq!(parse_route_verb("flush"), Ok(RouteVerb::Flush));
        assert_eq!(parse_route_verb("monitor"), Ok(RouteVerb::Monitor));
        assert_eq!(
            parse_route_verb("teleport"),
            Err(NetconfigError::InvalidArgument)
        );
    }

    #[test]
    fn test_insert_and_lookup() {
        let mut table = MemoryRouteTable::new();
        table
            .insert(RouteEntry {
                destination: "192.168.1.0",
                gateway: Some("192.168.1.1"),
                interface: Some("eth0"),
            })
            .unwrap();
        let found = table.lookup("192.168.1.0").unwrap();
        assert_eq!(found.gateway, Some("192.168.1.1"));
        assert!(table.lookup("10.0.0.0").is_none());
    }

    #[test]
    fn test_insert_replaces() {
        let mut table = MemoryRouteTable::new();
        table
            .insert(RouteEntry {
                destination: "default",
                gateway: Some("192.168.1.1"),
                interface: None,
            })
            .unwrap();
        table
            .insert(RouteEntry {
                destination: "default",
                gateway: Some("10.0.0.1"),
                interface: None,
            })
            .unwrap();
        assert_eq!(table.len(), 1);
        assert_eq!(table.lookup("default").unwrap().gateway, Some("10.0.0.1"));
    }

    #[test]
    fn test_remove_missing_reports_not_found() {
        let mut table = MemoryRouteTable::new();
        assert_eq!(
            table.remove("192.168.1.0"),
            Err(NetconfigError::NotFound)
        );
    }

    #[test]
    fn test_empty_table_misses() {
        let mut table = EmptyRouteTable;
        assert!(table.lookup("default").is_none());
        assert_eq!(
            table.remove("default"),
            Err(NetconfigError::NotFound)
        );
    }
}
