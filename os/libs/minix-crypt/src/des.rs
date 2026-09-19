//! DES password scheme: the traditional two-salt-char form and the
//! extended `_` form with per-entry iteration counts.
//!
//! C correspondence: `minix3/lib/libcrypt/crypt.c` — the `__crypt` body
//! after the non-DES dispatch (lines 522-637), `des_setkey` (631-655)
//! and `des_cipher` (663-757). The C builds pre-rotated permutation
//! tables at run time (`init_des`, 762-847); this port uses the standard
//! FIPS tables directly and reproduces the same external behavior:
//!
//! - the key is the first eight password characters, each doubled
//!   (parity bit dropped, crypt.c:524-531);
//! - one [`des_cipher`] call chains `num_iter` 16-round DES passes
//!   without intermediate IP/FP (the C hoists IE3264/CF6464 out of the
//!   iteration loop, crypt.c:687-742);
//! - the Bell Labs salt perturbs every pass: salt bit `p` swaps bits
//!   `p` and `p+24` of the 48-bit expansion result (crypt.c:210-216);
//! - the 64 output bits are encoded as 4+4+3 base-sixty-four chars,
//!   high six-bit group first (crypt.c:598-621).
//!
//! Pure fixed-size arithmetic; no allocation outside the output string.

use alloc::string::String;
use alloc::vec::Vec;

use crate::ITOA64;

/// Width masks for the shifted-bit representations (56-bit PC1 output,
/// 48-bit round keys / expansions, 28-bit key halves). Written as shifts
/// rather than hex literals so the width is countable at a glance.
const MASK28: u64 = (1 << 28) - 1;
const MASK48: u64 = (1 << 48) - 1;
const MASK56: u64 = (1 << 56) - 1;

/// Standard DES tables (`crypt.c:322-417`; FP is the inverse of IP).
const IP: [u8; 64] = [
    58, 50, 42, 34, 26, 18, 10, 2,
    60, 52, 44, 36, 28, 20, 12, 4,
    62, 54, 46, 38, 30, 22, 14, 6,
    64, 56, 48, 40, 32, 24, 16, 8,
    57, 49, 41, 33, 25, 17, 9, 1,
    59, 51, 43, 35, 27, 19, 11, 3,
    61, 53, 45, 37, 29, 21, 13, 5,
    63, 55, 47, 39, 31, 23, 15, 7,
];

const FP: [u8; 64] = [
    40, 8, 48, 16, 56, 24, 64, 32,
    39, 7, 47, 15, 55, 23, 63, 31,
    38, 6, 46, 14, 54, 22, 62, 30,
    37, 5, 45, 13, 53, 21, 61, 29,
    36, 4, 44, 12, 52, 20, 60, 28,
    35, 3, 43, 11, 51, 19, 59, 27,
    34, 2, 42, 10, 50, 18, 58, 26,
    33, 1, 41, 9, 49, 17, 57, 25,
];

const EXPAND: [u8; 48] = [
    32, 1, 2, 3, 4, 5, 4, 5, 6, 7, 8, 9,
    8, 9, 10, 11, 12, 13, 12, 13, 14, 15, 16, 17,
    16, 17, 18, 19, 20, 21, 20, 21, 22, 23, 24, 25,
    24, 25, 26, 27, 28, 29, 28, 29, 30, 31, 32, 1,
];

const P32: [u8; 32] = [
    16, 7, 20, 21, 29, 12, 28, 17, 1, 15, 23, 26,
    5, 18, 31, 10, 2, 8, 24, 14, 32, 27, 3, 9,
    19, 13, 30, 6, 22, 11, 4, 25,
];

const PC1: [u8; 56] = [
    57, 49, 41, 33, 25, 17, 9, 1, 58, 50, 42, 34, 26, 18,
    10, 2, 59, 51, 43, 35, 27, 19, 11, 3, 60, 52, 44, 36,
    63, 55, 47, 39, 31, 23, 15, 7, 62, 54, 46, 38, 30, 22,
    14, 6, 61, 53, 45, 37, 29, 21, 13, 5, 28, 20, 12, 4,
];

// The BSD source table (`crypt.c:364-373`) carries eight per-row padding
// entries (9, 18, 22, 25, 35, 38, 43, 54) that make the rotated key
// schedule invertible; dropping them restores the standard 48-entry PC2.
const PC2: [u8; 48] = [
    14, 17, 11, 24, 1, 5, 3, 28, 15, 6, 21, 10,
    23, 19, 12, 4, 26, 8, 16, 7, 27, 20, 13, 2,
    41, 52, 31, 37, 47, 55, 30, 40, 51, 45, 33, 48,
    44, 49, 39, 56, 34, 53, 46, 42, 50, 36, 29, 32,
];

const ROTATES: [u8; 16] = [1, 1, 2, 2, 2, 2, 2, 2, 1, 2, 2, 2, 2, 2, 2, 1];

const SBOX: [u8; 512] = [
    // S-box 1
    14, 4, 13, 1, 2, 15, 11, 8, 3, 10, 6, 12, 5, 9, 0, 7,
    0, 15, 7, 4, 14, 2, 13, 1, 10, 6, 12, 11, 9, 5, 3, 8,
    4, 1, 14, 8, 13, 6, 2, 11, 15, 12, 9, 7, 3, 10, 5, 0,
    15, 12, 8, 2, 4, 9, 1, 7, 5, 11, 3, 14, 10, 0, 6, 13,
    // S-box 2
    15, 1, 8, 14, 6, 11, 3, 4, 9, 7, 2, 13, 12, 0, 5, 10,
    3, 13, 4, 7, 15, 2, 8, 14, 12, 0, 1, 10, 6, 9, 11, 5,
    0, 14, 7, 11, 10, 4, 13, 1, 5, 8, 12, 6, 9, 3, 2, 15,
    13, 8, 10, 1, 3, 15, 4, 2, 11, 6, 7, 12, 0, 5, 14, 9,
    // S-box 3
    10, 0, 9, 14, 6, 3, 15, 5, 1, 13, 12, 7, 11, 4, 2, 8,
    13, 7, 0, 9, 3, 4, 6, 10, 2, 8, 5, 14, 12, 11, 15, 1,
    13, 6, 4, 9, 8, 15, 3, 0, 11, 1, 2, 12, 5, 10, 14, 7,
    1, 10, 13, 0, 6, 9, 8, 7, 4, 15, 14, 3, 11, 5, 2, 12,
    // S-box 4
    7, 13, 14, 3, 0, 6, 9, 10, 1, 2, 8, 5, 11, 12, 4, 15,
    13, 8, 11, 5, 6, 15, 0, 3, 4, 7, 2, 12, 1, 10, 14, 9,
    10, 6, 9, 0, 12, 11, 7, 13, 15, 1, 3, 14, 5, 2, 8, 4,
    3, 15, 0, 6, 10, 1, 13, 8, 9, 4, 5, 11, 12, 7, 2, 14,
    // S-box 5
    2, 12, 4, 1, 7, 10, 11, 6, 8, 5, 3, 15, 13, 0, 14, 9,
    14, 11, 2, 12, 4, 7, 13, 1, 5, 0, 15, 10, 3, 9, 8, 6,
    4, 2, 1, 11, 10, 13, 7, 8, 15, 9, 12, 5, 6, 3, 0, 14,
    11, 8, 12, 7, 1, 14, 2, 13, 6, 15, 0, 9, 10, 4, 5, 3,
    // S-box 6
    12, 1, 10, 15, 9, 2, 6, 8, 0, 13, 3, 4, 14, 7, 5, 11,
    10, 15, 4, 2, 7, 12, 9, 5, 6, 1, 13, 14, 0, 11, 3, 8,
    9, 14, 15, 5, 2, 8, 12, 3, 7, 0, 4, 10, 1, 13, 11, 6,
    4, 3, 2, 12, 9, 5, 15, 10, 11, 14, 1, 7, 6, 0, 8, 13,
    // S-box 7
    4, 11, 2, 14, 15, 0, 8, 13, 3, 12, 9, 7, 5, 10, 6, 1,
    13, 0, 11, 7, 4, 9, 1, 10, 14, 3, 5, 12, 2, 15, 8, 6,
    1, 4, 11, 13, 12, 3, 7, 14, 10, 15, 6, 8, 0, 5, 9, 2,
    6, 11, 13, 8, 1, 4, 10, 7, 9, 5, 0, 15, 14, 2, 3, 12,
    // S-box 8
    13, 2, 8, 4, 6, 15, 11, 1, 10, 9, 3, 14, 5, 0, 12, 7,
    1, 15, 13, 8, 10, 3, 7, 4, 12, 5, 6, 11, 0, 14, 9, 2,
    7, 11, 4, 1, 9, 12, 14, 2, 0, 6, 10, 13, 15, 3, 5, 8,
    2, 1, 14, 7, 4, 10, 8, 13, 15, 12, 9, 0, 3, 5, 6, 11,
];

/// `ascii_to_bin` (crypt.c:476-488): base-sixty-four index for alphabet
/// members, the BSD arithmetic fallback for the rest.
fn ascii_to_bin(c: u8) -> u32 {
    if let Some(i) = ITOA64.iter().position(|&a| a == c) {
        return i as u32;
    }
    if c.is_ascii_lowercase() {
        u32::from(c - (b'a' - 38))
    } else if c.is_ascii_uppercase() {
        u32::from(c - (b'A' - 12))
    } else {
        u32::from(c - b'.')
    }
}

/// Permutation over the high bits of a word: output bit `i` is input
/// bit `tab[i]` (one-based, MSB-first), accumulated MSB-first.
fn permute(x: u64, tab: &[u8]) -> u64 {
    let mut out = 0u64;
    for &t in tab {
        out = (out << 1) | ((x >> (64 - u32::from(t))) & 1);
    }
    out
}

/// Sixteen round keys (`des_setkey`, crypt.c:631-655): PC1, per-half
/// rotations by `ROTATES`, PC2 — each key held in the low 48 bits.
fn key_schedule(key: &[u8; 8]) -> [u64; 16] {
    let mut block = 0u64;
    for &b in key {
        block = (block << 8) | u64::from(b);
    }
    let mut cd = permute(block, &PC1) & MASK56;
    let mut ks = [0u64; 16];
    for (r, &rot) in ROTATES.iter().enumerate() {
        let (c, d) = (cd >> 28, cd & MASK28);
        let rot = u32::from(rot);
        let c = ((c << rot) | (c >> (28 - rot))) & MASK28;
        let d = ((d << rot) | (d >> (28 - rot))) & MASK28;
        cd = (c << 28) | d;
        ks[r] = permute(cd << 8, &PC2) & MASK48;
    }
    ks
}

/// Round function: expansion, salt perturbation, key addition, the
/// eight S-boxes, and the P permutation.
fn feistel(r: u32, k: u64, salt: u32) -> u32 {
    // The 32-bit half is aligned to the top of the word for permute.
    let mut e = permute(u64::from(r) << 32, &EXPAND) & MASK48;
    for p in 0..24 {
        if (salt >> p) & 1 != 0 {
            // Salt bit p swaps expansion bits p and p+24 (0-based,
            // MSB-first) — the Bell Labs salt (crypt.c:210-216).
            let (i, j) = (47 - p, 23 - p);
            let bi = (e >> i) & 1;
            let bj = (e >> j) & 1;
            e = (e & !((1 << i) | (1 << j))) | (bi << j) | (bj << i);
        }
    }
    e ^= k;
    let mut out = 0u32;
    for box_index in 0..8 {
        let six = ((e >> (42 - 6 * box_index)) & 0x3f) as usize;
        let row = (((six >> 5) & 1) << 1) | (six & 1);
        let col = (six >> 1) & 0xf;
        let val = SBOX[box_index * 64 + row * 16 + col];
        out = (out << 4) | u32::from(val);
    }
    permute(u64::from(out) << 32, &P32) as u32
}

/// One `des_cipher` call (`crypt.c:663-757`): IP once, `iters` chained
/// 16-round passes each closed by the half swap, FP once.
fn des_cipher(block: u64, ks: &[u64; 16], salt: u32, iters: u32) -> u64 {
    let mut st = permute(block, &IP);
    for _ in 0..iters {
        let (mut l, mut r) = ((st >> 32) as u32, st as u32);
        for k in ks {
            let f = feistel(r, *k, salt);
            let next = r;
            r = l ^ f;
            l = next;
        }
        st = ((u64::from(r)) << 32) | u64::from(l);
    }
    permute(st, &FP)
}

/// Six-bit groups emitted high group first (crypt.c:598-621), four
/// chars per 24-bit word for the first two groups, three for the last.
fn encode(out: &mut Vec<u8>, b: &[u8; 8]) {
    let mut group = |word: u32, n: usize| {
        for shift in (0..n).rev() {
            out.push(ITOA64[((word >> (6 * shift)) & 0x3f) as usize]);
        }
    };
    group((u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]), 4);
    group((u32::from(b[3]) << 16) | (u32::from(b[4]) << 8) | u32::from(b[5]), 4);
    group(((u32::from(b[6]) << 8) | u32::from(b[7])) << 2, 3);
}

/// Key-block preparation (crypt.c:524-531): up to eight characters,
/// each doubled; the pointer stops advancing at a zero byte.
fn key_block(key: &[u8], consumed: &mut usize) -> [u8; 8] {
    let mut block = [0u8; 8];
    for slot in block.iter_mut() {
        let t = key.get(*consumed).copied().unwrap_or(0).wrapping_mul(2);
        if t != 0 {
            *consumed += 1;
        }
        *slot = t;
    }
    block
}

/// Traditional form: two salt chars, twenty-five passes
/// (crypt.c:545-552 salt assembly, 586-636 output).
pub fn crypt_traditional(key: &[u8], setting: &[u8]) -> Option<String> {
    if setting.len() < 2 {
        return None;
    }
    // ascii_is_unsafe (crypt.c:493-495): NUL, newline, colon.
    for &c in &setting[..2] {
        if c == 0 || c == b'\n' || c == b':' {
            return None;
        }
    }
    let mut consumed = 0;
    let kb = key_block(key, &mut consumed);
    let ks = key_schedule(&kb);
    let mut salt = 0u32;
    for i in (0..2).rev() {
        salt = (salt << 6) | ascii_to_bin(setting[i]);
    }
    let out64 = des_cipher(0, &ks, salt, 25).to_be_bytes();
    let mut out = Vec::with_capacity(13);
    out.extend_from_slice(&setting[..2]);
    encode(&mut out, &out64);
    Some(String::from_utf8(out).expect("crypt output is ascii"))
}

/// Extended `_` form: four iteration chars, four salt chars, and the
/// remaining key stirred into the key block eight characters at a time
/// (crypt.c:539-584).
pub fn crypt_extended(key: &[u8], setting: &[u8]) -> Option<String> {
    if setting.len() < 9 {
        return None;
    }
    let mut consumed = 0;
    let mut block = key_block(key, &mut consumed);
    let mut ks = key_schedule(&block);
    // Stir the rest of the password in, eight characters at a time
    // (crypt.c:539-556).
    while consumed < key.len() && key[consumed] != 0 {
        let stirred = des_cipher(u64::from_be_bytes(block), &ks, 0, 1);
        block = stirred.to_be_bytes();
        for slot in block.iter_mut() {
            let t = key.get(consumed).copied().unwrap_or(0).wrapping_mul(2);
            if t != 0 {
                consumed += 1;
            }
            *slot ^= t;
        }
        ks = key_schedule(&block);
    }
    // Iteration count and salt: four alphabet chars each, first char is
    // the high six bits (crypt.c:558-584).
    let mut num_iter = 0u32;
    for i in (0..4).rev() {
        let value = ascii_to_bin(setting[1 + i]);
        if ITOA64[value as usize] != setting[1 + i] {
            return None;
        }
        num_iter = (num_iter << 6) | value;
    }
    if num_iter == 0 {
        return None;
    }
    let mut salt = 0u32;
    for i in (0..4).rev() {
        let value = ascii_to_bin(setting[5 + i]);
        if ITOA64[value as usize] != setting[5 + i] {
            return None;
        }
        salt = (salt << 6) | value;
    }
    let out64 = des_cipher(0, &ks, salt, num_iter).to_be_bytes();
    let mut out = Vec::with_capacity(20);
    out.push(b'_');
    out.extend_from_slice(&setting[1..5]);
    out.extend_from_slice(&setting[5..9]);
    encode(&mut out, &out64);
    Some(String::from_utf8(out).expect("crypt output is ascii"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    /// Traditional form pinned against the host libcrypt.
    #[test]
    fn test_traditional_vectors() {
        let cases = [
            ("password", "..", "..UZoIyj/Hy/c"),
            ("password", "Ja", "Ja/uBmQcgHDwg"),
            ("password", "xx", "xxj31ZMTZzkVA"),
            ("abc", "ZZ", "ZZDHFQqZ8jOkk"),
            ("Hello world!", "fr", "frMrYEwqHe.bQ"),
            ("a", "/.", "/.n5Xb33Dkk1k"),
        ];
        for (pw, setting, want) in cases {
            assert_eq!(
                crypt_traditional(pw.as_bytes(), setting.as_bytes()),
                Some(want.to_string()),
                "pw={pw:?} setting={setting:?}"
            );
        }
    }

    /// Extended form pinned against the host libcrypt (small iteration
    /// counts so debug builds stay fast).
    #[test]
    fn test_extended_vectors() {
        let cases = [
            ("password", "_./..abcd", "_./..abcdl2BMogUW5JA"),
            ("Hello world!", "_./../abc", "_./../abccMJQGHRstns"),
            ("password", "_.0..abcd", "_.0..abcdP2c7ZLw14RM"),
            ("Hello world!", "_.0../abc", "_.0../abcON2RvjaCXFs"),
        ];
        for (pw, setting, want) in cases {
            assert_eq!(
                crypt_extended(pw.as_bytes(), setting.as_bytes()),
                Some(want.to_string()),
                "pw={pw:?} setting={setting:?}"
            );
        }
    }

    /// Malformed settings are rejected: short input, zero iterations.
    #[test]
    fn test_extended_rejects_invalid() {
        assert_eq!(crypt_extended(b"pw", b"_sh"), None);
        assert_eq!(crypt_extended(b"pw", b"_....abcd"), None);
    }
}


#[cfg(test)]
mod oracle_anchors {
    /// Intermediate values dumped from the instrumented C libcrypt
    /// (`des_setkey` round keys, one- and twenty-five-pass `des_cipher`
    /// outputs for key "password", zero salt) — pins the bit-level
    /// pipeline, not just the final strings.
    use super::*;
    #[test]
    fn c_oracle_intermediates() {
        let mut consumed = 0;
        let kb = key_block(b"password", &mut consumed);
        let ks = key_schedule(&kb);
        assert_eq!(ks[0], 0xefff_d7bb_2aa1, "ks0 mismatch");
        assert_eq!(ks[1], 0xaf57_7f33_6227, "ks1 mismatch");
        assert_eq!(feistel(0, ks[0], 0), 0x030b_5540, "f0");
        assert_eq!(
            permute(u64::from(0x1234_5678u32) << 32, &EXPAND) & MASK48,
            0x0a41_a82a_c3f0,
            "expand"
        );
        assert_eq!(feistel(0x1234_5678, ks[0], 0), 0x3ef8_7365, "fR");
        assert_eq!(des_cipher(0, &ks, 0, 1), 0xff66_062e_d77f_f70d, "out1");
        assert_eq!(des_cipher(0, &ks, 0, 25), 0x825d_14fa_f053_f81a, "out25");
    }
}
