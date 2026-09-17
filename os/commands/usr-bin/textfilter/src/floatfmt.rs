//! no_std float helpers shared by `jot` and `units` for their
//! printf-style `%f`/`%e`/`%g` rendering.
//!
//! The workspace carries no `libm`; the three primitives the C
//! programs lean on — floor, the decimal exponent of a normalized
//! value, and powers of ten — are provided here. The decimal
//! exponent comes from core's own `{:e}` formatting, so it is exact
//! with respect to the printed digits. Powers of ten use the
//! exactly-representable literals 1e0..1e22 and fold the rest, which
//! matches double arithmetic closely enough for `%.*g` at the
//! precisions these tools use (8 significant digits). NaN handling
//! beyond these paths is a registered corner.

use alloc::format;

/// Exact powers of ten up to 1e22 (the last one exactly representable).
const POW10: [f64; 23] = [
    1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10, 1e11, 1e12, 1e13, 1e14, 1e15, 1e16,
    1e17, 1e18, 1e19, 1e20, 1e21, 1e22,
];

/// floor() toward negative infinity (libm-free). Values beyond i64
/// saturate through the truncating cast, like a narrowing conversion.
pub fn floor(x: f64) -> f64 {
    let truncated = x as i64 as f64;
    if x < truncated {
        truncated - 1.0
    } else {
        truncated
    }
}

/// The decimal exponent floor(log10(|v|)) of a normalized value,
/// taken from core's scientific formatting (mantissa in [1, 10)).
pub fn decimal_exponent(v: f64) -> i32 {
    if v == 0.0 || v.is_nan() {
        return 0;
    }
    let scientific = format!("{:e}", v);
    match scientific.find('e') {
        Some(pos) => scientific[pos + 1..].parse().unwrap_or(0),
        None => 0,
    }
}

/// 10^exp for the range the `%g` paths exercise (exponent ±308).
pub fn pow10(exp: i32) -> f64 {
    let mut result = 1.0f64;
    let mut e = exp;
    while e >= 22 {
        result *= 1e22;
        e -= 22;
    }
    while e <= -22 {
        result /= 1e22;
        e += 22;
    }
    if e >= 0 {
        result * POW10[e as usize]
    } else {
        result / POW10[(-e) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_floor_direction() {
        assert_eq!(floor(2.7), 2.0);
        assert_eq!(floor(-2.1), -3.0);
        assert_eq!(floor(5.0), 5.0);
    }

    #[test]
    fn test_decimal_exponent() {
        assert_eq!(decimal_exponent(1.0), 0);
        assert_eq!(decimal_exponent(999.0), 2);
        assert_eq!(decimal_exponent(0.0001), -4);
        assert_eq!(decimal_exponent(0.00001), -5);
        assert_eq!(decimal_exponent(6.0e23), 23);
    }

    #[test]
    fn test_pow10_round_trip() {
        assert!((pow10(3) - 1000.0).abs() < 1e-9);
        assert!((pow10(-3) - 0.001).abs() < 1e-12);
        assert!((pow10(30) - 1e30).abs() < 1e21);
    }
}

/// Parses a leading float the way `sscanf("%lf")`/`strtod` do: any
/// numeric prefix converts (so `12abc` yields 12.0, consuming 2);
/// only a string with no leading number fails. Returns the value and
/// the consumed byte count. Hex floats are a registered corner.
pub fn parse_float_prefix(s: &str) -> Option<(f64, usize)> {
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let int_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let mut frac_len = 0;
    if i < b.len() && b[i] == b'.' {
        i += 1;
        let fs = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        frac_len = i - fs;
    }
    if i == int_start && frac_len == 0 {
        return None;
    }
    // Optional exponent; it only counts when digits follow.
    let mut end = i;
    if i < b.len() && (b[i] | 0x20) == b'e' {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        let ds = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > ds {
            end = j;
        }
    }
    s[..end].parse::<f64>().ok().map(|v| (v, end))
}
