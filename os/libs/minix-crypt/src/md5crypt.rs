//! MD5 password scheme (`$1$…`, the PHK `md5crypt`).
//!
//! C correspondence: `minix3/lib/libcrypt/md5crypt.c` (`__md5crypt`,
//! lines 47-147) over the `<md5.h>` context API. The algorithm's shape —
//! magic string, salt capping at eight bytes, the `MD5(pw,salt,pw)`
//! seeding, the odd/even byte tricks, the one-thousand-round stir, and
//! the six-group to-sixty-four encoding — is carried verbatim.

use alloc::string::String;
use alloc::vec::Vec;

use crate::md5::Md5;

/// Scheme magic (`MD5_MAGIC`, md5crypt.c:27).
pub const MD5_MAGIC: &str = "$1$";
/// Magic length (md5crypt.c:28).
const MD5_MAGIC_LEN: usize = 3;
/// Salt capping (md5crypt.c:38: at most eight characters before `$`).
const SALT_MAX: usize = 8;
/// Stir rounds (md5crypt.c:120).
const STIR_ROUNDS: u32 = 1000;

use crate::ITOA64;

/// Emit `n` base-sixty-four characters of `v` (`__crypt_to64`,
/// util.c:14-21).
fn to64(out: &mut Vec<u8>, mut v: u32, n: usize) {
    for _ in 0..n {
        out.push(ITOA64[(v & 0x3f) as usize]);
        v >>= 6;
    }
}

/// MD5 password encryption (`__md5crypt`, md5crypt.c:47-147).
///
/// `setting` may carry the magic prefix; the salt is everything after it
/// up to the first `$`, capped at eight bytes.
pub fn md5crypt(pw: &[u8], setting: &[u8]) -> String {
    // Refine the salt (md5crypt.c:57-64): skip the magic, stop at `$`,
    // cap at eight.
    let sp = if setting.len() >= MD5_MAGIC_LEN && &setting[..MD5_MAGIC_LEN] == MD5_MAGIC.as_bytes()
    {
        &setting[MD5_MAGIC_LEN..]
    } else {
        setting
    };
    let salt_end = sp
        .iter()
        .position(|&b| b == 0 || b == b'$')
        .unwrap_or(sp.len())
        .min(SALT_MAX);
    let salt = &sp[..salt_end];
    let sl = salt.len();
    let pwl = pw.len();

    let mut ctx = Md5::new();
    // The password first, since that is what is most unknown
    // (md5crypt.c:77-79).
    ctx.update(pw);
    // Then our magic string (md5crypt.c:82-84).
    ctx.update(MD5_MAGIC.as_bytes());
    // Then the raw salt (md5crypt.c:87-89).
    ctx.update(salt);

    // Then just as many characters of the MD5(pw,salt,pw)
    // (md5crypt.c:92-99).
    let mut ctx1 = Md5::new();
    ctx1.update(pw);
    ctx1.update(salt);
    ctx1.update(pw);
    let final_digest = ctx1.finalize();

    let mut pl = pwl as i64;
    while pl > 0 {
        let take = if pl > 16 { 16 } else { pl as usize };
        ctx.update(&final_digest[..take]);
        pl -= take as i64;
    }

    // Then something really weird... (md5crypt.c:104-112): the C zeroes
    // `final` first (md5crypt.c:83-84), so odd steps feed a zero byte.
    let mut i = pwl;
    while i != 0 {
        if i & 1 != 0 {
            ctx.update(&[0]);
        } else {
            ctx.update(&pw[..1]);
        }
        i >>= 1;
    }

    // Now make the output string (md5crypt.c:115-119): the main context's
    // digest becomes the starting `final` the stir below consumes.
    let mut final_digest = ctx.finalize();

    // And now, just to make sure things don't run too fast
    // (md5crypt.c:123-140).
    for i in 0..STIR_ROUNDS {
        let mut ctx1 = Md5::new();
        if i & 1 != 0 {
            ctx1.update(pw);
        } else {
            ctx1.update(&final_digest);
        }
        if i % 3 != 0 {
            ctx1.update(salt);
        }
        if i % 7 != 0 {
            ctx1.update(pw);
        }
        if i & 1 != 0 {
            ctx1.update(&final_digest);
        } else {
            ctx1.update(pw);
        }
        final_digest = ctx1.finalize();
    }

    // Encode: five 24-bit groups reading final[0,6,12], [1,7,13], …,
    // then a bare-byte group `l = final[11]` for the final two chars
    // (md5crypt.c:141-146).
    let mut out = Vec::with_capacity(MD5_MAGIC_LEN + sl + 1 + 22);
    out.extend_from_slice(MD5_MAGIC.as_bytes());
    out.extend_from_slice(salt);
    out.push(b'$');
    let group = |a: usize, b: usize, c: usize| -> u32 {
        ((final_digest[a] as u32) << 16)
            | ((final_digest[b] as u32) << 8)
            | (final_digest[c] as u32)
    };
    for (word, n) in [
        (group(0, 6, 12), 4),
        (group(1, 7, 13), 4),
        (group(2, 8, 14), 4),
        (group(3, 9, 15), 4),
        (group(4, 10, 5), 4),
        (final_digest[11] as u32, 2),
    ] {
        to64(&mut out, word, n);
    }
    String::from_utf8(out).expect("crypt output is ascii")
}
