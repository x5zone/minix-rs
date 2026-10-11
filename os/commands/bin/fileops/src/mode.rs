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
    /// `-v`: echo each applied spec. Not a C flag — the option string at
    /// `chmod.c:87` has no `v`; kept from this tree's first wiring and
    /// registered as a deviation.
    pub verbose: bool,
}

/// Why an option could not be accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChmodOption {
    /// A letter whose facility this tree has not landed; refused, not ignored.
    NotWired(char),
    /// Not in the option string at `chmod.c:87` and not a bare mode letter.
    Unknown(char),
}

/// Is `arg` a bare `-[perm]` *mode* rather than a flag? C's `case 'g' … 'x'`
/// arms step `optind` back over such an argument (`chmod.c:127-133`) so that
/// `chmod -r file` drops the read bit instead of being an option. `X` is in
/// C's list at `chmod.c:128`; this tree's parser has not taken it, and its
/// mode grammar folds `X` onto plain `x` rather than testing "is a directory
/// or already executable" — both registered as open findings instead of being
/// fixed beside the recursion.
pub fn is_bare_mode(arg: &str) -> bool {
    let b = arg.as_bytes();
    b.len() == 2
        && b[0] == b'-'
        && matches!(b[1], b'r' | b'w' | b'x' | b's' | b't' | b'u' | b'g' | b'o')
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
                    b'h' | b'H' | b'L' | b'P' => {
                        return Err(ChmodOption::NotWired(byte as char));
                    }
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

/// Apply `spec` (octal or symbolic) to `base` permission bits.
///
/// An all digit spec is octal and replaces the twelve low bits outright
/// (higher bits of `base`, such as the file type, are preserved). Anything
/// else is a comma separated list of symbolic clauses applied left to
/// right. Empty clauses (from `u+x,,go-w`) are rejected, matching the C
/// library refusing the whole spec.
pub fn apply_mode(base: u16, spec: &str) -> Result<u16, FileOpError> {
    if spec.is_empty() {
        return Err(FileOpError::InvalidArgument);
    }
    if spec.bytes().all(|b| b.is_ascii_digit()) {
        return apply_octal(base, spec);
    }
    let mut mode = base;
    for clause in spec.split(',') {
        mode = apply_clause(mode, clause)?;
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
/// addressed classes (`X` means execute only when the target already
/// executes somewhere or is a directory — directory knowledge lives with
/// the executor, so `X` behaves as `x` here and the difference is
/// documented).
fn what_bits(what: u8, who: u16, base: u16) -> Result<u16, FileOpError> {
    let bits;
    match what {
        b'r' => bits = 0o444,
        b'w' => bits = 0o222,
        b'x' | b'X' => bits = 0o111,
        b's' => bits = SET_USER_ID | SET_GROUP_ID,
        b't' => bits = STICKY,
        b'u' => bits = ((base >> 6) & 0o7) * 0o111,
        b'g' => bits = ((base >> 3) & 0o7) * 0o111,
        b'o' => bits = (base & 0o7) * 0o111,
        _ => return Err(FileOpError::InvalidArgument),
    }
    Ok(bits & who)
}

fn apply_clause(mut mode: u16, clause: &str) -> Result<u16, FileOpError> {
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
        let mut any = false;
        while pos < bytes.len() && !matches!(bytes[pos], b'+' | b'-' | b'=') {
            bits |= what_bits(bytes[pos], who, mode)?;
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
    }
    Ok(mode)
}

// ── strmode：把 mode 位回渲成十一个字符 ────────────────────────────
//
// 真源：`minix3/lib/libc/string/strmode.c`（NetBSD 1.19），声明在
// `minix3/include/unistd.h:403`。它写十一个字符再加终止符：一个类型字母、
// 九个权限字符（三组 execute 槽位兼作设身份位记号）、末尾一个填充位（启用
// 访问控制列表时会变成 `+`）。`minix3/bin/mv/mv.c:187-191` 用 `modep + 1`（跳过
// 类型字母取九个权限字符）并以 `modep[9] == ' '` 决定要不要补一个空格。

// 类型位优先复用本 crate 已在位的 `crate::cp` 常量块（单一真源）；下面五个
// 类型位与九个权限位尚未在那块里出现，就近补上。
const S_IFCHR: u32 = 0o020000; // `stat.h:140`
const S_IFBLK: u32 = 0o060000; // `stat.h:142`
const S_IFIFO: u32 = 0o010000; // `stat.h:139`
const S_IFSOCK: u32 = 0o140000; // `stat.h:146`
const S_IFWHT: u32 = 0o160000; // `stat.h:147`
// 归档态两位：它们在 `S_IFMT` 之外，所以 `strmode.c:71-80` 在 `S_IFREG` 支里
// 另判一轮（Minix 确实定义了 `stat.h:148-149`，所以那段 `#ifdef S_ARCH2` 是编得进来的）。
const S_ARCH1: u32 = 0o200000; // `stat.h:148`，`ls -l` 显示 `a`
const S_ARCH2: u32 = 0o400000; // `stat.h:149`，`ls -l` 显示 `A`

// 九个读/写/执行位（`stat.h` 的 `_S_IRUSR` 一类），按 strmode 的打印顺序。
const IRUSR: u32 = 0o00400;
const IWUSR: u32 = 0o00200;
const IXUSR: u32 = 0o00100;
const IRGRP: u32 = 0o00040;
const IWGRP: u32 = 0o00020;
const IXGRP: u32 = 0o00010;
const IROTH: u32 = 0o00004;
const IWOTH: u32 = 0o00002;
const IXOTH: u32 = 0o00001;

/// C 的 `strmode` 写出的十一个字符（类型、九个权限位、末尾填充位）。
/// 末尾的 NUL 不在数组里：本层只交回渲染结果，拼不拼字符串由调用方定。
pub type ModeString = [u8; 11];

/// `strmode(2)` 的回渲：按 C 逐位写十一个字符。
///
/// 三组 execute 槽位兼作设身份位记号：只能执行、无设身份→`x`；有设身份、不可
/// 执行→`S`/`T`（大写）；两者都有→`s`/`t`（小写）；都无→`-`。粘滞位占用其他组
/// 的 execute 槽（`strmode.c:111-178`）。
pub fn strmode(mode: u32) -> ModeString {
    let mut out = [b'-'; 11];
    out[0] = match mode & crate::cp::S_IFMT {
        crate::cp::S_IFDIR => b'd',
        S_IFCHR => b'c',
        S_IFBLK => b'b',
        crate::cp::S_IFREG => {
            // C 在常规文件支里再查归档态两位（`strmode.c:71-80`）。
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
        // C 的 default 支：模式里没有认得的类型位就回 `?`（strmode.c:106-108）。
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
    // C 在此位写一个空格，启用访问控制列表时才换成 `+`（strmode.c:179-180）。
    // `mv.c:188` 的 `modep[9] == ' '` 判的就是这里是不是空格：本布局下索引 9
    // 总是权限字符，所以那个条件永假——它只防住“把填充位误当权限末位”的变体。
    out[10] = b' ';
    out
}

/// 读/写槽位：位在对字母，位无则 `-`。
fn read_or_write(mode: u32, mask: u32, letter: u8) -> u8 {
    if mode & mask != 0 { letter } else { b'-' }
}

/// execute 兼设身份位槽位：对应 C 那个四分支 switch。
/// `upper`/`lower` 是大写与小写的设身份记号（用户与组用 `S`/`s`，其商用 `T`/`t`）。
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
    fn chmod_link_flags_are_refused() {
        for c in ['h', 'H', 'L', 'P'] {
            let arg = alloc::format!("-{c}");
            assert_eq!(
                ChmodFlags::parse(&[arg.as_str(), "755", "f"]),
                Err(ChmodOption::NotWired(c)),
                "-{c} must be refused explicitly"
            );
        }
    }

    #[test]
    fn bare_permission_letter_is_a_mode_not_a_flag() {
        // chmod.c:127-133: getopt has moved past `-r`, so C steps optind back
        // and the argument becomes the mode.
        let (f, rest) = ChmodFlags::parse(&["-r", "notes.txt"]).unwrap();
        assert_eq!(f, ChmodFlags::default());
        assert_eq!(rest, ["-r", "notes.txt"]);
        assert!(is_bare_mode("-r"));
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

    // ── strmode 回渲 ──────────────────────────────────────────────

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
        // 归档态两位只在常规文件支里查，ARCH2 优先（`strmode.c:71-80`）。
        assert_eq!(strmode(crate::cp::S_IFREG | S_ARCH1 | 0o644)[0], b'a');
        assert_eq!(strmode(crate::cp::S_IFREG | S_ARCH2 | 0o644)[0], b'A');
        assert_eq!(
            strmode(crate::cp::S_IFDIR | S_ARCH1 | 0o755)[0],
            b'd',
            "归档位不影响其它类型的字母"
        );
        // 没有可识别的类型位时走 C 的 default 支（strmode.c:106-108）。
        assert_eq!(strmode(0o644)[0], b'?');
    }

    #[test]
    fn strmode_permission_characters() {
        assert_eq!(&strmode(crate::cp::S_IFREG | 0o777)[1..10], b"rwxrwxrwx");
        assert_eq!(&strmode(crate::cp::S_IFREG | 0o644)[1..10], b"rw-r--r--");
        assert_eq!(&strmode(crate::cp::S_IFREG)[1..10], b"---------");
        // 每段只留一位：IRUSR + IWGRP + IXOTH。
        assert_eq!(&strmode(crate::cp::S_IFREG | 0o421)[1..10], b"r---w---x");
    }

    #[test]
    fn strmode_set_id_slots_replace_the_execute_letter() {
        // 设身份位与执行位的四种组合（C 那个四分支 switch）。
        assert_eq!(&strmode(crate::cp::S_IFREG | 0o4111)[1..10], b"--s--x--x");
        assert_eq!(&strmode(crate::cp::S_IFREG | 0o4000)[1..10], b"--S------");
        assert_eq!(&strmode(crate::cp::S_IFREG | 0o2000)[1..10], b"-----S---");
        assert_eq!(&strmode(crate::cp::S_IFREG | 0o1000)[1..10], b"--------T");
        // 三种同时到位的小写形（粘滞位用 `t`，另两个用 `s`）：0o7777 才是设
        // 身份两位加粘滞位的全填形；0o1777 只带粘滞位，所以 owner/group 仍是 `x`。
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
        // `mv.c:188` 的 `modep[9] == ' '` 在本布局下永假：索引 9 是权限字符，
        // 不是填充位。这一条钉住那个分隔空格为何总被输出。
        assert_ne!(s[9], b' ');
        assert_ne!(strmode(crate::cp::S_IFREG | 0o4755)[9], b' ');
    }
}
