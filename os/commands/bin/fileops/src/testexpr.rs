//! The `test`/`[` expression language over a file tester trait.
//!
//! Ground truth: `minix3/bin/test/test.c` (717 lines). The evaluator is a
//! textbook recursive descent with four levels (`test.c:160-163`):
//! `oexpr` (or) calls `aexpr` (and) calls `nexpr` (not) calls `primary`
//! (leaf tests and parentheses). The token table at lines 105 to 130 sorts
//! operators into comparison, file status, and string classes. This module
//! mirrors that exact shape: same four levels, same operator families.
//!
//! File status questions (`-e` exists, `-f` regular file, `-d` directory,
//! `-r`/`-w`/`-x` access, `-s` non empty, `-L` link, `-p` pipe, `-S`
//! socket, `-h`/`-L` link, `-nt`/`-ot` newer/older) go through [`FileTester`], so evaluation never touches a
//! real file system. Identity and time comparisons reduce to numbers the
//! tester reports.
//!
//! Two primaries used to live here that `test.c`'s tables never had: `-N`
//! (modified-since-read) and the bash-style `==`. Minix3's token tables list
//! `-e -f -d -c -b -p -S -L -h -u -g -k -r -w -x -s -t -n -z -O -G` plus the
//! binary `= != < > -eq … -ef` (`test.c:105-154`), so both inventions are gone
//! rather than kept as an unlabelled superset of the upstream language. What
//! the tables *did* have and this module lacked is now added: the `<`/`>`
//! string compares (`test.c:109`, `:111`, `:345-348`) and `-h` as the alias of
//! `-L` the table marks "for backwards compat" (`test.c:139`).

use crate::FileOpError;

/// Answers the file status questions an expression can ask.
///
/// Two implementations ship: [`EmptyFs`] (nothing exists — the honest
/// starting point) and [`TableFs`] (an in memory file list for tests).
/// The execution layer adds the live implementation (a real `FileTester`
/// backed by `stat`/`lstat` + [`access_bits`]) in the `test` binary.
pub trait FileTester {
    /// True when the path exists at all.
    fn exists(&self, path: &str) -> bool;
    /// File kind flags: regular, directory, link, pipe, socket, block or
    /// character device. Unset flags read false. `block` and `char` stay
    /// distinct so `-b` and `-c` answer separately (as C's `S_ISBLK` /
    /// `S_ISCHR` do; `minix3/bin/test/test.c:556-557`).
    fn kind(&self, path: &str) -> FileKind;
    /// True when the path is readable / writable / executable.
    fn access(&self, path: &str) -> Access;
    /// Byte size, or `None` when unknown.
    fn size(&self, path: &str) -> Option<u64>;
    /// Modification time in seconds, or `None` when unknown.
    fn mtime(&self, path: &str) -> Option<u64>;
    /// Permission-mode ornaments `-u`/`-g`/`-k` answer from.
    fn flags(&self, path: &str) -> ModeFlags;
    /// True when the file's owner id equals the caller's euid (`-O`).
    fn owned_by_me(&self, path: &str) -> bool;
    /// True when the file's group id equals the caller's egid (`-G`).
    fn grouped_by_me(&self, path: &str) -> bool;
    /// True when both paths name the same file (equal device + inode, `-ef`).
    fn same_file(&self, a: &str, b: &str) -> bool;
}

/// Permission-mode ornaments the `-u`/`-g`/`-k` operators report
/// (`S_ISUID`/`S_ISGID`/`S_ISVTX`, `sys/stat.h:111-112,145`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ModeFlags {
    /// Set-user-id on execution (`-u`).
    pub setuid: bool,
    /// Set-group-id on execution (`-g`).
    pub setgid: bool,
    /// Sticky bit (`-k`).
    pub sticky: bool,
}

/// File kind flags (a file has exactly one in practice; the tester reports
/// what it knows).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FileKind {
    /// Regular file.
    pub regular: bool,
    /// Directory.
    pub directory: bool,
    /// Symbolic link.
    pub link: bool,
    /// Named pipe.
    pub pipe: bool,
    /// Socket.
    pub socket: bool,
    /// Block device (`-b`).
    pub block: bool,
    /// Character device (`-c`).
    pub char_dev: bool,
}

/// Access rights flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Access {
    /// Readable.
    pub read: bool,
    /// Writable.
    pub write: bool,
    /// Executable (searchable for directories).
    pub execute: bool,
}

/// A file system where nothing exists: every status question answers
/// negatively. Scripts evaluated against it take the "missing file"
/// branches, which is exactly the safe default.
pub struct EmptyFs;

impl FileTester for EmptyFs {
    fn exists(&self, _path: &str) -> bool {
        false
    }

    fn kind(&self, _path: &str) -> FileKind {
        FileKind::default()
    }

    fn access(&self, _path: &str) -> Access {
        Access::default()
    }

    fn size(&self, _path: &str) -> Option<u64> {
        None
    }

    fn mtime(&self, _path: &str) -> Option<u64> {
        None
    }

    fn flags(&self, _path: &str) -> ModeFlags {
        ModeFlags::default()
    }

    fn owned_by_me(&self, _path: &str) -> bool {
        false
    }

    fn grouped_by_me(&self, _path: &str) -> bool {
        false
    }

    fn same_file(&self, _a: &str, _b: &str) -> bool {
        false
    }
}

/// One in memory file description.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TestFile<'a> {
    /// Path the description answers for.
    pub path: &'a str,
    /// Kind flags.
    pub kind: FileKind,
    /// Access flags.
    pub access: Access,
    /// Byte size.
    pub size: u64,
    /// Modification time in seconds.
    pub mtime: u64,
    /// Mode ornaments (`-u`/`-g`/`-k`).
    pub flags: ModeFlags,
    /// Stands in for `st_uid == euid` (`-O`).
    pub owned: bool,
    /// Stands in for `st_gid == egid` (`-G`).
    pub grouped: bool,
    /// Device id (for `-ef`).
    pub dev: u64,
    /// Inode number (for `-ef`).
    pub ino: u64,
}

/// A file system over a fixed list of descriptions (up to 16 files).
/// Unknown paths behave like [`EmptyFs`].
pub struct TableFs<'a> {
    /// File descriptions searched in order; the first path match wins.
    pub files: [Option<TestFile<'a>>; 16],
}

impl<'a> TableFs<'a> {
    /// An empty table.
    pub fn empty() -> Self {
        TableFs { files: [None; 16] }
    }

    fn lookup(&self, path: &str) -> Option<TestFile<'a>> {
        self.files.iter().find_map(|slot| match slot {
            Some(file) if file.path == path => Some(*file),
            _ => None,
        })
    }
}

impl FileTester for TableFs<'_> {
    fn exists(&self, path: &str) -> bool {
        self.lookup(path).is_some()
    }

    fn kind(&self, path: &str) -> FileKind {
        self.lookup(path).map_or(FileKind::default(), |file| file.kind)
    }

    fn access(&self, path: &str) -> Access {
        self.lookup(path).map_or(Access::default(), |file| file.access)
    }

    fn size(&self, path: &str) -> Option<u64> {
        self.lookup(path).map(|file| file.size)
    }

    fn mtime(&self, path: &str) -> Option<u64> {
        self.lookup(path).map(|file| file.mtime)
    }

    fn flags(&self, path: &str) -> ModeFlags {
        self.lookup(path).map_or(ModeFlags::default(), |file| file.flags)
    }

    fn owned_by_me(&self, path: &str) -> bool {
        self.lookup(path).is_some_and(|file| file.owned)
    }

    fn grouped_by_me(&self, path: &str) -> bool {
        self.lookup(path).is_some_and(|file| file.grouped)
    }

    fn same_file(&self, a: &str, b: &str) -> bool {
        match (self.lookup(a), self.lookup(b)) {
            (Some(x), Some(y)) => x.dev == y.dev && x.ino == y.ino,
            _ => false,
        }
    }
}

/// Evaluate `argv` (words after `test`, or inside `[` `]`) to a truth value.
///
/// An empty expression is false; excess words are an error (matching the C
/// tool refusing ambiguous expressions rather than guessing).
pub fn evaluate<T: FileTester>(tester: &T, argv: &[&str]) -> Result<bool, FileOpError> {
    if argv.is_empty() {
        return Ok(false);
    }
    let mut parser = Parser { tester, words: argv, pos: 0 };
    let value = parser.or_expr()?;
    if parser.pos != argv.len() {
        return Err(FileOpError::InvalidArgument);
    }
    Ok(value)
}

struct Parser<'w, T: FileTester> {
    tester: &'w T,
    words: &'w [&'w str],
    pos: usize,
}

impl<'w, T: FileTester> Parser<'w, T> {
    fn peek(&self) -> Option<&'w str> {
        self.words.get(self.pos).copied()
    }

    fn next(&mut self) -> Option<&'w str> {
        let word = self.words.get(self.pos).copied()?;
        self.pos += 1;
        Some(word)
    }

    /// `or` level: `a -o b`.
    fn or_expr(&mut self) -> Result<bool, FileOpError> {
        let mut value = self.and_expr()?;
        while self.peek() == Some("-o") {
            self.pos += 1;
            let right = self.and_expr()?;
            value = value || right;
        }
        Ok(value)
    }

    /// `and` level: `a -a b`.
    fn and_expr(&mut self) -> Result<bool, FileOpError> {
        let mut value = self.not_expr()?;
        while self.peek() == Some("-a") {
            self.pos += 1;
            let right = self.not_expr()?;
            value = value && right;
        }
        Ok(value)
    }

    /// `not` level: `! a`.
    fn not_expr(&mut self) -> Result<bool, FileOpError> {
        if self.peek() == Some("!") {
            self.pos += 1;
            Ok(!self.not_expr()?)
        } else {
            self.primary()
        }
    }

    /// Leaf level: parentheses, unary operators, binary comparisons, and
    /// bare strings (true when non empty).
    fn primary(&mut self) -> Result<bool, FileOpError> {
        match self.next().ok_or(FileOpError::InvalidArgument)? {
            "(" => {
                let value = self.or_expr()?;
                if self.next() != Some(")") {
                    return Err(FileOpError::InvalidArgument);
                }
                Ok(value)
            }
            "-e" => Ok(self.tester.exists(self.operand()?)),
            "-f" => Ok(self.tester.kind(self.operand()?).regular),
            "-d" => Ok(self.tester.kind(self.operand()?).directory),
            "-L" => Ok(self.tester.kind(self.operand()?).link),
            "-p" => Ok(self.tester.kind(self.operand()?).pipe),
            "-S" => Ok(self.tester.kind(self.operand()?).socket),
            "-b" => Ok(self.tester.kind(self.operand()?).block),
            "-c" => Ok(self.tester.kind(self.operand()?).char_dev),
            "-u" => Ok(self.tester.flags(self.operand()?).setuid),
            "-g" => Ok(self.tester.flags(self.operand()?).setgid),
            "-k" => Ok(self.tester.flags(self.operand()?).sticky),
            "-O" => Ok(self.tester.owned_by_me(self.operand()?)),
            "-G" => Ok(self.tester.grouped_by_me(self.operand()?)),
            // `-h` 与 `-L` 同义：`test.c:139` 明写「for backwards compat」。
            "-h" => Ok(self.tester.kind(self.operand()?).link),
            "-r" => Ok(self.tester.access(self.operand()?).read),
            "-w" => Ok(self.tester.access(self.operand()?).write),
            "-x" => Ok(self.tester.access(self.operand()?).execute),
            "-s" => Ok(self.tester.size(self.operand()?).is_some_and(|size| size > 0)),
            "-n" => Ok(!self.operand()?.is_empty()),
            "-z" => Ok(self.operand()?.is_empty()),
            "-t" => {
                // `-t fd`: true when the descriptor is a terminal. The
                // tester has no terminals; only descriptor numbers parse.
                let word = self.operand()?;
                if word.bytes().all(|b| b.is_ascii_digit()) && !word.is_empty() {
                    Ok(false)
                } else {
                    Err(FileOpError::InvalidArgument)
                }
            }
            word => self.binary_or_string(word),
        }
    }

    /// After a leading word: binary comparison or bare string truth.
    fn binary_or_string(&mut self, left: &str) -> Result<bool, FileOpError> {
        match self.peek() {
            // C 的比较表里只有单等号（`test.c:110`），没有 bash 的 `==`。
            Some("=") => {
                self.pos += 1;
                Ok(left == self.operand()?)
            }
            // `<`/`>`：按字节序的字符串比较，对位 `strcmp`（`test.c:345-348`）。
            Some("<") => {
                self.pos += 1;
                Ok(left.as_bytes() < self.operand()?.as_bytes())
            }
            Some(">") => {
                self.pos += 1;
                Ok(left.as_bytes() > self.operand()?.as_bytes())
            }
            Some("!=") => {
                self.pos += 1;
                Ok(left != self.operand()?)
            }
            Some("-eq") | Some("-ne") | Some("-gt") | Some("-ge") | Some("-lt")
            | Some("-le") => {
                let operator = self.peek().unwrap_or("");
                let (a, b) = self.integers(left)?;
                match operator {
                    "-eq" => Ok(a == b),
                    "-ne" => Ok(a != b),
                    "-gt" => Ok(a > b),
                    "-ge" => Ok(a >= b),
                    "-lt" => Ok(a < b),
                    _ => Ok(a <= b),
                }
            }
            Some("-nt") => {
                self.pos += 1;
                let right = self.operand()?;
                Ok(compare_time(self.tester, left, right, true)?)
            }
            Some("-ot") => {
                self.pos += 1;
                let right = self.operand()?;
                Ok(compare_time(self.tester, left, right, false)?)
            }
            Some("-ef") => {
                self.pos += 1;
                let right = self.operand()?;
                Ok(self.tester.same_file(left, right))
            }
            _ => Ok(!left.is_empty()),
        }
    }

    fn operand(&mut self) -> Result<&'w str, FileOpError> {
        self.next().ok_or(FileOpError::InvalidArgument)
    }

    /// Parse the integer comparison `left OP right`, consuming both sides.
    fn integers(&mut self, left: &str) -> Result<(i64, i64), FileOpError> {
        self.pos += 1;
        let right = self.operand()?;
        Ok((parse_integer(left)?, parse_integer(right)?))
    }
}

fn parse_integer(text: &str) -> Result<i64, FileOpError> {
    if text.is_empty() {
        return Err(FileOpError::InvalidArgument);
    }
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(FileOpError::InvalidArgument);
    }
    let mut value: i64 = 0;
    for byte in digits.bytes() {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as i64))
            .ok_or(FileOpError::InvalidArgument)?;
    }
    Ok(if negative { -value } else { value })
}

/// `-nt` (newer than, `newer=true`) and `-ot`: missing files make the
/// whole comparison false rather than erroring, matching the C tool.
fn compare_time<T: FileTester>(
    tester: &T,
    left: &str,
    right: &str,
    newer: bool,
) -> Result<bool, FileOpError> {
    match (tester.mtime(left), tester.mtime(right)) {
        (Some(a), Some(b)) => Ok(if newer { a > b } else { a < b }),
        _ => Ok(false),
    }
}

// Permission-class bits for [`access_bits`], mirroring the `S_I?OTH` /
// shift-by-3 lane layout C's `test_access` reads (`sys/stat.h:133-136`:
// `S_IXOTH 0o1`, `S_IWOTH 0o2`, `S_IROTH 0o4`; owner is two lanes up,
// group one lane up).
const S_IXOTH: u32 = 0o1;
const S_IWOTH: u32 = 0o2;
const S_IROTH: u32 = 0o4;

/// Resolve read/write/execute against a file's mode with the caller's
/// ids — the pure core of C `test_access` (`minix3/bin/test/test.c:495-533`).
///
/// This deliberately tests the mode bits instead of calling `access(2)`:
/// the C comment at lines 373-494 argues at length that mimicking `open()`
/// is useless for root (`test -r/-w/-x` would then never fail for an
/// existing file), so C checks `st_mode` directly. The same rule holds
/// here: root passes if any of the three classes carries the bit; an owner
/// matches the owner lane; a matching group (effective or any supplementary
/// group) matches the group lane; everyone else falls to the other lane.
///
/// `supplementary` is the caller's supplementary group list (C's
/// `getgroups` walk). The `test` binary passes an empty list — Minix
/// processes carry the credentials set by the parent, and the
/// supplementary-group lane is a documented narrow residual of the wiring
/// (recorded in `rewrite-notes/18-stage-commands/todo.md`), not a silent gap.
pub fn access_bits(
    mode: u32,
    st_uid: u32,
    st_gid: u32,
    euid: u32,
    egid: u32,
    supplementary: &[u32],
) -> Access {
    let lane = |base: u32| -> bool {
        let mask = if euid == 0 {
            (base << 6) | (base << 3) | base
        } else if st_uid == euid {
            base << 6
        } else if st_gid == egid || supplementary.contains(&st_gid) {
            base << 3
        } else {
            base
        };
        mode & mask != 0
    };
    Access {
        read: lane(S_IROTH),
        write: lane(S_IWOTH),
        execute: lane(S_IXOTH),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> TableFs<'static> {
        let mut fs = TableFs::empty();
        fs.files[0] = Some(TestFile {
            path: "/bin/sh",
            kind: FileKind {
                regular: true,
                ..FileKind::default()
            },
            access: Access {
                read: true,
                write: false,
                execute: true,
            },
            size: 90000,
            mtime: 100,
            flags: ModeFlags::default(),
            owned: true,
            grouped: false,
            dev: 1,
            ino: 100,
        });
        fs.files[1] = Some(TestFile {
            path: "/tmp",
            kind: FileKind {
                directory: true,
                ..FileKind::default()
            },
            access: Access {
                read: true,
                write: true,
                execute: true,
            },
            size: 512,
            mtime: 200,
            flags: ModeFlags::default(),
            owned: false,
            grouped: true,
            dev: 2,
            ino: 200,
        });
        fs
    }

    #[test]
    fn test_file_status_operators() {
        let fs = table();
        assert_eq!(evaluate(&fs, &["-f", "/bin/sh"]), Ok(true));
        assert_eq!(evaluate(&fs, &["-d", "/bin/sh"]), Ok(false));
        assert_eq!(evaluate(&fs, &["-d", "/tmp"]), Ok(true));
        assert_eq!(evaluate(&fs, &["-x", "/bin/sh"]), Ok(true));
        assert_eq!(evaluate(&fs, &["-w", "/bin/sh"]), Ok(false));
        assert_eq!(evaluate(&fs, &["-e", "/ghost"]), Ok(false));
    }

    #[test]
    fn test_empty_fs_answers_no() {
        let fs = EmptyFs;
        assert_eq!(evaluate(&fs, &["-e", "/bin/sh"]), Ok(false));
        assert_eq!(evaluate(&fs, &["-f", "/bin/sh"]), Ok(false));
    }

    #[test]
    fn test_string_operators() {
        let fs = EmptyFs;
        assert_eq!(evaluate(&fs, &["a", "=", "a"]), Ok(true));
        assert_eq!(evaluate(&fs, &["a", "!=", "b"]), Ok(true));
        assert_eq!(evaluate(&fs, &["-n", "x"]), Ok(true));
        assert_eq!(evaluate(&fs, &["-z", ""]), Ok(true));
        assert_eq!(evaluate(&fs, &["hello"]), Ok(true));
        assert_eq!(evaluate(&fs, &[""]), Ok(false));
    }

    #[test]
    fn test_integer_operators() {
        let fs = EmptyFs;
        assert_eq!(evaluate(&fs, &["3", "-eq", "3"]), Ok(true));
        assert_eq!(evaluate(&fs, &["3", "-ne", "4"]), Ok(true));
        assert_eq!(evaluate(&fs, &["4", "-gt", "3"]), Ok(true));
        assert_eq!(evaluate(&fs, &["3", "-le", "3"]), Ok(true));
        assert_eq!(evaluate(&fs, &["x", "-eq", "3"]), Err(FileOpError::InvalidArgument));
    }

    #[test]
    fn test_boolean_precedence() {
        let fs = EmptyFs;
        // `-a` binds tighter than `-o`: false -o (true -a true) is true.
        assert_eq!(
            evaluate(&fs, &["", "-o", "x", "-a", "y"]),
            Ok(true)
        );
        assert_eq!(
            evaluate(&fs, &["!", "", "-a", ""]),
            Ok(false)
        );
        assert_eq!(evaluate(&fs, &["(", "x", ")"]), Ok(true));
    }

    #[test]
    fn test_newer_older() {
        let fs = table();
        assert_eq!(evaluate(&fs, &["/tmp", "-nt", "/bin/sh"]), Ok(true));
        assert_eq!(evaluate(&fs, &["/bin/sh", "-ot", "/tmp"]), Ok(true));
        assert_eq!(evaluate(&fs, &["/ghost", "-nt", "/bin/sh"]), Ok(false));
    }

    #[test]
    fn test_malformed_rejected() {
        let fs = EmptyFs;
        assert_eq!(evaluate(&fs, &["(", "x"]), Err(FileOpError::InvalidArgument));
        assert_eq!(evaluate(&fs, &["-f"]), Err(FileOpError::InvalidArgument));
        assert_eq!(evaluate(&fs, &["a", "b"]), Err(FileOpError::InvalidArgument));
    }

    #[test]
    fn test_testers_share_the_trait() {
        let empty = EmptyFs;
        let table = table();
        let testers: [&dyn FileTester; 2] = [&empty, &table];
        assert!(!testers[0].exists("/bin/sh"));
        assert!(testers[1].exists("/bin/sh"));
    }

    #[test]
    fn test_owner_group_and_modified_operators() {
        let fs = table();
        // -O / -G: /bin/sh is owned-by-me, /tmp is group-by-me (fixture).
        assert_eq!(evaluate(&fs, &["-O", "/bin/sh"]), Ok(true));
        assert_eq!(evaluate(&fs, &["-O", "/tmp"]), Ok(false));
        assert_eq!(evaluate(&fs, &["-G", "/tmp"]), Ok(true));
        assert_eq!(evaluate(&fs, &["-G", "/bin/sh"]), Ok(false));
        // `<`/`>` 走 `strcmp` 的字节序（`test.c:109`、`:111`、`:345-348`）。
        assert_eq!(evaluate(&fs, &["a", "<", "b"]), Ok(true));
        assert_eq!(evaluate(&fs, &["b", "<", "a"]), Ok(false));
        assert_eq!(evaluate(&fs, &["b", ">", "a"]), Ok(true));
        assert_eq!(evaluate(&fs, &["a", ">", "b"]), Ok(false));
        assert_eq!(evaluate(&fs, &["a", "<", "a"]), Ok(false));
        assert_eq!(evaluate(&fs, &["a", ">", "a"]), Ok(false));
        // `==` 不在 C 的表里（只有单等号，`test.c:110`）：于是 `[ a == a ]` 不是
        // 真也不是假，而是「剩下的词没人吃」的语法错（退出码 2 那一支）。
        assert_eq!(
            evaluate(&fs, &["a", "==", "a"]),
            Err(FileOpError::InvalidArgument)
        );
        // C 的表里没有 `-N`，本层也不再假装有。
        assert_eq!(
            evaluate(&fs, &["-N", "/bin/sh"]),
            Err(FileOpError::InvalidArgument)
        );

        // -u / -g / -k: fixtures carry no setuid/setgid/sticky.
        assert_eq!(evaluate(&fs, &["-u", "/bin/sh"]), Ok(false));
    }

    #[test]
    fn test_h_is_the_alias_of_capital_l() {
        // `test.c:139` 把 `h` 与 `L` 指到同一个 token（`FILSYM`），注释写明
        // 「for backwards compat」——两条写法必须读同一个判据，也必须是 lstat 语义。
        let mut fs = TableFs::empty();
        fs.files[0] = Some(TestFile {
            path: "/tmp/link",
            kind: FileKind {
                link: true,
                ..FileKind::default()
            },
            ..test_file("/tmp/link")
        });
        assert_eq!(evaluate(&fs, &["-h", "/tmp/link"]), Ok(true));
        assert_eq!(evaluate(&fs, &["-L", "/tmp/link"]), Ok(true));
        assert_eq!(evaluate(&fs, &["-h", "/nope"]), Ok(false));
        assert_eq!(
            evaluate(&fs, &["-h", "/tmp/link"]),
            evaluate(&fs, &["-L", "/tmp/link"])
        );
    }

    #[test]
    fn test_block_and_char_are_distinct() {
        let mut fs = TableFs::empty();
        fs.files[0] = Some(TestFile {
            path: "/dev/rdisk",
            kind: FileKind {
                block: true,
                ..FileKind::default()
            },
            ..test_file("/dev/rdisk")
        });
        fs.files[1] = Some(TestFile {
            path: "/dev/null",
            kind: FileKind {
                char_dev: true,
                ..FileKind::default()
            },
            ..test_file("/dev/null")
        });
        assert_eq!(evaluate(&fs, &["-b", "/dev/rdisk"]), Ok(true));
        assert_eq!(evaluate(&fs, &["-c", "/dev/rdisk"]), Ok(false));
        assert_eq!(evaluate(&fs, &["-c", "/dev/null"]), Ok(true));
        assert_eq!(evaluate(&fs, &["-b", "/dev/null"]), Ok(false));
    }

    #[test]
    fn test_same_file_ef() {
        let mut fs = TableFs::empty();
        // Two paths, one file (same dev+ino). A third is a different file.
        fs.files[0] = Some(TestFile {
            path: "a",
            dev: 7,
            ino: 42,
            ..test_file("a")
        });
        fs.files[1] = Some(TestFile {
            path: "b",
            dev: 7,
            ino: 42,
            ..test_file("b")
        });
        fs.files[2] = Some(TestFile {
            path: "c",
            dev: 7,
            ino: 43,
            ..test_file("c")
        });
        assert_eq!(evaluate(&fs, &["a", "-ef", "b"]), Ok(true));
        assert_eq!(evaluate(&fs, &["a", "-ef", "c"]), Ok(false));
        // Missing operand → false, not an error (C: stat failure → 0).
        assert_eq!(evaluate(&fs, &["a", "-ef", "ghost"]), Ok(false));
    }

    /// A fully-default file description for fixture `..` overrides.
    fn test_file(path: &'static str) -> TestFile<'static> {
        TestFile {
            path,
            kind: FileKind::default(),
            access: Access::default(),
            size: 0,
            mtime: 0,
            flags: ModeFlags::default(),
            owned: false,
            grouped: false,
            dev: 0,
            ino: 0,
        }
    }

    #[test]
    fn test_access_bits_matches_c_test_access() {
        // Root passes if any class bit is set: 0o600 owner-rw, read → true
        // even though other-class has no read (test.c:510-511).
        let root = access_bits(0o600, 1000, 100, 0, 0, &[]);
        assert!(root.read);
        assert!(root.write);
        assert!(!root.execute);
        // Owner lane: euid == st_uid reads the owner bits (test.c:512-513).
        let owner = access_bits(0o640, 1000, 100, 1000, 100, &[]);
        assert!(owner.read); // owner rw-x: read set
        assert!(owner.write);
        assert!(!owner.execute);
        // Group lane via effective gid (test.c:514-515).
        let group = access_bits(0o040, 1000, 100, 7, 100, &[]);
        assert!(group.read); // group r--: read set
        assert!(!group.write);
        // Group lane via a supplementary group (test.c:517-528): st_gid
        // matches neither euid-owner nor egid, but appears in the
        // supplementary list, so the group lane governs. mode 0o010 is
        // S_IXGRP; execute is the only bit set.
        let sup = access_bits(0o010, 1000, 555, 7, 9, &[555]);
        assert!(sup.execute);
        assert!(!sup.read);
        // A non-matching supplementary list falls to the other lane.
        let not_sup = access_bits(0o010, 1000, 555, 7, 9, &[111]);
        assert!(!not_sup.execute); // other lane of 0o010 is 0
        // With no class match, the other lane governs (test.c: fallthrough).
        let other = access_bits(0o007, 1000, 100, 7, 9, &[]);
        assert_eq!(
            other,
            Access { read: true, write: true, execute: true }
        );
    }
}
