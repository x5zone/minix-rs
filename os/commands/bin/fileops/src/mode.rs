//! Permission bit arithmetic behind `chmod`.
//!
//! Ground truth: `minix3/bin/chmod/chmod.c` (`setmode` at line 166 turns
//! the mode text into an edit script, `getmode` at line 216 applies it to
//! the file's current bits). Two spellings:
//!
//! - Octal (`755`): the twelve permission bits written out directly.
//! - Symbolic (`u+x,go-w`, `a=rw`, `+x`): who (`u` user, `g` group, `o`
//!   others, `a` all), how (`+` add, `-` remove, `=` set exactly), what
//!   (`rwxXst` plus `u`/`g`/`o` copying one class from another).
//!
//! Special bits (set user identifier, set group identifier, sticky) live
//! above the nine `rwx` bits and are addressed by the same grammar.

use crate::FileOpError;
use alloc::vec::Vec;

/// User, group, and others read/write/execute bits plus the three special
/// bits, in classic octal positions.
pub const SET_USER_ID: u16 = 0o4000;
/// Set group identifier on execution.
pub const SET_GROUP_ID: u16 = 0o2000;
/// Sticky bit (restricted deletion).
pub const STICKY: u16 = 0o1000;

// ── options and walk decisions ───────────────────────────────────────────

/// The flags `chmod` acts on (`chmod.c:87-136`): the two that need no link
/// decision, plus the recursion. `-h` changes the link itself through
/// `lchmod` (`chmod.c:110-119` with `:159-161`), and `-H`/`-L`/`-P` pick the
/// traversal mode; all four hinge on facilities this tree does not wrap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChmodFlags {
    /// `-f`: a failed change sets the exit status but prints nothing
    /// (`chmod.c:107-108` with the `&& !fflag` test at `:215-219`).
    pub force: bool,
    /// `-R`: walk each operand directory (`chmod.c:104-105`, `:144-157`).
    pub recursive: bool,
    /// `-H`: follow the command-line operands only (`FTS_COMFOLLOW`,
    /// `chmod.c:151-152`). `-P` clears it (`:100-102`).
    pub comfollow: bool,
    /// `-L`: follow every symlink (`FTS_LOGICAL`, `chmod.c:153-156`).
    pub logical: bool,
    /// `-h`: act on the symlink itself. Minix's `lchmod` emulation makes a
    /// symlink a silent success (`tools/compat/lchmod.c`), and `-R -h` is
    /// rejected by name (`chmod.c:145-150`).
    pub hflag: bool,
    /// `-v`: echo each applied spec. Not a C flag — the option string at
    /// `chmod.c:87` has no `v`; kept from this tree's first wiring and
    /// registered as a deviation.
    pub verbose: bool,
}

/// Why an option could not be accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChmodOption {
    /// Not in the option string at `chmod.c:87` and not a bare mode letter.
    /// Every letter of C's option string is wired, so this is the only
    /// failure.
    Unknown(char),
}

/// Is `arg` a bare `-[perm]` *mode* rather than a flag? C's `case 'g' … 'X' … 'x'`
/// arms step `optind` back over such an argument (`chmod.c:127-133`) so that
/// `chmod -r file` drops the read bit and `chmod -X file` applies the conditional
/// execute bit instead of being an option.
pub fn is_bare_mode(arg: &str) -> bool {
    let b = arg.as_bytes();
    b.len() == 2
        && b[0] == b'-'
        && matches!(
            b[1],
            b'r' | b'w' | b'x' | b'X' | b's' | b't' | b'u' | b'g' | b'o'
        )
}

impl ChmodFlags {
    /// Scan the leading options the way BSD `getopt` does: clusters are
    /// allowed (`-Rf`), a bare `-[perm]` or a non-option ends the scan, and
    /// `--` ends it too. `args` is the list without `argv[0]`.
    pub fn parse<'a>(args: &[&'a str]) -> Result<(ChmodFlags, Vec<&'a str>), ChmodOption> {
        let mut f = ChmodFlags::default();
        let mut i = 0;
        while i < args.len() {
            let a = args[i];
            if a == "--" {
                i += 1;
                break;
            }
            if a.len() < 2 || !a.starts_with('-') || is_bare_mode(a) {
                break;
            }
            for &byte in a.as_bytes()[1..].iter() {
                match byte {
                    b'f' => f.force = true,
                    b'v' => f.verbose = true,
                    b'R' => f.recursive = true,
                    // `-P` (physical) clears the follow bits (`chmod.c:100-102`).
                    b'P' => {
                        f.comfollow = false;
                        f.logical = false;
                    }
                    b'H' => f.comfollow = true,
                    b'L' => f.logical = true,
                    b'h' => f.hflag = true,
                    _ => return Err(ChmodOption::Unknown(byte as char)),
                }
            }
            i += 1;
        }
        Ok((f, args[i..].to_vec()))
    }
}

/// `chmod.c:144-157` with `:186-210`: the recursive walk is physical, and the
/// only flag that could change a link (`-h`) may not be combined with `-R`
/// (`chmod.c:145-150`), so the links a physical walk meets are passed over —
/// `FTS_SL`/`FTS_SLNONE` reach `continue` (`chmod.c:201-210`) and never the
/// `change_mode` call. A non-recursive scan follows its single operand
/// (`FTS_COMFOLLOW`, `chmod.c:157-158`), so there the link's target is what
/// gets changed.
pub fn chmods_entry(mode: u32, physical_walk: bool) -> bool {
    !(physical_walk && crate::cp::is_link(mode))
}

/// Apply `spec` (octal or symbolic) to a file's current mode.
///
/// `current` is the file's own `st_mode`, type bits included: the `X` permission
/// letter tests them (`setmode.c:148-150` adds the execute bits only when the
/// original mode is a directory or already executes somewhere), and the octal
/// spelling preserves the type nibble the same way (`base & 0o170000`).
///
/// An all digit spec is octal and replaces the twelve low bits outright.
/// Anything else is a comma separated list of symbolic clauses applied left to
/// right. Empty clauses (from `u+x,,go-w`) are rejected, matching the C
/// library refusing the whole spec.
pub fn apply_mode(current: u32, spec: &str) -> Result<u16, FileOpError> {
    if spec.is_empty() {
        return Err(FileOpError::InvalidArgument);
    }
    // The low 16 bits carry everything this grammar addresses: the twelve
    // permission bits plus the four type bits (`S_IFDIR` is `0o040000`); the
    // two archive bits above them are not settable by `chmod`.
    let base = (current & 0o177777) as u16;
    // `getmode` tests the *original* mode for every `X` command, not the mode
    // as it is being built up (`setmode.c:93` takes `omode`, `:148-150` reads
    // it), so the answer is computed once here and carried through the clauses.
    let x_adds = current & (crate::cp::S_IFDIR | 0o111) != 0;
    if spec.bytes().all(|b| b.is_ascii_digit()) {
        return apply_octal(base, spec);
    }
    let mut mode = base;
    for clause in spec.split(',') {
        mode = apply_clause(mode, clause, x_adds)?;
    }
    Ok(mode)
}

fn apply_octal(base: u16, spec: &str) -> Result<u16, FileOpError> {
    // Digits 8 and 9 are not octal; more than four digits overflows the
    // twelve permission bits.
    if spec.len() > 4 || spec.bytes().any(|b| !(b'0'..=b'7').contains(&b)) {
        return Err(FileOpError::InvalidArgument);
    }
    let mut value: u16 = 0;
    for byte in spec.bytes() {
        value = value * 8 + (byte - b'0') as u16;
    }
    Ok((base & 0xF000) | (value & 0x0FFF))
}

/// Who mask: which of the three classes a clause addresses.
fn who_mask(who: &str) -> Result<u16, FileOpError> {
    if who.is_empty() {
        // No who means "all, honoring the creation mask" — the creation
        // mask lives with the executor, so the library reports all bits
        // and documents the handoff.
        return Ok(0o7777);
    }
    let mut mask = 0;
    for byte in who.bytes() {
        match byte {
            b'u' => mask |= 0o4700,
            b'g' => mask |= 0o2070,
            b'o' => mask |= 0o1007,
            b'a' => mask |= 0o7777,
            _ => return Err(FileOpError::InvalidArgument),
        }
    }
    Ok(mask)
}

/// Permission bits named by one `what` character, resolved against the
/// addressed classes. `X` is not handled here: `setmode.c:296-297` counts it into
/// a separate accumulator (`permXbits`) and `getmode`'s `case 'X'` only ever adds
/// those bits, and only when the original mode is a directory or already
/// executes — see `apply_clause`.
fn what_bits(what: u8, who: u16, base: u16) -> Result<u16, FileOpError> {
    let bits = match what {
        b'r' => 0o444,
        b'w' => 0o222,
        b'x' => 0o111,
        b's' => SET_USER_ID | SET_GROUP_ID,
        b't' => STICKY,
        b'u' => ((base >> 6) & 0o7) * 0o111,
        b'g' => ((base >> 3) & 0o7) * 0o111,
        b'o' => (base & 0o7) * 0o111,
        _ => return Err(FileOpError::InvalidArgument),
    };
    Ok(bits & who)
}

fn apply_clause(mut mode: u16, clause: &str, x_adds: bool) -> Result<u16, FileOpError> {
    let bytes = clause.as_bytes();
    let mut pos = 0;
    while pos < bytes.len() && matches!(bytes[pos], b'u' | b'g' | b'o' | b'a') {
        pos += 1;
    }
    let who = who_mask(&clause[..pos])?;
    if pos >= bytes.len() {
        return Err(FileOpError::InvalidArgument);
    }
    // One clause may chain several `how what` actions (`g+w-x`),
    // applied strictly left to right; each `=` clears the addressed
    // classes first (matching setmode chaining semantics).
    while pos < bytes.len() {
        let how = bytes[pos];
        if !matches!(how, b'+' | b'-' | b'=') {
            return Err(FileOpError::InvalidArgument);
        }
        pos += 1;
        let mut bits = 0;
        let mut xbits = 0;
        let mut any = false;
        while pos < bytes.len() && !matches!(bytes[pos], b'+' | b'-' | b'=') {
            // `setmode.c:296-297` counts `X` into its own accumulator, flushed
            // after the plain bits (`setmode.c:330-336`), so the order within a
            // clause is always "the plain bits first, then `X`". One narrow
            // difference: when a `-` or `=` clause reaches a copy letter
            // (`u`/`g`/`o`) C drops the accumulated `X` bits, flushing them only
            // for `+` (`setmode.c:316-318`); this grammar always flushes at the
            // clause end, so `chmod -Xu f` differs from C for that spelling.
            if bytes[pos] == b'X' {
                xbits |= 0o111 & who;
            } else {
                bits |= what_bits(bytes[pos], who, mode)?;
            }
            any = true;
            pos += 1;
        }
        // `=` with an empty what clears the addressed classes (`u=`).
        if !any && how != b'=' {
            return Err(FileOpError::InvalidArgument);
        }
        match how {
            b'+' => mode |= bits,
            b'-' => mode &= !bits,
            b'=' => {
                mode &= !who;
                mode |= bits;
            }
            _ => return Err(FileOpError::InvalidArgument),
        }
        // `getmode`'s `case 'X'` (`setmode.c:148-150`) is an unconditional OR
        // gated on the original mode — it never clears, not even in a `-`
        // clause, which is why `chmod -X` on a directory adds the bits.
        if x_adds {
            mode |= xbits;
        }
    }
    Ok(mode)
}

// ── strmode: render the mode bits back into eleven characters ─────
//
// Ground truth: `minix3/lib/libc/string/strmode.c` (NetBSD 1.19),
// declared in `minix3/include/unistd.h:403`. It writes eleven characters
// plus a terminator: one type letter, nine permission characters (the
// three execute slots double as set-id markers), and one trailing padding
// slot (which becomes `+` when an access control list is enabled).
// `minix3/bin/mv/mv.c:187-191` uses `modep + 1` (skipping the type letter
// to take the nine permission characters) and uses `modep[9] == ' '` to
// decide whether to append a space.

// Type bits reuse the `crate::cp` constant block already present in this
// crate where possible (single source); the five type bits and the nine
// permission bits below are not yet in that block, so they are filled in
// here.
const S_IFCHR: u32 = 0o020000; // `stat.h:140`
const S_IFBLK: u32 = 0o060000; // `stat.h:142`
const S_IFIFO: u32 = 0o010000; // `stat.h:139`
const S_IFSOCK: u32 = 0o140000; // `stat.h:146`
pub(crate) const S_IFWHT: u32 = 0o160000; // `stat.h:147`
// The two archive-state bits: they lie outside `S_IFMT`, so
// `strmode.c:71-80` tests them separately inside the `S_IFREG` branch
// (Minix does define `stat.h:148-149`, so that `#ifdef S_ARCH2` block does
// compile in).
const S_ARCH1: u32 = 0o200000; // `stat.h:148`, `ls -l` shows `a`
const S_ARCH2: u32 = 0o400000; // `stat.h:149`, `ls -l` shows `A`

// The nine read/write/execute bits (the `_S_IRUSR` family in `stat.h`), in
// strmode's printing order.
const IRUSR: u32 = 0o00400;
const IWUSR: u32 = 0o00200;
const IXUSR: u32 = 0o00100;
const IRGRP: u32 = 0o00040;
const IWGRP: u32 = 0o00020;
const IXGRP: u32 = 0o00010;
const IROTH: u32 = 0o00004;
const IWOTH: u32 = 0o00002;
const IXOTH: u32 = 0o00001;

/// The eleven characters C's `strmode` writes (type, nine permission
/// bits, trailing padding slot). The trailing NUL is not part of the
/// array: this layer only hands back the rendering, and the caller decides
/// whether to assemble a string.
pub type ModeString = [u8; 11];

/// Render as `strmode(2)` does: write the eleven characters bit by bit,
/// as C does.
///
/// The three execute slots double as set-id markers: executable only, no
/// set-id → `x`; set-id, not executable → `S`/`T` (upper case); both →
/// `s`/`t` (lower case); neither → `-`. The sticky bit takes over the
/// other group's execute slot (`strmode.c:111-178`).
pub fn strmode(mode: u32) -> ModeString {
    let mut out = [b'-'; 11];
    out[0] = match mode & crate::cp::S_IFMT {
        crate::cp::S_IFDIR => b'd',
        S_IFCHR => b'c',
        S_IFBLK => b'b',
        crate::cp::S_IFREG => {
            // C tests the two archive-state bits again inside the
            // regular-file branch (`strmode.c:71-80`).
            if mode & S_ARCH2 != 0 {
                b'A'
            } else if mode & S_ARCH1 != 0 {
                b'a'
            } else {
                b'-'
            }
        }
        crate::cp::S_IFLNK => b'l',
        S_IFSOCK => b's',
        S_IFIFO => b'p',
        S_IFWHT => b'w',
        // C's default branch: with no recognized type bit in the mode,
        // answer `?` (strmode.c:106-108).
        _ => b'?',
    };
    out[1] = read_or_write(mode, IRUSR, b'r');
    out[2] = read_or_write(mode, IWUSR, b'w');
    out[3] = execute_slot(mode, IXUSR, crate::cp::S_ISUID, b'S', b's');
    out[4] = read_or_write(mode, IRGRP, b'r');
    out[5] = read_or_write(mode, IWGRP, b'w');
    out[6] = execute_slot(mode, IXGRP, crate::cp::S_ISGID, b'S', b's');
    out[7] = read_or_write(mode, IROTH, b'r');
    out[8] = read_or_write(mode, IWOTH, b'w');
    out[9] = execute_slot(mode, IXOTH, crate::cp::S_ISVTX, b'T', b't');
    // C writes a space at this position and switches to `+` only when an
    // access control list is enabled (strmode.c:179-180). The
    // `modep[9] == ' '` test at `mv.c:188` asks whether this position is
    // a space: in this layout index 9 is always a permission character, so
    // that condition is always false — it only guards the variant that
    // mistakes the padding slot for the last permission position.
    out[10] = b' ';
    out
}

/// Read/write slot: the letter when the bit is set, `-` otherwise.
fn read_or_write(mode: u32, mask: u32, letter: u8) -> u8 {
    if mode & mask != 0 { letter } else { b'-' }
}

/// execute-with-set-id slot: corresponds to C's four-way switch.
/// `upper`/`lower` are the upper-case and lower-case set-id markers
/// (`S`/`s` for user and group, `T`/`t` for the other group).
fn execute_slot(mode: u32, exec: u32, set_id: u32, upper: u8, lower: u8) -> u8 {
    match (mode & exec != 0, mode & set_id != 0) {
        (false, false) => b'-',
        (true, false) => b'x',
        (false, true) => upper,
        (true, true) => lower,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── options and walk decisions ──────────────────────────────────────

    #[test]
    fn chmod_flags_default_to_all_off() {
        let (f, rest) = ChmodFlags::parse(&["644", "file"]).unwrap();
        assert_eq!(f, ChmodFlags::default());
        assert_eq!(rest, ["644", "file"]);
    }

    #[test]
    fn chmod_clusters_and_double_dash() {
        let (f, _) = ChmodFlags::parse(&["-Rf", "755", "d"]).unwrap();
        assert!(f.recursive && f.force);
        let (_, rest) = ChmodFlags::parse(&["--", "-R"]).unwrap();
        assert_eq!(rest, ["-R"], "-R after -- is an operand");
        assert_eq!(
            ChmodFlags::parse(&["-z", "755", "f"]),
            Err(ChmodOption::Unknown('z'))
        );
    }

    #[test]
    fn chmod_link_and_traversal_flags_are_wired() {
        let (f, rest) = ChmodFlags::parse(&["-h", "755", "f"]).unwrap();
        assert!(f.hflag);
        assert_eq!(rest, ["755", "f"]);
        let (f, _) = ChmodFlags::parse(&["-RH", "755", "d"]).unwrap();
        assert!(f.recursive && f.comfollow && !f.logical);
        let (f, _) = ChmodFlags::parse(&["-RL", "755", "d"]).unwrap();
        assert!(f.logical);
        // `-P` clears both follow bits (`chmod.c:100-102`).
        let (f, _) = ChmodFlags::parse(&["-HLP", "755", "d"]).unwrap();
        assert!(!f.comfollow && !f.logical);
    }

    #[test]
    fn bare_permission_letter_is_a_mode_not_a_flag() {
        // chmod.c:127-133: getopt has moved past `-r`, so C steps optind back
        // and the argument becomes the mode.
        let (f, rest) = ChmodFlags::parse(&["-r", "notes.txt"]).unwrap();
        assert_eq!(f, ChmodFlags::default());
        assert_eq!(rest, ["-r", "notes.txt"]);
        assert!(is_bare_mode("-r"));
        assert!(is_bare_mode("-X"), "C lists `X` too (chmod.c:128)");
        assert!(!is_bare_mode("-R"), "the recursion flag is upper case");
        assert!(!is_bare_mode("-rr"), "only a single letter counts");
        assert!(!is_bare_mode("r"), "and it needs the dash");
    }

    #[test]
    fn a_physical_walk_passes_over_links() {
        // chmod.c:201-210: FTS_SL and FTS_SLNONE reach `continue`, and the flag
        // that would change them (`-h`) cannot be combined with -R
        // (chmod.c:145-150).
        let link = crate::cp::S_IFLNK | 0o777;
        let reg = crate::cp::S_IFREG | 0o644;
        let dir = crate::cp::S_IFDIR | 0o755;
        assert!(!chmods_entry(link, true), "-R skips the link itself");
        assert!(
            chmods_entry(link, false),
            "the non-recursive scan follows it"
        );
        assert!(chmods_entry(reg, true));
        assert!(chmods_entry(dir, true));
    }

    #[test]
    fn x_letter_only_adds_when_the_original_is_a_directory_or_already_executes() {
        // setmode.c:148-150: `getmode` tests the *original* mode's directory bit
        // or any execute bit; neither holds, nothing is added.
        assert_eq!(apply_mode(0o100644, "a+X").unwrap(), 0o100644);
        // A directory gets all three execute bits.
        assert_eq!(apply_mode(0o040644, "a+X").unwrap(), 0o040755);
        // A file that already executes somewhere gets the remaining two.
        assert_eq!(apply_mode(0o100744, "a+X").unwrap(), 0o100755);
        // The addressed classes still bound it: `g+X` on a plain file is a no-op,
        // on a directory it is the group execute bit alone.
        assert_eq!(apply_mode(0o100604, "g+X").unwrap(), 0o100604);
        assert_eq!(apply_mode(0o040604, "g+X").unwrap(), 0o040614);
    }

    #[test]
    fn x_never_clears_even_in_a_minus_clause() {
        // setmode.c:330-336 flushes `permXbits` as its own `'X'` command, and
        // getmode's `'X'` arm can only OR: `-X` is a conditional add, never a
        // remove.
        assert_eq!(apply_mode(0o100755, "a-X").unwrap(), 0o100755);
        assert_eq!(apply_mode(0o100644, "a-X").unwrap(), 0o100644);
        // `-xX` on an executable file clears the bits and then adds them back,
        // because the gate reads the original mode, not the running one.
        assert_eq!(apply_mode(0o100744, "a-xX").unwrap(), 0o100755);
    }

    #[test]
    fn equals_then_x_keeps_the_conditional_add() {
        // `a=X` is a clear-everything command followed by the `'X'` command, and
        // the file type nibble survives both legs (the kernel masks it away).
        assert_eq!(apply_mode(0o040644, "a=X").unwrap(), 0o040111);
        // An already-executable regular file loses its read bits to the `=` clear
        // and only gets the execute bits back — `addcmd` turns `=` into
        // clear-then-set, and the set half carries nothing (`setmode.c:373-384`).
        assert_eq!(apply_mode(0o100755, "a=X").unwrap(), 0o100111);
        assert_eq!(apply_mode(0o100644, "a=X").unwrap(), 0o100000);
    }

    #[test]
    fn test_octal_replaces_low_bits() {
        assert_eq!(apply_mode(0o644, "755").unwrap(), 0o755);
        // File type bits survive.
        assert_eq!(apply_mode(0o100644, "755").unwrap(), 0o100755);
    }

    #[test]
    fn test_bad_octal_rejected() {
        assert_eq!(apply_mode(0, "888"), Err(FileOpError::InvalidArgument));
        assert_eq!(apply_mode(0, "12345"), Err(FileOpError::InvalidArgument));
        assert_eq!(apply_mode(0, ""), Err(FileOpError::InvalidArgument));
    }

    #[test]
    fn test_add_and_remove() {
        assert_eq!(apply_mode(0o644, "u+x").unwrap(), 0o744);
        assert_eq!(apply_mode(0o755, "go-w").unwrap(), 0o755);
        assert_eq!(apply_mode(0o777, "go-w").unwrap(), 0o755);
    }

    #[test]
    fn test_set_exactly() {
        assert_eq!(apply_mode(0o777, "u=rw").unwrap(), 0o677);
        assert_eq!(apply_mode(0o777, "a=rx").unwrap(), 0o555);
        assert_eq!(apply_mode(0o777, "o=").unwrap(), 0o770);
    }

    #[test]
    fn test_special_bits() {
        assert_eq!(apply_mode(0o755, "u+s").unwrap(), 0o4755);
        assert_eq!(apply_mode(0o777, "+t").unwrap(), 0o1777);
    }

    #[test]
    fn test_copy_between_classes() {
        // `g=u` copies the user triplet onto the group triplet.
        assert_eq!(apply_mode(0o700, "g=u").unwrap(), 0o770);
    }

    #[test]
    fn test_chained_clauses() {
        assert_eq!(apply_mode(0o000, "u+rw,go+r").unwrap(), 0o644);
    }

    #[test]
    fn test_bad_clause_rejected() {
        assert_eq!(apply_mode(0o644, "z+x"), Err(FileOpError::InvalidArgument));
        assert_eq!(apply_mode(0o644, "u"), Err(FileOpError::InvalidArgument));
        assert_eq!(apply_mode(0o644, "u+"), Err(FileOpError::InvalidArgument));
    }

    // ── strmode rendering ─────────────────────────────────────────

    #[test]
    fn strmode_type_letters() {
        assert_eq!(strmode(crate::cp::S_IFDIR | 0o755)[0], b'd');
        assert_eq!(strmode(S_IFCHR | 0o644)[0], b'c');
        assert_eq!(strmode(S_IFBLK | 0o644)[0], b'b');
        assert_eq!(strmode(crate::cp::S_IFREG | 0o644)[0], b'-');
        assert_eq!(strmode(crate::cp::S_IFLNK | 0o777)[0], b'l');
        assert_eq!(strmode(S_IFSOCK | 0o644)[0], b's');
        assert_eq!(strmode(S_IFIFO | 0o644)[0], b'p');
        assert_eq!(strmode(S_IFWHT | 0o644)[0], b'w');
        // The two archive-state bits are tested only in the regular-file
        // branch, with ARCH2 taking precedence (`strmode.c:71-80`).
        assert_eq!(strmode(crate::cp::S_IFREG | S_ARCH1 | 0o644)[0], b'a');
        assert_eq!(strmode(crate::cp::S_IFREG | S_ARCH2 | 0o644)[0], b'A');
        assert_eq!(
            strmode(crate::cp::S_IFDIR | S_ARCH1 | 0o755)[0],
            b'd',
            "归档位不影响其它类型的字母"
        );
        // With no recognizable type bit, C's default branch runs
        // (strmode.c:106-108).
        assert_eq!(strmode(0o644)[0], b'?');
    }

    #[test]
    fn strmode_permission_characters() {
        assert_eq!(&strmode(crate::cp::S_IFREG | 0o777)[1..10], b"rwxrwxrwx");
        assert_eq!(&strmode(crate::cp::S_IFREG | 0o644)[1..10], b"rw-r--r--");
        assert_eq!(&strmode(crate::cp::S_IFREG)[1..10], b"---------");
        // One bit per triplet: IRUSR + IWGRP + IXOTH.
        assert_eq!(&strmode(crate::cp::S_IFREG | 0o421)[1..10], b"r---w---x");
    }

    #[test]
    fn strmode_set_id_slots_replace_the_execute_letter() {
        // The four combinations of set-id and execute bits (C's four-way
        // switch).
        assert_eq!(&strmode(crate::cp::S_IFREG | 0o4111)[1..10], b"--s--x--x");
        assert_eq!(&strmode(crate::cp::S_IFREG | 0o4000)[1..10], b"--S------");
        assert_eq!(&strmode(crate::cp::S_IFREG | 0o2000)[1..10], b"-----S---");
        assert_eq!(&strmode(crate::cp::S_IFREG | 0o1000)[1..10], b"--------T");
        // The lower-case form when all three bits are present (the sticky
        // bit gives `t`, the other two give `s`): only 0o7777 fills both
        // set-id bits plus the sticky bit; 0o1777 carries the sticky bit
        // alone, so owner/group stay `x`.
        assert_eq!(&strmode(crate::cp::S_IFDIR | 0o7777)[1..10], b"rwsrwsrwt");
        assert_eq!(&strmode(crate::cp::S_IFDIR | 0o1777)[1..10], b"rwxrwxrwt");
    }

    #[test]
    fn strmode_pads_with_a_space_after_the_nine_permissions() {
        let s = strmode(crate::cp::S_IFREG | 0o644);
        assert_eq!(s.len(), 11);
        assert_eq!(
            s[10], b' ',
            "the access-control-list pad strmode.c:179 writes"
        );
        // The `modep[9] == ' '` test at `mv.c:188` is always false in this
        // layout: index 9 is a permission character, not the padding slot.
        // This pins down why that separating space is always emitted.
        assert_ne!(s[9], b' ');
        assert_ne!(strmode(crate::cp::S_IFREG | 0o4755)[9], b' ');
    }
}
