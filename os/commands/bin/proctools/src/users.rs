//! `users` — the sorted, de-duplicated login-name list behind the command.
//!
//! Ground truth: `minix3/usr.bin/users/users.c`. It reads the utmp logins,
//! insertion-sorts the entries by name (`users.c:90-99`, a linked-list
//! insertion sort comparing `strcmp(from->name, (*nextp)->name) > 0`), then
//! prints the first name followed by `" <name>"` for each *adjacent-distinct*
//! successor — one whitespace-separated line of unique logged-in names.
//!
//! This module holds the pure name-list transform: [`sorted_unique`] does the
//! same insertion sort and in-place adjacent de-duplication on a borrowed
//! slice of name strings, so the whole behavior is host-testable without a
//! kernel write channel (the thin bin supplies the collected names from the
//! utmp walk). Insertion sort (not `slice::sort_unstable`) is used both to
//! match C's algorithm exactly and to keep this crate's library `no_std` with
//! no `alloc` dependency — the sort is in place on a caller-provided slice.
//!
//! `str`'s `<` ordering is byte-wise (UTF-8 code-unit) order, which agrees
//! with C `strcmp` for the ASCII login names utmp stores.

/// Sort `names` by ascending byte order and collapse adjacent duplicates in
/// place, returning the count of distinct names left at the front of `names`.
///
/// Faithful to `users.c`'s sort-by-name then print-adjacent-distinct output:
/// after this call, `&names[..count]` is the sorted unique set, and the bin
/// joins those with single spaces.
pub fn sorted_unique(names: &mut [&str]) -> usize {
    // Insertion sort ascending — mirrors users.c's in-place linked-list sort
    // (stable, O(n^2), trivial for the handful of concurrent logins).
    for i in 1..names.len() {
        let mut j = i;
        while j > 0 && names[j - 1] > names[j] {
            names.swap(j - 1, j);
            j -= 1;
        }
    }
    // Collapse adjacent duplicates to the front (names are `&str`, Copy).
    let mut w = 0usize;
    for r in 0..names.len() {
        if w == 0 || names[r] != names[w - 1] {
            names[w] = names[r];
            w += 1;
        }
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_and_dedups_logins() {
        // users.c prints each distinct name once, sorted ascending.
        let mut names = ["carol", "alice", "carol", "bob", "alice"];
        let n = sorted_unique(&mut names);
        assert_eq!(&names[..n], ["alice", "bob", "carol"]);
    }

    #[test]
    fn single_name_no_change() {
        let mut names = ["root"];
        assert_eq!(sorted_unique(&mut names), 1);
        assert_eq!(names[0], "root");
    }

    #[test]
    fn empty_list_yields_zero() {
        // users.c: ncnt == 0 -> return 0 (prints nothing).
        let mut names: [&str; 0] = [];
        assert_eq!(sorted_unique(&mut names), 0);
    }

    #[test]
    fn all_duplicates_collapse_to_one() {
        let mut names = ["bob", "bob", "bob"];
        let n = sorted_unique(&mut names);
        assert_eq!(n, 1);
        assert_eq!(names[0], "bob");
    }

    #[test]
    fn already_sorted_distinct_preserved() {
        let mut names = ["ada", "bob", "cy"];
        let n = sorted_unique(&mut names);
        assert_eq!(&names[..n], ["ada", "bob", "cy"]);
    }

    #[test]
    fn sorts_unsorted_singletons() {
        let mut names = ["zed", "amy", "mia"];
        let n = sorted_unique(&mut names);
        assert_eq!(&names[..n], ["amy", "mia", "zed"]);
    }
}
