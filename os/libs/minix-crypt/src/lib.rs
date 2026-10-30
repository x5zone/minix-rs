//! `minix-crypt` — password hash backends behind the `crypt(3)` dispatch.
//!
//! C correspondence: `minix3/lib/libcrypt/` — the scheme dispatch of
//! `__crypt`/`crypt` (crypt.c:505-523) and the four hash backends
//! (`md5crypt.c`, `crypt-sha1.c`, `bcrypt.c`+`blowfish.c`, and the
//! traditional DES inside crypt.c). The consumer is init's single-user
//! password gate (`password.rs`), which classifies `pw_passwd` schemes
//! and asks for a verification.
//!
//! Backend status (each is its own batch):
//! - **MD5** (`$1$…`): implemented (`md5crypt`), pinned against
//!   host-generated golden vectors.
//! - **traditional + extended DES**: implemented (`des`), pinned
//!   against host libcrypt vectors.
//! - **SHA1** (`$sha1$…`): implemented (`sha1`), pinned against host
//!   libcrypt vectors.
//! - **bcrypt** (`$2*$`): not yet ported — [`crypt`] reports
//!   [`CryptError::Unsupported`]. Callers (the init gate) keep denying
//!   that scheme, which is the pre-crate behavior and stays honest.

#![no_std]
// [SD19-gate] compile gate for the silent-swallow-error family: a newly added
// unjustified `let _ =` (dropping a `#[must_use]` return value) reports one
// extra warning on every clippy run, upgrading this family from
// "audit-discoverable" to "compile-discoverable". Not raised to `deny` (zero
// risk of a hard stop); deliberate discards get `//[SD19-gate] <reason>` plus a
// statement-level `#[allow]` at each site.
#![warn(clippy::let_underscore_must_use)]

extern crate alloc;

pub mod bcrypt;
pub mod des;
pub mod md5;
pub mod md5crypt;
pub mod sha1;

use alloc::string::String;

/// Base-sixty-four alphabet for crypt output (`util.c:12-13`'s
/// `itoa64`, shared by the MD5 and DES encoders).
pub(crate) const ITOA64: &[u8; 64] =
    b"./0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// Why a crypt round failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CryptError {
    /// The scheme exists in C libcrypt but is not ported yet.
    Unsupported,
    /// The setting is malformed (the C `__crypt` would return NULL).
    Invalid,
}

/// Verify a password against a setting (the stored hash), mirroring
/// `crypt(key, salt)`'s compare use: re-derive with the same setting and
/// compare byte-for-byte.
pub fn verify(key: &[u8], setting: &str) -> Result<bool, CryptError> {
    let derived = crypt(key, setting)?;
    Ok(derived == setting)
}

/// `crypt(3)` dispatch (C `__crypt`, crypt.c:508-520):
/// `$2*` → bcrypt, `$s…` → SHA1, `$1$`/other `$` → MD5,
/// `_…` → extended DES, other → traditional DES.
pub fn crypt(key: &[u8], setting: &str) -> Result<String, CryptError> {
    let sb = setting.as_bytes();
    if sb.first() == Some(&b'$') {
        return match sb.get(1) {
            Some(b'2') => bcrypt::bcrypt(key, sb).ok_or(CryptError::Invalid),
            Some(b's') => sha1::crypt_sha1(key, sb).ok_or(CryptError::Invalid),
            _ => Ok(md5crypt::md5crypt(key, sb)),
        };
    }
    if sb.first() == Some(&b'_') {
        return des::crypt_extended(key, sb).ok_or(CryptError::Invalid);
    }
    des::crypt_traditional(key, sb).ok_or(CryptError::Invalid)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The RFC 1321 official vectors for MD5 itself (the anchor: if the core
    /// is wrong, every backend is wrong).
    #[test]
    fn test_md5_rfc1321_vectors() {
        let digest = |data: &[u8]| {
            let mut ctx = md5::Md5::new();
            ctx.update(data);
            ctx.finalize()
        };
        let hex = |d: &[u8]| {
            let mut out = alloc::string::String::new();
            for b in d {
                use core::fmt::Write as _;
                // [SD19-gate] the hex formatter writing into a String (the
                // `hex` closure inside this host test): `String`'s
                // `write_fmt` (via the `core::fmt::Write` trait) returns Err
                // only on allocation failure (OOM), which is unrecoverable in
                // a test environment; the failure surfaces as a hex mismatch
                // in a later `assert_eq!`, so discarding the write error is a
                // deliberate shape.
                #[allow(clippy::let_underscore_must_use)]
                let _ = out.write_fmt(format_args!("{:02x}", b));
            }
            out
        };
        assert_eq!(hex(&digest(b"")), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(hex(&digest(b"a")), "0cc175b9c0f1b6a831c399e269772661");
        assert_eq!(hex(&digest(b"abc")), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            hex(&digest(b"message digest")),
            "f96b697d7cb7938d525a2f31aaf161d0"
        );
        assert_eq!(
            hex(&digest(b"abcdefghijklmnopqrstuvwxyz")),
            "c3fcd3d76192e4007dfb496cca67e13b"
        );
        // Cross-block input (the last two cases of the RFC 1321 suite: 62
        // bytes and 80 bytes).
        let long = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        assert_eq!(
            hex(&digest(long.as_bytes())),
            "d174ab98d277d9f5a5611c2c9f419d9f"
        );
        let digits = "1234567890".repeat(8);
        assert_eq!(
            hex(&digest(digits.as_bytes())),
            "57edf4a22be3c955ac49da2e2107b67a"
        );
    }

    /// The `$1$` backend: golden vectors generated on the host with
    /// `openssl passwd -1 -salt S PW` (salts longer than 8 bytes are truncated
    /// per C md5crypt.c:38-40; the magic is stripped; a `$` inside the salt
    /// terminates it). The six cases cover long/short salts and different
    /// passwords.
    #[test]
    fn test_md5crypt_golden_vectors_from_openssl() {
        let cases: [(&str, &str, &str); 6] = [
            ("password", "$1$saltstring", "$1$saltstri$qQY4WxjABChYG1ccLpfkz/"),
            ("Hello world!", "$1$saltstring", "$1$saltstri$YMyguxXMBpd2TEZ.vS/3q1"),
            ("password", "$1$salty", "$1$salty$SzJsU4qDcXp536Acnlp6I."),
            ("Hello world!", "$1$salty", "$1$salty$O57rhxWlsnlM9uzHrDN861"),
            ("password", "$1$ab", "$1$ab$oKsM6dtDD2L1bKowOBX.7."),
            ("Hello world!", "$1$ab", "$1$ab$zRhLhQlF.oQpGoCIApPRg1"),
        ];
        for (pw, setting, want) in cases {
            assert_eq!(
                md5crypt::md5crypt(pw.as_bytes(), setting.as_bytes()),
                want,
                "pw={pw:?} setting={setting:?}"
            );
        }
    }

    /// verify round trip: a re-derivation matching the setting passes; a wrong
    /// password does not.
    #[test]
    fn test_verify_round_trip() {
        let setting = "$1$saltstri$qQY4WxjABChYG1ccLpfkz/";
        assert!(verify(b"password", setting).unwrap());
        assert!(!verify(b"wrong", setting).unwrap());
    }

    /// Unported backends honestly report Unsupported (SHA1 and bcrypt alike);
    /// traditional DES goes through the real implementation.
    #[test]
    fn test_unsupported_schemes_reported() {
        // $2b$: minix3's C only accepts $2$ and $2a$ (the version character
        // must not exceed '2').
        assert_eq!(
            crypt(b"pw", "$2b$12$abcdefghijklmnopqrstuv"),
            Err(CryptError::Invalid)
        );
        // SHA1: the real implementation (host libcrypt vectors).
        assert_eq!(
            crypt(b"password", "$sha1$10$ab").unwrap(),
            "$sha1$10$ab$jntRdwlglqBkw1n87bz5r/2ZrFcr"
        );
        // Traditional DES: the first eight password bytes plus two salt
        // characters.
        assert_eq!(
            crypt(b"password", "Ja").unwrap(),
            "Ja/uBmQcgHDwg"
        );
        // Extended DES (`_` + 4 iteration characters + 4 salt characters).
        assert_eq!(
            crypt(b"password", "_./..abcd").unwrap(),
            "_./..abcdl2BMogUW5JA"
        );
    }
}

