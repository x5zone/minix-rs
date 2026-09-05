//! System parameter names and the parameter table.
//!
//! Ground truth: `minix3/sbin/sysctl/sysctl.c`. The tree root carries the
//! node type `CTLTYPE_NODE` (near line 520, root flags near line 252).
//! Parameters are dotted names (`kern.hostname`, `net.inet.ip.forwarding`);
//! assignments carry `name=value`. The execution layer owns the kernel query;
//! this module owns the names and the table.

use crate::SysinfoError;

/// Largest dotted name depth accepted (prevents runaway recursion).
pub const MAX_NAME_DEPTH: usize = 8;

/// Largest single label length accepted.
pub const MAX_LABEL_LENGTH: usize = 32;

/// Check one dotted parameter name (`kern.hostname`).
pub fn check_sysctl_name(name: &str) -> Result<(), SysinfoError> {
    if name.is_empty() {
        return Err(SysinfoError::InvalidArgument);
    }
    let mut depth = 0;
    for label in name.split('.') {
        if label.is_empty() || label.len() > MAX_LABEL_LENGTH {
            return Err(SysinfoError::InvalidArgument);
        }
        if !label.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'
        }) {
            return Err(SysinfoError::InvalidArgument);
        }
        depth += 1;
    }
    if depth > MAX_NAME_DEPTH {
        return Err(SysinfoError::InvalidArgument);
    }
    Ok(())
}

/// Split one `name=value` assignment into its halves.
pub fn split_assignment(text: &str) -> Result<(&str, &str), SysinfoError> {
    let (name, value) = text.split_once('=').ok_or(SysinfoError::InvalidArgument)?;
    if name.is_empty() || value.is_empty() {
        return Err(SysinfoError::InvalidArgument);
    }
    check_sysctl_name(name)?;
    Ok((name, value))
}

/// One parameter row (name plus textual value).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SysctlRow<'a> {
    /// Dotted parameter name.
    pub name: &'a str,
    /// Textual value.
    pub value: &'a str,
}

/// Parameter table behind queries and assignments.
pub trait SysctlTable<'a> {
    /// Value for `name`, or `None` when absent.
    fn get(&self, name: &str) -> Option<&'a str>;
    /// Assign `value` to `name`.
    fn set(&mut self, name: &'a str, value: &'a str) -> Result<(), SysinfoError>;
}

/// Table backed by a fixed array (capacity sixteen rows).
pub struct MemorySysctlTable<'a> {
    names: [&'a str; 16],
    values: [&'a str; 16],
    count: usize,
}

impl<'a> MemorySysctlTable<'a> {
    /// An empty table.
    pub fn new() -> Self {
        MemorySysctlTable {
            names: [""; 16],
            values: [""; 16],
            count: 0,
        }
    }

    /// Number of stored rows. (Array tail beyond `count` is unused.)
    pub fn len(&self) -> usize {
        self.count
    }

    /// True when no row is stored.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

impl<'a> Default for MemorySysctlTable<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> SysctlTable<'a> for MemorySysctlTable<'a> {
    fn get(&self, name: &str) -> Option<&'a str> {
        for index in 0..self.count {
            if self.names[index] == name {
                return Some(self.values[index]);
            }
        }
        None
    }

    fn set(&mut self, name: &'a str, value: &'a str) -> Result<(), SysinfoError> {
        check_sysctl_name(name)?;
        for index in 0..self.count {
            if self.names[index] == name {
                self.values[index] = value;
                return Ok(());
            }
        }
        if self.count >= self.names.len() {
            return Err(SysinfoError::InvalidArgument);
        }
        self.names[self.count] = name;
        self.values[self.count] = value;
        self.count += 1;
        Ok(())
    }
}

/// Empty table (every query misses, every assignment is denied).
pub struct EmptySysctlTable;

impl<'a> SysctlTable<'a> for EmptySysctlTable {
    fn get(&self, _name: &str) -> Option<&'a str> {
        None
    }

    fn set(&mut self, _name: &'a str, _value: &'a str) -> Result<(), SysinfoError> {
        Err(SysinfoError::Denied)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_names_checked() {
        assert_eq!(check_sysctl_name("kern.hostname"), Ok(()));
        assert_eq!(check_sysctl_name(""), Err(SysinfoError::InvalidArgument));
        assert_eq!(
            check_sysctl_name("kern..hostname"),
            Err(SysinfoError::InvalidArgument)
        );
        assert_eq!(
            check_sysctl_name("kern.ho stname"),
            Err(SysinfoError::InvalidArgument)
        );
    }

    #[test]
    fn test_deep_names_rejected() {
        assert_eq!(
            check_sysctl_name("a.b.c.d.e.f.g.h.i"),
            Err(SysinfoError::InvalidArgument)
        );
    }

    #[test]
    fn test_assignment_splits() {
        assert_eq!(
            split_assignment("kern.hostname=mail"),
            Ok(("kern.hostname", "mail"))
        );
        assert_eq!(
            split_assignment("kern.hostname"),
            Err(SysinfoError::InvalidArgument)
        );
        assert_eq!(
            split_assignment("=mail"),
            Err(SysinfoError::InvalidArgument)
        );
    }

    #[test]
    fn test_memory_table_round_trip() {
        let mut table = MemorySysctlTable::new();
        table.set("kern.hostname", "mail").unwrap();
        assert_eq!(table.get("kern.hostname"), Some("mail"));
        table.set("kern.hostname", "backup").unwrap();
        assert_eq!(table.get("kern.hostname"), Some("backup"));
        assert_eq!(table.get("kern.domain"), None);
    }

    #[test]
    fn test_empty_table_denies() {
        let mut table = EmptySysctlTable;
        assert_eq!(table.get("kern.hostname"), None);
        assert_eq!(
            table.set("kern.hostname", "mail"),
            Err(SysinfoError::Denied)
        );
    }
}
