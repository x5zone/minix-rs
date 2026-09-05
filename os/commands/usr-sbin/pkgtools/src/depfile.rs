//! Dependency file vocabulary.
//!
//! Ground truth: `minix3/usr.bin/mkdep/mkdep.c`. The default output file is
//! `.depend` (the `DEFAULT_FILENAME` constant). Output lines follow
//! `target: dependency ...` shape; long lines join with a trailing backslash
//! continuation. The compiler calls stay with the execution layer; this module
//! owns the file name and the line shapes.

use crate::PkgError;

/// Default dependency output file.
pub const DEFAULT_DEPFILE: &str = ".depend";

/// Split one `target: dependencies` line into its halves.
///
/// The colon must exist, the target must be non empty, and at least one
/// dependency must follow. Surrounding blanks are trimmed.
pub fn split_dep_line(line: &str) -> Result<(&str, &str), PkgError> {
    let (target, deps) = line.split_once(':').ok_or(PkgError::InvalidArgument)?;
    let target = target.trim();
    let deps = deps.trim();
    if target.is_empty() || deps.is_empty() {
        return Err(PkgError::InvalidArgument);
    }
    Ok((target, deps))
}

/// True when `line` ends with a backslash continuation (trailing blanks
/// ignored, the backslash itself must be the last non blank character).
pub fn has_continuation(line: &str) -> bool {
    line.trim_end().ends_with('\\')
}

/// Group physical lines into logical lines: continuation runs collapse.
///
/// Each logical line starts a new run; a run continues while its last physical
/// line ends with a continuation marker. `out` receives the trimmed head line
/// of each run (the marker stripped), and the run count is returned. Because
/// slices borrow, continuation tails are implied by position, not copied.
pub fn join_continuations<'a>(lines: &[&'a str], out: &mut [&'a str]) -> Result<usize, PkgError> {
    let mut count = 0;
    let mut index = 0;
    while index < lines.len() {
        if count >= out.len() {
            return Err(PkgError::InvalidArgument);
        }
        out[count] = lines[index].trim_end().trim_end_matches('\\').trim_end();
        count += 1;
        loop {
            let continued = has_continuation(lines[index]);
            index += 1;
            if !continued || index >= lines.len() {
                break;
            }
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dep_line_splits() {
        assert_eq!(
            split_dep_line("main.o: main.c defs.h"),
            Ok(("main.o", "main.c defs.h"))
        );
        assert_eq!(split_dep_line("main.o:"), Err(PkgError::InvalidArgument));
        assert_eq!(split_dep_line(": main.c"), Err(PkgError::InvalidArgument));
        assert_eq!(split_dep_line("no colon here"), Err(PkgError::InvalidArgument));
    }

    #[test]
    fn test_continuation_detected() {
        assert!(has_continuation("main.o: main.c \\"));
        assert!(has_continuation("main.o: main.c \\   "));
        assert!(!has_continuation("main.o: main.c"));
    }

    #[test]
    fn test_lines_join() {
        let lines = ["main.o: main.c \\", "defs.h", "util.o: util.c"];
        let mut out = [""; 4];
        let count = join_continuations(&lines, &mut out).unwrap();
        assert_eq!(count, 2);
        assert_eq!(out[0], "main.o: main.c");
        assert_eq!(out[1], "util.o: util.c");
    }

    #[test]
    fn test_default_name() {
        assert_eq!(DEFAULT_DEPFILE, ".depend");
    }
}
