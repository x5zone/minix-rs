//! SHA-1 message digest and the HMAC-SHA1 password scheme.
//!
//! C correspondence: the digest consumed by `minix3/lib/libcrypt/` is
//! the FIPS 180-1 algorithm; the password scheme is `crypt-sha1.c`
//! (`__crypt_sha1`, lines 100-198) — PBKDF1 with HMAC-SHA1 as the
//! iteration function (per the file's own comment, lines 65-67):
//!
//! ```text
//! digest_0 = HMAC_SHA1(pw, salt || "$sha1$" || iterations)
//! digest_i = HMAC_SHA1(pw, digest_{i-1})        // i = 1 .. iterations-1
//! ```
//!
//! and the output is `"$sha1$" iterations "$" salt "$"` followed by the
//! twenty digest bytes in six-bit groups (`crypt-sha1.c:178-192`; the
//! last group wraps around to byte 0, line 187-190).

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::ITOA64;

/// Digest length in bytes (`SHA1_SIZE`, crypt.h:19).
pub const DIGEST_LENGTH: usize = 20;

/// FIPS 180-1 SHA-1 over a fixed-size buffer (inputs here are short:
/// salts, keys, and 20-byte digests).
pub fn sha1(data: &[u8]) -> [u8; DIGEST_LENGTH] {
    let mut h: [u32; 5] = [
        0x6745_2301,
        0xEFCD_AB89,
        0x98BA_DCFE,
        0x1032_5476,
        0xC3D2_E1F0,
    ];
    // Padding: 0x80, zeros, 64-bit big-endian bit count.
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut msg = Vec::with_capacity(data.len() + 72);
    msg.extend_from_slice(data);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 80];
        for (i, word) in w.iter_mut().enumerate().take(16) {
            *word = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for (i, &wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            // FIPS 180-1: TEMP = S5(A) + f + E + K + W; then E=D, D=C,
            // C=S30(B), B=A, A=TEMP.
            let tmp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = tmp;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }
    let mut digest = [0u8; DIGEST_LENGTH];
    for (i, word) in h.iter().enumerate() {
        digest[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    digest
}

/// HMAC-SHA1 (RFC 2104): block size sixty-four.
pub fn hmac_sha1(key: &[u8], data: &[u8]) -> [u8; DIGEST_LENGTH] {
    const BLOCK: usize = 64;
    let mut key_block = [0u8; BLOCK];
    if key.len() > BLOCK {
        key_block[..DIGEST_LENGTH].copy_from_slice(&sha1(key));
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }
    let mut inner = Vec::with_capacity(BLOCK + data.len());
    let mut outer = Vec::with_capacity(BLOCK + DIGEST_LENGTH);
    let mut ipad = [0x36u8; BLOCK];
    let mut opad = [0x5cu8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] ^= key_block[i];
        opad[i] ^= key_block[i];
    }
    inner.extend_from_slice(&ipad);
    inner.extend_from_slice(data);
    outer.extend_from_slice(&opad);
    outer.extend_from_slice(&sha1(&inner));
    sha1(&outer)
}

/// SHA1 password scheme (`__crypt_sha1`, crypt-sha1.c:100-198).
///
/// `setting` must carry the `$sha1$iterations$salt[$]` form; the C's
/// magic-less branch picks a random iteration count for hash *generation*
/// (crypt-sha1.c:114) and is not reproducible, so it is rejected here.
pub fn crypt_sha1(pw: &[u8], setting: &[u8]) -> Option<String> {
    const MAGIC: &[u8] = b"$sha1$";
    let rest = setting.strip_prefix(MAGIC)?;
    // Iterations: decimal digits up to '$' (crypt-sha1.c:145-148).
    let digits_len = rest.iter().position(|&b| b == b'$')?;
    let iterations: u32 = core::str::from_utf8(&rest[..digits_len])
        .ok()?
        .parse()
        .ok()?;
    let salt = &rest[digits_len + 1..];
    let salt_end = salt.iter().position(|&b| b == b'$').unwrap_or(salt.len());
    let salt = &salt[..salt_end];

    // Prime the pump with <salt><magic><iterations> (crypt-sha1.c:164-173),
    // then iterate HMAC with the password as key.
    let mut msg = Vec::with_capacity(salt.len() + MAGIC.len() + 10);
    msg.extend_from_slice(salt);
    msg.extend_from_slice(MAGIC);
    msg.extend_from_slice(&iterations.to_string().into_bytes());
    let mut digest = hmac_sha1(pw, &msg);
    for _ in 1..iterations {
        digest = hmac_sha1(pw, &digest);
    }

    // Output: magic, iterations, salt, then the digest in six-bit groups
    // with the last group reading bytes 18, 19, 0 (crypt-sha1.c:176-192).
    let mut out = Vec::with_capacity(MAGIC.len() + 10 + salt.len() + 1 + 28);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&iterations.to_string().into_bytes());
    out.push(b'$');
    out.extend_from_slice(salt);
    out.push(b'$');
    for start in (0..DIGEST_LENGTH - 3).step_by(3) {
        to64(
            &mut out,
            (u32::from(digest[start]) << 16)
                | (u32::from(digest[start + 1]) << 8)
                | u32::from(digest[start + 2]),
            4,
        );
    }
    to64(
        &mut out,
        (u32::from(digest[DIGEST_LENGTH - 2]) << 16)
            | (u32::from(digest[DIGEST_LENGTH - 1]) << 8)
            | u32::from(digest[0]),
        4,
    );
    Some(String::from_utf8(out).expect("crypt output is ascii"))
}

/// Emit `n` base-sixty-four characters, low six bits first
/// (`__crypt_to64`, util.c:14-21).
fn to64(out: &mut Vec<u8>, mut v: u32, n: usize) {
    for _ in 0..n {
        out.push(ITOA64[(v & 0x3f) as usize]);
        v >>= 6;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    fn hex(d: &[u8]) -> String {
        let mut out = String::new();
        for b in d {
            out.push(core::char::from_digit(u32::from(b >> 4), 16).expect("hex digit"));
            out.push(core::char::from_digit(u32::from(b & 0xf), 16).expect("hex digit"));
        }
        out
    }

    /// FIPS 180-1 corpus (NIST vectors).
    #[test]
    fn test_sha1_vectors() {
        assert_eq!(
            hex(&sha1(b"")),
            "da39a3ee5e6b4b0d3255bfef95601890afd80709"
        );
        assert_eq!(
            hex(&sha1(b"abc")),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            hex(&sha1(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")),
            "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
        );
    }

    /// RFC 2202 test cases 1 and 2 for HMAC-SHA1.
    #[test]
    fn test_hmac_sha1_vectors() {
        let key = [0x0bu8; 20];
        assert_eq!(
            hex(&hmac_sha1(&key, b"Hi There")),
            "b617318655057264e28bc0b6fb378c8ef146be00"
        );
        assert_eq!(
            hex(&hmac_sha1(b"Jefe", b"what do ya want for nothing?")),
            "effcdf6ae5eb2fa2d27416d5f184df9c259a7c79"
        );
    }

    /// End-to-end scheme vectors from the host libcrypt.
    #[test]
    fn test_crypt_sha1_vectors() {
        let cases = [
            (
                "password",
                "$sha1$1000$Wv9C",
                "$sha1$1000$Wv9C$k6AiAuZWKnvgEA8ueukQNPt5s8n0",
            ),
            (
                "Hello world!",
                "$sha1$24681$abcd",
                "$sha1$24681$abcd$wE2YOPyWahNQ5ROtXjvVwUCzEusk",
            ),
            (
                "password",
                "$sha1$10$ab",
                "$sha1$10$ab$jntRdwlglqBkw1n87bz5r/2ZrFcr",
            ),
        ];
        for (pw, setting, want) in cases {
            assert_eq!(
                crypt_sha1(pw.as_bytes(), setting.as_bytes()),
                Some(want.to_string()),
                "pw={pw:?} setting={setting:?}"
            );
        }
    }
}
