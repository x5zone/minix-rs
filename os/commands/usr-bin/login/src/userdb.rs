//! The user database trait: one interface, one implementation per stage.
//!
//! The C login chain reads accounts through the `getpwnam` family, backed
//! first by flat files and later (via `pwd_mkdb`) by a hashed database.
//! The Rust side cannot assume either store exists yet: password storage and
//! authentication belong to the process manager stage. So lookup goes
//! through [`UserDatabase`], with one implementation per deployment stage.
//! The program layer depends on the trait and stays unchanged while the
//! store underneath evolves — the same dependency inversion Redox applies
//! between its user programs and its schemes.

use crate::LoginError;
use crate::passwd::{PasswdEntry, parse_passwd_line};

/// Read only account lookup used by the login chain.
pub trait UserDatabase {
    /// Find the account called `name`, or `None` when it does not exist.
    fn find_user(&self, name: &str) -> Option<PasswdEntry<'_>>;
}

/// A database that knows nobody: every lookup misses.
///
/// This is the honest starting point. Until the process manager stage owns
/// authentication, refusing unknown users explicitly (`NotFound`) is safer
/// than guessing. The login program maps the miss to its retry loop, exactly
/// as the C `login` re-prompts after a failed lookup.
pub struct EmptyDatabase;

impl UserDatabase for EmptyDatabase {
    fn find_user(&self, _name: &str) -> Option<PasswdEntry<'_>> {
        None
    }
}

/// A database over an in memory list of `passwd` text lines.
///
/// Each line is parsed on demand, so a corrupt line never poisons the whole
/// table: it is skipped and the search continues. This mirrors how the C
/// library keeps scanning the file past a malformed row instead of aborting
/// the lookup.
pub struct SliceDatabase<'a> {
    /// Raw `passwd` lines searched in order; the first name match wins.
    pub lines: &'a [&'a str],
}

impl UserDatabase for SliceDatabase<'_> {
    fn find_user(&self, name: &str) -> Option<PasswdEntry<'_>> {
        self.lines.iter().find_map(|line| {
            let entry = parse_passwd_line(line).ok()?;
            (entry.name == name).then_some(entry)
        })
    }
}

/// The outcome of identifying one login attempt: found the account, or the
/// reason it cannot proceed.
pub fn identify<'a, D: UserDatabase>(
    database: &'a D,
    name: &str,
) -> Result<PasswdEntry<'a>, LoginError> {
    if name.is_empty() {
        return Err(LoginError::InvalidArgument);
    }
    database.find_user(name).ok_or(LoginError::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: [&str; 3] = [
        "root:x:0:0:System Owner:/root:/bin/sh",
        "# a comment line the scan must skip",
        "bob:x:1001:100::/home/bob:/bin/sh",
    ];

    #[test]
    fn test_empty_database_knows_nobody() {
        let database = EmptyDatabase;
        assert_eq!(database.find_user("root"), None);
    }

    #[test]
    fn test_slice_database_finds_account() {
        let database = SliceDatabase { lines: &TABLE };
        let entry = database.find_user("bob").unwrap();
        assert_eq!(entry.user_id, 1001);
        assert_eq!(entry.home, "/home/bob");
    }

    #[test]
    fn test_slice_database_skips_comment_lines() {
        let database = SliceDatabase { lines: &TABLE };
        assert_eq!(database.find_user("#"), None);
        assert!(database.find_user("root").is_some());
    }

    #[test]
    fn test_identify_maps_missing_user() {
        let database = SliceDatabase { lines: &TABLE };
        assert_eq!(identify(&database, "ghost"), Err(LoginError::NotFound));
    }

    #[test]
    fn test_identify_rejects_empty_name() {
        let database = SliceDatabase { lines: &TABLE };
        assert_eq!(identify(&database, ""), Err(LoginError::InvalidArgument));
    }

    #[test]
    fn test_trait_objects_are_interchangeable() {
        let empty = EmptyDatabase;
        let slice = SliceDatabase { lines: &TABLE };
        let databases: [&dyn UserDatabase; 2] = [&empty, &slice];
        let hits: usize = databases
            .iter()
            .filter(|database| database.find_user("root").is_some())
            .count();
        assert_eq!(hits, 1);
    }
}
