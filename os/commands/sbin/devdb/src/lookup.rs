//! The lookup trait: one interface, one implementation per database.
//!
//! The C `getent` command (`minix3/usr.bin/getent/getent.c`) answers
//! `getent database key` for many databases (group, hosts, services,
//! protocols, shells, ...) through one argument shape. Each database parses
//! differently but looks up the same way: by name. So every database in
//! this crate implements [`LookupTable`], and the program layer dispatches
//! on the database word while sharing the miss handling. This mirrors the
//! Redox approach of one behaviour interface behind several data sources.

use crate::DevDbError;
use crate::group::{GroupEntry, parse_group_line};
use crate::services::{ServiceEntry, parse_services_line};

/// Read only name lookup over one system database.
pub trait LookupTable {
    /// The row type this database returns.
    type Row<'a>
    where
        Self: 'a;
    /// Find the row called `key`, or `None` when it does not exist.
    ///
    /// Corrupt rows are skipped, never fatal: a single bad line must not
    /// hide the rest of the database.
    fn lookup<'a>(&'a self, key: &str) -> Option<Self::Row<'a>>;
}

/// The group database: keyed by group name.
pub struct GroupTable<'a> {
    /// Raw group file lines searched in order; the first match wins.
    pub lines: &'a [&'a str],
}

impl LookupTable for GroupTable<'_> {
    type Row<'b> = GroupEntry<'b> where Self: 'b;

    fn lookup<'a>(&'a self, key: &str) -> Option<GroupEntry<'a>> {
        self.lines.iter().find_map(|line| {
            let entry = parse_group_line(line).ok()?;
            (entry.name == key).then_some(entry)
        })
    }
}

/// The services database: keyed by service name or alias.
///
/// Unlike groups, a service answers to several names: the primary name plus
/// every alias. Looking up `www` finds the `http` row, exactly as the C
/// `getservbyname` resolves aliases.
pub struct ServicesTable<'a> {
    /// Raw services file lines searched in order; the first match wins.
    pub lines: &'a [&'a str],
}

impl LookupTable for ServicesTable<'_> {
    type Row<'b> = ServiceEntry<'b> where Self: 'b;

    fn lookup<'a>(&'a self, key: &str) -> Option<ServiceEntry<'a>> {
        self.lines.iter().find_map(|line| {
            let entry = parse_services_line(line).ok()??;
            if entry.name == key || entry.alias_list().contains(&key) {
                Some(entry)
            } else {
                None
            }
        })
    }
}

/// Shared miss handling for the `getent` program layer: look up `key` and
/// translate the miss into [`DevDbError::NotFound`].
pub fn getent<'a, T: LookupTable>(table: &'a T, key: &str) -> Result<T::Row<'a>, DevDbError> {
    if key.is_empty() {
        return Err(DevDbError::InvalidArgument);
    }
    table.lookup(key).ok_or(DevDbError::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GROUPS: [&str; 2] = ["wheel:x:0:root,operator", "daemon:x:1:"];
    const SERVICES: [&str; 3] = [
        "http\t\t80/tcp\t\twww",
        "# comment",
        "domain 53/udp",
    ];

    #[test]
    fn test_group_lookup_by_name() {
        let table = GroupTable { lines: &GROUPS };
        let entry = table.lookup("wheel").unwrap();
        assert_eq!(entry.group_id, 0);
        assert_eq!(table.lookup("ghost"), None);
    }

    #[test]
    fn test_services_lookup_by_alias() {
        let table = ServicesTable { lines: &SERVICES };
        let entry = table.lookup("www").unwrap();
        assert_eq!(entry.name, "http");
        assert_eq!(entry.port, 80);
    }

    #[test]
    fn test_services_lookup_skips_comments() {
        let table = ServicesTable { lines: &SERVICES };
        assert!(table.lookup("domain").is_some());
        assert_eq!(table.lookup("#"), None);
    }

    #[test]
    fn test_getent_maps_miss() {
        let table = GroupTable { lines: &GROUPS };
        assert_eq!(getent(&table, "ghost"), Err(DevDbError::NotFound));
        assert_eq!(getent(&table, ""), Err(DevDbError::InvalidArgument));
    }

    #[test]
    fn test_tables_are_interchangeable_by_shape() {
        let groups = GroupTable { lines: &GROUPS };
        let services = ServicesTable { lines: &SERVICES };
        assert!(getent(&groups, "daemon").is_ok());
        assert!(getent(&services, "domain").is_ok());
    }
}
