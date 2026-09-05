//! Password file line format.
//!
//! Ground truth: `minix3/etc/master.passwd` (ten colon separated fields:
//! name, encrypted password, user identifier, group identifier, login
//! class, password change time, account expiry time, full name field, home
//! directory, shell) and the shorter seven field `passwd` face derived from
//! it. The C library walks these lines with the `getpwnam` family; this
//! module parses one line at a time so callers can build any table on top.

use crate::LoginError;

/// One password database row, borrowed from the line it was parsed from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PasswdEntry<'a> {
    /// Login name, for example `root`.
    pub name: &'a str,
    /// Password placeholder (`x` when shadowed, `*` when login is blocked).
    pub password: &'a str,
    /// Numeric user identifier.
    pub user_id: u32,
    /// Numeric group identifier.
    pub group_id: u32,
    /// Full name / contact field.
    pub full_name: &'a str,
    /// Home directory path.
    pub home: &'a str,
    /// Login shell path (empty means the default shell).
    pub shell: &'a str,
}

/// Parse one `passwd` line: seven colon separated fields.
///
/// Accepts the ten field `master.passwd` shape as well, ignoring the three
/// extra time and class fields in the middle (positions 5 to 7), because
/// both faces describe the same account. A blocked account (`*` password)
/// still parses; blocking is a policy decision for the caller, not a parse
/// error.
pub fn parse_passwd_line<'a>(line: &'a str) -> Result<PasswdEntry<'a>, LoginError> {
    let fields: [&'a str; 10] = split_ten(line)?;
    Ok(PasswdEntry {
        name: check_name(fields[0])?,
        password: fields[1],
        user_id: parse_id(fields[2])?,
        group_id: parse_id(fields[3])?,
        full_name: fields[7],
        home: fields[8],
        shell: fields[9],
    })
}

/// Split a line into exactly seven or ten colon separated fields.
///
/// Seven fields is the classic `passwd` face; ten is the `master.passwd`
/// face (class, change time, and expiry sit between the group identifier
/// and the full name). Anything else is malformed. The middle three fields
/// of the ten field shape are returned as empty placeholders at positions
/// 4 to 6 so the tail fields always land on indices 7 to 9.
fn split_ten<'a>(line: &'a str) -> Result<[&'a str; 10], LoginError> {
    let mut fields: [&'a str; 10] = [""; 10];
    let mut count = 0;
    for part in line.split(':') {
        if count >= 10 {
            return Err(LoginError::InvalidArgument);
        }
        fields[count] = part;
        count += 1;
    }
    if count == 7 {
        // Shift the tail (full name, home, shell) from 4-6 to 7-9.
        fields[9] = fields[6];
        fields[8] = fields[5];
        fields[7] = fields[4];
        fields[6] = "";
        fields[5] = "";
        fields[4] = "";
        Ok(fields)
    } else if count == 10 {
        Ok(fields)
    } else {
        Err(LoginError::InvalidArgument)
    }
}

fn check_name(name: &str) -> Result<&str, LoginError> {
    if name.is_empty() {
        return Err(LoginError::InvalidArgument);
    }
    let ok = name
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.');
    if ok {
        Ok(name)
    } else {
        Err(LoginError::InvalidArgument)
    }
}

fn parse_id(text: &str) -> Result<u32, LoginError> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(LoginError::InvalidArgument);
    }
    let mut value: u32 = 0;
    for byte in text.bytes() {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
            .ok_or(LoginError::InvalidArgument)?;
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classic_seven_field_line() {
        let entry = parse_passwd_line("root:x:0:0:System Owner:/root:/bin/sh").unwrap();
        assert_eq!(entry.name, "root");
        assert_eq!(entry.user_id, 0);
        assert_eq!(entry.group_id, 0);
        assert_eq!(entry.home, "/root");
        assert_eq!(entry.shell, "/bin/sh");
    }

    #[test]
    fn test_master_passwd_ten_field_line() {
        let entry =
            parse_passwd_line("bob:hashed:1001:100:staff:0:0:Bob:/home/bob:/bin/sh").unwrap();
        assert_eq!(entry.name, "bob");
        assert_eq!(entry.user_id, 1001);
        assert_eq!(entry.full_name, "Bob");
        assert_eq!(entry.home, "/home/bob");
    }

    #[test]
    fn test_blocked_account_still_parses() {
        let entry = parse_passwd_line("nobody:*:999:999::/nonexistent:").unwrap();
        assert_eq!(entry.password, "*");
        assert_eq!(entry.shell, "");
    }

    #[test]
    fn test_wrong_field_count_rejected() {
        assert_eq!(
            parse_passwd_line("root:x:0:0"),
            Err(LoginError::InvalidArgument)
        );
        assert_eq!(
            parse_passwd_line("a:b:c:d:e:f:g:h:i:j:k"),
            Err(LoginError::InvalidArgument)
        );
    }

    #[test]
    fn test_empty_name_rejected() {
        assert_eq!(
            parse_passwd_line(":x:0:0::/root:/bin/sh"),
            Err(LoginError::InvalidArgument)
        );
    }

    #[test]
    fn test_non_numeric_id_rejected() {
        assert_eq!(
            parse_passwd_line("root:x:abc:0::/root:/bin/sh"),
            Err(LoginError::InvalidArgument)
        );
    }

    #[test]
    fn test_overflowing_id_rejected() {
        assert_eq!(
            parse_passwd_line("root:x:99999999999999999999:0::/root:/bin/sh"),
            Err(LoginError::InvalidArgument)
        );
    }

    #[test]
    fn test_errno_mapping() {
        assert_eq!(LoginError::InvalidArgument.as_errno(), 22);
        assert_eq!(LoginError::NotFound.as_errno(), 2);
        assert_eq!(LoginError::PermissionDenied.as_errno(), 13);
    }
}
