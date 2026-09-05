//! Package set vocabulary.
//!
//! Ground truth: `minix3/minix/commands/pkgin_sets/pkgin_sets.sh`. The script
//! offers three sets independently: the core tools set (`openssh`, `vim`,
//! `curl`), the development tools set (`git-base`, `bmake`, `gmake`,
//! `binutils`, `clang`), and the extras set (`bison`, `groff`, `perl`,
//! `python27`). Each set installs through one package installer call; typing
//! no at a confirmation prompt skips that set. The execution layer owns the
//! installer calls; this module owns the set names and the package database.

use crate::PkgError;

/// Package set names offered by the sets script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageSet {
    /// Core tools (remote shell, editor, transfer).
    Core,
    /// Development tools (version control, builders, toolchain).
    Development,
    /// Extras (parsers, text tools, interpreters).
    Extras,
}

/// Parse a set selection word (`1`, `2`, `3`, `core`, `dev`, `extras`, `all`).
pub fn parse_set_selection(word: &str) -> Result<Option<PackageSet>, PkgError> {
    match word {
        "1" | "core" => Ok(Some(PackageSet::Core)),
        "2" | "dev" | "devel" => Ok(Some(PackageSet::Development)),
        "3" | "extras" => Ok(Some(PackageSet::Extras)),
        "all" => Ok(None),
        _ => Err(PkgError::InvalidArgument),
    }
}

/// Check one package name (lowercase letters, digits, dashes, plus signs,
/// and dots; must start with a letter or digit).
pub fn check_package_name(name: &str) -> Result<(), PkgError> {
    if name.is_empty() {
        return Err(PkgError::InvalidArgument);
    }
    let mut first = true;
    for byte in name.bytes() {
        let ok = byte.is_ascii_lowercase()
            || byte.is_ascii_digit()
            || byte == b'-'
            || byte == b'+'
            || byte == b'.';
        if !ok {
            return Err(PkgError::InvalidArgument);
        }
        if first && !(byte.is_ascii_lowercase() || byte.is_ascii_digit()) {
            return Err(PkgError::InvalidArgument);
        }
        first = false;
    }
    Ok(())
}

/// Package database behind set installation.
pub trait PackageDb<'a> {
    /// Packages of `set` in installation order, or `None` when unknown.
    fn packages_of(&self, set: PackageSet) -> Option<&'a [&'a str]>;
    /// True when `package` is installed.
    fn is_installed(&self, package: &str) -> bool;
    /// Number of known sets.
    fn len(&self) -> usize;
    /// True when no set is known.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Database backed by borrowed slices (the script sets, compiled in).
pub struct SlicePackageDb<'a> {
    core: &'a [&'a str],
    development: &'a [&'a str],
    extras: &'a [&'a str],
    installed: &'a [&'a str],
}

impl<'a> SlicePackageDb<'a> {
    /// Build a database over borrowed package lists.
    pub fn new(
        core: &'a [&'a str],
        development: &'a [&'a str],
        extras: &'a [&'a str],
        installed: &'a [&'a str],
    ) -> Self {
        SlicePackageDb {
            core,
            development,
            extras,
            installed,
        }
    }
}

impl<'a> PackageDb<'a> for SlicePackageDb<'a> {
    fn packages_of(&self, set: PackageSet) -> Option<&'a [&'a str]> {
        match set {
            PackageSet::Core => Some(self.core),
            PackageSet::Development => Some(self.development),
            PackageSet::Extras => Some(self.extras),
        }
    }

    fn is_installed(&self, package: &str) -> bool {
        self.installed.contains(&package)
    }

    fn len(&self) -> usize {
        3
    }
}

/// Empty database (every set unknown, nothing installed).
pub struct EmptyPackageDb;

impl<'a> PackageDb<'a> for EmptyPackageDb {
    fn packages_of(&self, _set: PackageSet) -> Option<&'a [&'a str]> {
        None
    }

    fn is_installed(&self, _package: &str) -> bool {
        false
    }

    fn len(&self) -> usize {
        0
    }
}

/// Missing packages of one set (listed but not installed).
pub fn missing_of<'a, D: PackageDb<'a>>(
    db: &D,
    set: PackageSet,
    out: &mut [&'a str],
) -> Result<usize, PkgError> {
    let packages = db.packages_of(set).ok_or(PkgError::NotFound)?;
    let mut count = 0;
    for package in packages {
        if !db.is_installed(package) {
            if count >= out.len() {
                return Err(PkgError::InvalidArgument);
            }
            out[count] = package;
            count += 1;
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_db<'a>() -> SlicePackageDb<'a> {
        static CORE: &[&str] = &["openssh", "vim", "curl"];
        static DEV: &[&str] = &["git-base", "bmake", "clang"];
        static EXTRA: &[&str] = &["bison", "perl"];
        static INSTALLED: &[&str] = &["openssh", "bison"];
        SlicePackageDb::new(CORE, DEV, EXTRA, INSTALLED)
    }

    #[test]
    fn test_selections_parse() {
        assert_eq!(parse_set_selection("1"), Ok(Some(PackageSet::Core)));
        assert_eq!(parse_set_selection("core"), Ok(Some(PackageSet::Core)));
        assert_eq!(parse_set_selection("2"), Ok(Some(PackageSet::Development)));
        assert_eq!(parse_set_selection("dev"), Ok(Some(PackageSet::Development)));
        assert_eq!(parse_set_selection("3"), Ok(Some(PackageSet::Extras)));
        assert_eq!(parse_set_selection("all"), Ok(None));
        assert_eq!(
            parse_set_selection("everything"),
            Err(PkgError::InvalidArgument)
        );
    }

    #[test]
    fn test_package_names_checked() {
        assert_eq!(check_package_name("git-base"), Ok(()));
        assert_eq!(check_package_name("python27"), Ok(()));
        assert_eq!(check_package_name("g++"), Ok(()));
        assert_eq!(check_package_name(""), Err(PkgError::InvalidArgument));
        assert_eq!(check_package_name("-bad"), Err(PkgError::InvalidArgument));
        assert_eq!(check_package_name("has space"), Err(PkgError::InvalidArgument));
        assert_eq!(check_package_name("UPPER"), Err(PkgError::InvalidArgument));
    }

    #[test]
    fn test_slice_db_serves_sets() {
        let db = sample_db();
        assert_eq!(db.len(), 3);
        assert_eq!(db.packages_of(PackageSet::Core).unwrap().len(), 3);
        assert!(db.is_installed("openssh"));
        assert!(!db.is_installed("vim"));
    }

    #[test]
    fn test_empty_db_misses() {
        let db = EmptyPackageDb;
        assert_eq!(db.len(), 0);
        assert_eq!(db.packages_of(PackageSet::Core), None);
        assert!(!db.is_installed("openssh"));
    }

    #[test]
    fn test_missing_lists_uninstalled() {
        let db = sample_db();
        let mut out = [""; 8];
        let count = missing_of(&db, PackageSet::Core, &mut out).unwrap();
        assert_eq!(count, 2);
        assert_eq!(out[0], "vim");
        assert_eq!(out[1], "curl");
    }

    #[test]
    fn test_missing_unknown_set_reports_not_found() {
        let db = EmptyPackageDb;
        let mut out = [""; 8];
        assert_eq!(
            missing_of(&db, PackageSet::Core, &mut out),
            Err(PkgError::NotFound)
        );
    }

    #[test]
    fn test_error_numbers_match_unix() {
        assert_eq!(PkgError::InvalidArgument.as_errno(), 22);
        assert_eq!(PkgError::NotFound.as_errno(), 2);
        assert_eq!(PkgError::NoSpace.as_errno(), 28);
    }
}
