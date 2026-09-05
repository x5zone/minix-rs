//! Startup file sequencing for new shells.
//!
//! Ground truth: `minix3/bin/sh/main.c` lines 196 to 215. A shell whose
//! program name starts with a dash is a login shell: it reads the system
//! profile then the user profile. Afterwards (and for every other shell),
//! when interactive mode is on (or strict standard mode is off) and the
//! real and effective user and group identifiers agree, the file named by
//! the `ENV` variable is read when it is set and non empty.
//!
//! The live files: `minix3/etc/profile` (system wide login setup: library
//! path, time zone, optional time zone file), `minix3/etc/shrc` (interactive
//! setup: hostname in the prompt), `minix3/etc/csh.cshrc`, `csh.login`,
//! `csh.logout` (the C shell family's counterparts).

/// How the shell was started: the dash prefix test from `main.c:198`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellKind {
    /// Program name starts with `-`: a login shell.
    Login,
    /// Any other invocation.
    NonLogin,
}

/// Which startup files a new shell reads, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartupPlan<'a> {
    /// Ordered list of files to read (each at most one of the well known
    /// paths below).
    pub files: [&'a str; 3],
    /// How many of `files` are used.
    pub file_count: usize,
}

/// System wide login setup file.
pub const SYSTEM_PROFILE: &str = "/etc/profile";
/// Per user login setup file.
pub const USER_PROFILE: &str = ".profile";

/// Decide the startup file sequence.
///
/// `interactive` is the `-i` flag state; `strict_standard` is the POSIX
/// mode flag; `same_ids` is true when real and effective user and group
/// identifiers all agree (the privileged shell skips user controlled files
/// — a classic security rule); `env_file` is the value of `ENV`, if any.
pub fn plan_startup<'a>(
    kind: ShellKind,
    interactive: bool,
    strict_standard: bool,
    same_ids: bool,
    env_file: Option<&'a str>,
) -> StartupPlan<'a> {
    let mut plan = StartupPlan {
        files: ["", "", ""],
        file_count: 0,
    };
    if kind == ShellKind::Login {
        plan.files[0] = SYSTEM_PROFILE;
        plan.files[1] = USER_PROFILE;
        plan.file_count = 2;
    }
    if (interactive || !strict_standard)
        && same_ids
        && let Some(path) = env_file
        && !path.is_empty()
        && plan.file_count < plan.files.len()
    {
        plan.files[plan.file_count] = path;
        plan.file_count += 1;
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_login_shell_reads_both_profiles() {
        let plan = plan_startup(ShellKind::Login, false, true, true, None);
        assert_eq!(plan.file_count, 2);
        assert_eq!(plan.files[0], SYSTEM_PROFILE);
        assert_eq!(plan.files[1], USER_PROFILE);
    }

    #[test]
    fn test_interactive_shell_reads_env_file() {
        let plan = plan_startup(ShellKind::NonLogin, true, true, true, Some("/etc/shrc"));
        assert_eq!(plan.file_count, 1);
        assert_eq!(plan.files[0], "/etc/shrc");
    }

    #[test]
    fn test_login_plus_env_chains_three() {
        let plan = plan_startup(ShellKind::Login, true, true, true, Some("/etc/shrc"));
        assert_eq!(plan.file_count, 3);
        assert_eq!(plan.files[2], "/etc/shrc");
    }

    #[test]
    fn test_privileged_shell_skips_env() {
        // Different real and effective identifiers: no user file.
        let plan = plan_startup(ShellKind::NonLogin, true, true, false, Some("/etc/shrc"));
        assert_eq!(plan.file_count, 0);
    }

    #[test]
    fn test_empty_env_reads_nothing() {
        let plan = plan_startup(ShellKind::NonLogin, true, true, true, Some(""));
        assert_eq!(plan.file_count, 0);
    }

    #[test]
    fn test_batch_shell_reads_nothing() {
        let plan = plan_startup(ShellKind::NonLogin, false, true, true, Some("/etc/shrc"));
        assert_eq!(plan.file_count, 0);
    }
}
