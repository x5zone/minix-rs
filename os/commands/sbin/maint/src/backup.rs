//! Directory backup decisions.
//!
//! Ground truth: `minix3/minix/commands/backup/backup.c`. The header comment
//! lists the option letters (only directories at the top level, skip junk,
//! ask for another volume when out of space, only loose files, skip object
//! files, restore direction, skip assembler files, keep creation date,
//! verbose, compress), the copy buffer holds `COPY_SIZE 4096` bytes, each
//! directory holds at most `MAX_ENTRIES 512` entries, each path holds at most
//! `MAX_PATH 256` characters. The program resembles the `make` tool without a
//! makefile: when the target directory is empty everything is copied, when the
//! target holds an older backup only files that are new or out of date are
//! copied. The restore direction uncompresses when necessary.

use crate::MaintError;

/// Backup options, one boolean per option letter in `backup.c`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BackupOptions {
    /// `-d`: at the top level only back up directories, not loose files.
    pub only_directories: bool,
    /// `-j`: skip junk names (`*.Z`, `*.bak`, `*.log`, `a.out`, `core`).
    pub skip_junk: bool,
    /// `-m`: when out of space, ask for another volume instead of failing.
    pub ask_another_volume: bool,
    /// `-n`: only loose files are backed up, no directories.
    pub only_files: bool,
    /// `-o`: skip `*.o` object files.
    pub skip_objects: bool,
    /// `-r`: restore direction (uncompress when necessary).
    pub restore: bool,
    /// `-s`: skip `*.s` assembler files.
    pub skip_asm: bool,
    /// `-t`: set the target creation date equal to the source date.
    pub keep_date: bool,
    /// `-v`: verbose, announce what is being done.
    pub verbose: bool,
    /// `-z`: compress on backup, uncompress on restore.
    pub compress: bool,
}

/// Parse a flag word such as `-jv` or `-dm` (without a leading dash).
pub fn parse_backup_flags(word: &str) -> Result<BackupOptions, MaintError> {
    let mut options = BackupOptions::default();
    if word.is_empty() {
        return Err(MaintError::InvalidArgument);
    }
    for byte in word.bytes() {
        match byte {
            b'd' => options.only_directories = true,
            b'j' => options.skip_junk = true,
            b'm' => options.ask_another_volume = true,
            b'n' => options.only_files = true,
            b'o' => options.skip_objects = true,
            b'r' => options.restore = true,
            b's' => options.skip_asm = true,
            b't' => options.keep_date = true,
            b'v' => options.verbose = true,
            b'z' => options.compress = true,
            _ => return Err(MaintError::InvalidArgument),
        }
    }
    if options.only_directories && options.only_files {
        return Err(MaintError::InvalidArgument);
    }
    Ok(options)
}

/// True when `name` is junk that `-j` skips.
///
/// The list comes from the `backup.c` header comment: `*.Z`, `*.bak`,
/// `*.log`, `a.out`, and `core`. Suffix matching is byte exact; the caller
/// passes only the final path component.
pub fn is_junk_name(name: &str) -> bool {
    if name == "a.out" || name == "core" {
        return true;
    }
    name.ends_with(".Z") || name.ends_with(".bak") || name.ends_with(".log")
}

/// True when `name` is skipped because of the object and assembler options.
pub fn is_skipped_by_kind(name: &str, options: BackupOptions) -> bool {
    if options.skip_objects && name.ends_with(".o") {
        return true;
    }
    if options.skip_asm && name.ends_with(".s") {
        return true;
    }
    false
}

/// Copy decision for one source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyDecision {
    /// Target is missing, so the file must be copied.
    CopyNew,
    /// Source is newer than the target, so the file must be refreshed.
    CopyOutOfDate,
    /// Target is at least as new as the source, so nothing needs copying.
    SkipUpToDate,
    /// The name is filtered out by the active options.
    SkipFiltered,
}

/// Decide whether one file needs copying.
///
/// `target_modified` is `None` when the target file does not exist, otherwise
/// the target modification time in seconds. Times compare as plain integers;
/// equal times mean up to date (the same rule the `make` tool uses).
pub fn decide_copy(
    name: &str,
    is_directory: bool,
    top_level: bool,
    source_modified: u64,
    target_modified: Option<u64>,
    options: BackupOptions,
) -> CopyDecision {
    if top_level && options.only_directories && !is_directory {
        return CopyDecision::SkipFiltered;
    }
    if top_level && options.only_files && is_directory {
        return CopyDecision::SkipFiltered;
    }
    if options.skip_junk && is_junk_name(name) {
        return CopyDecision::SkipFiltered;
    }
    if is_skipped_by_kind(name, options) {
        return CopyDecision::SkipFiltered;
    }
    match target_modified {
        None => CopyDecision::CopyNew,
        Some(target) if source_modified > target => CopyDecision::CopyOutOfDate,
        Some(_) => CopyDecision::SkipUpToDate,
    }
}

/// File metadata behind the copy decision.
///
/// The file system walk stays with the execution layer; this trait exposes
/// only the two facts the pure decision needs (directory shape and
/// modification time) so tests can run without a file system.
pub trait MetaSource {
    /// True when `path` names a directory.
    fn is_directory(&self, path: &str) -> Option<bool>;
    /// Modification time of `path` in seconds, or `None` when missing.
    fn modified_of(&self, path: &str) -> Option<u64>;
}

/// Metadata table backed by parallel slices (path, directory flag, time).
pub struct SliceMeta<'a> {
    paths: &'a [&'a str],
    directories: &'a [bool],
    times: &'a [u64],
}

impl<'a> SliceMeta<'a> {
    /// Build a table; the three slices must describe the same entries.
    pub fn new(paths: &'a [&'a str], directories: &'a [bool], times: &'a [u64]) -> Self {
        SliceMeta {
            paths,
            directories,
            times,
        }
    }

    fn index_of(&self, path: &str) -> Option<usize> {
        self.paths.iter().position(|candidate| *candidate == path)
    }
}

impl MetaSource for SliceMeta<'_> {
    fn is_directory(&self, path: &str) -> Option<bool> {
        self.index_of(path).map(|index| self.directories[index])
    }

    fn modified_of(&self, path: &str) -> Option<u64> {
        self.index_of(path).map(|index| self.times[index])
    }
}

/// Metadata source where every path is missing (nothing was backed up yet).
pub struct EmptyMeta;

impl MetaSource for EmptyMeta {
    fn is_directory(&self, _path: &str) -> Option<bool> {
        None
    }

    fn modified_of(&self, _path: &str) -> Option<u64> {
        None
    }
}

/// Decide through a [`MetaSource`] so the execution layer never duplicates
/// the filtering rules.
pub fn decide_via_source<S: MetaSource>(
    source: &S,
    name: &str,
    source_path: &str,
    target_path: &str,
    top_level: bool,
    options: BackupOptions,
) -> Result<CopyDecision, MaintError> {
    let is_directory = source
        .is_directory(source_path)
        .ok_or(MaintError::NotFound)?;
    let source_time = source
        .modified_of(source_path)
        .ok_or(MaintError::NotFound)?;
    let target_time = source.modified_of(target_path);
    Ok(decide_copy(
        name,
        is_directory,
        top_level,
        source_time,
        target_time,
        options,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain() -> BackupOptions {
        BackupOptions::default()
    }

    #[test]
    fn test_flag_letters_parse() {
        let options = parse_backup_flags("jv").unwrap();
        assert!(options.skip_junk);
        assert!(options.verbose);
        assert!(!options.compress);
    }

    #[test]
    fn test_all_letters_parse() {
        let options = parse_backup_flags("djmorstvz").unwrap();
        assert!(options.only_directories);
        assert!(options.skip_junk);
        assert!(options.ask_another_volume);
        assert!(!options.only_files);
        assert!(options.skip_objects);
        assert!(options.restore);
        assert!(options.skip_asm);
        assert!(options.keep_date);
        assert!(options.verbose);
        assert!(options.compress);
    }

    #[test]
    fn test_unknown_letter_rejected() {
        assert_eq!(
            parse_backup_flags("jx"),
            Err(MaintError::InvalidArgument)
        );
    }

    #[test]
    fn test_empty_word_rejected() {
        assert_eq!(parse_backup_flags(""), Err(MaintError::InvalidArgument));
    }

    #[test]
    fn test_conflicting_top_level_rejected() {
        assert_eq!(
            parse_backup_flags("dn"),
            Err(MaintError::InvalidArgument)
        );
    }

    #[test]
    fn test_junk_names_detected() {
        assert!(is_junk_name("notes.bak"));
        assert!(is_junk_name("build.log"));
        assert!(is_junk_name("tool.Z"));
        assert!(is_junk_name("a.out"));
        assert!(is_junk_name("core"));
        assert!(!is_junk_name("notes.txt"));
        assert!(!is_junk_name("coreutils"));
    }

    #[test]
    fn test_missing_target_copies() {
        assert_eq!(
            decide_copy("notes.txt", false, false, 100, None, plain()),
            CopyDecision::CopyNew
        );
    }

    #[test]
    fn test_newer_source_refreshes() {
        assert_eq!(
            decide_copy("notes.txt", false, false, 200, Some(100), plain()),
            CopyDecision::CopyOutOfDate
        );
    }

    #[test]
    fn test_up_to_date_skips() {
        assert_eq!(
            decide_copy("notes.txt", false, false, 100, Some(100), plain()),
            CopyDecision::SkipUpToDate
        );
        assert_eq!(
            decide_copy("notes.txt", false, false, 50, Some(100), plain()),
            CopyDecision::SkipUpToDate
        );
    }

    #[test]
    fn test_top_level_directory_filter() {
        let mut options = plain();
        options.only_directories = true;
        assert_eq!(
            decide_copy("loose.txt", false, true, 100, None, options),
            CopyDecision::SkipFiltered
        );
        assert_eq!(
            decide_copy("docs", true, true, 100, None, options),
            CopyDecision::CopyNew
        );
    }

    #[test]
    fn test_junk_filter_applies() {
        let mut options = plain();
        options.skip_junk = true;
        assert_eq!(
            decide_copy("build.log", false, false, 100, None, options),
            CopyDecision::SkipFiltered
        );
    }

    #[test]
    fn test_object_and_asm_filters() {
        let mut options = plain();
        options.skip_objects = true;
        options.skip_asm = true;
        assert_eq!(
            decide_copy("main.o", false, false, 100, None, options),
            CopyDecision::SkipFiltered
        );
        assert_eq!(
            decide_copy("start.s", false, false, 100, None, options),
            CopyDecision::SkipFiltered
        );
        assert_eq!(
            decide_copy("main.c", false, false, 100, None, options),
            CopyDecision::CopyNew
        );
    }

    #[test]
    fn test_slice_source_decides() {
        let paths = ["src/a.txt", "dst/a.txt"];
        let directories = [false, false];
        let times = [200u64, 100u64];
        let source = SliceMeta::new(&paths, &directories, &times);
        assert_eq!(
            decide_via_source(&source, "a.txt", "src/a.txt", "dst/a.txt", false, plain()),
            Ok(CopyDecision::CopyOutOfDate)
        );
    }

    #[test]
    fn test_empty_source_reports_missing() {
        let source = EmptyMeta;
        assert_eq!(
            decide_via_source(&source, "a.txt", "src/a.txt", "dst/a.txt", false, plain()),
            Err(MaintError::NotFound)
        );
    }

    #[test]
    fn test_error_numbers_match_unix() {
        assert_eq!(MaintError::InvalidArgument.as_errno(), 22);
        assert_eq!(MaintError::NotFound.as_errno(), 2);
    }
}
