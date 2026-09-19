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

    /// MD5 本体的 RFC 1321 官方向量（铁锚：本体错则一切后端错）。
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
                let _ = core::fmt::write(&mut out, format_args!("{:02x}", b));
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
        // 跨块输入（RFC 1321 套件后两条：62 字节与 80 字节）。
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

    /// `$1$` 后端：宿主 `openssl passwd -1 -salt S PW` 生成的黄金向量
    /// （salt 超 8 字节按 C md5crypt.c:38-40 截断；magic 剥离；
    /// salt 内 `$` 终止）。六条覆盖长/短 salt 与不同口令。
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

    /// verify 闭环：重推导与 setting 一致即通过；错口令不通过。
    #[test]
    fn test_verify_round_trip() {
        let setting = "$1$saltstri$qQY4WxjABChYG1ccLpfkz/";
        assert!(verify(b"password", setting).unwrap());
        assert!(!verify(b"wrong", setting).unwrap());
    }

    /// 未移植后端如实报 Unsupported（SHA1 与 bcrypt 同类）；
    /// 传统 DES 走真实现。
    #[test]
    fn test_unsupported_schemes_reported() {
        // $2b$：minix3 的 C 只认 $2$ 与 $2a$（版本字符不得高于 '2'）。
        assert_eq!(
            crypt(b"pw", "$2b$12$abcdefghijklmnopqrstuv"),
            Err(CryptError::Invalid)
        );
        // SHA1：真实现（宿主 libcrypt 向量）。
        assert_eq!(
            crypt(b"password", "$sha1$10$ab").unwrap(),
            "$sha1$10$ab$jntRdwlglqBkw1n87bz5r/2ZrFcr"
        );
        // 传统 DES：口令前八字节 + 盐两位。
        assert_eq!(
            crypt(b"password", "Ja").unwrap(),
            "Ja/uBmQcgHDwg"
        );
        // 扩展 DES（`_` + 4 迭代字符 + 4 盐字符）。
        assert_eq!(
            crypt(b"password", "_./..abcd").unwrap(),
            "_./..abcdl2BMogUW5JA"
        );
    }
}

