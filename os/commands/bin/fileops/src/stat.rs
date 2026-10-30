//! `stat`'s option face and format-string parsing (the first deciding-half
//! package).
//!
//! Ground truth is `minix3/usr.bin/stat/stat.c` (NetBSD 1.38). The command's
//! skeleton is a small format machine: a stream of templates carrying field
//! codes, plus a set of switches that replace the template with predefined
//! ones. This package does only two things: "read which predefined template the
//! command line selects" and "split a template into renderable pieces"; turning
//! pieces into text (field values, radix and width, time rendering, symlink
//! tail) is the next package, with the thin shell and the system-call legs
//! after it.
//!
//! The split works because C's `output()` (`stat.c:368-566`) only produces
//! "sub-format string + parsed parameters" segments while scanning the
//! template, then hands them to `format1()` (`stat.c:568-1151`) to fetch values
//! and render. The two halves share no state, so the parsed form can be pinned
//! by tests on its own.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// The predefined templates. C expresses them as four macros plus
/// `TIME_FORMAT` (`stat.c:94-115`); they are copied verbatim as constants here
/// so that a host test can assert directly "which preset was chosen".
pub const DEF_FORMAT: &str =
    "%d %i %Sp %l %Su %Sg %r %z \"%Sa\" \"%Sm\" \"%Sc\" \"%SB\" %k %b %#Xf %N";
/// Preset for `-r` (`stat.c:97-98`).
pub const RAW_FORMAT: &str = "%d %i %#p %l %u %g %r %z %a %m %c %B %k %b %f %N";
/// Preset for `-l` (`stat.c:99`).
pub const LS_FORMAT: &str = "%Sp %l %Su %Sg %Z %Sm %N%SY";
/// Preset for `-l -F` (`stat.c:100`).
pub const LSF_FORMAT: &str = "%Sp %l %Su %Sg %Z %Sm %N%T%SY";
/// Preset for `-s` (`stat.c:101-105`).
pub const SHELL_FORMAT: &str = "st_dev=%d st_ino=%i st_mode=%#p st_nlink=%l \
    st_uid=%u st_gid=%g st_rdev=%r st_size=%z st_atime=%Sa st_mtime=%Sm \
    st_ctime=%Sc st_birthtime=%SB st_blksize=%k st_blocks=%b st_flags=%f";
/// Preset for `-x` (`stat.c:106-113`).
pub const LINUX_FORMAT: &str = "  File: \"%N\"%n  Size: %-11z  FileType: %HT%n  \
    Mode: (%04OLp/%.10Sp)         Uid: (%5u/%8Su)  Gid: (%5g/%8Sg)%nDevice: %Hd,%Ld   \
    Inode: %i    Links: %l%nAccess: %Sa%nModify: %Sm%nChange: %Sc";
/// The default time template (`stat.c:115`); `-s` uses `%s` and `-x` uses
/// `%c` (`stat.c:302-311`).
pub const TIME_FORMAT: &str = "%b %e %T %Y";

/// Field codes. The `SHOW_*` list at `stat.c:158-178`, one letter each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Datum {
    /// `%R`:realpath(`stat.c:158`).
    Realpath,
    /// `%d`:`st_dev`.
    Dev,
    /// `%i`:`st_ino`.
    Ino,
    /// `%p`:`st_mode`.
    Mode,
    /// `%l`:`st_nlink`.
    Nlink,
    /// `%u`:`st_uid`.
    Uid,
    /// `%g`:`st_gid`.
    Gid,
    /// `%r`:`st_rdev`.
    Rdev,
    /// `%a`:`st_atime`.
    Atime,
    /// `%m`:`st_mtime`.
    Mtime,
    /// `%c`:`st_ctime`.
    Ctime,
    /// `%B`:`st_birthtime`.
    Btime,
    /// `%z`:`st_size`.
    Size,
    /// `%b`:`st_blocks`.
    Blocks,
    /// `%k`:`st_blksize`.
    Blksize,
    /// `%f`:`st_flags`.
    Flags,
    /// `%v`:`st_generation`.
    Gen,
    /// `%Y`: the symlink's contents.
    Symlink,
    /// `%T`: the file type name.
    Filetype,
    /// `%N`: the file name.
    Filename,
    /// `%Z`: `sizerdev` — char/block devices give `rdev`, everything else
    /// gives `size` (the comment at `stat.c:447-448`).
    SizeRdev,
}

impl Datum {
    /// Parse the field-code letter (the long `fmtcase` chain at
    /// `stat.c:515-537`).
    fn from_char(c: u8) -> Option<Datum> {
        Some(match c {
            b'R' => Datum::Realpath,
            b'd' => Datum::Dev,
            b'i' => Datum::Ino,
            b'p' => Datum::Mode,
            b'l' => Datum::Nlink,
            b'u' => Datum::Uid,
            b'g' => Datum::Gid,
            b'r' => Datum::Rdev,
            b'a' => Datum::Atime,
            b'm' => Datum::Mtime,
            b'c' => Datum::Ctime,
            b'B' => Datum::Btime,
            b'z' => Datum::Size,
            b'b' => Datum::Blocks,
            b'k' => Datum::Blksize,
            b'f' => Datum::Flags,
            b'v' => Datum::Gen,
            b'Y' => Datum::Symlink,
            b'T' => Datum::Filetype,
            b'N' => Datum::Filename,
            b'Z' => Datum::SizeRdev,
            _ => return None,
        })
    }
}

/// The radix/form selection letters `D O U X F S` (`stat.c:140-145`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutFmt {
    Decimal,
    Octal,
    Unsigned,
    Hex,
    Float,
    String,
}

impl OutFmt {
    /// Parse the output-form letter (`stat.c:494-503`).
    fn from_char(c: u8) -> Option<OutFmt> {
        Some(match c {
            b'D' => OutFmt::Decimal,
            b'O' => OutFmt::Octal,
            b'U' => OutFmt::Unsigned,
            b'X' => OutFmt::Hex,
            b'F' => OutFmt::Float,
            b'S' => OutFmt::String,
            _ => return None,
        })
    }
}

/// Sub-field selection `H M L` (`stat.c:154-156`): take the high, middle, or
/// low slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hilo {
    High,
    Middle,
    Low,
}

impl Hilo {
    /// Parse the sub-field letter (`stat.c:505-513`).
    fn from_char(c: u8) -> Option<Hilo> {
        Some(match c {
            b'H' => Hilo::High,
            b'M' => Hilo::Middle,
            b'L' => Hilo::Low,
            _ => return None,
        })
    }
}

/// Format-parameter bits. `FLAG_POUND`…`FLAG_MINUS` at `stat.c:117-121`; the
/// values are copied bit for bit.
pub const FLAG_POUND: u32 = 0x01;
/// See `FLAG_POUND` (`stat.c:118`).
pub const FLAG_SPACE: u32 = 0x02;
/// See `FLAG_POUND` (`stat.c:119`).
pub const FLAG_PLUS: u32 = 0x04;
/// See `FLAG_POUND` (`stat.c:120`).
pub const FLAG_ZERO: u32 = 0x08;
/// See `FLAG_POUND` (`stat.c:121`).
pub const FLAG_MINUS: u32 = 0x10;

/// Single-character shorthand pieces (the `SIMPLE_*` set at `stat.c:129-132`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Simple {
    /// `%n`: a newline.
    Newline,
    /// `%t`: a tab.
    Tab,
    /// `%%`: one percent sign.
    Percent,
    /// `%@`: the operand's ordinal among the files (`stat.c:417-423`; `fn`
    /// starts at 1).
    FileNumber,
}

/// One complete field spec, carrying the sub-format string C uses in its error
/// message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec<'a> {
    /// Bitwise OR of the `FLAG_*` bits (`stat.c:451-462`).
    pub flags: u32,
    /// Field width; `-1` means absent (C's initial value, `stat.c:468`).
    pub size: i32,
    /// Precision; `-1` means absent (C's initial value, `stat.c:479`).
    pub prec: i32,
    /// Output form; `None` when absent (C sets `ofmt = 0`, `stat.c:501`).
    pub ofmt: Option<OutFmt>,
    /// Sub-field; `None` when absent (C sets `hilo = 0`, `stat.c:512`).
    pub hilo: Option<Hilo>,
    /// Field code.
    pub datum: Datum,
    /// The verbatim text of this sub-format string (from the leading `%`
    /// through the field code). C passes it to `format1()` together with its
    /// length (`stat.c:570`), and both error sites echo it via
    /// `"%.*s: bad format"`: unreadable field codes during scanning at
    /// `stat.c:555-556`, and a field rejecting the requested form while values
    /// are fetched, at `stat.c:989`, `:997`, `:1117`.
    pub source: &'a str,
}

/// A piece of a split template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece<'a> {
    /// An ordinary byte, emitted verbatim.
    Literal(u8),
    /// One of the four shorthand forms `%n` / `%t` / `%%` / `%@`.
    Simple(Simple),
    /// One field spec.
    Spec(Spec<'a>),
}

/// Bad format. C always does `errx(1, "%.*s: bad format", …)`
/// (`stat.c:554-556`); here the sub-format string is handed to the caller to
/// render, and the pure logic layer need not know the exit code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadFormat {
    /// The sub-format string echoed in the error text (from `%` through the
    /// offending character, inclusive).
    pub span: String,
}

/// Split a template (C's `output()` main loop, `stat.c:380-556`).
///
/// The shape follows C: after `%`, first try the four shorthand letters; then
/// eat up to five parameter-bit letters; then a decimal width, an optional `.`
/// with a decimal precision, an optional form letter, and an optional sub-field
/// letter; finally a field code is mandatory, otherwise the whole sub-format
/// string (including the offending character) goes into `BadFormat`.
pub fn parse_format(fmt: &str) -> Result<Vec<Piece<'_>>, BadFormat> {
    let bytes = fmt.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] != b'%' {
            out.push(Piece::Literal(bytes[i]));
            i += 1;
            continue;
        }
        // The sub-format string starts at this '%' (C's `subfmt = statfmt`,
        // `stat.c:398`).
        let start = i;
        i += 1;

        // The four single-character shorthands (`stat.c:402-425`).
        if i < bytes.len() {
            let simple = match bytes[i] {
                b'n' => Some(Simple::Newline),
                b't' => Some(Simple::Tab),
                b'%' => Some(Simple::Percent),
                b'@' => Some(Simple::FileNumber),
                _ => None,
            };
            if let Some(s) = simple {
                out.push(Piece::Simple(s));
                i += 1;
                continue;
            }
        }

        let mut flags = 0u32;
        while i < bytes.len() {
            let f = match bytes[i] {
                b'#' => FLAG_POUND,
                b' ' => FLAG_SPACE,
                b'+' => FLAG_PLUS,
                b'0' => FLAG_ZERO,
                b'-' => FLAG_MINUS,
                _ => break,
            };
            flags |= f;
            i += 1;
        }

        let mut size = -1i32;
        if i < bytes.len() && bytes[i].is_ascii_digit() {
            size = 0;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                let next = size
                    .checked_mul(10)
                    .and_then(|v| v.checked_add((bytes[i] - b'0') as i32));
                match next {
                    Some(v) if v >= 0 => {
                        size = v;
                        i += 1;
                    }
                    // C checks whether the multiply-add went negative
                    // (`stat.c:469-475`); overflow to negative and values beyond
                    // the representable range both fall into the same
                    // bad-format arm, with the error text taken by the same
                    // rule.
                    _ => return Err(bad(fmt, start, i)),
                }
            }
        }

        let mut prec = -1i32;
        if i < bytes.len() && bytes[i] == b'.' {
            i += 1;
            prec = 0;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                let next = prec
                    .checked_mul(10)
                    .and_then(|v| v.checked_add((bytes[i] - b'0') as i32));
                match next {
                    Some(v) if v >= 0 => {
                        prec = v;
                        i += 1;
                    }
                    _ => return Err(bad(fmt, start, i)),
                }
            }
        }

        // At most one form letter and one sub-field letter (`stat.c:494-513`).
        let ofmt = if i < bytes.len() {
            OutFmt::from_char(bytes[i])
        } else {
            None
        };
        if ofmt.is_some() {
            i += 1;
        }
        let hilo = if i < bytes.len() {
            Hilo::from_char(bytes[i])
        } else {
            None
        };
        if hilo.is_some() {
            i += 1;
        }

        let datum = match bytes.get(i).copied().and_then(Datum::from_char) {
            Some(d) => d,
            None => return Err(bad(fmt, start, i)),
        };
        i += 1;

        out.push(Piece::Spec(Spec {
            flags,
            size,
            prec,
            ofmt,
            hilo,
            datum,
            source: &fmt[start..i],
        }));
    }

    Ok(out)
}

/// The bad-format error text: C slices the sub-format string to the length
/// `statfmt - subfmt + 1` (`stat.c:554-556`). Here `i` stops at the offending
/// position (possibly the end of the string), so the span is the closed range
/// `[start, i]`; at the end of the string C counts the `'\0'` into the length
/// too, but what is actually printed is still the text up to the end.
fn bad(fmt: &str, start: usize, i: usize) -> BadFormat {
    let end = (i + 1).min(fmt.len());
    BadFormat {
        span: fmt[start..end].to_string(),
    }
}

/// Which predefined template the command line selects. C expresses it as the
/// single integer `fmtchar` (`stat.c:204`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FmtChar {
    /// Custom `-f` (or the default template).
    Format,
    /// `-l` (`LSF_FORMAT` when `-F` is given).
    Ls,
    /// `-r`.
    Raw,
    /// `-s`.
    Shell,
    /// `-x`.
    Linux,
}

/// One binary switching personality by `argv[0]`: `readlink` is `stat`'s other
/// face (`stat.c:219-232`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Personality {
    Stat,
    Readlink,
}

/// The parsed result of the option face.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan<'a> {
    /// The personality.
    pub personality: Personality,
    /// The selected template: a preset constant or the one given to `-f`.
    pub format: &'a str,
    /// Time template (`-t`), `TIME_FORMAT` when absent; `-s`/`-x` have their
    /// own defaults (`stat.c:292-318`).
    pub timefmt: &'a str,
    /// `-F`: `-l` uses the variant carrying `%T`.
    pub ls_f: bool,
    /// `-L`: `stat()` first, falling back to `lstat()` on `ENOENT`
    /// (`stat.c:327-334`).
    pub use_stat: bool,
    /// `-n`: no trailing newline (`stat.c:559-560`).
    pub no_nl: bool,
    /// `-q`: stay silent when a value cannot be fetched.
    pub quiet: bool,
    /// Operand start (C's `optind`).
    pub operands: usize,
}

/// A failure of the option face. `Usage` corresponds to C's `usage()` exit
/// (`stat.c:357-365`); `Conflict` corresponds to the two `errx` sites
/// (`stat.c:261-263`, `stat.c:289-291`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    /// The `usage: <prog> <synopsis>` path.
    Usage,
    /// `can't use format '%c' with '%c'`: two template switches collide
    /// (`stat.c:261-263`).
    FormatConflict { first: char, second: char },
    /// `can't use format '%c' with -F`(`stat.c:289-291`).
    NotWithDashF { fmtchar: char },
}

/// Scan the arguments (C's `getopt` string is at `stat.c:219-232`: `stat` uses
/// `"f:FlLnqrst:x"` and `readlink` uses `"fnqsv"`).
///
/// `prog_name` is passed already stripped of its directory (C's
/// `getprogname()`, rule in `minix3/lib/csu/common/crt0-common.c:156-165`,
/// taken through the shell's single `bin_support::prog_name`), because the
/// personality test is `strcmp(getprogname(), "readlink") == 0` at `stat.c:220`
/// — whole-string equality, not "ends with `readlink`": `/bin/my-readlink`
/// still takes the stat personality in C.
///
/// Both personalities' differences follow C: readlink's default template is
/// `%Y`, `quiet` is on by default, `-f` means `%R`, `-s` only sets `quiet`
/// again, and `-v` clears `quiet`; stat's `-F -L -l -r -t -x` are all unknown
/// switches there (absent from its option string).
pub fn parse_args<'a>(args: &[&'a str], prog_name: &str) -> Result<Plan<'a>, PlanError> {
    let readlink = prog_name == "readlink";
    let mut format: Option<&'a str> = None;
    let mut timefmt: Option<&'a str> = None;
    let mut fmtchar: Option<char> = None;
    let mut ls_f = false;
    let mut use_stat = false;
    let mut no_nl = false;
    let mut quiet = readlink;

    let mut i = 0usize;
    while i < args.len() {
        let a = args[i];
        if a == "--" {
            i += 1;
            break;
        }
        let bytes = a.as_bytes();
        if bytes.len() < 2 || bytes[0] != b'-' {
            break; // BSD getopt stops at the first non-option
        }
        let mut k = 1usize;
        while k < bytes.len() {
            let c = bytes[k] as char;
            match c {
                'F' if !readlink => ls_f = true,
                'L' if !readlink => use_stat = true,
                'n' => no_nl = true,
                'q' => quiet = true,
                'v' if readlink => quiet = false,
                'f' => {
                    if readlink {
                        // readlink's option string is `"fnqsv"` (`stat.c:221`):
                        // `f` has no colon, so `-f` takes no value; it means
                        // "report only the resolved path".
                        format = Some("%R");
                    } else {
                        let value = take_value(args, &mut i, bytes, &mut k)?;
                        format = Some(value);
                        // `stat.c:245-258`: `-f` and `-l/-r/-s/-x` share the
                        // `fmtchar` slot, so a second template switch
                        // collides.
                        claim_fmtchar(&mut fmtchar, 'f')?;
                    }
                }
                'l' | 'r' | 's' => {
                    if readlink {
                        // `stat.c:246-249`: readlink's `-s` only sets quiet
                        // again. `-l`/`-r` are not in its option string and
                        // fall into the unknown branch below.
                        if c == 's' {
                            quiet = true;
                        } else {
                            return Err(PlanError::Usage);
                        }
                    } else {
                        claim_fmtchar(&mut fmtchar, c)?;
                    }
                }
                'x' if !readlink => claim_fmtchar(&mut fmtchar, c)?,
                't' if !readlink => timefmt = Some(take_value(args, &mut i, bytes, &mut k)?),
                _ => return Err(PlanError::Usage),
            }
            k += 1;
        }
        i += 1;
    }

    // `stat.c:279-291`: with no template switch given, `-F` folds to `-l`;
    // otherwise the default template applies.
    let fmtchar = match fmtchar {
        Some(c) => c,
        None if ls_f => 'l',
        None => 'f',
    };
    if ls_f && fmtchar != 'l' {
        return Err(PlanError::NotWithDashF { fmtchar });
    }

    if readlink {
        return Ok(Plan {
            personality: Personality::Readlink,
            format: format.unwrap_or("%Y"),
            timefmt: timefmt.unwrap_or(TIME_FORMAT),
            ls_f,
            use_stat,
            no_nl,
            quiet,
            operands: i,
        });
    }

    // `-s` and `-x` each carry their own time default (`stat.c:292-318`).
    let timefmt = match fmtchar {
        's' => timefmt.unwrap_or("%s"),
        'x' => timefmt.unwrap_or("%c"),
        _ => timefmt.unwrap_or(TIME_FORMAT),
    };
    let format: &'a str = match fmtchar {
        'f' => format.unwrap_or(DEF_FORMAT),
        'l' => {
            if ls_f {
                LSF_FORMAT
            } else {
                LS_FORMAT
            }
        }
        'r' => RAW_FORMAT,
        's' => SHELL_FORMAT,
        'x' => LINUX_FORMAT,
        _ => DEF_FORMAT,
    };

    Ok(Plan {
        personality: Personality::Stat,
        format,
        timefmt,
        ls_f,
        use_stat,
        no_nl,
        quiet,
        operands: i,
    })
}

/// Take a value-taking switch's value: the rest of the cluster when characters
/// remain (getopt's `-fVALUE` form), otherwise the next argv entry; with
/// neither it is a missing argument, and getopt answers `?` in C, going to
/// usage.
fn take_value<'a>(
    args: &[&'a str],
    i: &mut usize,
    bytes: &[u8],
    k: &mut usize,
) -> Result<&'a str, PlanError> {
    if *k + 1 < bytes.len() {
        let value = &args[*i][*k + 1..];
        *k = bytes.len() - 1; // consume the cluster's rest (the outer loop adds 1)
        return Ok(value);
    }
    let next = *i + 1;
    if next >= args.len() {
        return Err(PlanError::Usage);
    }
    *i = next;
    Ok(args[next])
}

/// `stat.c:255-257`: when `fmtchar` is already taken, report
/// `can't use format '%c' with '%c'`.
fn claim_fmtchar(slot: &mut Option<char>, c: char) -> Result<(), PlanError> {
    if let Some(first) = *slot {
        return Err(PlanError::FormatConflict { first, second: c });
    }
    *slot = Some(c);
    Ok(())
}

/// One time field: whole seconds plus nanoseconds (C's `st_*time` and
/// `st_*timensec`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeField {
    pub secs: i64,
    pub nsecs: i64,
}

impl TimeField {
    /// Assemble a time field.
    pub const fn new(secs: i64, nsecs: i64) -> Self {
        TimeField { secs, nsecs }
    }
}

/// A file status made of plain fields. The thin shell copies the status
/// structure the kernel returns field by field, so this layer never touches the
/// system-call face (layering boundary: only the thin shell may use `minix_sys`
/// to read a handle).
///
/// Widths follow C's field widths, because `format1()`'s `small` test (the
/// `sizeof(...) == 4` family at `stat.c:606`, `:636`, `:684`) really does change
/// values in non-decimal forms: 32-bit fields are first truncated to 32 bits in
/// octal/unsigned/hex form (`stat.c:1126-1128`).
#[derive(Debug, Clone, Copy)]
pub struct StatData {
    /// `st_dev` (`dev_t`, 64-bit).
    pub dev: u64,
    /// `st_mode` (`mode_t`, 32-bit).
    pub mode: u32,
    /// `st_ino` (64-bit).
    pub ino: u64,
    /// `st_nlink` (32-bit).
    pub nlink: u32,
    /// `st_uid` (32-bit).
    pub uid: u32,
    /// `st_gid` (32-bit).
    pub gid: u32,
    /// `st_rdev` (64-bit).
    pub rdev: u64,
    /// `st_atim`.
    pub atime: TimeField,
    /// `st_mtim`.
    pub mtime: TimeField,
    /// `st_ctim`.
    pub ctime: TimeField,
    /// `st_birthtim`.
    pub btime: TimeField,
    /// `st_size` (64-bit, signed).
    pub size: i64,
    /// `st_blocks` (64-bit, the count of 512-byte blocks).
    pub blocks: i64,
    /// `st_blksize` (32-bit).
    pub blksize: i32,
    /// `st_flags` (32-bit).
    pub flags: u32,
    /// `st_gen` (32-bit). The field is not named `gen`: that is a reserved word
    /// in the 2024 edition of the language.
    pub generation: u32,
}

/// The allowed-form set; bit values are identical to C's `FMTF_*`
/// (`stat.c:147-152`).
mod fmtf {
    pub const DECIMAL: u8 = 0x01;
    pub const OCTAL: u8 = 0x02;
    pub const UNSIGNED: u8 = 0x04;
    pub const HEX: u8 = 0x08;
    pub const FLOAT: u8 = 0x10;
    pub const STRING: u8 = 0x20;
}

/// External information needed to render one spec. The symlink contents are read
/// once by the thin shell and passed in: C also `readlink()`s in place inside
/// `format1()` (`stat.c:844-861`), and keeping that leg on the thin-shell side
/// is what makes this layer's rendering fully testable.
pub struct Render<'a> {
    /// The file status.
    pub data: &'a StatData,
    /// Operand name; `None` means standard input (C's `file == NULL`, printing
    /// `(stdin)`).
    pub name: Option<&'a str>,
    /// The symlink target. Meaningful only when `mode` really is a symlink.
    pub link: Option<&'a str>,
    /// The resolved result `%R` needs: the thin shell resolves through
    /// `crate::realpath` and passes it in (`None` means resolution failed,
    /// corresponding to C's `realpath()` returning `NULL`, `stat.c:824-833`).
    pub resolved: Option<&'a str>,
    /// Time template (`-t`, the `-s`/`-x` default, or `TIME_FORMAT`).
    pub timefmt: &'a str,
    /// Operand ordinal for `%@`; C starts at 1 (`fn = 1` at `stat.c:278`).
    pub file_number: usize,
}

/// The observable side effects of one render round; the thin shell decides the
/// exit code and the trailing newline from them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Report {
    /// C's global `linkfail`: set when `%Y` cannot read the target or `%R`
    /// fails to resolve (`stat.c:850`, `:829`, `:859`); the `readlink`
    /// personality uses it as the exit code (`stat.c:354`).
    pub linkfail: bool,
    /// Whether this round's output ends with a newline (`stat.c:559-560` decides
    /// from it whether to add one).
    pub ends_with_newline: bool,
}

/// A failure of the render stage. C does `errx(1, "%.*s: bad format", flen,
/// fmt)` at both sites.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FaceError {
    /// A sub-field or form letter that does not fit the field
    /// (`stat.c:994-996`), or a string form with no string to print
    /// (`stat.c:1116-1117`). Carries the verbatim sub-format string.
    BadFormat(String),
}

/// The fetched value of one field. `num` is always present (for numeric forms),
/// `text` only when a string form exists, and `nsecs` can only be carried by
/// time fields for the `F` form.
struct Field {
    num: u64,
    text: Option<String>,
    allowed: u8,
    default: OutFmt,
    small: bool,
    shift: u32,
    linkfail: bool,
    /// Only time fields can carry it: the `F` form prints
    /// `seconds.nanoseconds` (`stat.c:1029-1099`).
    nsecs: i64,
    /// The `%Z` arm for char/block devices: C renders the two slices separately
    /// and joins them with a comma (`stat.c:966-978`); the numeric/string form
    /// choice applies to each slice.
    pair_of_rdev: bool,
}

impl Field {
    fn numeric(num: u64, allowed: u8, default: OutFmt, small: bool) -> Field {
        Field {
            num,
            text: None,
            allowed,
            default,
            small,
            shift: 0,
            linkfail: false,
            nsecs: 0,
            pair_of_rdev: false,
        }
    }
}

/// Fetch one field's value. Corresponds to the big `switch (what)` at the top of
/// `format1()` (`stat.c:600-990`). `hilo` is folded here: a sub-field letter a
/// field consumes is taken and set to `None`, one it does not consume is left
/// for the caller to judge as bad format, following C.
fn take_field(spec: &Spec<'_>, ctx: &Render<'_>) -> Result<(Field, Option<Hilo>), FaceError> {
    let d = ctx.data;
    let mut hilo = spec.hilo;
    let field = match spec.datum {
        Datum::Dev | Datum::Rdev => {
            let raw = if spec.datum == Datum::Dev {
                d.dev
            } else {
                d.rdev
            };
            // C's slice rule: `major()` takes the high slice, `minor()` the
            // low one (`stat.c:618-625`).
            let num = match hilo {
                Some(Hilo::High) => {
                    hilo = None;
                    major_of(raw)
                }
                Some(Hilo::Low) => {
                    hilo = None;
                    minor_of(raw)
                }
                _ => raw,
            };
            let mut f = Field::numeric(
                num,
                fmtf::DECIMAL | fmtf::OCTAL | fmtf::UNSIGNED | fmtf::HEX | fmtf::STRING,
                OutFmt::Unsigned,
                false,
            );
            // `devname(3)` would have to scan `/dev` for a matching entry; this
            // tree has no such leg, so take C's not-found arm: print `???`
            // (`sdata = "???"` at `stat.c:616`).
            f.text = Some("???".to_string());
            f
        }
        Datum::Ino => Field::numeric(
            d.ino,
            fmtf::DECIMAL | fmtf::OCTAL | fmtf::UNSIGNED | fmtf::HEX,
            OutFmt::Unsigned,
            false,
        ),
        Datum::Mode => {
            // Take the trailing pad off `strmode`'s eleven characters, then
            // slice (`stat.c:636-665`: high takes the user triple, middle the
            // group triple, low the other triple).
            let mut smode = crate::mode::strmode(d.mode).to_vec();
            if smode.last() == Some(&b' ') {
                smode.pop();
            }
            let (num, slice) = match hilo {
                Some(Hilo::High) => {
                    // The number folds to the type bits plus the user triple
                    // (`stat.c:652`: `data >>= 12`).
                    hilo = None;
                    ((d.mode >> 12) as u64, &smode[1..4])
                }
                Some(Hilo::Middle) => {
                    hilo = None;
                    (((d.mode >> 9) & 0o7) as u64, &smode[4..7])
                }
                Some(Hilo::Low) => {
                    hilo = None;
                    ((d.mode & 0o777) as u64, &smode[7..10])
                }
                _ => (d.mode as u64, smode.as_slice()),
            };
            Field {
                num,
                text: Some(String::from_utf8_lossy(slice).into_owned()),
                allowed: fmtf::DECIMAL | fmtf::OCTAL | fmtf::UNSIGNED | fmtf::HEX | fmtf::STRING,
                default: OutFmt::Octal,
                small: true,
                shift: 0,
                linkfail: false,
                nsecs: 0,
                pair_of_rdev: false,
            }
        }
        Datum::Nlink => Field::numeric(
            d.nlink as u64,
            fmtf::DECIMAL | fmtf::OCTAL | fmtf::UNSIGNED | fmtf::HEX,
            OutFmt::Unsigned,
            true,
        ),
        // This tree has no password-database wrapper yet (the same prerequisite
        // as `chown`), so C's "not found" arm always applies: print the `(123)`
        // form (`stat.c:685-690`, `:699-704`), not a bare number.
        Datum::Uid => named_id(d.uid as u64, true),
        Datum::Gid => named_id(d.gid as u64, false),
        Datum::Size => {
            let mut f = Field::numeric(
                d.size as u64,
                fmtf::DECIMAL | fmtf::OCTAL | fmtf::UNSIGNED | fmtf::HEX,
                OutFmt::Unsigned,
                false,
            );
            // The high/middle/low slices are "scaled by powers of two to G/M/K"
            // at `stat.c:767-782`, then "shift right one, add one, shift right
            // one" before printing (`stat.c:1144-1148`), i.e. rounding half up.
            f.shift = match hilo {
                Some(Hilo::High) => {
                    hilo = None;
                    30
                }
                Some(Hilo::Middle) => {
                    hilo = None;
                    20
                }
                Some(Hilo::Low) => {
                    hilo = None;
                    10
                }
                _ => 0,
            };
            f
        }
        Datum::Blocks => Field::numeric(
            d.blocks as u64,
            fmtf::DECIMAL | fmtf::OCTAL | fmtf::UNSIGNED | fmtf::HEX,
            OutFmt::Unsigned,
            false,
        ),
        Datum::Blksize => Field::numeric(
            d.blksize as u64,
            fmtf::DECIMAL | fmtf::OCTAL | fmtf::UNSIGNED | fmtf::HEX,
            OutFmt::Unsigned,
            true,
        ),
        Datum::Flags => Field::numeric(
            d.flags as u64,
            fmtf::DECIMAL | fmtf::OCTAL | fmtf::UNSIGNED | fmtf::HEX,
            OutFmt::Unsigned,
            true,
        ),
        Datum::Gen => Field::numeric(
            d.generation as u64,
            fmtf::DECIMAL | fmtf::OCTAL | fmtf::UNSIGNED | fmtf::HEX,
            OutFmt::Unsigned,
            true,
        ),
        Datum::Atime | Datum::Mtime | Datum::Ctime | Datum::Btime => {
            let t = match spec.datum {
                Datum::Atime => d.atime,
                Datum::Mtime => d.mtime,
                Datum::Ctime => d.ctime,
                _ => d.btime,
            };
            time_field(t, ctx.timefmt)
        }
        Datum::Realpath => {
            // This arm shares `%Y`'s prefix algorithm (`stat.c:822-838`): write
            // `" -> "` first, then let realpath write into `path + 4`; on
            // failure, speak once, set `linkfail`, and empty the string. The
            // default form is filled in after the pointer is computed, so a bare
            // `%R` carries no prefix and an explicit `%SR` does. Resolution
            // itself lives in the thin shell (`crate::realpath`); this layer
            // only consumes the result handed back.
            let explicit_string = spec.ofmt == Some(OutFmt::String);
            match (ctx.name, ctx.resolved) {
                (None, _) => text_field("(stdin)".to_string()),
                (_, Some(target)) if explicit_string => text_field(format!(" -> {target}")),
                (_, Some(target)) => text_field(target.to_string()),
                (_, None) => {
                    let mut f = text_field(String::new());
                    f.linkfail = true;
                    f
                }
            }
        }
        Datum::Symlink => {
            // C's order must be kept: the default form is filled in after the
            // pointer is computed (`stat.c:855-856`), so a bare `%Y` carries no
            // ` -> ` prefix and an explicit `%SY` does (the comment at
            // `stat.c:443-446`).
            let explicit_string = spec.ofmt == Some(OutFmt::String);
            if (d.mode & crate::cp::S_IFMT) == crate::cp::S_IFLNK {
                let body = ctx.link.unwrap_or_default().to_string();
                let text = if explicit_string {
                    format!(" -> {body}")
                } else {
                    body
                };
                Field {
                    num: 0,
                    text: Some(text),
                    allowed: fmtf::STRING,
                    default: OutFmt::String,
                    small: false,
                    shift: 0,
                    linkfail: ctx.link.is_none(),
                    nsecs: 0,
                    pair_of_rdev: false,
                }
            } else {
                Field {
                    num: 0,
                    text: Some(String::new()),
                    allowed: fmtf::STRING,
                    default: OutFmt::String,
                    small: false,
                    shift: 0,
                    linkfail: true,
                    nsecs: 0,
                    pair_of_rdev: false,
                }
            }
        }
        Datum::Filetype => {
            let (name, mark) = type_pair(d.mode);
            match hilo {
                None | Some(Hilo::Low) => {
                    hilo = None;
                    text_field(mark.to_string())
                }
                Some(Hilo::High) => {
                    hilo = None;
                    text_field(name.to_string())
                }
                _ => Field::numeric(0, fmtf::STRING, OutFmt::String, false),
            }
        }
        Datum::Filename => {
            let Some(name) = ctx.name else {
                let f = text_field("(stdin)".to_string());
                // `(stdin)` has no path to slice; C drops the high/low slices
                // here (`stat.c:919-926`).
                if hilo == Some(Hilo::High) || hilo == Some(Hilo::Low) {
                    hilo = None;
                }
                return Ok((f, hilo));
            };
            if spec.hilo.is_none() {
                text_field(name.to_string())
            } else {
                let trimmed = name.trim_end_matches('/');
                match hilo {
                    Some(Hilo::High) => {
                        hilo = None;
                        let dir = trimmed.rsplit_once('/').map_or(".", |(head, _)| head);
                        text_field(dir.to_string())
                    }
                    Some(Hilo::Low) => {
                        hilo = None;
                        let base = trimmed.rsplit_once('/').map_or(trimmed, |(_, tail)| tail);
                        text_field(base.to_string())
                    }
                    _ => text_field(name.to_string()),
                }
            }
        }
        Datum::SizeRdev => {
            // `%Z`: char/block devices take `rdev`'s high and low slices, the
            // rest take `size` (`stat.c:961-989`; C uses two recursive calls to
            // build `major,minor`).
            let is_dev = (d.mode & crate::cp::S_IFMT) == crate::cp::S_IFCHR
                || (d.mode & crate::cp::S_IFMT) == crate::cp::S_IFBLK;
            if is_dev {
                let mut f = Field::numeric(
                    0,
                    fmtf::DECIMAL | fmtf::OCTAL | fmtf::UNSIGNED | fmtf::HEX,
                    OutFmt::Unsigned,
                    false,
                );
                f.pair_of_rdev = true;
                f
            } else {
                let mut f = take_field(
                    &Spec {
                        flags: spec.flags,
                        size: spec.size,
                        prec: spec.prec,
                        ofmt: spec.ofmt,
                        hilo: None,
                        datum: Datum::Size,
                        source: spec.source,
                    },
                    ctx,
                )?;
                f.1 = None;
                f.0
            }
        }
    };
    Ok((field, hilo))
}

/// `%Su`/`%Sg`: print the name when it resolves, `(number)` when it does not.
/// This tree has no password database, so the latter always applies; this arm
/// has the same shape as C when `getpwuid()` returns `NULL` (`stat.c:685-690`).
fn named_id(value: u64, is_user: bool) -> Field {
    let mut f = Field::numeric(
        value,
        fmtf::DECIMAL | fmtf::OCTAL | fmtf::UNSIGNED | fmtf::HEX | fmtf::STRING,
        OutFmt::Unsigned,
        true,
    );
    f.text = Some(format!("({})", value));
    let _ = is_user;
    f
}

fn text_field(text: String) -> Field {
    Field {
        num: 0,
        text: Some(text),
        allowed: fmtf::STRING,
        default: OutFmt::String,
        small: false,
        shift: 0,
        linkfail: false,
        nsecs: 0,
        pair_of_rdev: false,
    }
}

/// A time field: numeric forms give whole seconds, the string form goes through
/// the `strftime` subset, and the float form carries nanoseconds too
/// (`stat.c:741-758`). C uses `localtime()`; this layer follows the same
/// convention as `date`/`who` — no timezone library, so the breakdown is UTC.
fn time_field(t: TimeField, timefmt: &str) -> Field {
    let mut text = String::new();
    if t.secs >= 0 || t.secs.checked_add(0).is_some() {
        let broken = minix_proctools::dates::breakdown(t.secs);
        let mut buf = [0u8; 256];
        // Hand back the seconds themselves so `%s` has something to print (it
        // is the `-s` default template).
        if let Ok(used) =
            minix_proctools::dates::format_at(timefmt, &broken, Some(t.secs), &mut buf)
        {
            text = String::from_utf8_lossy(&buf[..used]).into_owned();
        }
    }
    Field {
        num: t.secs as u64,
        text: Some(text),
        allowed: fmtf::DECIMAL
            | fmtf::OCTAL
            | fmtf::UNSIGNED
            | fmtf::HEX
            | fmtf::FLOAT
            | fmtf::STRING,
        default: OutFmt::Decimal,
        small: false,
        shift: 0,
        linkfail: false,
        nsecs: t.nsecs,
        pair_of_rdev: false,
    }
}

/// Major device number. The macro pair Minix3 provides for NetBSD-derived tools
/// is at `minix3/tools/compat/compat_defs.h:1254`: `(x & 0x000fff00) >> 8`.
fn major_of(dev: u64) -> u64 {
    (dev & 0x000f_ff00) >> 8
}

/// Minor device number. Same place, `compat_defs.h:1259-1260`: the high part
/// `0xfff00000 >> 12` OR'd with the low eight bits.
fn minor_of(dev: u64) -> u64 {
    ((dev & 0xfff0_0000) >> 12) | (dev & 0x0000_00ff)
}

/// `%T`'s two faces: the high slice is the name, the low slice (and the default)
/// is the `ls`-style mark (`stat.c:867-912`). An unknown type gives `???` in
/// both.
fn type_pair(mode: u32) -> (&'static str, &'static str) {
    const S_IFMT: u32 = crate::cp::S_IFMT;
    match mode & S_IFMT {
        crate::cp::S_IFIFO => ("Fifo File", "|"),
        crate::cp::S_IFCHR => ("Character Device", "c"),
        crate::cp::S_IFDIR => ("Directory", "/"),
        crate::cp::S_IFBLK => ("Block Device", "b"),
        crate::cp::S_IFREG => ("Regular File", if mode & 0o111 != 0 { "*" } else { "-" }),
        crate::cp::S_IFLNK => ("Symbolic Link", "@"),
        crate::cp::S_IFSOCK => ("Socket", "="),
        crate::mode::S_IFWHT => ("Whiteout File", "%"),
        _ => ("???", "?"),
    }
}

/// Render the piece list into text (`output()`'s output leg plus all of
/// `format1()`'s form selection).
///
/// Consecutive ordinary bytes are merged into one segment before landing in
/// `out`, so that non-ASCII text in the template (a UTF-8 multibyte sequence is
/// split into per-byte pieces) is not torn apart.
pub fn render(
    pieces: &[Piece<'_>],
    ctx: &Render<'_>,
    out: &mut String,
) -> Result<Report, FaceError> {
    let mut linkfail = false;
    let mut pending: Vec<u8> = Vec::new();
    for piece in pieces {
        match piece {
            Piece::Literal(b) => pending.push(*b),
            Piece::Simple(s) => {
                flush(&mut pending, out);
                out.push(match s {
                    Simple::Newline => '\n',
                    Simple::Tab => '\t',
                    Simple::Percent => '%',
                    Simple::FileNumber => {
                        // C prints `fn` with `%d` (`stat.c:417-424`).
                        pending.clear();
                        out.push_str(&itoa(ctx.file_number as i64));
                        continue;
                    }
                });
            }
            Piece::Spec(spec) => {
                flush(&mut pending, out);
                linkfail |= render_spec(spec, ctx, out)?;
            }
        }
    }
    flush(&mut pending, out);
    Ok(Report {
        linkfail,
        ends_with_newline: out.ends_with('\n'),
    })
}

fn flush(pending: &mut Vec<u8>, out: &mut String) {
    if !pending.is_empty() {
        out.push_str(&String::from_utf8_lossy(pending));
        pending.clear();
    }
}

/// Render one field spec and return whether C's `linkfail` should be set.
fn render_spec(spec: &Spec<'_>, ctx: &Render<'_>, out: &mut String) -> Result<bool, FaceError> {
    let (field, leftover_hilo) = take_field(spec, ctx)?;
    let form = match spec.ofmt {
        Some(f) => f,
        None => field.default,
    };
    // `stat.c:994-996`: an unconsumed sub-field slice, or a form outside the
    // field's allowed set, is bad format.
    if leftover_hilo.is_some() || !allowed_has(field.allowed, form) {
        return Err(FaceError::BadFormat(spec.source.to_string()));
    }
    if field.pair_of_rdev {
        // C recursively renders the major and minor slices here and joins them
        // with a comma; width and precision apply to each slice.
        let head = render_number(
            spec.flags,
            spec.size,
            spec.prec,
            form,
            major_of(ctx.data.rdev),
        );
        let tail = render_number(
            spec.flags,
            spec.size,
            spec.prec,
            form,
            minor_of(ctx.data.rdev),
        );
        out.push_str(&format!("{head},{tail}"));
        return Ok(field.linkfail);
    }
    if form == OutFmt::String {
        let Some(text) = &field.text else {
            // `stat.c:1116-1117`: a string form was asked for but there is no
            // string to print.
            return Err(FaceError::BadFormat(spec.source.to_string()));
        };
        let body = if spec.flags & FLAG_POUND != 0 {
            vis(text)
        } else {
            text.clone()
        };
        out.push_str(&pad(&body, spec.size, spec.prec, spec.flags));
        return Ok(field.linkfail);
    }
    if form == OutFmt::Float {
        out.push_str(&render_float(
            spec.flags,
            spec.size,
            spec.prec,
            field.num,
            field.nsecs,
        ));
        return Ok(field.linkfail);
    }
    let mut value = field.num;
    if field.small && form != OutFmt::Decimal {
        // `stat.c:1128-1130`: 32-bit fields are first truncated to 32 bits in
        // non-decimal forms, so sign extension does not fill the output with
        // f's.
        value &= 0xffff_ffff;
    }
    if field.shift > 0 {
        // `stat.c:1144-1148`: shift right one, add one, shift right one —
        // divide by the power of two, rounding half up.
        value = (value >> (field.shift - 1)) + 1;
        value >>= 1;
    }
    out.push_str(&render_number(
        spec.flags, spec.size, spec.prec, form, value,
    ));
    Ok(field.linkfail)
}

fn allowed_has(allowed: u8, form: OutFmt) -> bool {
    let bit = match form {
        OutFmt::Decimal => fmtf::DECIMAL,
        OutFmt::Octal => fmtf::OCTAL,
        OutFmt::Unsigned => fmtf::UNSIGNED,
        OutFmt::Hex => fmtf::HEX,
        OutFmt::Float => fmtf::FLOAT,
        OutFmt::String => fmtf::STRING,
    };
    allowed & bit != 0
}

/// Numeric forms: equivalent to C's assembled `%[flags][width][.prec]ll[doxu]`.
fn render_number(flags: u32, width: i32, prec: i32, form: OutFmt, value: u64) -> String {
    let signed = form == OutFmt::Decimal;
    let negative = signed && (value as i64) < 0;
    let magnitude = if negative {
        (value as i64).unsigned_abs()
    } else {
        value
    };
    let digits = match form {
        OutFmt::Hex => format!("{:x}", magnitude),
        OutFmt::Octal => format!("{:o}", magnitude),
        // The unsigned form must print all 64 bits (`%z` on a negative
        // `st_size` prints the wrapped huge number).
        OutFmt::Unsigned => format!("{magnitude}"),
        _ => itoa(magnitude as i64),
    };
    let digits = if prec >= 0 {
        pad_left(&digits, prec as usize, b'0' as char)
    } else {
        digits
    };

    let mut prefix = String::new();
    if flags & FLAG_POUND != 0 {
        match form {
            OutFmt::Hex => prefix.push_str("0x"),
            OutFmt::Octal if !digits.starts_with('0') => prefix.push('0'),
            _ => {}
        }
    }
    let mut sign = String::new();
    if negative {
        sign.push('-');
    } else if signed {
        if flags & FLAG_PLUS != 0 {
            sign.push('+');
        } else if flags & FLAG_SPACE != 0 {
            sign.push(' ');
        }
    }

    let body = format!("{sign}{prefix}{digits}");
    let zero_pad = flags & FLAG_ZERO != 0 && prec < 0 && flags & FLAG_MINUS == 0;
    if width > 0 && body.chars().count() < width as usize {
        let fill = width as usize - body.chars().count();
        if zero_pad {
            // Zero padding goes after the sign and prefix, before the digits
            // (`%#08x` prints `0x0000ff`).
            return format!("{}{}{}{}", sign, prefix, "0".repeat(fill), digits);
        }
        if flags & FLAG_MINUS != 0 {
            return format!("{body}{}", " ".repeat(fill));
        }
        return format!("{}{body}", " ".repeat(fill));
    }
    body
}

/// The `F` form: `seconds.nanoseconds`. C gives the width to the seconds
/// (`size -= prec + 1`, `stat.c:1056-1062`), and a precision above nine digits
/// is padded with zeros (`stat.c:1088-1093`).
fn render_float(flags: u32, width: i32, prec: i32, secs: u64, nsecs: i64) -> String {
    // A precision of 0 means "seconds only", with no decimal point either
    // (`stat.c:1031-1042`).
    if prec == 0 {
        return render_number(flags, width, -1, OutFmt::Decimal, secs);
    }
    // No precision given = print all available digits (nine); a given
    // precision is followed as is (`stat.c:1045-1048`).
    let want = if prec < 0 { 9 } else { prec };
    let shown = want.min(9) as usize;
    // The width goes to the seconds: C first subtracts the decimal point and
    // the fraction digits (`stat.c:1050-1062`).
    let secs_width = if width > 0 { width - (want + 1) } else { width };
    let head = render_number(flags, secs_width, -1, OutFmt::Decimal, secs);
    let mut frac = nsecs.unsigned_abs();
    for _ in shown..9 {
        frac /= 10;
    }
    let mut body = format!("{head}.{frac}");
    if let Some(pos) = body.find('.') {
        let digits = body.len() - pos - 1;
        if digits < shown {
            body.insert_str(pos + 1, &"0".repeat(shown - digits));
        }
    }
    // Anything beyond nine digits can only be zero padding (`stat.c:1088-1093`).
    if want > 9 {
        body.push_str(&"0".repeat((want - 9) as usize));
    }
    body
}

/// A decimal integer literal (the minimal implementation of C's `%d`/`%lld`).
fn itoa(value: i64) -> String {
    format!("{value}")
}

/// Left-pad to `width` characters, only when short.
fn pad_left(text: &str, width: usize, fill: char) -> String {
    if text.chars().count() >= width {
        return text.to_string();
    }
    format!(
        "{}{text}",
        fill.to_string().repeat(width - text.chars().count())
    )
}

/// String forms: precision truncates, width pads. The `0` flag does not apply
/// to strings — on C's side that depends on whether libc applies zero padding to
/// `%s` the way glibc does; this tree takes POSIX's conservative reading and
/// uses spaces, and none of the five preset templates reaches this form.
fn pad(text: &str, width: i32, prec: i32, flags: u32) -> String {
    let mut body: String = if prec >= 0 {
        text.chars().take(prec as usize).collect()
    } else {
        text.to_string()
    };
    if width > 0 && body.chars().count() < width as usize {
        let fill = " ".repeat(width as usize - body.chars().count());
        if flags & FLAG_MINUS != 0 {
            body.push_str(&fill);
        } else {
            body = format!("{fill}{body}");
        }
    }
    body
}

/// A subset of `vis(3)`: `VIS_CSTYLE | VIS_OCTAL | VIS_WHITE` (`stat.c:1000-1006`
/// uses it when `#` meets a string form). Printable ASCII comes out verbatim,
/// the seven control characters with names plus `NUL` take the backslash form,
/// and everything else (spaces and all non-ASCII bytes) takes three-digit octal.
fn vis(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        match ch {
            '\0' => out.push_str("\\0"),
            '\u{7}' => out.push_str("\\a"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{b}' => out.push_str("\\v"),
            '\\' => out.push_str("\\\\"),
            c if c.is_ascii_graphic() => out.push(c),
            // VIS_WHITE counts a space as whitespace to escape; a non-ASCII
            // byte is not printable in C's locale either, so both take
            // three-digit octal.
            c => {
                for byte in c.encode_utf8(&mut [0u8; 4]).as_bytes() {
                    if *byte == b' ' || !byte.is_ascii_graphic() {
                        out.push_str(&format!("\\{:03o}", *byte));
                    } else {
                        out.push(*byte as char);
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The field-code letter (the inverse of `Datum::from_char`, for assembling
    /// source text in tests).
    fn datum_letter(datum: Datum) -> char {
        match datum {
            Datum::Realpath => 'R',
            Datum::Dev => 'd',
            Datum::Ino => 'i',
            Datum::Mode => 'p',
            Datum::Nlink => 'l',
            Datum::Uid => 'u',
            Datum::Gid => 'g',
            Datum::Rdev => 'r',
            Datum::Atime => 'a',
            Datum::Mtime => 'm',
            Datum::Ctime => 'c',
            Datum::Btime => 'B',
            Datum::Size => 'z',
            Datum::Blocks => 'b',
            Datum::Blksize => 'k',
            Datum::Flags => 'f',
            Datum::Gen => 'v',
            Datum::Symlink => 'Y',
            Datum::Filetype => 'T',
            Datum::Filename => 'N',
            Datum::SizeRdev => 'Z',
        }
    }

    /// A bare field spec — no parameter bits, no width, no precision — i.e. C's
    /// "every datum has a sensible default output form".
    fn bare(datum: Datum) -> Piece<'static> {
        let source = Box::leak(format!("%{}", datum_letter(datum)).into_boxed_str());
        Piece::Spec(Spec {
            flags: 0,
            size: -1,
            prec: -1,
            ofmt: None,
            hilo: None,
            datum,
            source,
        })
    }

    #[test]
    fn plain_bytes_pass_through() {
        let pieces = parse_format("ab").unwrap();
        assert_eq!(pieces, vec![Piece::Literal(b'a'), Piece::Literal(b'b')]);
    }

    #[test]
    fn the_four_simple_forms_are_one_character_each() {
        let pieces = parse_format("%n%t%%%@").unwrap();
        assert_eq!(
            pieces,
            vec![
                Piece::Simple(Simple::Newline),
                Piece::Simple(Simple::Tab),
                Piece::Simple(Simple::Percent),
                Piece::Simple(Simple::FileNumber),
            ]
        );
    }

    #[test]
    fn a_bare_datum_needs_no_flags_or_width() {
        let pieces = parse_format("%i").unwrap();
        let Piece::Spec(s) = &pieces[0] else {
            panic!("%i 应是字段件");
        };
        assert_eq!(s.datum, Datum::Ino);
        assert_eq!((s.flags, s.size, s.prec), (0, -1, -1));
        assert!(s.ofmt.is_none() && s.hilo.is_none());
        assert_eq!(s.source, "%i");
    }

    #[test]
    fn flags_are_or_ed_in_bit_order_from_the_c_macros() {
        let pieces = parse_format("%-# 0+z").unwrap();
        let Piece::Spec(s) = &pieces[0] else {
            panic!("-z 应是字段件");
        };
        assert_eq!(
            s.flags,
            FLAG_MINUS | FLAG_POUND | FLAG_SPACE | FLAG_ZERO | FLAG_PLUS
        );
        assert_eq!(s.datum, Datum::Size);
    }

    #[test]
    fn width_then_precision_then_form_then_subfield() {
        // `%04OLp`: width 4, zero padding, octal, low slice, `st_mode` (the
        // original from LINUX_FORMAT).
        let pieces = parse_format("%04OLp").unwrap();
        let Piece::Spec(s) = &pieces[0] else {
            panic!("应是字段件");
        };
        assert_eq!((s.flags, s.size, s.prec), (FLAG_ZERO, 4, -1));
        assert_eq!(s.ofmt, Some(OutFmt::Octal));
        assert_eq!(s.hilo, Some(Hilo::Low));
        assert_eq!(s.datum, Datum::Mode);
        assert_eq!(s.source, "%04OLp");
    }

    #[test]
    fn a_dot_without_digits_still_sets_precision_zero() {
        let pieces = parse_format("%.10Sp").unwrap();
        let Piece::Spec(s) = &pieces[0] else {
            panic!("应是字段件");
        };
        assert_eq!((s.size, s.prec), (-1, 10));
        assert_eq!(s.ofmt, Some(OutFmt::String));
        let pieces = parse_format("%.p").unwrap();
        let Piece::Spec(s) = &pieces[0] else {
            panic!("应是字段件");
        };
        assert_eq!(s.prec, 0);
    }

    #[test]
    fn every_c_datum_letter_parses() {
        const LETTERS: [(u8, Datum); 21] = [
            (b'R', Datum::Realpath),
            (b'd', Datum::Dev),
            (b'i', Datum::Ino),
            (b'p', Datum::Mode),
            (b'l', Datum::Nlink),
            (b'u', Datum::Uid),
            (b'g', Datum::Gid),
            (b'r', Datum::Rdev),
            (b'a', Datum::Atime),
            (b'm', Datum::Mtime),
            (b'c', Datum::Ctime),
            (b'B', Datum::Btime),
            (b'z', Datum::Size),
            (b'b', Datum::Blocks),
            (b'k', Datum::Blksize),
            (b'f', Datum::Flags),
            (b'v', Datum::Gen),
            (b'Y', Datum::Symlink),
            (b'T', Datum::Filetype),
            (b'N', Datum::Filename),
            (b'Z', Datum::SizeRdev),
        ];
        for (letter, want) in LETTERS {
            let fmt = format!("%{}", letter as char);
            let pieces = parse_format(&fmt).unwrap_or_else(|_| panic!("{fmt} 应可解析"));
            assert_eq!(pieces.len(), 1, "{fmt} 只该有一件");
            assert_eq!(pieces[0], bare(want), "{fmt} 的解析形不匹配");
        }
    }

    #[test]
    fn unknown_letters_are_bad_format_and_the_span_includes_the_offender() {
        let err = parse_format("%Q").unwrap_err();
        assert_eq!(err.span, "%Q");
        // Parameter bits and width are consumed but the field code is wrong: the
        // span runs through the offending position.
        let err = parse_format("%-12Q").unwrap_err();
        assert_eq!(err.span, "%-12Q");
    }

    #[test]
    fn a_trailing_percent_is_bad_format_without_eating_a_real_char() {
        let err = parse_format("%").unwrap_err();
        assert_eq!(err.span, "%");
        // `%c` itself is a valid field code (`st_ctime`), so a bad spelling at
        // the end of the string needs an unrecognized letter.
        let err = parse_format("ab%Q").unwrap_err();
        assert_eq!(err.span, "%Q");
    }

    #[test]
    fn a_hilo_needs_a_datum_after_it() {
        let err = parse_format("%H").unwrap_err();
        assert_eq!(err.span, "%H");
    }

    #[test]
    fn oversized_width_falls_into_the_same_bad_format() {
        // C's criterion is "the multiply-add wraps negative under signed
        // arithmetic" (`stat.c:469-475`), so its width ceiling actually depends
        // on the width of int and the wrap value; this implementation checks with
        // checked arithmetic, and any width beyond the representable range falls
        // into bad format on the spot. The span stops at the digit that blew the
        // computation up (C advances the cursor one character before reporting,
        // so its echo may carry one extra following character — that narrow
        // difference does not affect which spellings are rejected).
        let err = parse_format("%9999999999z").unwrap_err();
        assert_eq!(err.span, "%9999999999");
        // The same holds for the precision digits (`stat.c:479-488`).
        let err = parse_format("%.9999999999z").unwrap_err();
        assert_eq!(err.span, "%.9999999999");
    }

    #[test]
    fn mixed_literal_and_specs_keep_their_order() {
        let pieces = parse_format("Size: %-11z%n").unwrap();
        // Six literal pieces for "Size: ", one field spec, and one newline
        // shorthand.
        assert_eq!(pieces.len(), 8);
        assert_eq!(pieces[0], Piece::Literal(b'S'));
        assert!(matches!(pieces[6], Piece::Spec(_)));
        assert!(matches!(pieces[7], Piece::Simple(Simple::Newline)));
    }

    #[test]
    fn the_default_plan_is_def_format_and_default_timefmt() {
        let plan = parse_args(&["/etc/rc"], "stat").unwrap();
        assert_eq!(plan.personality, Personality::Stat);
        assert_eq!(plan.format, DEF_FORMAT);
        assert_eq!(plan.timefmt, TIME_FORMAT);
        assert_eq!(plan.operands, 0);
        assert!(!plan.ls_f && !plan.use_stat && !plan.no_nl && !plan.quiet);
    }

    #[test]
    fn each_template_switch_picks_its_predefined_format() {
        assert_eq!(parse_args(&["-r", "f"], "stat").unwrap().format, RAW_FORMAT);
        assert_eq!(
            parse_args(&["-s", "f"], "stat").unwrap().format,
            SHELL_FORMAT
        );
        assert_eq!(
            parse_args(&["-x", "f"], "stat").unwrap().format,
            LINUX_FORMAT
        );
        assert_eq!(parse_args(&["-l", "f"], "stat").unwrap().format, LS_FORMAT);
        // -F alone does not change the template choice: C folds it into
        // fmtchar='l' (stat.c:281-286).
        assert_eq!(parse_args(&["-F", "f"], "stat").unwrap().format, LSF_FORMAT);
    }

    #[test]
    fn shell_and_linux_forms_carry_their_own_time_default() {
        assert_eq!(parse_args(&["-s"], "stat").unwrap().timefmt, "%s");
        assert_eq!(parse_args(&["-x"], "stat").unwrap().timefmt, "%c");
        // A given -t wins, including alongside -s/-x (stat.c:261-263).
        assert_eq!(
            parse_args(&["-s", "-t", "%Y"], "stat").unwrap().timefmt,
            "%Y"
        );
    }

    #[test]
    fn two_template_switches_conflict() {
        assert_eq!(
            parse_args(&["-r", "-s"], "stat").unwrap_err(),
            PlanError::FormatConflict {
                first: 'r',
                second: 's'
            }
        );
        // Writing the same letter twice collides too: C's criterion only checks
        // whether fmtchar is already taken.
        assert!(matches!(
            parse_args(&["-x", "-x"], "stat").unwrap_err(),
            PlanError::FormatConflict { .. }
        ));
    }

    #[test]
    fn the_ls_f_flag_only_pairs_with_the_l_flag() {
        assert_eq!(
            parse_args(&["-F", "-r"], "stat").unwrap_err(),
            PlanError::NotWithDashF { fmtchar: 'r' }
        );
        assert_eq!(
            parse_args(&["-F", "-x"], "stat").unwrap_err(),
            PlanError::NotWithDashF { fmtchar: 'x' }
        );
    }

    #[test]
    fn dash_f_shares_the_template_slot_so_the_second_switch_conflicts() {
        let plan = parse_args(&["-f", "%N", "f"], "stat").unwrap();
        assert_eq!(plan.format, "%N");
        // The FALLTHROUGH at `stat.c:245-258` makes `-f` itself claim
        // `fmtchar='f'`, so a second template switch (including a second `-f`)
        // collides.
        assert_eq!(
            parse_args(&["-f", "%N", "-r"], "stat").unwrap_err(),
            PlanError::FormatConflict {
                first: 'f',
                second: 'r'
            }
        );
        assert_eq!(
            parse_args(&["-f", "%N", "-f", "%Z"], "stat").unwrap_err(),
            PlanError::FormatConflict {
                first: 'f',
                second: 'f'
            }
        );
        // A missing value goes to usage (getopt answers `?` in C).
        assert_eq!(parse_args(&["-f"], "stat"), Err(PlanError::Usage));
        // The value form attached to the cluster (getopt's `-fVALUE`) is
        // recognized too.
        assert_eq!(parse_args(&["-f%N"], "stat").unwrap().format, "%N");
        // The rest of the same cluster all belongs to `-f` and is not scanned as
        // options again.
        let plan = parse_args(&["-nqf%N", "x"], "stat").unwrap();
        assert!(plan.no_nl && plan.quiet);
        assert_eq!(plan.format, "%N");
        assert_eq!(plan.operands, 1);
    }

    #[test]
    fn clustered_switches_and_operand_boundary() {
        let plan = parse_args(&["-Lnq", "a", "b"], "stat").unwrap();
        assert!(plan.use_stat && plan.no_nl && plan.quiet);
        assert_eq!(plan.operands, 1);
        // A lone `-` is not an option but a stdin operand (getopt's behavior).
        let plan = parse_args(&["-", "f"], "stat").unwrap();
        assert_eq!(plan.operands, 0);
    }

    #[test]
    fn unknown_switch_is_usage() {
        assert_eq!(parse_args(&["-Z"], "stat"), Err(PlanError::Usage));
    }

    #[test]
    fn the_readlink_personality_flips_three_things_at_once() {
        let plan = parse_args(&["/etc"], "readlink").unwrap();
        assert_eq!(plan.personality, Personality::Readlink);
        assert_eq!(plan.format, "%Y");
        assert!(plan.quiet);
        // readlink's -f takes no value (`f` has no colon in its option string)
        // and means %R; -v clears quiet.
        assert_eq!(parse_args(&["-f"], "readlink").unwrap().format, "%R");
        assert!(!parse_args(&["-v"], "readlink").unwrap().quiet);
        // `-s` in readlink only sets quiet again (`stat.c:246-249`), and
        // `-l`/`-r`/`-x`/`-F`/`-L`/`-t` are not in its option string.
        assert!(parse_args(&["-s"], "readlink").unwrap().quiet);
        assert_eq!(parse_args(&["-l"], "readlink"), Err(PlanError::Usage));
        assert_eq!(parse_args(&["-F"], "readlink"), Err(PlanError::Usage));
        assert_eq!(parse_args(&["-t", "%Y"], "readlink"), Err(PlanError::Usage));
        // stat's `-v` is likewise absent from its option string.
        assert_eq!(parse_args(&["-v"], "stat"), Err(PlanError::Usage));
    }
}

#[cfg(test)]
mod face_tests {
    use super::*;

    fn sample() -> StatData {
        StatData {
            dev: 2,
            mode: 0o100644,
            ino: 42,
            nlink: 1,
            uid: 0,
            gid: 0,
            rdev: 0,
            atime: TimeField::new(1_000_000_000, 123_456_789),
            mtime: TimeField::new(1_000_000_001, 123_456_789),
            ctime: TimeField::new(1_000_000_002, 0),
            btime: TimeField::new(1_000_000_003, 999_000_000),
            size: 4096,
            blocks: 8,
            blksize: 4096,
            flags: 0x21,
            generation: 7,
        }
    }

    fn face(
        fmt: &str,
        data: &StatData,
        name: Option<&str>,
        link: Option<&str>,
        timefmt: &str,
    ) -> String {
        let pieces = parse_format(fmt).unwrap_or_else(|e| panic!("{fmt} 应可解析: {}", e.span));
        let ctx = Render {
            data,
            name,
            link,
            resolved: None,
            timefmt,
            file_number: 3,
        };
        let mut out = String::new();
        render(&pieces, &ctx, &mut out).unwrap_or_else(|e| panic!("{fmt} 应可渲染: {e:?}"));
        out
    }

    #[test]
    fn the_default_template_lays_out_every_field_in_order() {
        let mut d = sample();
        d.generation = 7;
        assert_eq!(
            face(DEF_FORMAT, &d, Some("/etc/rc"), None, TIME_FORMAT),
            "2 42 -rw-r--r-- 1 (0) (0) 0 4096 \"Sep  9 01:46:40 2001\" \
             \"Sep  9 01:46:41 2001\" \"Sep  9 01:46:42 2001\" \"Sep  9 01:46:43 2001\" \
             4096 8 0x21 /etc/rc"
        );
    }

    #[test]
    fn the_raw_template_leaves_the_quotes_off_and_uses_numbers() {
        assert_eq!(
            face(RAW_FORMAT, &sample(), Some("/etc/rc"), None, TIME_FORMAT),
            "2 42 0100644 1 0 0 0 4096 1000000000 1000000001 1000000002 1000000003 4096 8 33 /etc/rc"
        );
    }

    #[test]
    fn the_ls_like_templates_mark_type_and_follow_the_link() {
        let mut d = sample();
        d.mode = crate::cp::S_IFLNK | 0o777;
        // Only the explicit string form `%SY` carries ` -> ` (the pointer at
        // stat.c:855-856 is computed before the default form).
        assert_eq!(
            face(
                LS_FORMAT,
                &d,
                Some("/tmp/link"),
                Some("/etc/rc"),
                TIME_FORMAT
            ),
            "lrwxrwxrwx 1 (0) (0) 4096 Sep  9 01:46:41 2001 /tmp/link -> /etc/rc"
        );
        assert_eq!(
            face(
                LSF_FORMAT,
                &d,
                Some("/tmp/link"),
                Some("/etc/rc"),
                TIME_FORMAT
            ),
            "lrwxrwxrwx 1 (0) (0) 4096 Sep  9 01:46:41 2001 /tmp/link@ -> /etc/rc"
        );
    }

    #[test]
    fn the_shell_and_linux_templates_print_assignments_and_blocks() {
        assert!(
            face(SHELL_FORMAT, &sample(), Some("f"), None, "%s").starts_with(
                "st_dev=2 st_ino=42 st_mode=0100644 st_nlink=1 st_uid=0 st_gid=0 st_rdev=0 \
             st_size=4096 st_atime=1000000000 st_mtime=1000000001 st_ctime=1000000002 \
             st_birthtime=1000000003 st_blksize=4096 st_blocks=8 st_flags=33"
            )
        );
        let linux = face(LINUX_FORMAT, &sample(), Some("f"), None, "%c");
        assert!(
            linux.starts_with("  File: \"f\"\n  Size: 4096"),
            "实际：{linux:?}"
        );
        assert!(linux.contains("FileType: Regular File"), "实际：{linux:?}");
        assert!(linux.contains("Mode: (0644/-rw-r--r--)"), "实际：{linux:?}");
        assert!(linux.contains("Device: 0,2   Inode: 42"), "实际：{linux:?}");
    }

    #[test]
    fn mode_string_form_drops_the_padding_and_the_octal_default_keeps_the_type_bits() {
        assert_eq!(
            face("%Sp", &sample(), None, None, TIME_FORMAT),
            "-rw-r--r--"
        );
        assert_eq!(face("%p", &sample(), None, None, TIME_FORMAT), "100644");
        assert_eq!(face("%#p", &sample(), None, None, TIME_FORMAT), "0100644");
        assert_eq!(face("%Dp", &sample(), None, None, TIME_FORMAT), "33188");
    }

    #[test]
    fn the_three_subfield_letters_slice_the_permission_groups() {
        let mut d = sample();
        d.mode = 0o100754;
        // Only the string form slices the permission groups (`stmp += …;
        // stmp[3] = 0` at `stat.c:650-666`).
        assert_eq!(face("%SHp", &d, None, None, TIME_FORMAT), "rwx");
        assert_eq!(face("%SMp", &d, None, None, TIME_FORMAT), "r-x");
        assert_eq!(face("%SLp", &d, None, None, TIME_FORMAT), "r--");
        // A bare slice is still numeric: high folds to the type bits
        // (`stat.c:652`), low takes those nine bits (`:664`), and middle takes
        // `(data >> 9) & 07` (`:658`).
        assert_eq!(face("%Hp", &d, None, None, TIME_FORMAT), "10");
        assert_eq!(face("%Lp", &d, None, None, TIME_FORMAT), "754");
        // The middle slice is an inconsistency in C itself: the numeric arm folds
        // the sticky/setgid/setuid triple, while the string arm slices the group
        // permission (`stmp += 4`, `:659`). With all special bits clear in
        // 0o100754, the numeric form prints 0 while the string form prints
        // `r-x`. Copied rather than "fixed" — external behavior wins.
        assert_eq!(face("%Mp", &d, None, None, TIME_FORMAT), "0");
        assert_eq!(face("%SMp", &d, None, None, TIME_FORMAT), "r-x");
    }

    #[test]
    fn hex_with_pound_and_zero_padding_put_the_zeros_after_the_prefix() {
        // The width covers the whole field (including `0x`), so zero padding is
        // two digits rather than four.
        assert_eq!(
            face("%#08Xz", &sample(), None, None, TIME_FORMAT),
            "0x001000"
        );
        assert_eq!(face("%8Xz", &sample(), None, None, TIME_FORMAT), "    1000");
        assert_eq!(
            face("%-8z|", &sample(), None, None, TIME_FORMAT),
            "4096    |"
        );
    }

    #[test]
    fn sign_flags_only_ride_the_decimal_form() {
        // The default is the unsigned form, so the `+` and space flags have no
        // effect on it (the same as C's `%+llu`).
        assert_eq!(face("%+z", &sample(), None, None, TIME_FORMAT), "4096");
        assert_eq!(face("%+Dz", &sample(), None, None, TIME_FORMAT), "+4096");
        assert_eq!(face("% Dz", &sample(), None, None, TIME_FORMAT), " 4096");
        assert_eq!(face("%+Xz", &sample(), None, None, TIME_FORMAT), "1000");
        let mut neg = sample();
        neg.size = -1;
        assert_eq!(face("%Dz", &neg, None, None, TIME_FORMAT), "-1");
        // The default is the unsigned form, so a negative size prints the wrapped
        // huge number (the same as C's `%llu`).
        assert_eq!(
            face("%z", &neg, None, None, TIME_FORMAT),
            "18446744073709551615"
        );
    }

    #[test]
    fn precision_pads_the_digits_while_width_pads_the_field() {
        assert_eq!(face("%.8i", &sample(), None, None, TIME_FORMAT), "00000042");
        assert_eq!(face("%6i", &sample(), None, None, TIME_FORMAT), "    42");
        assert_eq!(
            face("%6.8i", &sample(), None, None, TIME_FORMAT),
            "00000042"
        );
        // Precision on the string form truncates.
        assert_eq!(
            face("%.4SN", &sample(), Some("/etc/rc"), None, TIME_FORMAT),
            "/etc"
        );
    }

    #[test]
    fn size_subfields_divide_by_powers_of_two_and_round() {
        assert_eq!(face("%Lz", &sample(), None, None, TIME_FORMAT), "4");
        assert_eq!(face("%Mz", &sample(), None, None, TIME_FORMAT), "0");
        assert_eq!(face("%Hz", &sample(), None, None, TIME_FORMAT), "0");
        let mut big = sample();
        big.size = 4 * 1024 * 1024;
        assert_eq!(face("%Mz", &big, None, None, TIME_FORMAT), "4");
        assert_eq!(face("%Hz", &big, None, None, TIME_FORMAT), "0");
    }

    #[test]
    fn times_default_to_seconds_and_the_string_form_uses_the_time_template() {
        assert_eq!(face("%m", &sample(), None, None, TIME_FORMAT), "1000000001");
        assert_eq!(
            face("%Sm", &sample(), None, None, TIME_FORMAT),
            "Sep  9 01:46:41 2001"
        );
        // With the time template switched to `%s`, the string form degrades to
        // whole seconds.
        assert_eq!(face("%Sm", &sample(), None, None, "%s"), "1000000001");
    }

    #[test]
    fn the_float_form_lays_the_nanoseconds_behind_the_decimal_point() {
        assert_eq!(
            face("%Fm", &sample(), None, None, TIME_FORMAT),
            "1000000001.123456789"
        );
        assert_eq!(
            face("%.2Fm", &sample(), None, None, TIME_FORMAT),
            "1000000001.12"
        );
        assert_eq!(
            face("%.0Fm", &sample(), None, None, TIME_FORMAT),
            "1000000001"
        );
        // When more than nine digits are requested, the excess is zero padding
        // (stat.c:1088-1093).
        assert_eq!(
            face("%.12Fc", &sample(), None, None, TIME_FORMAT),
            "1000000002.000000000000"
        );
        assert_eq!(
            face("%.6FB", &sample(), None, None, TIME_FORMAT),
            "1000000003.999000"
        );
    }

    #[test]
    fn the_link_arrow_appears_only_for_the_explicit_string_form() {
        let mut d = sample();
        d.mode = crate::cp::S_IFLNK | 0o777;
        assert_eq!(
            face("%Y", &d, Some("l"), Some("/tmp/x"), TIME_FORMAT),
            "/tmp/x"
        );
        assert_eq!(
            face("%SY", &d, Some("l"), Some("/tmp/x"), TIME_FORMAT),
            " -> /tmp/x"
        );
        // On a non-symlink `%Y` prints the empty string and sets linkfail
        // (stat.c:858-861).
        let pieces = parse_format("%Y").unwrap();
        let ctx = Render {
            data: &sample(),
            name: Some("f"),
            link: None,
            resolved: None,
            timefmt: TIME_FORMAT,
            file_number: 1,
        };
        let mut out = String::new();
        let report = render(&pieces, &ctx, &mut out).unwrap();
        assert_eq!(out, "");
        assert!(report.linkfail);
    }

    #[test]
    fn the_readlink_personality_reports_a_missing_target_through_the_report() {
        let mut d = sample();
        d.mode = crate::cp::S_IFLNK | 0o777;
        let pieces = parse_format("%Y").unwrap();
        let ctx = Render {
            data: &d,
            name: Some("l"),
            link: None,
            resolved: None,
            timefmt: TIME_FORMAT,
            file_number: 1,
        };
        let mut out = String::new();
        assert!(render(&pieces, &ctx, &mut out).unwrap().linkfail);
    }

    #[test]
    fn filename_subfields_split_the_directory_and_the_basename() {
        assert_eq!(
            face("%N", &sample(), Some("/etc/rc"), None, TIME_FORMAT),
            "/etc/rc"
        );
        assert_eq!(
            face("%HN", &sample(), Some("/etc/rc"), None, TIME_FORMAT),
            "/etc"
        );
        assert_eq!(
            face("%LN", &sample(), Some("/etc/rc"), None, TIME_FORMAT),
            "rc"
        );
        // Trailing slashes are stripped before slicing (stat.c:929-937).
        assert_eq!(
            face("%HN", &sample(), Some("/etc/rc//"), None, TIME_FORMAT),
            "/etc"
        );
        // For a path without a slash, the high slice prints `.`
        // (stat.c:939-941).
        assert_eq!(face("%HN", &sample(), Some("rc"), None, TIME_FORMAT), ".");
        assert_eq!(face("%N", &sample(), None, None, TIME_FORMAT), "(stdin)");
    }

    #[test]
    fn filetype_names_itself_in_the_high_form_and_marks_it_in_the_low_form() {
        assert_eq!(
            face("%HT", &sample(), None, None, TIME_FORMAT),
            "Regular File"
        );
        assert_eq!(face("%LT", &sample(), None, None, TIME_FORMAT), "-");
        let mut exec = sample();
        exec.mode = 0o100755;
        assert_eq!(face("%T", &exec, None, None, TIME_FORMAT), "*");
        let mut dir = sample();
        dir.mode = crate::cp::S_IFDIR | 0o755;
        assert_eq!(face("%HT", &dir, None, None, TIME_FORMAT), "Directory");
        assert_eq!(face("%LT", &dir, None, None, TIME_FORMAT), "/");
    }

    #[test]
    fn device_datums_split_major_and_minor_and_sizerdev_folds_to_the_pair() {
        let mut dev = sample();
        dev.mode = crate::cp::S_IFCHR | 0o666;
        // `makedev(3, 1)` encodes to 0x301 under the encoding at :1264 in the
        // same file.
        dev.rdev = 0x301;
        assert_eq!(face("%Hr", &dev, None, None, TIME_FORMAT), "3");
        assert_eq!(face("%Lr", &dev, None, None, TIME_FORMAT), "1");
        assert_eq!(face("%Z", &dev, None, None, TIME_FORMAT), "3,1");
        // Non-devices take the size arm (stat.c:979-988).
        assert_eq!(face("%Z", &sample(), None, None, TIME_FORMAT), "4096");
    }

    #[test]
    fn the_devname_branch_prints_the_marks_c_prints_when_it_finds_nothing() {
        assert_eq!(face("%Sd", &sample(), None, None, TIME_FORMAT), "???");
    }

    #[test]
    fn ids_print_parenthesised_without_a_password_database_behind_them() {
        assert_eq!(face("%Su", &sample(), None, None, TIME_FORMAT), "(0)");
        assert_eq!(face("%u", &sample(), None, None, TIME_FORMAT), "0");
        let mut d = sample();
        d.gid = 1000;
        assert_eq!(face("%Sg", &d, None, None, TIME_FORMAT), "(1000)");
        assert_eq!(face("%g", &d, None, None, TIME_FORMAT), "1000");
    }

    #[test]
    fn the_pound_flag_on_a_string_vis_encodes_control_and_space() {
        assert_eq!(
            face("%#SN", &sample(), Some("a\tb c"), None, TIME_FORMAT),
            "a\\tb\\040c"
        );
        // The backslash itself is escaped too, and non-ASCII bytes take
        // three-digit octal.
        assert_eq!(
            face("%#SN", &sample(), Some("a\\b"), None, TIME_FORMAT),
            "a\\\\b"
        );
        assert_eq!(
            face("%#SN", &sample(), Some("é"), None, TIME_FORMAT),
            "\\303\\251"
        );
    }

    #[test]
    fn unsupported_subfield_or_form_is_the_same_bad_format_c_reports() {
        let pieces = parse_format("%Mf").unwrap();
        let ctx = Render {
            data: &sample(),
            name: Some("f"),
            link: None,
            resolved: None,
            timefmt: TIME_FORMAT,
            file_number: 1,
        };
        let mut out = String::new();
        assert_eq!(
            render(&pieces, &ctx, &mut out).unwrap_err(),
            FaceError::BadFormat("%Mf".to_string())
        );
        // The inode number has no string form (FMTF_STRING is absent from the
        // allowed set at stat.c:639-641).
        let pieces = parse_format("%Si").unwrap();
        assert_eq!(
            render(&pieces, &ctx, &mut String::new()).unwrap_err(),
            FaceError::BadFormat("%Si".to_string())
        );
    }

    #[test]
    fn the_realpath_letter_renders_what_the_shell_resolved() {
        // `%R` shares `%Y`'s prefix rule: the bare form prints no ` -> `, only
        // the explicit string form does (`stat.c:822-838`). On resolution failure
        // it prints nothing and sets `linkfail`; with no operand it prints
        // `(stdin)`.
        let pieces = parse_format("%R|%SR").unwrap();
        let d = sample();
        let mut out = String::new();
        render(
            &pieces,
            &Render {
                data: &d,
                name: Some("/etc/rc"),
                link: None,
                resolved: Some("/etc/rc"),
                timefmt: TIME_FORMAT,
                file_number: 1,
            },
            &mut out,
        )
        .unwrap();
        assert_eq!(out, "/etc/rc| -> /etc/rc");

        let mut out = String::new();
        let report = render(
            &parse_format("%R").unwrap(),
            &Render {
                data: &d,
                name: Some("/etc/rc"),
                link: None,
                resolved: None,
                timefmt: TIME_FORMAT,
                file_number: 1,
            },
            &mut out,
        )
        .unwrap();
        assert_eq!(out, "");
        assert!(report.linkfail);

        let mut out = String::new();
        render(
            &parse_format("%R").unwrap(),
            &Render {
                data: &d,
                name: None,
                link: None,
                resolved: Some("/whatever"),
                timefmt: TIME_FORMAT,
                file_number: 1,
            },
            &mut out,
        )
        .unwrap();
        assert_eq!(out, "(stdin)");
    }

    #[test]
    fn literals_and_simple_forms_reach_the_output_in_order() {
        let pieces = parse_format("%@ of %@: %z%n").unwrap();
        let ctx = Render {
            data: &sample(),
            name: None,
            link: None,
            resolved: None,
            timefmt: TIME_FORMAT,
            file_number: 2,
        };
        let mut out = String::new();
        let report = render(&pieces, &ctx, &mut out).unwrap();
        assert_eq!(out, "2 of 2: 4096\n");
        assert!(report.ends_with_newline);
        assert!(!report.linkfail);
    }
}
