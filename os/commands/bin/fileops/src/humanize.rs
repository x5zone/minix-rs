//! `humanize_number` — a byte count rendered with a unit suffix.
//!
//! Ground truth: `minix3/lib/libc/gen/humanize_number.c` (NetBSD 1.16), 161
//! lines, with its flag bits declared in `minix3/include/stdlib.h:292-298`. The
//! only caller in this tree so far is `du -h` (`du.c:258-278`), which hands it a
//! five-byte buffer, an empty suffix, `HN_AUTOSCALE` and
//! `HN_B | HN_NOSPACE | HN_DECIMAL`.
//!
//! Three properties of the C function are easy to lose in a rewrite and are
//! pinned by the tests here. The value is scaled with *two digits* of headroom —
//! `bytes` is multiplied by 100 up front (`:103-104`) and the scaling loop
//! compares against `max - 50` (`:132`) — so the rounding at the end can push a
//! `9.9` up to `10` without another division step. The unit table is indexed by
//! the scale number, and `HN_B` is what puts a `B` at index zero
//! (`:72-75`); without it a scale of zero renders as a bare number with no
//! letter. And a rendering that does not fit the buffer is not an error the
//! caller can see: `snprintf` leaves the clipped text in the buffer while
//! returning the length it would have written, which is why `du` prints a cut
//! string instead of failing — except in the decimal branch, where C checks the
//! room for `.N` up front and returns -1 instead (`:146-148`), and by then the
//! buffer has already been emptied at `:88-89`.
//!
//! Not ported: `HN_GETSCALE`, the mode that answers with the chosen scale index
//! instead of writing text (`:135-138`). Nothing in this tree asks for that
//! answer, so a request for it is refused rather than silently mis-shaped.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// `stdlib.h:292`: keep one decimal digit when the scaled value is below 10.
pub const HN_DECIMAL: i32 = 0x01;
/// `stdlib.h:293`: no space between the number and the unit.
pub const HN_NOSPACE: i32 = 0x02;
/// `stdlib.h:294`: name the byte itself, i.e. put `B` at scale zero.
pub const HN_B: i32 = 0x04;
/// `stdlib.h:295`: SI units (1000) rather than binary (1024).
pub const HN_DIVISOR_1000: i32 = 0x08;
/// `stdlib.h:297`: answer with the scale index rather than the text. Not ported.
pub const HN_GETSCALE: i32 = 0x10;
/// `stdlib.h:298`: pick the largest scale that fits the buffer.
pub const HN_AUTOSCALE: i32 = 0x20;

/// The largest scale the C table reaches (`maxscale = 7`, `:79`):
/// `B`/`K`/`M`/`G`/`T`/`P`/`E`.
const MAX_SCALE: usize = 7;

/// `:59-76`: the two divisors and the four prefix tables, indexed by whether
/// `HN_DIVISOR_1000` and `HN_B` are set.
fn prefixes(flags: i32) -> (i64, [&'static str; MAX_SCALE]) {
    let si = flags & HN_DIVISOR_1000 != 0;
    let named_bytes = flags & HN_B != 0;
    let divisor = if si { 1000 } else { 1024 };
    let table = match (si, named_bytes) {
        (true, true) => ["B", "k", "M", "G", "T", "P", "E"],
        (true, false) => ["", "k", "M", "G", "T", "P", "E"],
        (false, true) => ["B", "K", "M", "G", "T", "P", "E"],
        (false, false) => ["", "K", "M", "G", "T", "P", "E"],
    };
    (divisor, table)
}

/// `humanize_number(buf, len, bytes, suffix, scale, flags)` for the
/// text-producing modes. `buf_len` is the size of C's buffer *including* its NUL
/// slot, so the text is clipped to `buf_len - 1` characters exactly as
/// `snprintf` clips it. `None` is C's -1.
///
/// `scale` carries the flag bits C puts in the same int: a plain 0..=6 asks for
/// that fixed scale, `HN_AUTOSCALE` asks for the largest that fits.
pub fn humanize_number(
    buf_len: usize,
    bytes: i64,
    suffix: &str,
    scale: i32,
    flags: i32,
) -> Option<String> {
    if scale & HN_GETSCALE != 0 {
        return None; // documented above
    }
    let (divisor, table) = prefixes(flags);
    // `:81-83`: an explicit scale has to stay inside the table.
    if scale >= MAX_SCALE as i32 && scale & HN_AUTOSCALE == 0 {
        return None;
    }

    // `:90-109`: the sign, the two digits of headroom, and the `post` path for a
    // value too big to multiply by 100 — that factor then waits until after the
    // divisions, and two more characters of the budget go to it.
    let sign: i64;
    let mut value: i64;
    let mut post: i64 = 1;
    let mut baselen: usize;
    if bytes < 0 {
        sign = -1;
        baselen = 3; // sign, digit, prefix
        let magnitude = bytes.checked_neg().unwrap_or(i64::MAX);
        if magnitude < i64::MAX / 100 {
            value = magnitude * 100;
        } else {
            value = magnitude;
            post = 100;
            baselen += 2;
        }
    } else {
        sign = 1;
        baselen = 2; // digit, prefix
        if bytes < i64::MAX / 100 {
            value = bytes * 100;
        } else {
            value = bytes;
            post = 100;
            baselen += 2;
        }
    }

    // `:110-120`: the separator and the suffix are part of the budget, and a
    // buffer that cannot hold `x y` plus a NUL is refused outright.
    let sep: &'static str = if flags & HN_NOSPACE != 0 {
        ""
    } else {
        baselen += 1;
        " "
    };
    let baselen = baselen + suffix.len();
    if buf_len < baselen + 1 {
        return None;
    }

    // `:122-141`: with autoscaling the number may use whatever digits the room
    // left after `baselen` holds; the fixed-scale path just divides that many
    // times. Either way the loop stops at the end of the table.
    let mut i: usize = 0;
    if scale & HN_AUTOSCALE != 0 {
        let digits = buf_len - baselen;
        let mut max: i64 = 100;
        for _ in 0..digits {
            max = max.saturating_mul(10);
        }
        while value >= max - 50 && i < MAX_SCALE {
            value /= divisor;
            i += 1;
        }
    } else {
        while (i as i32) < scale && i < MAX_SCALE {
            value /= divisor;
            i += 1;
        }
    }
    let value = value.checked_mul(post)?;

    // `:144-160`: below 9.9 with DECIMAL set and at least one scaling done, the
    // rendering is `s1.s2`; otherwise it is the whole number rounded by adding
    // 50 and dropping the two extra digits.
    let text = if value < 995 && i > 0 && flags & HN_DECIMAL != 0 {
        if buf_len < baselen + 1 + 2 {
            return None;
        }
        let b = (value + 5) / 10;
        format!(
            "{}.{}{}{}{}",
            sign * (b / 10),
            b % 10,
            sep,
            table[i],
            suffix
        )
    } else {
        let whole = sign * ((value + 50) / 100);
        format!("{}{}{}{}", whole, sep, table[i], suffix)
    };

    Some(clipped(&text, buf_len))
}

/// Keep the first `buf_len - 1` bytes of `text`, the way `snprintf` fills a
/// buffer of that size and terminates it.
fn clipped(text: &str, buf_len: usize) -> String {
    if buf_len == 0 {
        return String::new();
    }
    let room = buf_len - 1;
    if text.len() <= room {
        return text.to_string();
    }
    // The renderings are ASCII, so a byte cut cannot split a character; guard
    // anyway so a non-ASCII suffix cannot panic.
    let keep: Vec<u8> = text.bytes().take(room).collect();
    String::from_utf8(keep).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact call `du -h` makes: five-byte buffer, empty suffix, autoscale,
    /// `HN_B | HN_NOSPACE | HN_DECIMAL` (`du.c:266-272`).
    fn du_h(bytes: i64) -> Option<String> {
        humanize_number(5, bytes, "", HN_AUTOSCALE, HN_B | HN_NOSPACE | HN_DECIMAL)
    }

    #[test]
    fn du_shape_renders_the_single_letter_units() {
        // 24 blocks of 512 bytes is what `du /etc` reads on the machine.
        assert_eq!(du_h(24 * 512), Some(String::from("12K")));
        assert_eq!(du_h(512), Some(String::from("512B")));
        assert_eq!(du_h(1024), Some(String::from("1.0K")));
        assert_eq!(du_h(1024 * 1024), Some(String::from("1.0M")));
        assert_eq!(du_h(1024 * 1024 * 1024), Some(String::from("1.0G")));
    }

    #[test]
    fn the_two_digit_headroom_is_what_decides_the_scale() {
        // The bound the scaling loop tests is `max - 50`, i.e. 99_950 for this
        // five-byte buffer, so 102_400 (one KiB) divides once more and lands on
        // the decimal form — 1 MiB minus 1 KiB rounds up to 1.0M rather than
        // printing 1023K.
        assert_eq!(du_h(100 * 1024), Some(String::from("100K")));
        assert_eq!(du_h(1024 * 1024 - 1024), Some(String::from("1.0M")));
        assert_eq!(du_h(1024 * 1024 * 1024 * 1024), Some(String::from("1.0T")));
    }

    #[test]
    fn a_fixed_scale_divides_exactly_that_many_times() {
        assert_eq!(
            humanize_number(10, 4096, "", 1, HN_B | HN_DECIMAL),
            Some(String::from("4.0 K")),
            "the table names the scale, and `B' only occupies scale zero"
        );
        assert_eq!(
            humanize_number(10, 4096, "", 2, HN_B),
            Some(String::from("0 M")),
            "without DECIMAL the value is rounded to a whole unit, and 4 KiB is \
             under half a MiB"
        );
    }

    #[test]
    fn hn_b_is_what_names_the_byte_and_the_default_separator_is_a_space() {
        assert_eq!(
            humanize_number(10, 512, "", 0, HN_AUTOSCALE | HN_B),
            Some(String::from("512 B"))
        );
        assert_eq!(
            humanize_number(10, 512, "", 0, HN_AUTOSCALE),
            Some(String::from("512 ")),
            "without HN_B a scale of zero carries no letter, and the separator is \
             still there: humanize_number.c:157"
        );
    }

    #[test]
    fn si_units_divide_by_a_thousand_and_spell_the_kilo_lower_case() {
        assert_eq!(
            humanize_number(10, 2000, "", 1, HN_B | HN_DIVISOR_1000 | HN_DECIMAL),
            Some(String::from("2.0 k")),
            "humanize_number.c:63 spells the SI kilo lower case"
        );
    }

    #[test]
    fn a_suffix_is_part_of_the_length_budget() {
        assert_eq!(
            humanize_number(10, 1024, "iB", 1, HN_NOSPACE | HN_DECIMAL),
            Some(String::from("1.0KiB"))
        );
        assert_eq!(
            humanize_number(4, 1024, "iB", 1, HN_NOSPACE | HN_DECIMAL),
            None,
            "`1.0KiB' plus a NUL does not fit four bytes"
        );
    }

    #[test]
    fn a_wide_rendering_is_clipped_and_a_decimal_one_is_refused() {
        // The non-decimal path has no room check of its own, so snprintf's clip
        // is what the caller sees: 123456789 bytes at scale 0 is `123456789B`,
        // and a five-byte buffer keeps the first four characters.
        assert_eq!(
            humanize_number(5, 123_456_789, "", 0, HN_B),
            Some(String::from("1234"))
        );
        // The decimal path checks room for `.N` first (humanize_number.c:146-148)
        // and returns -1, which for du means an empty field on the console.
        assert_eq!(
            humanize_number(3, 1024, "", 1, HN_B | HN_NOSPACE | HN_DECIMAL),
            None
        );
    }

    #[test]
    fn negative_values_keep_the_sign_and_share_the_same_budget() {
        assert_eq!(du_h(-24 * 512), Some(String::from("-12K")));
        assert_eq!(
            du_h(-512),
            None,
            "the sign costs one more byte of baselen, so the decimal branch's \
             room check fails: humanize_number.c:92"
        );
    }

    #[test]
    fn a_scale_beyond_the_table_and_an_empty_buffer_are_refused() {
        assert_eq!(humanize_number(10, 1024, "", 7, HN_B), None);
        assert_eq!(humanize_number(0, 1024, "", 0, HN_B), None);
    }

    #[test]
    fn the_unported_getscale_mode_is_refused_not_mis_shaped() {
        assert_eq!(
            humanize_number(5, 1024, "", HN_GETSCALE, HN_B),
            None,
            "HN_GETSCALE answers with a scale index rather than text; no caller \
             in this tree needs it yet"
        );
    }
}
