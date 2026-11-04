//! The password-file lookup: the `getpwnam` family's deciding half
//! (`minix3/lib/libc/gen/getpwent.c`).
//!
//! Ground truth for the line shape: `minix3/etc/master.passwd` (ten colon
//! separated fields: name, password, uid, gid, class, change time, expiry,
//! full name, home, shell) and the classic seven-field `passwd` face
//! derived from it. C's NetBSD libc reads the `pwd_mkdb` hash databases and
//! fills `struct passwd` through `_pw_parse`/`__pw_scan`
//! (`getpwent.c:150-176`); this tree has no db machinery, so the text file
//! is the database — the same seven/ten-field line grammar the `login`
//! command already adjudicated. What the two faces share is what callers
//! see: a matching name yields the account, everything else (file missing,
//! name absent, line malformed) looks like `getpwnam` answering NULL.
//!
//! Pure text only: the caller reads the database bytes (the file lives at
//! C `_PATH_PASSWD`, `/etc/passwd`) and hands them over, keeping this crate
//! on the "pure decision down, system interaction up" split.

/// One password database row, borrowed from the scanned text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Account<'a> {
    /// Login name, for example `root`.
    pub name: &'a str,
    /// Password placeholder (`x` when shadowed, `*` when login is blocked).
    pub password: &'a str,
    /// Numeric user identifier.
    pub uid: u32,
    /// Numeric group identifier.
    pub gid: u32,
    /// Full name / contact field.
    pub full_name: &'a str,
    /// Home directory path.
    pub home: &'a str,
    /// Login shell path.
    pub shell: &'a str,
}

impl<'a> Account<'a> {
    /// Parse one database line (without its newline): seven colon separated
    /// fields, or the ten-field `master.passwd` shape with the class and
    /// the two time fields (positions 4 to 6) ignored — both faces describe
    /// the same account. A malformed line answers `None`, which the scanner
    /// treats as "skip this line" (`_pw_parse` returning 0).
    pub fn parse(line: &'a str) -> Option<Account<'a>> {
        let mut fields: [&'a str; 10] = [""; 10];
        let mut count = 0;
        for part in line.split(':') {
            if count >= 10 {
                return None;
            }
            fields[count] = part;
            count += 1;
        }
        if count == 7 {
            // Shift the tail (full name, home, shell) from 4-6 to 7-9 so
            // both shapes read the same slots.
            fields[9] = fields[6];
            fields[8] = fields[5];
            fields[7] = fields[4];
        } else if count != 10 {
            return None;
        }
        if fields[0].is_empty() {
            return None;
        }
        Some(Account {
            name: fields[0],
            password: fields[1],
            uid: parse_id(fields[2])?,
            gid: parse_id(fields[3])?,
            full_name: fields[7],
            home: fields[8],
            shell: fields[9],
        })
    }
}

/// A decimal identifier; anything else (empty, junk, overflow) is a parse
/// failure, matching `__pw_scan`'s strict numeric fields.
fn parse_id(text: &str) -> Option<u32> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut value: u32 = 0;
    for byte in text.bytes() {
        value = value.checked_mul(10)?.checked_add((byte - b'0') as u32)?;
    }
    Some(value)
}

/// `getpwnam(name)` over one database's bytes: scan the lines and return
/// the first account whose login name matches. Malformed lines are skipped
/// (a caller warning C answers through `pw_scan`'s warn channel, which this
/// layer does not carry). A missing file never reaches here — the caller's
/// read failure maps to the same face as a NULL answer.
pub fn lookup_in<'a>(text: &'a str, name: &str) -> Option<Account<'a>> {
    text.lines().find_map(|line| {
        let line = line.strip_suffix('\r').unwrap_or(line);
        Account::parse(line).filter(|account| account.name == name)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_seven_field_line_parses_all_slots() {
        let entry = Account::parse("root:x:0:0:System Owner:/root:/bin/sh").unwrap();
        assert_eq!(entry.name, "root");
        assert_eq!(entry.password, "x");
        assert_eq!(entry.uid, 0);
        assert_eq!(entry.gid, 0);
        assert_eq!(entry.full_name, "System Owner");
        assert_eq!(entry.home, "/root");
        assert_eq!(entry.shell, "/bin/sh");
    }

    #[test]
    fn master_passwd_ten_field_shape_skips_the_middle_three() {
        let entry = Account::parse("root:x:0:0:daemon:0:0:System Owner:/root:/bin/sh").unwrap();
        // Positions 4-6 (class, change, expiry) are ignored; the tail lands
        // on the same slots as the seven-field face.
        assert_eq!(entry.uid, 0);
        assert_eq!(entry.full_name, "System Owner");
        assert_eq!(entry.home, "/root");
        assert_eq!(entry.shell, "/bin/sh");
    }

    #[test]
    fn malformed_lines_are_none() {
        // Six fields, thirteen fields, a non-numeric uid, an empty name.
        assert!(Account::parse("root:x:0:0:too:few").is_none());
        assert!(Account::parse("a:b:c:d:e:f:g:h:i:j:k:l:m").is_none());
        assert!(Account::parse("root:x:nope:0:n:/root:/bin/sh").is_none());
        assert!(Account::parse(":x:0:0:n:/root:/bin/sh").is_none());
        // An overflowing identifier is as malformed as a non-numeric one.
        assert!(Account::parse("root:x:99999999999:0:n:/root:/bin/sh").is_none());
    }

    #[test]
    fn lookup_returns_the_first_matching_name() {
        let db = "root:x:0:0:System Owner:/root:/bin/sh\n\
                  daemon:x:1:1:daemon:/dev/null:/bin/false\n\
                  guest:*:1000:1000:Guest:/home/guest:\n";
        let hit = lookup_in(db, "daemon").unwrap();
        assert_eq!(hit.uid, 1);
        assert_eq!(hit.shell, "/bin/false");
        // A blocked account still resolves: blocking is policy, not parse.
        assert_eq!(lookup_in(db, "guest").unwrap().uid, 1000);
        assert!(lookup_in(db, "nosuch").is_none());
    }

    #[test]
    fn lookup_skips_malformed_lines_and_cariage_returns() {
        let db = "broken-line\n\
                  root:x:0:0:Owner:/root:/bin/sh\r\n\
                  again:x:2:2\n";
        assert_eq!(lookup_in(db, "root").unwrap().uid, 0);
        assert!(lookup_in(db, "again").is_none(), "坏行被跳过而不是中止扫描");
    }

    #[test]
    fn empty_database_answers_none() {
        assert!(lookup_in("", "root").is_none());
    }
}
