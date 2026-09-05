//! Group file line format (`/etc/group`).
//!
//! Ground truth: `minix3/etc/group` (`group:password:group_id:members`,
//! members comma separated, empty member list allowed) read by the C
//! library `getgrnam` family and queried by `getent group`.

use crate::DevDbError;

/// Maximum members kept per group; more is a malformed line.
pub const MAX_MEMBERS: usize = 16;

/// One parsed group row, borrowed from the line it was parsed from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupEntry<'a> {
    /// Group name, for example `wheel`.
    pub name: &'a str,
    /// Numeric group identifier.
    pub group_id: u32,
    /// Member login names.
    pub members: [&'a str; MAX_MEMBERS],
    /// How many of `members` are used.
    pub member_count: usize,
}

impl<'a> GroupEntry<'a> {
    /// The member names as a slice.
    pub fn member_list(&self) -> &[&'a str] {
        &self.members[..self.member_count]
    }
}

/// Parse one group line: four colon separated fields.
///
/// The member field may be empty (a group with no members). Member names
/// follow the same character rule as login names.
pub fn parse_group_line<'a>(line: &'a str) -> Result<GroupEntry<'a>, DevDbError> {
    let mut fields = line.split(':');
    let name = fields.next().ok_or(DevDbError::InvalidArgument)?;
    let _password = fields.next().ok_or(DevDbError::InvalidArgument)?;
    let id_text = fields.next().ok_or(DevDbError::InvalidArgument)?;
    let members_text = fields.next().ok_or(DevDbError::InvalidArgument)?;
    if fields.next().is_some() {
        return Err(DevDbError::InvalidArgument);
    }
    let mut entry = GroupEntry {
        name: check_name(name)?,
        group_id: parse_id(id_text)?,
        members: [""; MAX_MEMBERS],
        member_count: 0,
    };
    if !members_text.is_empty() {
        for member in members_text.split(',') {
            if entry.member_count >= MAX_MEMBERS {
                return Err(DevDbError::InvalidArgument);
            }
            entry.members[entry.member_count] = check_name(member)?;
            entry.member_count += 1;
        }
    }
    Ok(entry)
}

fn check_name(name: &str) -> Result<&str, DevDbError> {
    if name.is_empty() {
        return Err(DevDbError::InvalidArgument);
    }
    let ok = name
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.');
    if ok {
        Ok(name)
    } else {
        Err(DevDbError::InvalidArgument)
    }
}

fn parse_id(text: &str) -> Result<u32, DevDbError> {
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
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_group_with_members() {
        let entry = parse_group_line("wheel:x:0:root,operator").unwrap();
        assert_eq!(entry.name, "wheel");
        assert_eq!(entry.group_id, 0);
        assert_eq!(entry.member_list(), &["root", "operator"]);
    }

    #[test]
    fn test_group_without_members() {
        let entry = parse_group_line("nogroup:x:32766:").unwrap();
        assert_eq!(entry.member_count, 0);
    }

    #[test]
    fn test_wrong_field_count_rejected() {
        assert_eq!(
            parse_group_line("wheel:x:0"),
            Err(DevDbError::InvalidArgument)
        );
        assert_eq!(
            parse_group_line("wheel:x:0:a:b"),
            Err(DevDbError::InvalidArgument)
        );
    }

    #[test]
    fn test_empty_name_rejected() {
        assert_eq!(
            parse_group_line(":x:0:root"),
            Err(DevDbError::InvalidArgument)
        );
    }

    #[test]
    fn test_non_numeric_id_rejected() {
        assert_eq!(
            parse_group_line("wheel:x:abc:root"),
            Err(DevDbError::InvalidArgument)
        );
    }

    #[test]
    fn test_errno_mapping() {
        assert_eq!(DevDbError::InvalidArgument.as_errno(), 22);
        assert_eq!(DevDbError::NotFound.as_errno(), 2);
    }
}
