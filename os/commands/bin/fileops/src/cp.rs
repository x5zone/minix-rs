//! `cp` — copy files: the pure operand, path, mode and pump layer.
//!
//! Ground truth: `minix3/bin/cp/cp.c` (option scan at line 117, target
//! classification at 237 to 287, path concatenation at 387 to 417, the type
//! dispatch at 443 to 537) and `minix3/bin/cp/utils.c` (`copy_file` at 93 to
//! 296: the create/retain mode arithmetic and the read/write pump; `copy_link`,
//! `copy_fifo`, `copy_special` at 305 to 352). The filesystem legs — `stat`,
//! `open`, the prompt, the copy, the preservation calls — live in the thin
//! program `src/bin/cp.rs`.
//!
//! Wired shape: `cp [-f|-i] [-p] [-v] src target`, `cp [-f|-i] [-p] [-v] src ...
//! directory`, and `cp -R` over either, which duplicates the hierarchy rooted
//! at each source. The walk rides this crate's own directory decoder
//! (`crate::dents`) instead of fts(3), the same substitution `rm -r` made. What
//! is still refused: `-r` (the deprecated logical recursion, `cp.c:175-188`),
//! `-a`/`-P`/`-H`/`-L` (the symlink-traversal modes, and with `-P` the
//! link-preserving `lchown`/`lchmod` this tree does not wrap), `-l` (hard-link
//! instead of copy) and `-N` (dangling links copied as links).

use alloc::string::String;
use alloc::vec::Vec;

// ── ABI constants, taken from the Minix headers, not from Linux ──────────
//
// `minix3/sys/sys/stat.h:111-159`: `S_ISUID 04000`, `S_ISGID 02000`,
// `S_ISVTX 01000`, the type mask and the directory/link bits.
// `minix3/sys/sys/stat.h:138-147`: the type mask plus every type this switch
// has to tell apart (the file systems can hand them to `cp`).
pub const S_IFMT: u32 = 0o170000;
pub const S_IFIFO: u32 = 0o010000;
pub const S_IFCHR: u32 = 0o020000;
pub const S_IFDIR: u32 = 0o040000;
pub const S_IFBLK: u32 = 0o060000;
pub const S_IFREG: u32 = 0o100000;
pub const S_IFLNK: u32 = 0o120000;
pub const S_IFSOCK: u32 = 0o140000;
pub const S_ISUID: u32 = 0o004000;
pub const S_ISGID: u32 = 0o002000;
pub const S_ISVTX: u32 = 0o001000;
pub const S_IRWXU: u32 = 0o000700;
pub const S_IRWXG: u32 = 0o000070;
pub const S_IRWXO: u32 = 0o000007;
/// `utils.c:280-281` `RETAINBITS`: everything a fresh copy may keep, sticky
/// included.
pub const RETAIN_BITS: u32 = S_ISUID | S_ISGID | S_ISVTX | S_IRWXU | S_IRWXG | S_IRWXO;

// `minix3/sys/sys/fcntl.h:64-100`: `O_RDONLY 0`, `O_WRONLY 0x1`,
// `O_CREAT 0x200`, `O_TRUNC 0x400`.
pub const O_RDONLY: i32 = 0x00000000;
pub const O_WRONLY: i32 = 0x00000001;
pub const O_CREAT: i32 = 0x00000200;
pub const O_TRUNC: i32 = 0x00000400;

/// Whether the file type bits say directory (`stat.h:171` `S_ISDIR`).
pub fn is_dir(mode: u32) -> bool {
    mode & S_IFMT == S_IFDIR
}

/// Whether the file type bits say a plain file (`stat.h:174` `S_ISREG`).
pub fn is_reg(mode: u32) -> bool {
    mode & S_IFMT == S_IFREG
}

/// Whether the file type bits say a symbolic link (`stat.h:177` `S_ISLNK`), the
/// test `utils.c:142` makes before writing through a destination name.
pub fn is_link(mode: u32) -> bool {
    mode & S_IFMT == S_IFLNK
}

// ── options ──────────────────────────────────────────────────────────────

/// The option set `cp` acts on (`cp.c:117-167`, the five that need no
/// symlink-traversal decision).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CpFlags {
    /// `-f`: overwrite without asking, and remove an unopenable destination
    /// and retry (`utils.c:150-156`).
    pub force: bool,
    /// `-i`: ask before overwriting — and only when standard input is a
    /// terminal, because `cp.c:148` writes `iflag = isatty(fileno(stdin))`.
    pub interactive: bool,
    /// `-p`: reproduce mode, ownership and timestamps (`utils.c:270-296`).
    pub preserve: bool,
    /// `-R`: duplicate the hierarchy rooted at each source (`cp.c:293-548`).
    pub recursive: bool,
    /// `-v`: print `src -> dst` for every file that did not fail
    /// (`cp.c:539-540`).
    pub verbose: bool,
}

/// Why an option cluster could not be accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpOption {
    /// A letter whose feature this tree has not landed yet; the program says
    /// so and exits 1 instead of silently ignoring it.
    NotWired(char),
    /// Not in the option string at all — usage and exit 1.
    Unknown(char),
}

impl CpFlags {
    /// Parse the option string `"HLNPRfailprv"` (`cp.c:117`). `args` is the
    /// argument list without `argv[0]`. `stdin_is_tty` decides whether `-i`
    /// takes effect, reproducing `cp.c:148`.
    ///
    /// BSD `getopt` stops at the first non-option (there is no permutation),
    /// and `--` ends the scan.
    pub fn parse<'a>(
        args: &[&'a str],
        stdin_is_tty: bool,
    ) -> Result<(CpFlags, Vec<&'a str>), CpOption> {
        let mut f = CpFlags::default();
        let mut i = 0;
        while i < args.len() {
            let a = args[i];
            if a == "--" {
                i += 1;
                break;
            }
            if a.len() < 2 || !a.starts_with('-') {
                break; // first non-option ends option scanning
            }
            for &byte in a.as_bytes()[1..].iter() {
                match byte {
                    b'f' => {
                        f.force = true;
                        f.interactive = false; // cp.c:145 clears iflag
                    }
                    b'i' => {
                        f.interactive = stdin_is_tty;
                        f.force = false; // cp.c:149 clears fflag
                    }
                    b'p' => f.preserve = true,
                    // `-R` is wired: the hierarchy walk rides this crate's own
                    // directory decoder (see the bin's `copy_tree`). The rest of
                    // the family still needs facilities this tree lacks.
                    b'R' => f.recursive = true,
                    b'v' => f.verbose = true,
                    b'H' | b'L' | b'N' | b'P' | b'a' | b'l' | b'r' => {
                        return Err(CpOption::NotWired(byte as char));
                    }
                    _ => return Err(CpOption::Unknown(byte as char)),
                }
            }
            i += 1;
        }
        Ok((f, args[i..].to_vec()))
    }
}

// ── operand shapes ───────────────────────────────────────────────────────

/// What the target name currently resolves to (`cp.c:237-245`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetState {
    /// `stat` failed with `ENOENT`: the destination does not exist yet.
    Missing,
    /// `stat` succeeded; `is_dir` is the file type bit.
    Exists { is_dir: bool },
}

/// The copy case `cp.c:223-287` distinguishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpShape {
    /// Case (1): the target is not a directory, so it becomes the copy.
    FileToFile,
    /// Case (2): the target is a directory, so each source lands inside it.
    FileToDir,
    /// Case (3) (`cp.c:268-269`): `-R`, a directory source, and a target that
    /// does not exist. The target takes the source's contents, so the source's
    /// own name is not appended to it.
    DirToDne,
}

/// Why the operands do not describe a copy this program can attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpShapeError {
    /// `cp.c:249-250`: more than one source with a non-directory target.
    TooManySources,
    /// `cp.c:275-281`: a trailing slash on a target that is (or would be) a
    /// plain file; `target_exists` picks between "directory %s does not
    /// exist" and "%s is not a directory".
    SlashOnNonDir { target_exists: bool },
}

/// Classify the operands exactly as `cp.c:245-287` does: the directory test on
/// the target comes first, the source count is checked inside case (1), the
/// directory-source-without-target case (3) is recognised only for a recursive
/// copy, and the trailing-slash complaint comes last — and only for case (1),
/// so `cp -R dir dst/` with neither `dir` nor `dst` existing is a copy, not a
/// complaint (`cp.c:275` gates on `type == FILE_TO_FILE`).
///
/// `source_is_dir` must come from the same status the scan uses: `lstat` for a
/// recursive copy, `stat` otherwise (`cp.c:259-262`).
pub fn classify(
    source_count: usize,
    target: TargetState,
    source_is_dir: bool,
    recursive: bool,
    have_trailing_slash: bool,
) -> Result<CpShape, CpShapeError> {
    let exists_and_dir = match target {
        TargetState::Missing => false,
        TargetState::Exists { is_dir } => is_dir,
    };
    if exists_and_dir {
        return Ok(CpShape::FileToDir);
    }
    if source_count != 1 {
        return Err(CpShapeError::TooManySources);
    }
    if target == TargetState::Missing && recursive && source_is_dir {
        return Ok(CpShape::DirToDne);
    }
    if have_trailing_slash {
        return Err(CpShapeError::SlashOnNonDir {
            target_exists: matches!(target, TargetState::Exists { .. }),
        });
    }
    Ok(CpShape::FileToFile)
}

/// `cp.c:81-84` (`STRIP_TRAILING_SLASH`) and `cp.c:293-297`: trailing slashes
/// come off, but a lone `/` survives — the root has no other spelling, and
/// both C loops keep at least one character.
pub fn strip_trailing_slashes(path: &str) -> &str {
    let mut s = path;
    while s.len() > 1 && s.ends_with('/') {
        s = &s[..s.len() - 1];
    }
    s
}

/// Whether the raw name had a trailing slash (`cp.c:213`).
pub fn has_trailing_slash(path: &str) -> bool {
    path.ends_with('/')
}

/// `cp.c:387-398`: the component appended to a directory target is the
/// source's last path component, and a source whose last component is `..`
/// contributes `.` instead (that is how `cp -R .. /tmp` lands on `/tmp/.`
/// rather than `/tmp/..`).
pub fn appended_component(source: &str) -> &str {
    let stripped = strip_trailing_slashes(source);
    let last = match stripped.rfind('/') {
        Some(i) => &stripped[i + 1..],
        None => stripped,
    };
    if last == ".." { "." } else { last }
}

/// `cp.c:400-416`: build the destination name. Case (1) keeps the target as
/// written (minus its trailing slash); case (3) also lands on the target,
/// because `cp.c:396-397` sets `base` to the end of the source path so the root
/// contributes an empty component; case (2) joins target and component with
/// exactly one separator, then strips trailing slashes again.
pub fn join_target(target: &str, source: &str, shape: CpShape) -> String {
    match shape {
        CpShape::FileToFile | CpShape::DirToDne => String::from(target),
        CpShape::FileToDir => {
            let component = appended_component(source);
            let mut s = String::from(target);
            if !s.ends_with('/') && !component.starts_with('/') {
                s.push('/');
            }
            s.push_str(component);
            String::from(strip_trailing_slashes(&s))
        }
    }
}

// ── mode arithmetic ──────────────────────────────────────────────────────

/// `utils.c:145-146`: the mode a fresh destination is created with — the
/// source's bits minus setuid and setgid. The kernel still applies the
/// process umask on top, which is why C sets the umask aside only in the
/// retain branch below.
pub fn create_mode(from_mode: u32) -> u32 {
    from_mode & !(S_ISUID | S_ISGID)
}

/// `utils.c:280-288`: when the source was setuid/setgid and belongs to the
/// copying user, the destination regains everything `RETAINBITS` allows
/// except what the user's own umask forbids.
pub fn retained_mode(from_mode: u32, myumask: u32) -> u32 {
    from_mode & RETAIN_BITS & !myumask
}

/// `utils.c:373`: the mask `-p` copies through — note it excludes the sticky
/// bit, unlike `RETAIN_BITS`.
pub fn preserved_mode(from_mode: u32) -> u32 {
    from_mode & (S_ISUID | S_ISGID | S_IRWXU | S_IRWXG | S_IRWXO)
}

/// `utils.c:282-287`: the retain branch only runs for a newly created
/// destination that is not `-p`, whose source carries a setuid or setgid bit
/// owned by the current user.
pub fn regains_set_bits(
    dne: bool,
    preserve: bool,
    from_mode: u32,
    from_uid: u32,
    myuid: u32,
) -> bool {
    !preserve && dne && (from_mode & (S_ISUID | S_ISGID)) != 0 && from_uid == myuid
}

/// `utils.c:109` plus `utils.c:142-143`: whether an existing destination may be
/// opened for writing. `tolnk = (Rflag && !(Lflag || Hflag)) || Pflag` — with
/// the traversal modes refused that is the recursive scan — and such a scan
/// refuses to write through a destination that is itself a symbolic link: the
/// link is removed and a fresh file created in its place.
pub fn writes_through(physical_scan: bool, dest_is_link: bool) -> bool {
    !(physical_scan && dest_is_link)
}

/// `cp.c:502-506`: the mode the post-order `chmod` of a directory that had to be
/// created re-applies. With `-p` the `setfile` just above has already masked the
/// source mode down to the bits it dares apply (sticky excluded), and C hands
/// that masked value to `chmod`; without `-p` the raw source mode goes through
/// and the file system keeps the low bits, sticky included.
pub fn post_create_dir_mode(src_mode: u32, preserve: bool) -> u32 {
    if preserve {
        preserved_mode(src_mode)
    } else {
        src_mode
    }
}

// ── stat comparisons ─────────────────────────────────────────────────────

/// The four `stat` fields `cp` compares (`cp.c:424-434`), kept as a plain
/// value so the decision is testable without a filesystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileIds {
    /// `st_dev`.
    pub dev: u64,
    /// `st_ino`.
    pub ino: u64,
    /// `st_mode`.
    pub mode: u32,
    /// `st_uid`.
    pub uid: u32,
    /// `st_gid`.
    pub gid: u32,
}

/// `cp.c:424-425`: same device and same inode means the two names are one
/// file, and copying it onto itself is refused with a warning.
pub fn is_identical(src: &FileIds, dst: &FileIds) -> bool {
    src.dev == dst.dev && src.ino == dst.ino
}

/// `cp.c:433-434`: a non-directory source may never be written over an
/// existing directory.
pub fn overwrites_dir(src_mode: u32, dst_mode: u32) -> bool {
    !is_dir(src_mode) && is_dir(dst_mode)
}

/// `cp.c:456-462`: without `-R`/`-r` a directory source is refused and the walk
/// skips it; with `-R` the directory is the walk's own business.
pub fn is_unwanted_directory(src_mode: u32, recursive: bool) -> bool {
    !recursive && is_dir(src_mode)
}

// ── the copy pump ────────────────────────────────────────────────────────

/// Which leg of the pump failed: `utils.c:249-251` names the destination on a
/// short or failed write, `utils.c:253-255` names the source on a read error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyError {
    /// `read` of the source failed.
    Read,
    /// `write` of the destination came back short or failed.
    Write,
}

/// The two file legs the pump drives. Sealing them behind a trait keeps the
/// loop testable without a filesystem, the same shape `minix_diskimg::dd`
/// uses for its copy pump.
pub trait CopyIo {
    /// Fill `buf` from the source; `Ok(0)` is end of file.
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, CopyError>;
    /// Push the whole of `buf` to the destination; a short or failed write is
    /// `Err(CopyError::Write)`.
    fn write(&mut self, buf: &[u8]) -> Result<(), CopyError>;
}

/// `utils.c:244-255`: read one block, write that same block, stop at the
/// first mismatch. Returns the number of bytes moved (C's `ptotal`).
///
/// The C mmap shortcut at `utils.c:196-240` is an optimization bounded by
/// `MMAP_MAX_SIZE`; a tree with no `mmap` in the command layer takes the
/// `use_read` leg for every size, which is the observable behavior C falls
/// back to anyway.
pub fn pump(io: &mut dyn CopyIo, scratch: &mut [u8]) -> Result<u64, CopyError> {
    let mut total: u64 = 0;
    loop {
        let n = io.read(scratch)?;
        if n == 0 {
            return Ok(total);
        }
        io.write(&scratch[..n])?;
        total += n as u64;
    }
}

/// `utils.c:183`: C only enters the copy loop when the recorded size is
/// positive — a file whose `st_size` is zero is opened and closed without a
/// single `read`. That is observable for a FIFO or a device file, so the
/// program reproduces it instead of "fixing" it.
pub fn pumps_data(from_size: i64) -> bool {
    from_size > 0
}

/// Which copy routine a source entry needs, from its `st_mode` type bits — the
/// same eight-way switch `cp.c:443-537` runs on `curr->fts_statp->st_mode &
/// S_IFMT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryAction {
    /// `S_IFDIR`: create it, then walk it (`cp.c:455-516`).
    Directory,
    /// `S_IFLNK`: copy the link text, not the target (`cp.c:444-453`).
    Symlink,
    /// `S_IFREG` and the plain-file branch of everything else (`cp.c:534-537`).
    Regular,
    /// `S_IFIFO`: recreate the fifo (`cp.c:526-533` with `Rflag`).
    Fifo,
    /// `S_IFBLK`: recreate the node (`cp.c:517-525` with `Rflag`).
    BlockSpecial,
    /// `S_IFCHR`: as above for a character device.
    CharacterSpecial,
    /// A socket has no branch of its own: `cp.c:534-537` lets it fall into the
    /// `default:` file copy, where the `open` of the socket fails and the copy
    /// is reported as failed. No type bit this switch recognises lands there too.
    Socket,
    /// No `S_IF*` value this switch knows; like `Socket`, the `default:` arm.
    Unknown,
}

/// `cp.c:443`: the dispatch on the source's type bits. The recursive scan is
/// physical (`cp.c:174, 190-196`), so it reaches this through `lstat` and a
/// symlink is copied by `copy_link` instead of being descended into.
pub fn entry_action(mode: u32) -> EntryAction {
    match mode & S_IFMT {
        S_IFDIR => EntryAction::Directory,
        S_IFLNK => EntryAction::Symlink,
        S_IFIFO => EntryAction::Fifo,
        S_IFBLK => EntryAction::BlockSpecial,
        S_IFCHR => EntryAction::CharacterSpecial,
        S_IFSOCK => EntryAction::Socket,
        S_IFREG => EntryAction::Regular,
        _ => EntryAction::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── options ──────────────────────────────────────────────────────────

    #[test]
    fn defaults_are_all_off() {
        let (f, rest) = CpFlags::parse(&["a", "b"], true).unwrap();
        assert_eq!(
            f,
            CpFlags {
                force: false,
                interactive: false,
                preserve: false,
                recursive: false,
                verbose: false
            }
        );
        assert_eq!(rest, ["a", "b"]);
    }

    #[test]
    fn interactive_only_when_stdin_is_a_terminal() {
        // cp.c:148 `iflag = isatty(fileno(stdin))`: piped input disables -i.
        let (f, _) = CpFlags::parse(&["-i", "a", "b"], true).unwrap();
        assert!(f.interactive);
        let (f, _) = CpFlags::parse(&["-i", "a", "b"], false).unwrap();
        assert!(!f.interactive, "no prompt when stdin is not a tty");
    }

    #[test]
    fn f_and_i_clear_each_other_last_wins() {
        let (f, _) = CpFlags::parse(&["-i", "-f", "a", "b"], true).unwrap();
        assert!(f.force && !f.interactive);
        let (f, _) = CpFlags::parse(&["-f", "-i", "a", "b"], true).unwrap();
        assert!(f.interactive && !f.force);
    }

    #[test]
    fn clusters_and_double_dash() {
        let (f, rest) = CpFlags::parse(&["-pv", "x", "y"], true).unwrap();
        assert!(f.preserve && f.verbose);
        assert_eq!(rest, ["x", "y"]);
        let (_, rest) = CpFlags::parse(&["--", "-v"], true).unwrap();
        assert_eq!(rest, ["-v"], "-v after -- is an operand");
    }

    #[test]
    fn unwired_letters_are_reported_not_ignored() {
        for c in ['r', 'a', 'l', 'H', 'L', 'P', 'N'] {
            let arg = alloc::format!("-{c}");
            assert_eq!(
                CpFlags::parse(&[arg.as_str(), "a", "b"], true),
                Err(CpOption::NotWired(c)),
                "-{c} must be refused explicitly"
            );
        }
    }

    #[test]
    fn recursive_letter_is_wired() {
        // `-R` (`cp.c:134-136`) now selects the hierarchy walk; it clears
        // nothing else, so `-Rv` is recursion plus the echo.
        let (f, rest) = CpFlags::parse(&["-R", "src", "dst"], true).unwrap();
        assert!(f.recursive);
        assert!(!f.preserve && !f.force && !f.interactive);
        assert_eq!(rest, ["src", "dst"]);
        let (f, _) = CpFlags::parse(&["-Rv"], true).unwrap();
        assert!(f.recursive && f.verbose);
    }

    #[test]
    fn unknown_letter_is_usage_error() {
        assert_eq!(
            CpFlags::parse(&["-z", "a", "b"], true),
            Err(CpOption::Unknown('z'))
        );
    }

    // ── shapes ───────────────────────────────────────────────────────────

    #[test]
    fn directory_target_takes_every_source_count() {
        let t = TargetState::Exists { is_dir: true };
        assert_eq!(classify(1, t, false, false, false), Ok(CpShape::FileToDir));
        assert_eq!(classify(3, t, false, false, false), Ok(CpShape::FileToDir));
        // cp.c:245: an existing directory target wins over case (3), so `-R` on
        // a directory source still lands inside it.
        assert_eq!(classify(1, t, true, true, false), Ok(CpShape::FileToDir));
    }

    #[test]
    fn non_directory_target_takes_exactly_one_source() {
        assert_eq!(
            classify(
                2,
                TargetState::Exists { is_dir: false },
                false,
                false,
                false
            ),
            Err(CpShapeError::TooManySources)
        );
        assert_eq!(
            classify(2, TargetState::Missing, false, false, false),
            Err(CpShapeError::TooManySources)
        );
        assert_eq!(
            classify(1, TargetState::Missing, false, false, false),
            Ok(CpShape::FileToFile)
        );
    }

    #[test]
    fn source_count_is_checked_before_the_slash() {
        // cp.c:249 usage fires before the cp.c:275 trailing-slash complaint.
        assert_eq!(
            classify(2, TargetState::Exists { is_dir: false }, false, false, true),
            Err(CpShapeError::TooManySources)
        );
    }

    #[test]
    fn recursive_directory_source_into_a_missing_target_is_case_three() {
        // cp.c:268-269: DIR_TO_DNE needs all three — the recursion, a directory
        // source, and a target that stat could not find.
        assert_eq!(
            classify(1, TargetState::Missing, true, true, false),
            Ok(CpShape::DirToDne)
        );
        assert_eq!(
            classify(1, TargetState::Missing, true, false, false),
            Ok(CpShape::FileToFile),
            "without -R a directory source is refused later (cp.c:456)"
        );
        assert_eq!(
            classify(1, TargetState::Missing, false, true, false),
            Ok(CpShape::FileToFile),
            "a plain-file source is copied onto the target name"
        );
        // cp.c:275 only complains for FILE_TO_FILE, so `cp -R dir dst/` with dst
        // missing is a copy, not a usage error.
        assert_eq!(
            classify(1, TargetState::Missing, true, true, true),
            Ok(CpShape::DirToDne)
        );
        assert_eq!(join_target("dst", "src", CpShape::DirToDne), "dst");
    }

    #[test]
    fn trailing_slash_on_a_file_target_is_refused() {
        assert_eq!(
            classify(1, TargetState::Exists { is_dir: false }, false, false, true),
            Err(CpShapeError::SlashOnNonDir {
                target_exists: true
            }),
            "cp.c:280 '%s is not a directory'"
        );
        assert_eq!(
            classify(1, TargetState::Missing, false, false, true),
            Err(CpShapeError::SlashOnNonDir {
                target_exists: false
            }),
            "cp.c:277 'directory %s does not exist'"
        );
    }

    #[test]
    fn trailing_slashes_strip_but_root_survives() {
        assert_eq!(strip_trailing_slashes("a/b//"), "a/b");
        assert_eq!(strip_trailing_slashes("/"), "/");
        assert_eq!(strip_trailing_slashes(""), "");
        assert!(has_trailing_slash("dir/"));
        assert!(!has_trailing_slash("dir"));
    }

    #[test]
    fn dotdot_appends_a_dot() {
        // cp.c:393-395: `if (!strcmp(&curr->fts_path[base], "..")) base += 1;`
        assert_eq!(appended_component(".."), ".");
        assert_eq!(appended_component("a/.."), ".");
        assert_eq!(appended_component("a/../"), ".");
        assert_eq!(appended_component("a/b"), "b");
        assert_eq!(appended_component("b"), "b");
        assert_eq!(appended_component("a/."), ".");
    }

    #[test]
    fn joining_inserts_exactly_one_separator() {
        assert_eq!(
            join_target("dir", "a/b.txt", CpShape::FileToDir),
            "dir/b.txt"
        );
        assert_eq!(join_target("/", "x", CpShape::FileToDir), "/x");
        assert_eq!(join_target("d/e", "..", CpShape::FileToDir), "d/e/.");
        assert_eq!(
            join_target("plain", "src", CpShape::FileToFile),
            "plain",
            "case (1) copies onto the target name itself"
        );
    }

    // ── mode arithmetic ──────────────────────────────────────────────────

    #[test]
    fn create_mode_drops_set_id_bits_only() {
        assert_eq!(create_mode(0o4755), 0o0755);
        assert_eq!(create_mode(0o2755), 0o0755);
        assert_eq!(create_mode(0o1755), 0o1755, "sticky is kept at creation");
    }

    #[test]
    fn retained_mode_keeps_sticky_and_respects_umask() {
        // 0o4777 with umask 0o022 loses group and other write bits.
        assert_eq!(retained_mode(0o4777, 0o022), 0o4755);
        // 0o4755 has no write bits for group/other, so the umask is a no-op.
        assert_eq!(retained_mode(0o4755, 0o022), 0o4755);
        assert_eq!(retained_mode(0o0777, 0o777), 0o0000);
        assert_eq!(retained_mode(0o7777, 0o000), 0o7777, "sticky survives");
    }

    #[test]
    fn preserved_mode_drops_sticky() {
        // utils.c:373 masks with S_ISUID|S_ISGID|S_IRWX* — no S_ISVTX.
        assert_eq!(preserved_mode(0o1755), 0o0755);
        assert_eq!(preserved_mode(0o4700), 0o4700);
    }

    #[test]
    fn retain_branch_needs_all_four_conditions() {
        // utils.c:282-287: !pflag && dne && (S_ISUID|S_ISGID) && st_uid == myuid
        assert!(regains_set_bits(true, false, 0o4755, 1000, 1000));
        assert!(
            !regains_set_bits(false, false, 0o4755, 1000, 1000),
            "target exists"
        );
        assert!(!regains_set_bits(true, true, 0o4755, 1000, 1000), "-p path");
        assert!(
            !regains_set_bits(true, false, 0o0755, 1000, 1000),
            "no set-id bit"
        );
        assert!(
            !regains_set_bits(true, false, 0o4755, 0, 1000),
            "another owner"
        );
    }

    #[test]
    fn a_recursive_scan_never_writes_through_a_link() {
        assert!(writes_through(false, true), "default scan: truncate");
        assert!(!writes_through(true, true), "-R scan: replace the link");
        assert!(writes_through(true, false), "a plain destination is opened");
        assert!(writes_through(false, false));
    }

    #[test]
    fn post_order_dir_stamp_follows_the_preserve_mask() {
        // Without -p the raw source mode goes to `chmod`, type bits and sticky
        // bit included (the file system keeps the low 12).
        assert_eq!(
            post_create_dir_mode(S_IFDIR | 0o1755, false),
            S_IFDIR | 0o1755
        );
        // With -p, `setfile` has masked the mode down to what it dares apply:
        // the sticky bit is gone, the setgid bit is not, the type bits are off.
        assert_eq!(post_create_dir_mode(S_IFDIR | 0o1755, true), 0o0755);
        assert_eq!(post_create_dir_mode(S_IFDIR | 0o2750, true), 0o2750);
    }

    // ── stat comparisons ─────────────────────────────────────────────────

    fn ids(dev: u64, ino: u64, mode: u32) -> FileIds {
        FileIds {
            dev,
            ino,
            mode,
            uid: 0,
            gid: 0,
        }
    }

    #[test]
    fn identical_needs_both_device_and_inode() {
        let dir = S_IFDIR;
        assert!(is_identical(&ids(3, 17, 0), &ids(3, 17, 0)));
        assert!(
            !is_identical(&ids(3, 17, 0), &ids(4, 17, 0)),
            "other device"
        );
        assert!(!is_identical(&ids(3, 17, 0), &ids(3, 18, 0)), "other inode");
        assert!(
            !is_identical(&ids(3, 17, dir), &ids(3, 18, dir)),
            "same dir not"
        );
    }

    #[test]
    fn directory_target_may_not_be_overwritten_by_a_file() {
        assert!(overwrites_dir(0o100644, S_IFDIR));
        assert!(
            !overwrites_dir(S_IFDIR, S_IFDIR),
            "dir over dir is the walk's problem"
        );
        assert!(!overwrites_dir(0o100644, 0o100644));
        assert!(is_dir(S_IFDIR));
        assert!(!is_dir(S_IFLNK));
    }

    #[test]
    fn unwanted_directory_matches_the_mode_bits() {
        assert!(is_unwanted_directory(S_IFDIR | 0o755, false));
        assert!(!is_unwanted_directory(0o100644, false));
        // cp.c:456 `if (!Rflag && !rflag)`: with -R a directory source is the
        // walk's business, not a refusal.
        assert!(!is_unwanted_directory(S_IFDIR | 0o755, true));
    }

    #[test]
    fn entry_dispatch_covers_every_type_bit() {
        assert_eq!(entry_action(S_IFDIR | 0o755), EntryAction::Directory);
        assert_eq!(entry_action(S_IFLNK | 0o777), EntryAction::Symlink);
        assert_eq!(entry_action(S_IFREG | 0o644), EntryAction::Regular);
        assert_eq!(entry_action(S_IFIFO | 0o644), EntryAction::Fifo);
        assert_eq!(
            entry_action(S_IFBLK | 0o644),
            EntryAction::BlockSpecial,
            "cp.c:517-525 recreates the node with mknod"
        );
        assert_eq!(entry_action(S_IFCHR | 0o644), EntryAction::CharacterSpecial);
        assert_eq!(entry_action(S_IFSOCK), EntryAction::Socket);
        assert!(is_link(S_IFLNK | 0o777));
        assert!(!is_link(S_IFDIR | 0o755));
        // cp.c:443 masks the type out, so the permission bits cannot shift it.
        assert_eq!(entry_action(S_IFSOCK | 0o777), EntryAction::Socket);
        assert_eq!(entry_action(S_IFREG | 0o7777), EntryAction::Regular);
        assert_eq!(entry_action(0o160000), EntryAction::Unknown);
        assert_eq!(entry_action(0o644), EntryAction::Unknown);
    }

    // ── pump ─────────────────────────────────────────────────────────────

    /// Scripted legs: `reads` are fed in order, `writes` are recorded.
    struct Mock {
        reads: Vec<usize>,
        next: usize,
        fail_read: bool,
        short_write: Option<usize>,
        fail_write: bool,
        written: Vec<Vec<u8>>,
    }

    impl Mock {
        fn new(reads: &[usize]) -> Self {
            Mock {
                reads: reads.to_vec(),
                next: 0,
                fail_read: false,
                short_write: None,
                fail_write: false,
                written: Vec::new(),
            }
        }
    }

    impl CopyIo for Mock {
        fn read(&mut self, buf: &mut [u8]) -> Result<usize, CopyError> {
            if self.fail_read {
                return Err(CopyError::Read);
            }
            let n = self
                .reads
                .get(self.next)
                .copied()
                .unwrap_or(0)
                .min(buf.len());
            self.next += 1;
            for b in buf[..n].iter_mut() {
                *b = b'a';
            }
            Ok(n)
        }

        fn write(&mut self, buf: &[u8]) -> Result<(), CopyError> {
            if self.fail_write {
                return Err(CopyError::Write);
            }
            if self.short_write.is_some_and(|n| n < buf.len()) {
                return Err(CopyError::Write); // utils.c:247 rcount != wcount
            }
            self.written.push(buf.to_vec());
            Ok(())
        }
    }

    #[test]
    fn pump_moves_every_block_and_counts_them() {
        let mut m = Mock::new(&[8, 5, 0]);
        let mut scratch = [0u8; 8];
        assert_eq!(pump(&mut m, &mut scratch), Ok(13));
        assert_eq!(m.written.len(), 2);
        assert_eq!(m.written[1].len(), 5, "the last block is written whole");
    }

    #[test]
    fn pump_stops_immediately_on_an_empty_input() {
        let mut m = Mock::new(&[0]);
        assert_eq!(pump(&mut m, &mut [0u8; 16]), Ok(0));
        assert!(m.written.is_empty());
    }

    #[test]
    fn short_write_is_a_write_failure() {
        let mut m = Mock::new(&[4, 0]);
        m.short_write = Some(3);
        assert_eq!(pump(&mut m, &mut [0u8; 8]), Err(CopyError::Write));
    }

    #[test]
    fn failed_write_is_reported_as_write() {
        let mut m = Mock::new(&[4]);
        m.fail_write = true;
        assert_eq!(pump(&mut m, &mut [0u8; 8]), Err(CopyError::Write));
    }

    #[test]
    fn failed_read_is_reported_as_read() {
        let mut m = Mock::new(&[4]);
        m.fail_read = true;
        assert_eq!(pump(&mut m, &mut [0u8; 8]), Err(CopyError::Read));
    }

    #[test]
    fn pump_uses_the_caller_scratch_and_never_overreads() {
        // A 4096-byte scratch against a source that reports 4096 twice then a
        // 1-byte tail: the tail must be written as one byte, not padded out.
        let mut m = Mock::new(&[4096, 4096, 1, 0]);
        let mut scratch = [0u8; 4096];
        assert_eq!(pump(&mut m, &mut scratch), Ok(8193));
        assert_eq!(m.written.last().unwrap().len(), 1);
    }

    #[test]
    fn zero_sized_source_never_enters_the_loop() {
        // utils.c:183: st_size 0 means no read at all (the FIFO quirk).
        assert!(!pumps_data(0));
        assert!(pumps_data(1));
        assert!(!pumps_data(-1), "a negative size is not pumped either");
    }
}
