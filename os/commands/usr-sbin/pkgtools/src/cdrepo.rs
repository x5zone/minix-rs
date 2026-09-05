//! Compact disc repository location.
//!
//! Ground truth: `minix3/minix/commands/pkgin_cd/pkgin_cd.sh`. The repository
//! path follows `packages/{release}/{arch}/All`, the summary file is
//! `pkg_summary.bz2`. The script searches the root directory first and the
//! mounted disc second (`/` then `/mnt`), uses the first location holding the
//! summary file, and hands the remaining arguments to the package installer.
//! An optional user file supplies the disc drive name. Mounting and installing
//! stay with the execution layer; this module owns the path building and the
//! search order.

use crate::PkgError;

/// Summary file name inside a repository directory.
pub const SUMMARY_FILE: &str = "pkg_summary.bz2";

/// Build a repository path (`packages/{release}/{arch}/All`) into `out`.
pub fn build_repo_path(
    release: &str,
    arch: &str,
    out: &mut [u8],
) -> Result<usize, PkgError> {
    if release.is_empty() || arch.is_empty() {
        return Err(PkgError::InvalidArgument);
    }
    if !release
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'-')
        || !arch
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        return Err(PkgError::InvalidArgument);
    }
    let prefix = b"packages/";
    let middle = b"/";
    let suffix = b"/All";
    let needed = prefix.len() + release.len() + middle.len() + arch.len() + suffix.len();
    if out.len() < needed {
        return Err(PkgError::InvalidArgument);
    }
    let mut written = 0;
    out[written..written + prefix.len()].copy_from_slice(prefix);
    written += prefix.len();
    out[written..written + release.len()].copy_from_slice(release.as_bytes());
    written += release.len();
    out[written..written + middle.len()].copy_from_slice(middle);
    written += middle.len();
    out[written..written + arch.len()].copy_from_slice(arch.as_bytes());
    written += arch.len();
    out[written..written + suffix.len()].copy_from_slice(suffix);
    written += suffix.len();
    Ok(written)
}

/// Search roots in script order (file system root first, disc second).
pub const SEARCH_ROOTS: &[&str] = &["/", "/mnt"];

/// Pick the first root whose summary path is present.
///
/// `present` answers whether `root/repo_path/summary` exists; the file system
/// walk stays with the caller.
pub fn pick_repo_root<'a>(
    roots: &[&'a str],
    present: impl Fn(&str) -> bool,
) -> Option<&'a str> {
    roots.iter().copied().find(|root| present(root))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(release: &str, arch: &str) -> String {
        let mut out = [0u8; 64];
        let len = build_repo_path(release, arch, &mut out).unwrap();
        String::from_utf8_lossy(&out[..len]).into_owned()
    }

    #[test]
    fn test_path_builds() {
        assert_eq!(render("3.4.0", "x86_64"), "packages/3.4.0/x86_64/All");
    }

    #[test]
    fn test_bad_words_rejected() {
        let mut out = [0u8; 64];
        assert_eq!(
            build_repo_path("", "x86_64", &mut out),
            Err(PkgError::InvalidArgument)
        );
        assert_eq!(
            build_repo_path("3.4.0", "", &mut out),
            Err(PkgError::InvalidArgument)
        );
        assert_eq!(
            build_repo_path("3.4 0", "x86_64", &mut out),
            Err(PkgError::InvalidArgument)
        );
    }

    #[test]
    fn test_small_buffer_rejected() {
        let mut out = [0u8; 4];
        assert_eq!(
            build_repo_path("3.4.0", "x86_64", &mut out),
            Err(PkgError::InvalidArgument)
        );
    }

    #[test]
    fn test_search_order_prefers_root() {
        let roots = ["/", "/mnt"];
        let found = pick_repo_root(&roots, |root| root == "/" || root == "/mnt");
        assert_eq!(found, Some("/"));
        let found = pick_repo_root(&roots, |root| root == "/mnt");
        assert_eq!(found, Some("/mnt"));
        let found = pick_repo_root(&roots, |_| false);
        assert_eq!(found, None);
    }

    #[test]
    fn test_summary_name() {
        assert_eq!(SUMMARY_FILE, "pkg_summary.bz2");
    }
}
