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
//! socket, `-b`/`-c` devices, `-N` modified since read, `-nt`/`-ot`
//! newer/older) go through [`FileTester`], so evaluation never touches a
//! real file system. Identity and time comparisons reduce to numbers the
//! tester reports.

use crate::FileOpError;

/// Answers the file status questions an expression can ask.
///
/// Two implementations ship: [`EmptyFs`] (nothing exists — the honest
/// starting point) and [`TableFs`] (an in memory file list for tests).
/// The execution layer adds the live implementation later without touching
/// the evaluator.
pub trait FileTester {
    /// True when the path exists at all.
    fn exists(&self, path: &str) -> bool;
    /// File kind flags: regular, directory, link, pipe, socket, block or
    /// character device. Unset flags read false.
    fn kind(&self, path: &str) -> FileKind;
    /// True when the path is readable / writable / executable.
    fn access(&self, path: &str) -> Access;
    /// Byte size, or `None` when unknown.
    fn size(&self, path: &str) -> Option<u64>;
    /// Modification time in seconds, or `None` when unknown.
    fn mtime(&self, path: &str) -> Option<u64>;
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
    /// Block or character device.
    pub device: bool,
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
            "-b" | "-c" => Ok(self.tester.kind(self.operand()?).device),
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
            Some("=") | Some("==") => {
                self.pos += 1;
                Ok(left == self.operand()?)
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
}
