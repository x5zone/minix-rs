//! The digest core of `cksum -a`: all eight entries of C's `hashes[]` table are
//! wired up — MD2, MD4, MD5, RMD160, SHA1, SHA256, SHA384, SHA512.
//!
//! Two of the families are **not rewritten here**, because this repository
//! already has them: [`md5`](minix_crypt::md5) and [`sha1`](minix_crypt::sha1)
//! in `os/libs/minix-crypt`, whose ground truth is likewise the original C in
//! this repository — `minix3/common/lib/libc/md/md5c.c` (`MD5Transform`) and
//! `minix3/common/lib/libc/hash/sha1/sha1.c` (`SHA1Transform`). Two MD5 copies
//! in one workspace is a net loss: it makes people ask "which one is real", a
//! question that should not exist. This module therefore does only two things:
//! **wire up what already exists**, and **add what is genuinely missing**. The
//! remaining four are newly translated here, each matched to its source:
//!
//! * MD4 — `MD4Transform` in `minix3/common/lib/libc/md/md4c.c`;
//! * SHA256, SHA384, SHA512 — `SHA256_Transform` and `SHA512_Transform` in
//!   `minix3/common/lib/libc/hash/sha2/sha2.c` (SHA384 and SHA512 share the
//!   latter core, differing only in the initial values and a 16-byte tail
//!   truncation, exactly as `sha2.c` itself writes them).
//!
//! The last two have independent structures in their sources: MD2 follows
//! `minix3/lib/libc/hash/md2/md2.c` (its 256-entry S table and 18-round mixing
//! step are the RFC 1319 set; the source calls itself
//! `cut-n-pasted from rfc1319`; it does not follow the "64-byte block plus
//! trailing bit count" skeleton but works in 32-byte blocks, computing a
//! checksum first and folding it in with the message), and RMD160 follows
//! `minix3/common/lib/libc/hash/rmd160/rmd160.c` (two lines of 80 rounds each;
//! the only family whose right line uses its function groups **in reverse**
//! while the constant groups stay forward — see the table comments below).
//!
//! Wiring several families into one seam is worth more than a few saved
//! implementations: **the vector gate now covers every wired family at once**,
//! while the two in `minix-crypt` did not carry equally strong criteria —
//! `sha1.rs` has `test_sha1_vectors`, `md5.rs` has not a single test
//! (`grep -c "#\[test\]" os/libs/minix-crypt/src/md5.rs` → 0 today), and both
//! sets of vectors use short inputs. The lengths at the end of this module,
//! taken at 64/128-byte block boundaries plus the one-million-byte case, cover
//! exactly the shapes short vectors cannot: cross-block folding and padding
//! that spills into another block.
//!
//! All families share the same skeleton: cut the input into whole blocks, append
//! "one `0x80`, some `0x00`s, the trailing bit count", and fold each block into
//! a state vector. Order is load-bearing here — the `0x80` must immediately
//! follow the last data byte, and the bit count must sit at the very end of the
//! last block. Writing the length as "the total after padding", or putting the
//! `0x80` at the other end of the zero padding, does not raise an error; it just
//! turns every input into a different digest. Such defects can only be pinned by
//! known-input → known-digest pairs. The families genuinely disagree in only
//! three places, all about "the trailing field" and "how the state is spread
//! into bytes":
//!
//! * Block length: the MD family and SHA-256 use 64 bytes; SHA-384/512 use 128.
//! * The bit-count field: the MD family records 8 bytes **little-endian**
//!   (`Encode` in `md5c.c`/`md4c.c` writes from the low byte), SHA-1/256 record
//!   8 bytes big-endian (`SHA1Final`'s comment calls itself
//!   `Endian independent`; it takes bytes via `(3-(i&3))*8` shifts), and SHA-512
//!   records **16 bytes** big-endian (`sha2.c` splits the 128-bit counter into
//!   two `u64`s; for short inputs the high 8 bytes are always zero).
//! * Spreading the digest into bytes: the MD family goes little-endian, the SHA
//!   family big-endian; SHA384 takes the first 48 bytes of SHA512's 64.
//!
//! In other words, the same input must produce the same hex string on
//! x86-64/AArch64/RISC-V — byte order is **prescribed by the algorithm**, not by
//! the machine, so this module uses only explicit `to_be_bytes`/`from_le_bytes`
//! and never leaves it to the host.
//!
//! The reporting half (`MD5 (file) = <digest>` with its `-n`/`-q` variants, and
//! the standard-input leg) lives in `src/bin/cksum.rs`; the formatting criterion
//! is C's `End` (`minix3/lib/libc/md/mdXhl.c`,
//! `minix3/lib/libc/hash/hashhl.c`): per byte, high nibble first then low, looked
//! up in `0123456789abcdef`, **lowercase**, always `2 * digest bytes` long.

use alloc::string::String;

/// `sha2.c`'s SHA-256 constant table (64 entries, the "Hash constant words K
/// for SHA-256" one).
/// Copied mechanically from the source, not recomputed from `floor(2^32 ·
/// frac(p))` for the first eight primes — recomputation would blur "copied
/// wrong" and "the algorithm is like this", and the source file is itself the
/// criterion.
static SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// `sha2.c`'s `sha256_initial_hash_value[8]`, the initial values `SHA256_Init`
/// loads into the state. The SHA512 family's eight initial values differ and are
/// 64-bit words; they live in the next block.
static SHA256_H0: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// The message-word indices from `md4c.c`'s 48 `FF`/`GG`/`HH` lines, taken
/// mechanically from the source (same reason as `SHA256_K`).
static MD4_WORD: [usize; 48] = [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
    0x00, 0x04, 0x08, 0x0c, 0x01, 0x05, 0x09, 0x0d, 0x02, 0x06, 0x0a, 0x0e, 0x03, 0x07, 0x0b, 0x0f,
    0x00, 0x08, 0x04, 0x0c, 0x02, 0x0a, 0x06, 0x0e, 0x01, 0x09, 0x05, 0x0d, 0x03, 0x0b, 0x07, 0x0f,
];

/// The rotation amounts of those same 48 lines (`md4c.c`'s `S11`…`S34`).
static MD4_SHIFT: [u32; 48] = [
    0x03, 0x07, 0x0b, 0x13, 0x03, 0x07, 0x0b, 0x13, 0x03, 0x07, 0x0b, 0x13, 0x03, 0x07, 0x0b, 0x13,
    0x03, 0x05, 0x09, 0x0d, 0x03, 0x05, 0x09, 0x0d, 0x03, 0x05, 0x09, 0x0d, 0x03, 0x05, 0x09, 0x0d,
    0x03, 0x09, 0x0b, 0x0f, 0x03, 0x09, 0x0b, 0x0f, 0x03, 0x09, 0x0b, 0x0f, 0x03, 0x09, 0x0b, 0x0f,
];

/// The four initial values of `md4c.c`'s `MD4Init`. They equal MD5's four; both
/// C sources write them out separately, so both are written separately here.
static MD4_H0: [u32; 4] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476];

// All five tables below are extracted mechanically from the C sources by
// `tmp/cksum-384/gen_tables3.py`: MD2's S table from
// `minix3/lib/libc/hash/md2/md2.c:51-70`, and RMD160's four from the 8th and
// 9th arguments of that file's 160 `R(...)` lines (rotation amount and
// message-word index). The generator also verified that "change the
// function/constant group every 16 rounds" and "cycle the target register as
// a,e,d,c,b" both hold on **both lines**, so below the group number is just
// `i / 16` and the target register is `(-i) mod 5`; there is no need to copy a
// separate 80-entry group table.
// The one counter-intuitive thing about the right line: its **function** groups
// run backwards (rounds 0..15 use `F4`, the last 16 use `F0`), while the
// **constant** groups run forwards (`KK0 → KK4`) — this was read from
// `rmd160.c`'s lines, not from a textbook.
/// The 256-entry S table at `md2.c:51-70` (the source calls itself
/// `cut-n-pasted from rfc1319`).
static MD2_S: [u8; 256] = [
    41, 46, 67, 201, 162, 216, 124, 1, 61, 54, 84, 161, 236, 240, 6, 19, 98, 167, 5, 243, 192, 199,
    115, 140, 152, 147, 43, 217, 188, 76, 130, 202, 30, 155, 87, 60, 253, 212, 224, 22, 103, 66,
    111, 24, 138, 23, 229, 18, 190, 78, 196, 214, 218, 158, 222, 73, 160, 251, 245, 142, 187, 47,
    238, 122, 169, 104, 121, 145, 21, 178, 7, 63, 148, 194, 16, 137, 11, 34, 95, 33, 128, 127, 93,
    154, 90, 144, 50, 39, 53, 62, 204, 231, 191, 247, 151, 3, 255, 25, 48, 179, 72, 165, 181, 209,
    215, 94, 146, 42, 172, 86, 170, 198, 79, 184, 56, 210, 150, 164, 125, 182, 118, 252, 107, 226,
    156, 116, 4, 241, 69, 157, 112, 89, 100, 113, 135, 32, 134, 91, 207, 101, 230, 45, 168, 2, 27,
    96, 37, 173, 174, 176, 185, 246, 28, 70, 97, 105, 52, 64, 126, 15, 85, 71, 163, 35, 221, 81,
    175, 58, 195, 92, 249, 206, 186, 197, 234, 38, 44, 83, 13, 110, 133, 40, 132, 9, 211, 223, 205,
    244, 65, 129, 77, 82, 106, 220, 55, 200, 108, 193, 171, 250, 36, 225, 123, 8, 12, 189, 177, 74,
    120, 136, 149, 139, 227, 99, 232, 109, 233, 203, 213, 254, 59, 0, 29, 57, 242, 239, 183, 14,
    102, 88, 208, 228, 166, 119, 114, 248, 235, 117, 75, 10, 49, 68, 80, 180, 143, 237, 31, 26,
    219, 153, 141, 51, 159, 17, 131, 20,
];

static RMD_LEFT_WORD: [usize; 80] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 7, 4, 13, 1, 10, 6, 15, 3, 12, 0, 9, 5,
    2, 14, 11, 8, 3, 10, 14, 4, 9, 15, 8, 1, 2, 7, 0, 6, 13, 11, 5, 12, 1, 9, 11, 10, 0, 8, 12, 4,
    13, 3, 7, 15, 14, 5, 6, 2, 4, 0, 5, 9, 7, 12, 2, 10, 14, 1, 3, 8, 11, 6, 15, 13,
];

static RMD_LEFT_SHIFT: [u32; 80] = [
    11, 14, 15, 12, 5, 8, 7, 9, 11, 13, 14, 15, 6, 7, 9, 8, 7, 6, 8, 13, 11, 9, 7, 15, 7, 12, 15,
    9, 11, 7, 13, 12, 11, 13, 6, 7, 14, 9, 13, 15, 14, 8, 13, 6, 5, 12, 7, 5, 11, 12, 14, 15, 14,
    15, 9, 8, 9, 14, 5, 6, 8, 6, 5, 12, 9, 15, 5, 11, 6, 8, 13, 12, 5, 12, 13, 14, 11, 8, 5, 6,
];

static RMD_RIGHT_WORD: [usize; 80] = [
    5, 14, 7, 0, 9, 2, 11, 4, 13, 6, 15, 8, 1, 10, 3, 12, 6, 11, 3, 7, 0, 13, 5, 10, 14, 15, 8, 12,
    4, 9, 1, 2, 15, 5, 1, 3, 7, 14, 6, 9, 11, 8, 12, 2, 10, 0, 4, 13, 8, 6, 4, 1, 3, 11, 15, 0, 5,
    12, 2, 13, 9, 7, 10, 14, 12, 15, 10, 4, 1, 5, 8, 7, 6, 2, 13, 14, 0, 3, 9, 11,
];

static RMD_RIGHT_SHIFT: [u32; 80] = [
    8, 9, 9, 11, 13, 15, 15, 5, 7, 7, 8, 11, 14, 14, 12, 6, 9, 13, 15, 7, 12, 8, 9, 11, 7, 7, 12,
    7, 6, 15, 13, 11, 9, 7, 15, 11, 8, 6, 6, 14, 12, 13, 5, 14, 13, 13, 7, 5, 15, 5, 8, 11, 14, 14,
    6, 14, 6, 9, 12, 9, 12, 5, 15, 8, 8, 5, 12, 9, 12, 5, 14, 6, 8, 13, 6, 5, 15, 13, 11, 11,
];

/// `sha2.c`'s SHA-512 constant table (80 entries).
static SHA512_K: [u64; 80] = [
    0x428a2f98d728ae22,
    0x7137449123ef65cd,
    0xb5c0fbcfec4d3b2f,
    0xe9b5dba58189dbbc,
    0x3956c25bf348b538,
    0x59f111f1b605d019,
    0x923f82a4af194f9b,
    0xab1c5ed5da6d8118,
    0xd807aa98a3030242,
    0x12835b0145706fbe,
    0x243185be4ee4b28c,
    0x550c7dc3d5ffb4e2,
    0x72be5d74f27b896f,
    0x80deb1fe3b1696b1,
    0x9bdc06a725c71235,
    0xc19bf174cf692694,
    0xe49b69c19ef14ad2,
    0xefbe4786384f25e3,
    0x0fc19dc68b8cd5b5,
    0x240ca1cc77ac9c65,
    0x2de92c6f592b0275,
    0x4a7484aa6ea6e483,
    0x5cb0a9dcbd41fbd4,
    0x76f988da831153b5,
    0x983e5152ee66dfab,
    0xa831c66d2db43210,
    0xb00327c898fb213f,
    0xbf597fc7beef0ee4,
    0xc6e00bf33da88fc2,
    0xd5a79147930aa725,
    0x06ca6351e003826f,
    0x142929670a0e6e70,
    0x27b70a8546d22ffc,
    0x2e1b21385c26c926,
    0x4d2c6dfc5ac42aed,
    0x53380d139d95b3df,
    0x650a73548baf63de,
    0x766a0abb3c77b2a8,
    0x81c2c92e47edaee6,
    0x92722c851482353b,
    0xa2bfe8a14cf10364,
    0xa81a664bbc423001,
    0xc24b8b70d0f89791,
    0xc76c51a30654be30,
    0xd192e819d6ef5218,
    0xd69906245565a910,
    0xf40e35855771202a,
    0x106aa07032bbd1b8,
    0x19a4c116b8d2d0c8,
    0x1e376c085141ab53,
    0x2748774cdf8eeb99,
    0x34b0bcb5e19b48a8,
    0x391c0cb3c5c95a63,
    0x4ed8aa4ae3418acb,
    0x5b9cca4f7763e373,
    0x682e6ff3d6b2b8a3,
    0x748f82ee5defb2fc,
    0x78a5636f43172f60,
    0x84c87814a1f0ab72,
    0x8cc702081a6439ec,
    0x90befffa23631e28,
    0xa4506cebde82bde9,
    0xbef9a3f7b2c67915,
    0xc67178f2e372532b,
    0xca273eceea26619c,
    0xd186b8c721c0c207,
    0xeada7dd6cde0eb1e,
    0xf57d4f7fee6ed178,
    0x06f067aa72176fba,
    0x0a637dc5a2c898a6,
    0x113f9804bef90dae,
    0x1b710b35131c471b,
    0x28db77f523047d84,
    0x32caab7b40c72493,
    0x3c9ebe0a15c9bebc,
    0x431d67c49c100d4c,
    0x4cc5d4becb3e42b6,
    0x597f299cfc657e2a,
    0x5fcb6fab3ad6faec,
    0x6c44198c4a475817,
];

/// `sha2.c`'s `sha512_initial_hash_value[8]`.
static SHA512_H0: [u64; 8] = [
    0x6a09e667f3bcc908,
    0xbb67ae8584caa73b,
    0x3c6ef372fe94f82b,
    0xa54ff53a5f1d36f1,
    0x510e527fade682d1,
    0x9b05688c2b3e6c1f,
    0x1f83d9abfb41bd6b,
    0x5be0cd19137e2179,
];

/// `sha2.c`'s `sha384_initial_hash_value[8]`: the same core as SHA512; only this
/// initial value and the output length differ.
static SHA384_H0: [u64; 8] = [
    0xcbbb9d5dc1059ed8,
    0x629a292a367cd507,
    0x9159015a3070dd17,
    0x152fecd8f70e5939,
    0x67332667ffc00b31,
    0x8eb44a8768581511,
    0xdb0c2e0d64f98fa7,
    0x47b5481dbefa4fa4,
];

/// The shape of that trailing "bit count" field. All three differences live
/// here, and getting it wrong raises no error — it only changes the digest: the
/// MD family writes 8 bytes little-endian (`Encode` in `md4c.c`/`md5c.c`),
/// SHA-1/256 write 8 bytes big-endian (`SHA1Final` in `sha1.c`), and
/// SHA-384/512 write **16 bytes** big-endian (`sha2.c` splits the counter into
/// two `u64`s; the high half is always zero for the input lengths this
/// repository meets, so only the low half is written here and the high half
/// stays in the zero-initialized buffer).
#[derive(Clone, Copy)]
enum Counter {
    Le64,
    Be64,
    Be128,
}

impl Counter {
    /// How many bytes this field occupies — whether the padding still fits in
    /// the current block depends on it.
    fn len(self) -> usize {
        match self {
            Counter::Le64 | Counter::Be64 => 8,
            Counter::Be128 => 16,
        }
    }

    fn write(self, slot: &mut [u8], bits: u64) {
        match self {
            Counter::Le64 => slot.copy_from_slice(&bits.to_le_bytes()),
            Counter::Be64 => slot.copy_from_slice(&bits.to_be_bytes()),
            Counter::Be128 => slot[8..].copy_from_slice(&bits.to_be_bytes()),
        }
    }
}

/// Cut `data` plus the "`0x80` + zero padding + trailing bit count" segment into
/// `block_len`-byte blocks, handing each to `sink`. `block_len` is 64 (MD family
/// and SHA-256) or 128 (SHA-384/512).
///
/// "Does it still fit" is the whole difficulty of this code: the `0x80` sits at
/// `rem`, the bit count needs the last `counter.len()` cells of the block, so the
/// fit condition is `rem + counter.len() + 1 <= block_len`; a 64-byte block with
/// an 8-byte field gives `rem <= 55`, a 128-byte block with a 16-byte field gives
/// `rem <= 111`. The boundary is written as `>` (when it fails, hand this block
/// over first and then emit one extra block of pure zeros) rather than as
/// `>= 56`, which welds two constants together — when `rem` is exactly 56 (or
/// 112) the `0x80` already occupies the field's position, and writing the length
/// into that same block would overwrite the padding: no error, just a different
/// digest for every input.
fn feed_blocks(data: &[u8], block_len: usize, counter: Counter, sink: &mut impl FnMut(&[u8])) {
    let whole = data.len() / block_len;
    let rem = data.len() % block_len;
    for index in 0..whole {
        sink(&data[index * block_len..(index + 1) * block_len]);
    }

    let bits = (data.len() as u64).wrapping_mul(8);
    let mut tail = [0u8; 128];
    tail[..rem].copy_from_slice(&data[whole * block_len..]);
    tail[rem] = 0x80;
    if rem + counter.len() + 1 > block_len {
        sink(&tail[..block_len]);
        tail = [0u8; 128];
    }
    let at = block_len - counter.len();
    counter.write(&mut tail[at..block_len], bits);
    sink(&tail[..block_len]);
}

/// `sha2.c`'s `SHA256_Transform`: 64 rounds, eight named registers (the `SH256R`
/// macro passes `a`…`h`), with word expansion using `sigma0_256`/`sigma1_256`,
/// the "two rotations plus one shift" functions. Which is the rotation and which
/// the shift (`rotate_right` vs `>>`) must not be confused: in
/// `sigma0_256(x) = ROTR(7)^ROTR(18)^(x>>3)` the third term is a **shift**;
/// writing it as `ROTR(3)` would bring the 3 shifted-out bits back.
fn sha256_transform(state: &mut [u32; 8], block: &[u8]) {
    let mut word = [0u32; 64];
    for index in 0..16 {
        word[index] = u32::from_be_bytes(block[index * 4..index * 4 + 4].try_into().unwrap());
    }
    for index in 16..64 {
        let s0 = {
            let x = word[index - 15];
            x.rotate_right(7) ^ x.rotate_right(18) ^ (x >> 3)
        };
        let s1 = {
            let x = word[index - 2];
            x.rotate_right(17) ^ x.rotate_right(19) ^ (x >> 10)
        };
        word[index] = word[index - 16]
            .wrapping_add(s0)
            .wrapping_add(word[index - 7])
            .wrapping_add(s1);
    }

    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for round in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ (!e & g);
        let temp1 = h
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(SHA256_K[round])
            .wrapping_add(word[round]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let temp2 = s0.wrapping_add(maj);

        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(temp1);
        d = c;
        c = b;
        b = a;
        a = temp1.wrapping_add(temp2);
    }

    for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *slot = slot.wrapping_add(value);
    }
}

/// `md4c.c`'s `MD4Transform`: 48 rounds, three 16-round groups each swapping a
/// boolean function and a constant (the first adds zero, the second adds
/// `0x5a827999`, the third adds `0x6ed9eba1` — **the third number is not** MD5's
/// family constant, nor the `0x6d70756d` common in textbooks; it is taken
/// verbatim from `md4c.c`'s `HH` macro). The only difference from MD5 is at the
/// end of each round: MD4's `FF` macro lacks the `(a) += (b)` statement, so the
/// value handed back to the register is the rotated value itself.
fn md4_transform(state: &mut [u32; 4], block: &[u8]) {
    let mut word = [0u32; 16];
    for (index, slot) in word.iter_mut().enumerate() {
        *slot = u32::from_le_bytes(block[index * 4..index * 4 + 4].try_into().unwrap());
    }

    let [mut a, mut b, mut c, mut d] = *state;
    for round in 0..48 {
        let group = round / 16;
        let mixed = match group {
            0 => (b & c) | (!b & d),
            1 => (b & c) | (b & d) | (c & d),
            _ => b ^ c ^ d,
        };
        let constant = match group {
            0 => 0,
            1 => 0x5a82_7999,
            _ => 0x6ed9_eba1,
        };
        let value = mixed
            .wrapping_add(a)
            .wrapping_add(word[MD4_WORD[round]])
            .wrapping_add(constant)
            .rotate_left(MD4_SHIFT[round]);
        a = d;
        d = c;
        c = b;
        b = value;
    }

    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
}

/// `sha2.c`'s `SHA512_Transform`: 80 rounds, 128-byte blocks, `u64` words. The
/// rotation counts of the four composite functions are matched one by one
/// against `sha2.c`'s `Sigma0_512`/`Sigma1_512`/`sigma0_512`/`sigma1_512`:
/// `28/34/39`, `14/18/41`, `1/8` plus a **shift** of `7`, `19/61` plus a
/// **shift** of `6`.
fn sha512_transform(state: &mut [u64; 8], block: &[u8]) {
    let mut word = [0u64; 80];
    for index in 0..16 {
        word[index] = u64::from_be_bytes(block[index * 8..index * 8 + 8].try_into().unwrap());
    }
    for index in 16..80 {
        let s0 = {
            let x = word[index - 15];
            x.rotate_right(1) ^ x.rotate_right(8) ^ (x >> 7)
        };
        let s1 = {
            let x = word[index - 2];
            x.rotate_right(19) ^ x.rotate_right(61) ^ (x >> 6)
        };
        word[index] = word[index - 16]
            .wrapping_add(s0)
            .wrapping_add(word[index - 7])
            .wrapping_add(s1);
    }

    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for round in 0..80 {
        let s1 = e.rotate_right(14) ^ e.rotate_right(18) ^ e.rotate_right(41);
        let ch = (e & f) ^ (!e & g);
        let temp1 = h
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(SHA512_K[round])
            .wrapping_add(word[round]);
        let s0 = a.rotate_right(28) ^ a.rotate_right(34) ^ a.rotate_right(39);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let temp2 = s0.wrapping_add(maj);

        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(temp1);
        d = c;
        c = b;
        b = a;
        a = temp1.wrapping_add(temp2);
    }

    for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *slot = slot.wrapping_add(value);
    }
}

/// MD5's 16-byte digest: uses the existing streaming implementation in
/// `minix-crypt` (`Md5::new`/`update`/`finalize`, matching C's
/// `MD5Init`/`MD5Update`/`MD5Final`).
fn md5(data: &[u8]) -> [u8; 16] {
    let mut context = minix_crypt::md5::Md5::new();
    context.update(data);
    context.finalize()
}

/// SHA1's 20-byte digest: uses the existing `sha1` in `minix-crypt` (FIPS 180-1).
fn sha1(data: &[u8]) -> [u8; 20] {
    minix_crypt::sha1::sha1(data)
}

/// SHA256's 32-byte digest: the one newly translated here (`sha2.c`'s
/// `SHA256_Final` hands out the eight state words spread big-endian into 32
/// bytes).
fn sha256(data: &[u8]) -> [u8; 32] {
    let mut state = SHA256_H0;
    feed_blocks(data, 64, Counter::Be64, &mut |block| {
        sha256_transform(&mut state, block);
    });
    let mut digest = [0u8; 32];
    for (index, word) in state.iter().enumerate() {
        digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    digest
}

/// The shared core of SHA-512's 64-byte digest: `sha2.c`'s `SHA512_Data`/
/// `SHA384_Data` are likewise the same core with different initial values
/// (`SHA384_Init` loads `sha384_initial_hash_value`, `SHA512_Init` loads
/// `sha512_initial_hash_value`).
fn sha512_with(data: &[u8], initial: [u64; 8]) -> [u8; 64] {
    let mut state = initial;
    feed_blocks(data, 128, Counter::Be128, &mut |block| {
        sha512_transform(&mut state, block);
    });
    let mut digest = [0u8; 64];
    for (index, word) in state.iter().enumerate() {
        digest[index * 8..index * 8 + 8].copy_from_slice(&word.to_be_bytes());
    }
    digest
}

/// SHA512's 64-byte digest.
fn sha512(data: &[u8]) -> [u8; 64] {
    sha512_with(data, SHA512_H0)
}

/// SHA384's 48-byte digest: `sha2.c`'s `SHA384_Final` writes out only the first
/// six state words, i.e. it takes the first 48 bytes of SHA512's 64 — it does
/// **not** compute a separate 384-bit digest.
fn sha384(data: &[u8]) -> [u8; 48] {
    let full = sha512_with(data, SHA384_H0);
    let mut digest = [0u8; 48];
    digest.copy_from_slice(&full[..48]);
    digest
}

/// MD4's 16-byte digest (the core newly translated here; `minix-crypt` has no
/// MD4).
fn md4(data: &[u8]) -> [u8; 16] {
    let mut state = MD4_H0;
    feed_blocks(data, 64, Counter::Le64, &mut |block| {
        md4_transform(&mut state, block);
    });
    let mut digest = [0u8; 16];
    for (index, word) in state.iter().enumerate() {
        digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    digest
}

/// MD2's 32-byte block folding (`md2.c`'s `MD2Transform`). In the state,
/// `X[0..16)` is the chaining value, `X[16..32)` the input block being
/// accumulated, and `X[32..48)` their XOR; `index` starts at 16 rather than 0 —
/// the first 16 cells hold the previous block's chaining value, and getting this
/// wrong turns MD2 into a different function.
struct Md2 {
    x: [u8; 48],
    checksum: [u8; 16],
    index: usize,
}

impl Md2 {
    fn new() -> Self {
        Self {
            x: [0u8; 48],
            checksum: [0u8; 16],
            index: 16,
        }
    }

    /// `MD2Update`: fold once 32 bytes are full; the fold pulls the write
    /// pointer back to 16 (not 0).
    fn feed(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.x[self.index] = *byte;
            self.index += 1;
            if self.index == 32 {
                self.transform();
            }
        }
    }

    fn transform(&mut self) {
        let mut last = self.checksum[15];
        for j in 0..16 {
            self.x[32 + j] = self.x[j] ^ self.x[16 + j];
            self.checksum[j] ^= MD2_S[usize::from(self.x[16 + j] ^ last)];
            last = self.checksum[j];
        }
        let mut t = 0usize;
        for j in 0..18 {
            for k in 0..48 {
                let value = self.x[k] ^ MD2_S[t];
                self.x[k] = value;
                t = usize::from(value);
            }
            t = (t + j) % 256;
        }
        self.index = 16;
    }

    /// `MD2Final`: first pad with as many bytes as are missing, each carrying
    /// the missing count, then feed the **current** checksum in whole; finally
    /// hand back `X[0..16)`. The pad length lands in 1..=16 because the write
    /// pointer is never 0.
    fn finish(mut self) -> [u8; 16] {
        let padlen = 32 - self.index;
        let pad = [padlen as u8; 16];
        self.feed(&pad[..padlen]);
        let checksum = self.checksum;
        self.feed(&checksum);
        let mut digest = [0u8; 16];
        digest.copy_from_slice(&self.x[..16]);
        digest
    }
}

/// `rmd160.c`'s `H0`…`H4` (the same as MD5/MD4's four, and the fifth the same as
/// SHA1's fifth — three source files each wrote them out, so three copies live
/// here too; see their comments).
static RMD_H0: [u32; 5] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476, 0xc3d2e1f0];

/// The left line's five group constants (the `K` array, entries 0 through 4).
static RMD_K: [u32; 5] = [0x00000000, 0x5a827999, 0x6ed9eba1, 0x8f1bbcdc, 0xa953fd4e];

/// The right line's five group constants `KK0`…`KK4`: **in a different order
/// from the left**, and the right line uses its function groups in reverse (see
/// the table comment above).
static RMD_KK: [u32; 5] = [0x50a28be6, 0x5c4dd124, 0x6d703ef3, 0x7a6d76e9, 0x00000000];

/// `rmd160.c`'s five boolean functions (the left line uses them forward by group
/// number `i / 16`, the right line in reverse).
fn rmd_f(group: usize, x: u32, y: u32, z: u32) -> u32 {
    match group {
        0 => x ^ y ^ z,
        1 => (x & y) | (!x & z),
        2 => (x | !y) ^ z,
        3 => (x & z) | (y & !z),
        _ => x ^ (y | !z),
    }
}

/// One line's 80 rounds (`rmd160.c`'s `R` macro: `a = ROL(s, a + F(b,c,d) + x[r]
/// + K) + e; c = ROL(10, c);`). The target register cycles as `a,e,d,c,b`,
/// equivalent to array index `(-i) mod 5`; the three operands are the three after
/// it, and the final addend is the one after those. The left line uses constant
/// table `K` with function groups forward; the right line uses `KK` with
/// function groups backward.
fn rmd160_line(
    state: &[u32; 5],
    words: &[u32; 16],
    order: &[usize; 80],
    shifts: &[u32; 80],
    constants: &[u32; 5],
    reversed: bool,
) -> [u32; 5] {
    let mut r = *state;
    for i in 0..80 {
        let (word, shift) = (order[i], shifts[i]);
        let group = i / 16;
        let target = (5 - i % 5) % 5;
        let f_group = if reversed { 4 - group } else { group };
        let mixed = rmd_f(
            f_group,
            r[(target + 1) % 5],
            r[(target + 2) % 5],
            r[(target + 3) % 5],
        );
        r[target] = r[target]
            .wrapping_add(mixed)
            .wrapping_add(words[word])
            .wrapping_add(constants[group])
            .rotate_left(shift)
            .wrapping_add(r[(target + 4) % 5]);
        let third = (target + 2) % 5;
        r[third] = r[third].rotate_left(10);
    }
    r
}

/// One block transform of RMD160: the left and right lines do 80 rounds each,
/// then merge as follows (the six lines of `rmd160.c`'s
/// `t = state[1] + cc + d; …`). Each line adds "its own register set" back to the
/// **initial** state, so the six values in the merge mix "new left five-tuple,
/// new right five-tuple, and old state" — copied from the source.
fn rmd160_transform(state: &mut [u32; 5], block: &[u8]) {
    let mut words = [0u32; 16];
    for (index, slot) in words.iter_mut().enumerate() {
        *slot = u32::from_le_bytes(block[index * 4..index * 4 + 4].try_into().unwrap());
    }

    let l = rmd160_line(
        state,
        &words,
        &RMD_LEFT_WORD,
        &RMD_LEFT_SHIFT,
        &RMD_K,
        false,
    );
    let r = rmd160_line(
        state,
        &words,
        &RMD_RIGHT_WORD,
        &RMD_RIGHT_SHIFT,
        &RMD_KK,
        true,
    );

    // The two addends in those six merge lines **come from different lines**:
    // in `t = state[1] + cc + d;` the `cc` is the left line's third register
    // saved into `aa`…`ee`, and `d` is the right line's current fourth — the
    // source finishes the left line and stores its result, then reloads `a`…`e`
    // with the initial state and runs the right line. Swapping the two indices
    // raises no error, it just changes the digest: this repository's first run
    // on the empty input failed exactly that way, caught by the first case in
    // `rmdtest.c`.
    let old = *state;
    state[0] = old[1].wrapping_add(l[2]).wrapping_add(r[3]);
    state[1] = old[2].wrapping_add(l[3]).wrapping_add(r[4]);
    state[2] = old[3].wrapping_add(l[4]).wrapping_add(r[0]);
    state[3] = old[4].wrapping_add(l[0]).wrapping_add(r[1]);
    state[4] = old[0].wrapping_add(l[1]).wrapping_add(r[2]);
}

/// MD2's 16-byte digest.
fn md2(data: &[u8]) -> [u8; 16] {
    let mut context = Md2::new();
    context.feed(data);
    context.finish()
}

/// RMD160's 20-byte digest: 64-byte blocks, trailing bit count in 8 bytes
/// **little-endian** (`RMD160Final`'s `PUT_64BIT_LE`), state spread
/// little-endian — the same padding rule as MD5, so it shares [`feed_blocks`].
fn rmd160(data: &[u8]) -> [u8; 20] {
    let mut state = RMD_H0;
    feed_blocks(data, 64, Counter::Le64, &mut |block| {
        rmd160_transform(&mut state, block);
    });
    let mut digest = [0u8; 20];
    for (index, word) in state.iter().enumerate() {
        digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    digest
}

/// The step where C's `End` (`mdXhl.c:44-66`, and the same-named function in
/// `hashhl.c`) formats a digest: per byte take the high nibble then the low, look
/// up `0123456789abcdef`, **lowercase**. The uppercase copy appears only in the
/// algorithm name of the report header (`cksum.c`'s `hash->hashname`); do not mix
/// the two.
fn hex(digest: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(digest.len() * 2);
    for byte in digest {
        text.push(DIGITS[usize::from(byte >> 4)] as char);
        text.push(DIGITS[usize::from(byte & 0x0f)] as char);
    }
    text
}

/// The eight algorithms in C's `hashes[]` table
/// (`minix3/usr.bin/cksum/cksum.c:120-144`). **The repository's only name
/// table**: the `-a` match, the name in the `MD5 mutually exclusive with sum`
/// sentence, and the report header `MD5 (file) = …` all come from here, so
/// "whether the algorithm was selected" and "whether this family can be
/// computed" are the same thing in the type system — there is no intermediate
/// state of "name recognized but not computable", and the thin shell therefore
/// cannot print an empty-input digest as a pretend result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algo {
    Md2,
    Md4,
    Md5,
    Rmd160,
    Sha1,
    Sha256,
    Sha384,
    Sha512,
}

impl Algo {
    /// The eight entries of C's `hashes[]` table, **in table order**
    /// (`minix3/usr.bin/cksum/cksum.c:120-144`: md2, md4, md5, rmd160, sha1,
    /// sha256, sha384, sha512). The order matters: when `cksum -c` recognizes an
    /// algorithm name at the start of a line it `strncmp`s against this table
    /// item by item and takes the first prefix hit (`:343-352`).
    pub const ALL: [Algo; 8] = [
        Algo::Md2,
        Algo::Md4,
        Algo::Md5,
        Algo::Rmd160,
        Algo::Sha1,
        Algo::Sha256,
        Algo::Sha384,
        Algo::Sha512,
    ];

    /// `hashname` → algorithm. **Case-insensitive**: C's `case 'a'` uses
    /// `strcasecmp(hashes[i].hashname, optarg)` (`cksum.c:201-207`), so `-a md5`,
    /// `-a MD5`, and `-a Md5` are the same entry in C. This is the repository's
    /// only name table — the `-a` leg in `src/cksum.rs`, the uppercase name in
    /// diagnostics, and the report header all come from it; copying it twice
    /// would bring back the question "which one is real".
    pub fn from_hashname(hashname: &str) -> Option<Algo> {
        Self::from_hashname_bytes(hashname.as_bytes())
    }

    /// The **byte entry** to the same table. C's
    /// `strcasecmp(hashes[i].hashname, optarg)` (`cksum.c:201-207`) compares two
    /// byte strings; `optarg` is that stretch of argv, with no encoding
    /// question, so when argv arrives as bytes the lookup must be byte-wise too.
    /// There is still only one name table.
    pub fn from_hashname_bytes(hashname: &[u8]) -> Option<Algo> {
        Self::ALL.into_iter().find(|algo| {
            let want = algo.hashname().as_bytes();
            want.len() == hashname.len()
                && want
                    .iter()
                    .zip(hashname)
                    .all(|(a, b)| a.eq_ignore_ascii_case(b))
        })
    }

    /// The **line-start prefix** of a line in a check file → algorithm. This
    /// criterion **differs** from the one above: in `do_check` C uses
    /// `strncmp(buf, nhash->hashname, strlen(nhash->hashname))`
    /// (`cksum.c:343-352`), **case-sensitive**, accepting only the uppercase name
    /// from the table. Measured on the reference (shape g-10): the line
    /// `md5 (f) = <digest>` is not recognized as an algorithm and takes the
    /// `unknown hash` path (`:358`) rather than the forgiving `-a` leg. If both
    /// places shared one match rule, `-a`'s case-insensitivity would seep into
    /// the check file — behavior C does not have.
    pub fn by_line_prefix(line: &str) -> Option<Algo> {
        Self::ALL
            .into_iter()
            .find(|algo| line.starts_with(algo.hashname()))
    }

    /// The `hashname` column of C's `hashes[]` table (the uppercase copy).
    /// Diagnostic sentences and the default report header use it
    /// (`cksum.c:231-234`'s `warnx("%s mutually exclusive with sum",
    /// hash->hashname)`).
    pub fn hashname(self) -> &'static str {
        match self {
            Algo::Md2 => "MD2",
            Algo::Md4 => "MD4",
            Algo::Md5 => "MD5",
            Algo::Rmd160 => "RMD160",
            Algo::Sha1 => "SHA1",
            Algo::Sha256 => "SHA256",
            Algo::Sha384 => "SHA384",
            Algo::Sha512 => "SHA512",
        }
    }

    /// The digest body as lowercase hex, `2 * digest bytes` long.
    pub fn hex_digest(self, data: &[u8]) -> String {
        match self {
            Algo::Md2 => hex(&md2(data)),
            Algo::Md4 => hex(&md4(data)),
            Algo::Md5 => hex(&md5(data)),
            Algo::Sha1 => hex(&sha1(data)),
            Algo::Rmd160 => hex(&rmd160(data)),
            Algo::Sha256 => hex(&sha256(data)),
            Algo::Sha384 => hex(&sha384(data)),
            Algo::Sha512 => hex(&sha512(data)),
        }
    }
}

/// The eight fixed strings C's `MD5TestSuite` prints
/// (`minix3/usr.bin/cksum/md5.c:118-133`, eight calls at `:122-132`).
/// **All eight algorithms share them**: apart from `HASHTYPE`/`HASHLEN` and an
/// `#include "md5.c"`, `md2.c`/`md4.c`/`md5.c`/`rmd160.c`/`sha1.c`/`sha256.c`/
/// `sha384.c`/`sha512.c` contain nothing else (21–25 lines each today), so C's
/// `-x` is "the same text × eight digests". The last one in C is a
/// backslash-continued string concatenation (`:131-132`), 80 characters
/// together.
pub const TEST_SUITE: [&str; 8] = [
    "",
    "a",
    "abc",
    "message digest",
    "abcdefghijklmnopqrstuvwxyz",
    "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
    "12345678901234567890123456789012345678901234567890123456789012345678901234567890",
];

/// The block length of C's `-t` leg (`md5.c:53`'s `TEST_BLOCK_LEN`).
pub const TIME_TRIAL_BLOCK_LEN: usize = 1000;
/// The number of folds on C's `-t` leg (`md5.c:54`'s `TEST_BLOCK_COUNT`).
pub const TIME_TRIAL_BLOCK_COUNT: usize = 1000;

/// The bytes C's `-t` feeds to the digest core: `md5.c:86-87` first builds a
/// block with `block[i] = i & 0xff` (1000 bytes), then `Update`s that **same
/// block** a thousand times (`:94-95`), 1000 × 1000 bytes in total, so the `j`-th
/// byte is `(j % 1000) & 0xff`. This shell's digest core is a single-shot
/// `hex_digest`, so the thousand folds are flattened into one sequence here — the
/// same thing as the registered `CKSUM-DIGEST-STREAMING` item (same value,
/// differing only in memory footprint).
pub fn time_trial_data() -> alloc::vec::Vec<u8> {
    (0..TIME_TRIAL_BLOCK_LEN * TIME_TRIAL_BLOCK_COUNT)
        .map(|index| (index % TIME_TRIAL_BLOCK_LEN) as u8)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        Algo, TIME_TRIAL_BLOCK_LEN, hex, md2, md4, md5, rmd160, sha1, sha256, sha384, sha512,
        time_trial_data,
    };

    /// The 1000 × 1000 bytes fed on the `-t` leg must have C's shape
    /// (`md5.c:86-87` builds the block, `:94-95` folds it a thousand times:
    /// `block[i] = i & 0xff` inside the block, the same block a thousand times).
    /// The expected digests come from the full reference built from this
    /// repository's C source (`tmp/cksum-hash-state/readings.log`, shapes t-1 and
    /// t-2; recipe `bash tmp/cksum-hash-state/build-cksum-ref.sh`).
    #[test]
    fn the_time_trial_payload_matches_the_c_block_pattern() {
        let data = time_trial_data();
        assert_eq!(
            data.len(),
            TIME_TRIAL_BLOCK_LEN * 1000,
            "一千枚一千字节的块"
        );
        assert_eq!(
            (data[0], data[255], data[256], data[999]),
            (0, 255, 0, 231),
            "块内是 `i & 0xff`，第 999 个字节是 999 % 256"
        );
        assert_eq!(
            hex(&md5(&data)),
            "f217fb0b8599c956eaeb81611e7a8758",
            "MD5 那一行 `Digest = …` 的期望值"
        );
        assert_eq!(
            hex(&sha512(&data)),
            "0b7912f30c0528ee0a481dd1087068b908c9c4f5a908271e0760f7abacd52a95523a9a5e3c0ef0e9b29a04326a723c2d5410e1f1a116228400aeeb532d3a9619",
            "SHA512 同一形：块长 128 与 1000 不整除，摆错就立刻分叉"
        );
    }

    /// This module's gates accept only two sources of truth: expectation files
    /// in the repository, and readings from **a reference built from this
    /// repository's C source** (the recipe is in
    /// `rewrite-notes/18-stage-commands/todo.md`). The reference reproduced the
    /// in-repository expectations below — `md5test-out`/`sha1test-out`/
    /// `sha1test2-out`, the seven `md4test.c` cases, and the two `t_sha2.c`
    /// tables — before it was used to produce readings at other lengths. No hash
    /// implementation from the host libc or the host OpenSSL counts as ground
    /// truth.
    const ALPHA: &[u8] = b"abcdefghijklmnopqrstuvwxyz";

    /// Input shapes covered jointly by the reference and the in-repository
    /// expectations: lengths taken at 64-byte block boundaries.
    /// `(length, md5, sha1, sha256)`, the input being the first `length` bytes of
    /// a repeating `ALPHA`. The expectations for lengths 0/1/3 also appear in
    /// `minix3/tests/lib/libc/hash/data/md5test-out` and `sha1test-out` (those
    /// three rows' inputs are `""`/`a`/`abc`), so this table carries its own
    /// anchor into the repository.
    const VECTORS: &[(usize, &str, &str, &str)] = &[
        (
            0,
            "d41d8cd98f00b204e9800998ecf8427e",
            "da39a3ee5e6b4b0d3255bfef95601890afd80709",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            1,
            "0cc175b9c0f1b6a831c399e269772661",
            "86f7e437faa5a7fce15d1ddcb9eaeaea377667b8",
            "ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb",
        ),
        (
            3,
            "900150983cd24fb0d6963f7d28e17f72",
            "a9993e364706816aba3e25717850c26c9cd0d89d",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
        (
            55,
            "0d7ae056b2f015cd7dc67494efd658f1",
            "a617d006d1ca12671785098a19a87fe58443bde9",
            "595615dbe4f0f407ae397d08b4c2cb870cb9b0e11937416f950c5160acf9c005",
        ),
        (
            56,
            "31fcfb5165169eb55898e7e4cf34d19a",
            "4ad5bb7ae3c4024768d364b77c52128ea3cffebe",
            "784f623b787495078e93ff28a25b581df0584055a7e71d8cd90c454716b92f51",
        ),
        (
            57,
            "fd62afaf3aa1e2a52882cb464f5ccc4d",
            "e1b3b34da0f7b299090824d9aa81fff6711a79ad",
            "808f0738aa4401bdee842e5a15a7baad5809f976d8eb6f9bd2683cebd2e8d671",
        ),
        (
            63,
            "1b30c0670c15e7da3c2ba7bce77ebe99",
            "fc8a5ab77259625085ead3ec96515b3b8d933fad",
            "5ca3e1ef5207490eac01a795e5cc94d59582a5118bf9534665c8668d87aa647c",
        ),
        (
            64,
            "a2eaf6295c32adc403865fd96a2f182b",
            "93249d4c2f8903ebf41ac358473148ae6ddd7042",
            "2fcd5a0d60e4c941381fcc4e00a4bf8be422c3ddfafb93c809e8d1e2bfffae8e",
        ),
        (
            65,
            "eba2cce0ca8df47e62414a736b3105a2",
            "cf2a63cc308225cf07b498d2309a01dd0df52f67",
            "1b3cd1877ab2f2f19f7be001722554f336cb799df0329de0bb4c118dc6abc06d",
        ),
        (
            119,
            "b05187e08da41fa3ef16bd56afaafd99",
            "edd0f1133d0e4ca5f3e98bb7e0295f31d20d2cdb",
            "faef67da856d6fd9c8d12f9ed0a4fefd3cf0ce085ab43e2907418d457e3c354b",
        ),
        (
            120,
            "62af9b597a9f55e16ab2b897387fc052",
            "23a58eee587aa1f50d19a969ab36a3fe3e88c393",
            "c9512b08619c19fbb503c7da6b46ef20301e5f7a7a5f43989182398536f5c5c8",
        ),
        (
            127,
            "f41f37f8b5a7782d5cfeafc43e3d3c1c",
            "4add81c7de2e43d621cdb34132bbb23178f4c500",
            "1b9907986f18ddc2b37e18be502d82d69d6e18bb9dff9e16f2aceea7d88cb933",
        ),
        (
            128,
            "3e8c1ccbd71838ef3df4b72e57fb9bf6",
            "3698f9beb7cdf5b1e1ce786672fb234143d4df89",
            "6c05be2c4268843ae47e68e611277ce62c02153f2f4d2e1e2a1a4b44f766cf74",
        ),
        (
            129,
            "a9ca66cd8dd6b0f6d8cbb6a21bbf5206",
            "1d96f2c973fdd492d08f2a16f950d654912b61ca",
            "cd6bba8374324cbcc0c296b94f35299c0b9820393116358ac3afaf091a4955a5",
        ),
        (
            1000,
            "303fb697b589019cb3edba04b794e575",
            "0c1e754ad8a0130e18bf2d3b0a57e29ad95e75cd",
            "915e53a44c18b19bb06ba5b3f5fcaf1dc4651e8404c63425cfc6174e74659d87",
        ),
    ];

    fn payload(length: usize) -> alloc::vec::Vec<u8> {
        (0..length)
            .map(|index| ALPHA[index % ALPHA.len()])
            .collect()
    }

    /// Block boundaries and the padding criterion (MD5/SHA1/SHA256, all with
    /// 64-byte blocks): lengths 55/56/57 are the two sides of the "does it fit"
    /// cell (at 55 the `0x80` and the length share one block; from 56 on, an
    /// extra block of pure zeros is needed), 63/64/65 are the three sides of
    /// "full block", and 119/120/127/128/129 repeat the same criterion across two
    /// blocks. Dropping one row misses a whole class of input. This gate
    /// straddles two seams: MD5/SHA1 use the padding legs of the two
    /// `minix-crypt` copies, SHA256 uses this module's copy.
    #[test]
    fn the_three_families_match_the_reference_at_every_block_boundary() {
        for (length, expected_md5, expected_sha1, expected_sha256) in VECTORS {
            let data = payload(*length);
            assert_eq!(hex(&md5(&data)), *expected_md5, "md5 长度 {length}");
            assert_eq!(hex(&sha1(&data)), *expected_sha1, "sha1 长度 {length}");
            assert_eq!(
                hex(&sha256(&data)),
                *expected_sha256,
                "sha256 长度 {length}"
            );
        }
    }

    /// The three newly wired families are compared against reference readings by
    /// the same rule: `md4` and `sha384`/`sha512`. Besides the 64-byte block
    /// boundaries (55/56/57, 63/64/65), there are **128-byte block** boundaries
    /// (111/112/119/120/127/128/129, 247/248) and whole-block counts
    /// (1024/1025, 1152) — the SHA-512 family's "does it fit" threshold is
    /// `rem <= 111` (the block tail is reserved for the 16-byte bit count), same
    /// origin as the 64-byte family's but a different number; without this set,
    /// half the padding scheme goes untested. `(length, md4, sha384, sha512)`, the
    /// input again a repeating alphabet.
    const VECTORS_WIDE: &[(usize, &str, &str, &str)] = &[
        (
            0,
            "31d6cfe0d16ae931b73c59d7e0c089c0",
            "38b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da274edebfe76f65fbd51ad2f14898b95b",
            "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e",
        ),
        (
            1,
            "bde52cb31de33e46245e05fbdbd6fb24",
            "54a59b9f22b0b80880d8427e548b7c23abd873486e1f035dce9cd697e85175033caa88e6d57bc35efae0b5afd3145f31",
            "1f40fc92da241694750979ee6cf582f2d5d7d28e18335de05abc54d0560e0f5302860c652bf08d560252aa5e74210546f369fbbbce8c12cfc7957b2652fe9a75",
        ),
        (
            3,
            "a448017aaf21d8525fc10ae87aa6729d",
            "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7",
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f",
        ),
        (
            55,
            "dd3d4546abbd95d12059090017f36605",
            "c43c2f147c861c4d062c5875a1f270339fe44fd93d45b1d0a06ca4650c796cb0d5193b64d1e40f51ab9d216e237b2fe4",
            "9e50039c6597302efab3d4390664894b7adc7039ee61f22d752148f8c5d1924573278186434c21874a7502ef2bd76f3d2902c41a4ac5d6668c9146520f62ffa8",
        ),
        (
            56,
            "e72e93b48028e33f5cf0fa49017436c5",
            "39ba0be242cc0522431da9b1b9a33409beb54de6ca261a28fdd0874e9adc11496c510c91b7b7f4ef1676dfc7824238e0",
            "9b9d6a750ece4b006491a375fc467024bd1e77a84f416b0de602ec75129a743ea5f880804481e91cd9231a58d01d26cf4a3a02e8f8c77b5f81b3fa40f2be395a",
        ),
        (
            57,
            "24bbc6695a4950d5e1e543498a77f0d6",
            "3df110e80da84d15fce56f1cf6c833746076b09cf796c8935f2c65ec8dafb762e3e761ce98b1c20e4c341d8a0cb7cea2",
            "1b1bc716fd46a36381b12a69a40908b82e73af39f2dff1e6cb0e802d5491136d0629a7090e15072a0cded611e552f0fb0fe56b7e095207919431e513ff4b669b",
        ),
        (
            63,
            "4f58683635e5c54a102b623ac768a8d0",
            "c625b33243c083c02248d110080dbc2955f1b39bc4d35edd227bcc01411fb75608410b3ad342d96519facfde36cd0005",
            "455561e15e7c42a73191e90b2c03fe0c569c9e6cd570ed0351f84063525cd68ca92c02948eb178cfc53300c94e21e5816ddfb020d98a74e0b09ebdd6650ee99b",
        ),
        (
            64,
            "0e15e9469255749a626ab50d260ca5de",
            "d5e32c2bc13209ae109192511493feb92a96264813f56a10b233d8ca89f4637f8a51b7d4f790bce3719d7eaa4bef299b",
            "1cdcbebccccaed3fa2acc8bcd1835c67514bd29657a348de629aedb0d5503377285b78656a3a794f4cef88708916d23f78ead08d710101e3c9ba6a823ffc6067",
        ),
        (
            65,
            "8e4bed68550b86c596869bc952bebfba",
            "fde3ac1fa47f72d946d1b9212fb0620149d301145227d972098c78bdfc615f6da997a0b1fb165cc13b8843e85bfbd4ae",
            "a10611b486051ecfba0086e5e0d4f324feabc5ff21e62fe775ff40a71b455993f4ba4da3e9d917f2e2d5a2d28cc84be0ac608c0d631eafc593dd0ba1bca30764",
        ),
        (
            111,
            "cbdaa8b739fb2658608d614f21a2ea8e",
            "b075a2f3b7d768e18f586f1f19586a4f7109ca72ba0120947e7813d25f499f92470d819a4d6e42d3b19be8141627c51d",
            "a467698069eae8ed1e0c6dbfd1b4a247a9f1e7ff4e3af62145ed26f4468bc094610878b7644091141370a47a7638bddc95dbfe8971c34d13c4815d4bb1b3e7f2",
        ),
        (
            112,
            "0ebf60e4ad1fa8b3638fd57d47cad2da",
            "5523cfb77f9c55e0ccafec5b87d79cde643012283b71188e408c5aeae919a3f29337574d5c729b339d9553984ab0014e",
            "a473c93732eef627d02e86d19047a422b586110848ec17dcea13af282a152f7654b0c711e277fd42c1d94bea8b7fed615c52bb0f849227e16240afffc7c56e29",
        ),
        (
            119,
            "1ac68952bb71cb863e7fab2ba18e0298",
            "8f5dac30b1d7e1ada1ee7e7b8566eb9106ebc45889109ba740ce13fb37edcdb27f69e1c487f6c4f991d971fe8f36771d",
            "c410b6871650695997a0ec625682bf57d02ffde3cff7908f7f63f96d251cb999921bc2e383613917106cb2ecc79d558e8e28c00e0b033b8f8ffb07cf93ae05d4",
        ),
        (
            120,
            "e787de409d1ad1c3094d67f980811c53",
            "5fb54ffefee23abc81bd160fceed81582d54cdf0b47377c4a215cb1f5a87258d1d7b2dc0c09c77d7c01808f6bbb3fc9a",
            "483c0e06c4477e4b8abeb357a53037a545640fb9b489141731a7bd1d7380c27bf4a26536ee36f518c670cef4d0447dd9073fc5a45ab1a827e5b4de5831c6f91f",
        ),
        (
            127,
            "28977931ae508f6916c7c2292af8a2f0",
            "fb908cfc02679017ab44a8e1008dec239ea36a381a5c7a75bc6d64f70df306f7df0fcf94cd62a45a15685b5bb93c70d7",
            "c6ce82cb15c8d7040ef3704b1cb161ae3a7f0ac430433e6d52e3f37bf561271bfd014328660d29ee733a9f7f178671390eb0f80231603c8360246753e6406b8d",
        ),
        (
            128,
            "54d61a9826b1f5f0356ab6dc9165fcb8",
            "674b2e80ff8d94008de7409c7b1f878f9fae3a0a6dae2f982cca7e3aaef91bf325d3eb568263a2e1e6856ac75070062a",
            "217d3d9c0952c3e4907f06d4fbf34460ee852c6af591b07c2fa1c5e16455836374c95ae33e184227913f8a2e227e3bbd5187ce57aa1bad11a80f622412eb0884",
        ),
        (
            129,
            "4acc353651e0c9c7c4fc952d8edb84a1",
            "797d56268a2849f4c8fba0f620f1d3dcc94d16b14243d5aae0015c39aa51c13fc455653ec13926cd91393ca42b44709c",
            "18d5a20f40cd7903e625ac1e4fc10971fb3bd6cecb12a684f7600975340649eebccd11cc97fe9e36bbcb3ae75d6d7a0dc2e60e0022c579e87c3ee2e3ffaf65cc",
        ),
        (
            247,
            "11734d14efcc02c76980e194f0c99f9a",
            "017900da3019269a5d706088e759006dd19ae847428a236b79cd056e5ae79c7484b741df75d2c4d799bc5ad1264fe476",
            "26a5f050d6b66749e369b663406b81e4fd19dfe01c7c88baf2c16607ade997c69f607c93e0d08bc7a4e51e1460688175cf44f5497357532fed25b86e335f839d",
        ),
        (
            248,
            "477f3f4db3b60df2896203dba7f4e6d6",
            "6239bd12b2f9625bcb985baebe430f87a8265d7b0c03bb83aea34355e6e7fc42d65e42707b6784e62fb1e26f89bc4cbc",
            "ea10ff2e9926acad2232b2924c746cbcebc3fc20358dae58a759f248e0238076fdc9e0d1943eec4010c0863c0ce7dd7091871a0b5676217de4bca1e25d28b7a6",
        ),
        (
            1000,
            "c47afd094a5c37d91b5c87d3eec7272d",
            "8c7a911d5555a45b1a1ffac92be72815d6b681645ce5d511e77ab119940a9d9c1876d5fc6dd38f7c2b44813fd0203276",
            "c6017363f579f50a015ce5e21b966a94ecd72a32f624b648263b891df07a2780533400778bd6a71a3507733ddfc4a81454a17859cf4f68571fbaeb73bd70b09d",
        ),
        (
            1024,
            "e3693ea2ad200737e5cdfb3ea5d18397",
            "826158645941a684d0510f04a0ff6cdc894f4fdd22694c9eb12bd4ab007be7200f3b0d6d6ef85154c8f9fdcd33cfa39e",
            "f76c5a48f06baac622fa0bab58eb309a4b2154ca44768eda06afbb4b38c4e82ba2d7ecdc4bda217ed5b9040950c39cd280cc4da6295e9ffad742938b0a3f158a",
        ),
        (
            1025,
            "d2bd9d1b10fb7168b8591510d58871c8",
            "7751d3fdabb1691b8e0fe07cb70a9f8524db649bca66b808295cc7791d307624554ab512e23b967eb490f85b043dcd19",
            "17d1844ba75e4e76874a11617fe4c25afc04f93883f354d36a75315c22430ef63013c3253200af4d207a72ca20cf27273a29140bc6db3a1e3f1ca30e937109c2",
        ),
        (
            1119,
            "ec042f0f914add5c5e08eb1f367ecb71",
            "e75ed485b30c7213bc97de1b9ed2ca4de9f02cdc0cf7ee9f3d79672ac5c5408dc3a19d565d70cfe5dffded25efa62967",
            "e791b93e0026f63874ced226d9821dbf16a990872751e7cc4c0adc64a6c6f7e0078c14c4f274be669f7b121ae3a5904272f58788960fd646eb5d77c2e6ffb825",
        ),
        (
            1152,
            "5791ba73987f9e9e08be9ce349ba3fc7",
            "743cf7d55f3aa39ebffa1434f817a46b467c38c8ffb7e62f9ca0ed49df488c63b54813f5b5595832a80e2628aaf9b5b4",
            "4894f32a755c689c3eee7682ceb3c0f3f1e3b74ee15100a869e79c34ea06303d45ce2de5b4375023cb32dbb16003c4c28240f9647da86544f865855c19cb082e",
        ),
    ];

    #[test]
    fn md4_sha384_sha512_match_the_reference_at_every_block_boundary() {
        for (length, expected_md4, expected_sha384, expected_sha512) in VECTORS_WIDE {
            let data = payload(*length);
            assert_eq!(hex(&md4(&data)), *expected_md4, "md4 长度 {length}");
            assert_eq!(
                hex(&sha384(&data)),
                *expected_sha384,
                "sha384 长度 {length}"
            );
            assert_eq!(
                hex(&sha512(&data)),
                *expected_sha512,
                "sha512 长度 {length}"
            );
        }
    }

    /// The repository has only one set of MD4 expectations, and it is not in
    /// Minix's own test directory:
    /// `minix3/crypto/external/bsd/openssl/dist/crypto/md4/md4test.c:75-94`, the
    /// seven RSA-suite cases — their inputs are the same strings, word for word,
    /// as the seven lines of `md5test-in`, which conveniently allows a
    /// side-by-side comparison of the two families. The reference reproduced all
    /// seven before producing other readings (the first section of
    /// `tmp/cksum-384/gen_vectors2.py`, all seven matching).
    #[test]
    fn md4_matches_the_vendored_rsa_suite() {
        for (text, expected) in [
            ("", "31d6cfe0d16ae931b73c59d7e0c089c0"),
            ("a", "bde52cb31de33e46245e05fbdbd6fb24"),
            ("abc", "a448017aaf21d8525fc10ae87aa6729d"),
            ("message digest", "d9130a8164549fe818874806e1c7014b"),
            (
                "abcdefghijklmnopqrstuvwxyz",
                "d79e1c308aa5bbcdeea8ed63df412da9",
            ),
            (
                "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
                "043f8582f241db351ce627e153e7f0e4",
            ),
            (
                "12345678901234567890123456789012345678901234567890123456789012345678901234567890",
                "e33b4ddc9c38f2199c3e7b164fcc0536",
            ),
        ] {
            assert_eq!(hex(&md4(text.as_bytes())), expected, "md4 {text:?}");
        }
    }

    /// Six short rows from each of `minix3/tests/lib/libc/hash/t_sha2.c`'s two
    /// tables: they pin **initial values and truncation** (one core for both
    /// families, differing only in that initial value and the output width);
    /// cross-block shapes are pinned by the boundary table above. `hello, world`
    /// leads the table because it is the author of `sha2.c`'s own comparison
    /// string and is not part of the standard suite.
    #[test]
    fn sha384_and_sha512_match_the_in_tree_sha2_suite() {
        for (text, expected) in [
            (
                "hello, world",
                "1fcdb6059ce05172a26bbe2a3ccc88ed5a8cd5fc53edfd9053304d429296a6da23b1cd9e5c9ed3bb34f00418a70cdb7e",
            ),
            (
                "",
                "38b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da274edebfe76f65fbd51ad2f14898b95b",
            ),
            (
                "a",
                "54a59b9f22b0b80880d8427e548b7c23abd873486e1f035dce9cd697e85175033caa88e6d57bc35efae0b5afd3145f31",
            ),
            (
                "ab",
                "c7be03ba5bcaa384727076db0018e99248e1a6e8bd1b9ef58a9ec9dd4eeebb3f48b836201221175befa74ddc3d35afdd",
            ),
            (
                "abc",
                "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7",
            ),
            (
                "abcdefghijklmnopqrstuvwq",
                "87dc70a2eaa0dd0b687f91f26383866161026e41bb310a9e32b7a17c99284db85b9743476a30caeedf3fbb3c8072bc5e",
            ),
        ] {
            assert_eq!(hex(&sha384(text.as_bytes())), expected, "sha384 {text:?}");
        }
        for (text, expected) in [
            (
                "hello, world",
                "8710339dcb6814d0d9d2290ef422285c9322b7163951f9a0ca8f883d3305286f44139aa374848e4174f5aada663027e4548637b6d19894aec4fb6c46a139fbf9",
            ),
            (
                "",
                "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e",
            ),
            (
                "a",
                "1f40fc92da241694750979ee6cf582f2d5d7d28e18335de05abc54d0560e0f5302860c652bf08d560252aa5e74210546f369fbbbce8c12cfc7957b2652fe9a75",
            ),
            (
                "ab",
                "2d408a0717ec188158278a796c689044361dc6fdde28d6f04973b80896e1823975cdbf12eb63f9e0591328ee235d80e9b5bf1aa6a44f4617ff3caf6400eb172d",
            ),
            (
                "abc",
                "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f",
            ),
            (
                "abcdefghijklmnopqrstuvwq",
                "404eb5652173323320cac6bf8d9714aef0747693a8ab4570700c6262268d367f30e31c44fa66860568ff058fe39c9aa8dac76bc78566c691a884cb9052c4aa0a",
            ),
        ] {
            assert_eq!(hex(&sha512(text.as_bytes())), expected, "sha512 {text:?}");
        }
    }

    /// Readings for one million `a`s on the three new families (from the
    /// reference, the same input as the `sha1test2-out` case): `md4` uses
    /// 64-byte blocks and the other two 128-byte blocks, and a million is not a
    /// multiple of either, so this pins that "fold down to the last padding
    /// block" loses no state at either block length.
    #[test]
    fn the_megabyte_input_covers_the_new_three_families_too() {
        let million = alloc::vec![b'a'; 1_000_000];
        assert_eq!(hex(&md4(&million)), "bbce80cc6bb65e5c6745e30d4eeca9a4");
        assert_eq!(
            hex(&sha384(&million)),
            "9d0e1809716474cb086e834e310a4a1ced149e9c00f248527972cec5704c2a5b07b8b3dc38ecc4ebae97ddd87f3d8985"
        );
        assert_eq!(
            hex(&sha512(&million)),
            "e718483d0ce769644e2e42c7bc15b4638e1f98b13b2044285632a803afa973ebde0ff244877ea60a4cb0432ce577c31beb009c5c2c49aa2e4eadb217ad8cc09b"
        );
    }

    /// Block-boundary readings for the last two families (`md2` and `rmd160`).
    /// MD2's "block" is 32 bytes and the 16-byte checksum is folded in at the
    /// end, so its boundaries are the 15/16/17, 31/32/33, 47/48/49 group; RMD160
    /// follows MD5's padding rule, so 55/56/57 and 63/64/65 apply as is. One
    /// table takes both block lengths together because "the pad length equals
    /// exactly the number of missing bytes" covers 1..=16 for MD2 and 1..=56 for
    /// the 64-byte family — getting either range wrong raises no error, it just
    /// changes the digest.
    const VECTORS_TAIL: &[(usize, &str, &str)] = &[
        (
            0,
            "8350e5a3e24c153df2275c9f80692773",
            "9c1185a5c5e9fc54612808977ee8f548b2258d31",
        ),
        (
            1,
            "32ec01ec4a6dac72c0ab96fb34c0b5d1",
            "0bdc9d2d256b3ee9daae347be6f4dc835a467ffe",
        ),
        (
            3,
            "da853b0d3f88d99b30283a69e6ded6bb",
            "8eb208f7e05d987a9b044a8e98c6b087f15a0bfc",
        ),
        (
            15,
            "879f1ddf42343b52e24e125a0f341b2f",
            "da5bb9bb038e9a19345f51fa09687ba5a65c1a26",
        ),
        (
            16,
            "7f17759865db3f1cd4f77daa7b203a72",
            "b82fdad53d6b15ed6c9eee1f5efd6a223ac762eb",
        ),
        (
            17,
            "1dad5ac3d40d6d941ba781a6ef3e5dfe",
            "6ce79bb3b6d8d55522cbeb3b5194dad0053f6a79",
        ),
        (
            31,
            "e652e240b56075db2a006652a7351dd0",
            "5405faeba95403c669288bea4eb1959fdc528c61",
        ),
        (
            32,
            "658390b7b7a6fbd1d86a820d8787a8e1",
            "6091165d09650c4a59c5d5b5646992a12886c51e",
        ),
        (
            33,
            "b28343990d2b602363dd30992c729349",
            "f85c0b6abf96e62f14149d2cd2e140da9cac0094",
        ),
        (
            47,
            "d4c57c25bac72da7c9434ebd7d6e9a93",
            "903049b5b68a75703c7cbf4279e2797ab6054482",
        ),
        (
            48,
            "b69100212e9f0ad816852e0a8f8f57e8",
            "b1bc0ab0dd5b9d000dc8e12b5c3026a10cbfe2eb",
        ),
        (
            49,
            "20d3ef47cf3a984a67ab247390b43606",
            "e1930d52eeb588b95bf7064347c719340ebcdc42",
        ),
        (
            55,
            "c69942e2b09034f44f00badb84fa2d93",
            "75f6e327c514be052bdd410fdc0f6bed2e703249",
        ),
        (
            56,
            "52cea32a938fe11167af0c14eb824482",
            "056b3f24e189536f8819b8e6be02a4376288de08",
        ),
        (
            57,
            "9ce0ec64745cddfdd11cde24d67406e0",
            "34a7a407763c224a3a9c8be376d3fbedb853d367",
        ),
        (
            63,
            "f54b7cd8a0598976a44138eb836e7f60",
            "88d3b31ea17bd3487da4352d8d44e72843f4c112",
        ),
        (
            64,
            "dfa7b1d0481876a8902e22edbdae6503",
            "c5c50e8d87bdc081dd5c2c1e6dbf541e8f502416",
        ),
        (
            65,
            "5d27d6854dda5d309da354fc96acce68",
            "4bc74cbe399ab68263d76278f0e4bc341696f4ea",
        ),
        (
            111,
            "db7bb40da17ef36616fea12dc4df38a2",
            "0464125dd495e65551867f9a106ebdbf63fdffea",
        ),
        (
            112,
            "54cee6c6c675e50e7f736833dc479f17",
            "906d6edba6ed56e6d6e44456a7fffff4ccdddad6",
        ),
        (
            119,
            "123714d861789dd37ed88b6ff62bafc5",
            "4db8e511326bd50148834c0b8edc61b2a123f114",
        ),
        (
            120,
            "79e8b570be5d506b33fd70a6747cfffe",
            "25c3b35237a91a73093a0b82142c1789e7f39868",
        ),
        (
            127,
            "af72f8dedd211bddeb9bf299f0628685",
            "c2210f16f11f40cfb6eb660f39434cad7b889f14",
        ),
        (
            128,
            "f1c82e64f50236da95c6c618c7fca169",
            "b217edb651754da3f74fa62b9f456575fa64988b",
        ),
        (
            129,
            "657c06f81ce10dc88ae5e075a2d9bd08",
            "5c7857de1bc7d0a144e4597766c2214b58c5f53a",
        ),
        (
            247,
            "c73ce70dd56a0de57a0dac032a3958f6",
            "775b1ec7c35e62c621a9c86ba71c1f16921411d5",
        ),
        (
            248,
            "43abc86913912acd721d675c85895f0e",
            "b64ac4576de1dfb605908f6807f33a7c668918fe",
        ),
        (
            1000,
            "0b856b8132a9fc1c126e91198fa3c057",
            "05cb46d08ac400a31a37844dcccf9811c68c3d1a",
        ),
        (
            1024,
            "f6ea4c6e7b34e714d89ee04a10db2f91",
            "7120c2405e6fbf44b791f71ac4bf322f8cdcbdf7",
        ),
        (
            1025,
            "a049f1aae1f5b610875570dc43610015",
            "023b7d84dfdbc86f3de4f2482c40eded0b631654",
        ),
        (
            1152,
            "6a2e7bcc14b5033ca29373fe5156758f",
            "96fe50949dc1f41eeefb98996d6dc96008a93cd2",
        ),
    ];

    #[test]
    fn md2_and_rmd160_match_the_reference_at_their_own_block_boundaries() {
        for (length, expected_md2, expected_rmd160) in VECTORS_TAIL {
            let data = payload(*length);
            assert_eq!(hex(&md2(&data)), *expected_md2, "md2 长度 {length}");
            assert_eq!(
                hex(&rmd160(&data)),
                *expected_rmd160,
                "rmd160 长度 {length}"
            );
        }
    }

    /// The in-repository RMD160 expectations:
    /// `minix3/crypto/external/bsd/openssl/dist/crypto/ripemd/rmdtest.c:85-103`,
    /// the eight cases (the input strings come from the same source as the
    /// MD5/MD4 suites; the last line here is **ninety** digit characters, ten
    /// more than MD5's eighty — the suites in different files are not exactly the
    /// same, so one comb cannot be dragged across all). The reference compared
    /// all eight one by one before producing other readings
    /// (`tmp/cksum-384/gen_vectors3.py`, all eight matching).
    #[test]
    fn rmd160_matches_the_vendored_suite() {
        for (text, expected) in [
            ("", "9c1185a5c5e9fc54612808977ee8f548b2258d31"),
            ("a", "0bdc9d2d256b3ee9daae347be6f4dc835a467ffe"),
            ("abc", "8eb208f7e05d987a9b044a8e98c6b087f15a0bfc"),
            ("message digest", "5d0689ef49d2fae572b881b123a85ffa21595f36"),
            (
                "abcdefghijklmnopqrstuvwxyz",
                "f71c27109c692c1b56bbdceb5b9d2865b3708dbc",
            ),
            (
                "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
                "12a053384a9c0c88e405a06c27dcf49ada62eb2b",
            ),
            (
                "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
                "b0e20b6e3116640286ed3a87a5713079b21f5189",
            ),
            (
                "12345678901234567890123456789012345678901234567890123456789012345678901234567890",
                "9b752e45573d4b39f4dbd3323cab82bf63326bfb",
            ),
        ] {
            assert_eq!(hex(&rmd160(text.as_bytes())), expected, "rmd160 {text:?}");
        }
    }

    /// Readings for one million `a`s on the last two families (the same input, so
    /// all eight families are horizontally comparable).
    #[test]
    fn the_megabyte_input_covers_the_last_two_families_too() {
        let million = alloc::vec![b'a'; 1_000_000];
        assert_eq!(hex(&md2(&million)), "8c0a09ff1216ecaf95c8130953c62efd");
        assert_eq!(
            hex(&rmd160(&million)),
            "52783243c1697bdbe16d37f97f68f08325dc1528"
        );
    }

    /// The in-repository expectation files: the seven lines of `md5test-in`,
    /// newline-stripped, correspond to the seven strings of `md5test-out`
    /// (lengths 0/1/3/14/26/62/80 are written here as well, because the shape of
    /// these seven lines is exactly how `h_hash.c` drives the reading: its
    /// `CHOMP` strips only **one** trailing newline, so the hashed input does
    /// **not** include the newline).
    #[test]
    fn md5_matches_the_in_tree_vector_file() {
        for (length, text, expected) in [
            (0, "", "d41d8cd98f00b204e9800998ecf8427e"),
            (1, "a", "0cc175b9c0f1b6a831c399e269772661"),
            (3, "abc", "900150983cd24fb0d6963f7d28e17f72"),
            (14, "message digest", "f96b697d7cb7938d525a2f31aaf161d0"),
            (
                26,
                "abcdefghijklmnopqrstuvwxyz",
                "c3fcd3d76192e4007dfb496cca67e13b",
            ),
            (
                62,
                "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
                "d174ab98d277d9f5a5611c2c9f419d9f",
            ),
            (
                80,
                "12345678901234567890123456789012345678901234567890123456789012345678901234567890",
                "57edf4a22be3c955ac49da2e2107b67a",
            ),
        ] {
            assert_eq!(text.len(), length, "抄进来的输入自身就得是那一行的长度");
            assert_eq!(hex(&md5(text.as_bytes())), expected, "md5 {text:?}");
        }
    }

    /// Three in-repository SHA1 readings: the two lines of `sha1test-in` (the
    /// second is the 56-character one, exactly the cell where padding needs an
    /// extra block) and the one million `a`s of `sha1test2-out` (`t_hash.sh`
    /// pipes `jot -s '' -b 'a' -n 1000000` into standard input, `regress()`
    /// strips only the single trailing newline, so what is hashed is **exactly
    /// one million `a`s**).
    #[test]
    fn sha1_matches_the_in_tree_vectors_including_the_megabyte_one() {
        assert_eq!(
            hex(&sha1(b"abc")),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            hex(&sha1(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
        );
        let million = alloc::vec![b'a'; 1_000_000];
        assert_eq!(
            hex(&sha1(&million)),
            "34aa973cd4c4daa4f61eeb2bdbad27316534016f"
        );
    }

    /// The MD5 and SHA256 readings for the same one million `a`s come from the
    /// reference (the same `md5c.c` and `sha2.c`). This gate pins "state is not
    /// reset while folding across blocks": a million is not a multiple of 64, and
    /// the last `sink` call is an independent padding block after the whole
    /// blocks, the same path as the length-is-a-multiple-of-64 cell.
    #[test]
    fn a_megabyte_input_folds_block_after_block_without_losing_state() {
        let million = alloc::vec![b'a'; 1_000_000];
        assert_eq!(hex(&md5(&million)), "7707d6ae4e027c70eea2a935c2296f21");
        assert_eq!(
            hex(&sha256(&million)),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    /// The 56-character case in
    /// `minix3/external/bsd/bind/dist/bin/tests/hashes/t_hashes.c`: it is the
    /// repository's **only** 56-byte MD5 expectation, pinning one crossing of
    /// `md5c.c`'s second 16-round group (`GG`) with the padding block.
    #[test]
    fn md5_matches_the_bind_test_vector_for_the_56_byte_string() {
        assert_eq!(
            hex(&md5(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "8215ef0796a20bcaaae116d3876c664a"
        );
    }

    /// Two rendering facts: lowercase, and the length is twice the digest bytes
    /// (C's `End` gets `malloc(33)`/`HASH_STRLEN`, i.e. `2 * bytes + 1`).
    #[test]
    fn the_rendering_is_lowercase_and_double_width() {
        assert_eq!(hex(&[0x0a, 0xff, 0x00]), "0aff00");
        assert_eq!(Algo::Md5.hex_digest(b"abc").len(), 32);
        assert_eq!(Algo::Sha1.hex_digest(b"abc").len(), 40);
        assert_eq!(Algo::Md2.hex_digest(b"abc").len(), 32);
        assert_eq!(Algo::Md4.hex_digest(b"abc").len(), 32);
        assert_eq!(Algo::Rmd160.hex_digest(b"abc").len(), 40);
        assert_eq!(Algo::Sha256.hex_digest(b"abc").len(), 64);
        assert_eq!(Algo::Sha384.hex_digest(b"abc").len(), 96);
        assert_eq!(Algo::Sha512.hex_digest(b"abc").len(), 128);
    }

    /// All eight are recognized; no outside spelling is. The common spelling
    /// `ripemd160` is **not** in C's table (`cksum.c:120-144` has only `rmd160`),
    /// so its `None` is the criterion rather than an omission — C reports
    /// `illegal argument to -a option` for it. Case-**insensitivity** also
    /// follows C: its `case 'a'` uses `strcasecmp` (`cksum.c:201-207`), so
    /// `-a md5`, `-a MD5`, and `-a Md5` are the same entry in C — there is only
    /// this one name table in the repository, and case normalization happens here
    /// too, with no second seam normalizing again.
    #[test]
    fn all_eight_names_answer_and_no_other_spelling_does() {
        assert_eq!(
            Algo::from_hashname("MD5")
                .map(|algo| algo.hex_digest(b"abc"))
                .as_deref(),
            Some("900150983cd24fb0d6963f7d28e17f72")
        );
        for name in [
            "RIPEDM160",
            "ripemd160",
            "CRC",
            "OLD1",
            "SHA224",
            "MD1",
            "SHA2",
            "",
        ] {
            assert!(
                Algo::from_hashname(name).is_none(),
                "{name} 的本体没接或不是摘要名，不该给出算法"
            );
        }
        // Several spellings of one name point to the same algorithm, and
        // `hashname()` returns the uppercase copy from the table.
        for spelling in ["md5", "MD5", "Md5", "sHa1", "rmd160", "RMD160"] {
            let Some(algo) = Algo::from_hashname(spelling) else {
                panic!("{spelling} 该按 C 的 strcasecmp 认出来");
            };
            assert_eq!(
                algo.hashname().to_ascii_lowercase(),
                spelling.to_ascii_lowercase()
            );
        }
        for (name, width) in [
            ("MD2", 32),
            ("MD4", 32),
            ("MD5", 32),
            ("RMD160", 40),
            ("SHA1", 40),
            ("SHA256", 64),
            ("SHA384", 96),
            ("SHA512", 128),
        ] {
            assert_eq!(
                Algo::from_hashname(name).map(|algo| algo.hex_digest(b"abc").len()),
                Some(width),
                "{name} 已接本体，输出宽度就该是 2 × 摘要字节数"
            );
        }
    }
}
