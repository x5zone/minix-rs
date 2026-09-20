//! Root password verification for the single-user gate (ARCH A-12).
//!
//! Covers the C dispatch at `minix3/lib/libcrypt/crypt.c:505-520` and
//! the gate's passwd read (`getpwnam("root")`, init.c:733).
//! Design contract: `.design/04-design.v1.md §1.1`, doc 12.
//!
//! Scheme dispatch and the four hash backends both live behind
//! `minix-crypt` (the shared-infrastructure split libcrypt itself
//! makes): `crypt(3)` re-derives with the stored setting and compares,
//! which is exactly `verify`'s shape here.


use alloc::{boxed::Box, string::String, string::ToString, vec::Vec};

/// The gate's password verifier: re-derive with the stored setting and
/// compare — the `crypt(3)` shape. Owned so it can capture the hash.
pub type RootVerifier = Box<dyn Fn(&str) -> bool>;

/// The parsed `pw_passwd` of root (C: `pp->pw_passwd`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasswordHash {
    /// Empty string — no password, the gate does not run (C:
    /// `*pp->pw_passwd == '\0'`, init.c:750).
    Empty,
    /// `*` or `!` — the account is locked; nothing verifies.
    Locked,
    /// Traditional DES: two salt chars then 11 hash chars.
    Descrypt { salt: String },
    /// `$1$salt$` — MD5 scheme.
    Md5,
    /// `$2*$` — bcrypt scheme.
    Bcrypt,
    /// `$sha1$` — SHA-1 scheme.
    Sha1,
}

/// Classify a `pw_passwd` (C: `__crypt`'s dispatch,
/// crypt.c:508-520).
pub fn parse_password_hash(pw_passwd: &str) -> PasswordHash {
    if pw_passwd.is_empty() {
        return PasswordHash::Empty;
    }
    if pw_passwd == "*" || pw_passwd == "!" {
        return PasswordHash::Locked;
    }
    if let Some(rest) = pw_passwd.strip_prefix('$') {
        return match rest.as_bytes().first() {
            Some(b'2') => PasswordHash::Bcrypt,
            Some(b's') => PasswordHash::Sha1,
            // C: any other $-prefix defaults to MD5 (crypt.c:515-518).
            _ => PasswordHash::Md5,
        };
    }
    // DES settings are the two leading salt characters.
    let salt_len = pw_passwd.chars().take(2).count().min(2);
    PasswordHash::Descrypt {
        salt: pw_passwd[..salt_len].to_string(),
    }
}

/// Whether the scheme has a hash backend in this build. Locked
/// accounts and empty passwords are decided without one. All four
/// libcrypt schemes (DES, MD5, SHA1, bcrypt) are ported in
/// `minix-crypt`, so every variant is covered.
pub fn scheme_has_backend(hash: &PasswordHash) -> bool {
    matches!(
        hash,
        PasswordHash::Empty
            | PasswordHash::Locked
            | PasswordHash::Descrypt { .. }
            | PasswordHash::Md5
            | PasswordHash::Bcrypt
            | PasswordHash::Sha1
    )
}

/// Extract root's password hash from an `/etc/passwd` body (C:
/// `getpwnam("root")`, init.c:733).
///
/// `None` when no root line exists — the C shape where the gate does
/// not run at all.
pub fn root_password_hash(passwd_body: &str) -> Option<String> {
    for line in passwd_body.lines() {
        let fields: Vec<&str> = line.split(':').collect();
        if fields.len() >= 2 && fields[0] == "root" {
            return Some(fields[1].to_string());
        }
    }
    None
}

/// Build the gate's verifier over a hash (the `verify_password` seed
/// for doc 04's deps). Locked accounts deny everything; real schemes
/// re-derive through `minix-crypt::verify` — the `crypt(3)` compare —
/// and a backend error also denies (the safe direction: a wrong answer
/// costs the admin a ^D, never an unauthorized shell).
pub fn build_verifier(hash: &str) -> Option<RootVerifier> {
    match parse_password_hash(hash) {
        PasswordHash::Empty => None,
        PasswordHash::Locked => Some(Box::new(|_| false)),
        _ => {
            let stored = hash.to_string();
            Some(Box::new(move |password: &str| {
                matches!(
                    minix_crypt::verify(password.as_bytes(), &stored),
                    Ok(true)
                )
            }))
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_hash_is_no_gate() {
        assert_eq!(parse_password_hash(""), PasswordHash::Empty);
        assert!(build_verifier("").is_none());
    }

    #[test]
    fn test_locked_hash_denies_everything() {
        for locked in ["*", "!"] {
            assert_eq!(parse_password_hash(locked), PasswordHash::Locked);
            let verify = build_verifier(locked).unwrap();
            assert!(!verify("anything"));
        }
    }

    #[test]
    fn test_des_salt_is_two_leading_chars() {
        assert_eq!(
            parse_password_hash("xyAjYtmfRYx/."),
            PasswordHash::Descrypt { salt: "xy".into() }
        );
    }

    #[test]
    fn test_non_des_prefixes_dispatch() {
        assert_eq!(parse_password_hash("$1$ab$"), PasswordHash::Md5);
        assert_eq!(parse_password_hash("$2b$12$"), PasswordHash::Bcrypt);
        assert_eq!(parse_password_hash("$sha1$"), PasswordHash::Sha1);
    }

    #[test]
    fn test_root_line_extraction() {
        let body = "root:Vvura9AdWCkic:0:0:root:/root:/bin/sh\ndaemon:*:1:1:daemon:/:\n";
        assert_eq!(
            root_password_hash(body).as_deref(),
            Some("Vvura9AdWCkic")
        );
        assert_eq!(root_password_hash("daemon:*:1:1:daemon:/"), None);
    }

    /// 真装闭环：四方案正确口令放行、错误口令拒绝（黄金向量来自
    /// 宿主 libcrypt）。
    #[test]
    fn test_build_verifier_round_trip_all_schemes() {
        let cases = [
            ("password", "xyAjYtmfRYx/."), // 传统 DES（宿主 libcrypt 真值）
            ("password", "$1$saltstri$qQY4WxjABChYG1ccLpfkz/"), // MD5
            ("password", "$sha1$10$ab$jntRdwlglqBkw1n87bz5r/2ZrFcr"), // SHA1
            (
                "password",
                "$2a$05$abcdefghijklmnopqrstuuWG29KuyeAicPCJODk1zjyGvyQUU2awu",
            ), // bcrypt
        ];
        for (password, stored) in cases {
            let verify = build_verifier(stored).expect("verifier");
            assert!(verify(password), "correct password denied: {stored:?}");
            assert!(
                !verify("wrong-password"),
                "wrong password accepted: {stored:?}"
            );
        }
    }

    /// 后端报错（畸形 setting）按 deny 处理，绝不放行。
    #[test]
    fn test_malformed_setting_denies() {
        let verify = build_verifier("$1$").expect("verifier");
        assert!(!verify("anything"));
    }
}
