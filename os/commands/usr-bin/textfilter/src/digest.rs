//! `cksum -a` 的摘要本体：MD5、SHA1、SHA256 三族的小写十六进制摘要。
//!
//! 三族里前两族**不在这里重写**。本仓已经有它们：`os/libs/minix-crypt` 的
//! [`md5`](minix_crypt::md5) 与 [`sha1`](minix_crypt::sha1)，两份的真源同样是
//! `minix3` 那三份 C 原件里的 `minix3/common/lib/libc/md/md5c.c`（`MD5Transform`）与
//! `minix3/common/lib/libc/hash/sha1/sha1.c`（`SHA1Transform`）。同一个工作区里放两份
//! MD5 是净损失——它会让人去问「哪一份是真的」，而这个问题本来不该存在。所以本模块只做
//! 两件事：**接上已经有的**，以及**补上确实没有的**——SHA256 在 `minix-crypt` 里没有
//! （那里只有 `md5.rs`、`md5crypt.rs`、`sha1.rs`、`des.rs`、`bcrypt.rs`），于是按
//! `minix3/common/lib/libc/hash/sha2/sha2.c` 的 `SHA256_Transform` 在此新译。
//!
//! 剩下五枚算法（`md2`、`md4`、`rmd160`、`sha384`、`sha512`）的真源是
//! `minix3/lib/libc/hash/md2/md2.c`、`minix3/common/lib/libc/md/md4c.c`、
//! `minix3/common/lib/libc/hash/rmd160/rmd160.c` 与 `sha2.c` 的 64 位那一半，都还在
//! `CKSUM-HASH-DIGEST` 的后一片里。
//!
//! 把三族接进同一道缝的收益不只是少一份实现：**向量门从此一次覆盖三族**。`minix-crypt`
//! 那两份自己带的判据并不等重——`sha1.rs` 有 `test_sha1_vectors`，而 `md5.rs` 一枚测试也没有
//!（现查 `grep -c "#\[test\]" os/libs/minix-crypt/src/md5.rs` → 0），且两处的向量都是短输入。
//! 本模块末尾那十五个长度按 64 字节块的边界取，再加一百万字节那一枚，覆盖的正是「跨块
//! 折叠 + 填充换块」这一类短向量测不到的形状。
//!
//! 三族的骨架相同：把输入按 64 字节切块，末尾补上「一个 `0x80`、若干 `0x00`、8 字节的
//! 比特数」，逐块折叠进一个状态向量。次序在这里是承重的——`0x80` 必须紧跟最后一个数据
//! 字节，长度必须落在最后一块的 56..64。把长度写成「补完之后的总长」，或者把 `0x80`
//! 放到零填充的另一头，摘要值不会报错，只会让每个输入都变成另一个摘要；这类缺陷只能靠
//! 已知输入→已知摘要的对照钉住。族与族真正不一致的地方在字节序：
//!
//! * 长度字段：MD 族记小端（`md5c.c` 的 `Encode` 从低位字节起写），SHA 族记大端
//!   （`SHA1Final` 那句注释自称 `Endian independent`，按 `(3-(i&3))*8` 位移取字节）。
//! * 摘要本身：MD5 把四个状态字按小端摊成 16 字节，SHA1/SHA256 按大端摊。
//!
//! 也就是说同一份输入在 x86-64/AArch64/RISC-V 上必须得到同一串十六进制——字节序是
//! **算法规定的**，不是机器决定的，所以本模块只把显式的 `to_be_bytes`/`from_be_bytes`
//! 交给自己那一份（SHA256），另两族交给 `minix-crypt` 的对应实现。
//!
//! 报文那一半（`MD5 (file) = <digest>` 与它的 `-n`/`-q` 两个变体、标准输入那一腿）住在
//! `src/bin/cksum.rs`；排版的判据是 C 的 `End`（`minix3/lib/libc/md/mdXhl.c`、
//! `minix3/lib/libc/hash/hashhl.c`）：逐字节先高四位后低四位、查 `0123456789abcdef`、
//! **小写**，长度恒为 `2 * 摘要字节数`。

use alloc::string::String;

/// `sha2.c` 的 `K256[64]`（「Hash constant words K for SHA-256」那一份表）。
/// 逐条机械抄自源文件，不按 `floor(2^32 · frac(p))` 的前八个素数小数重算——重算会把
/// 「抄错」和「算法本就如此」混成一团，而源文件本身就是判据。
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

/// `sha2.c` 的 `sha256_initial_hash_value[8]`，即 `SHA256_Init` 装进状态的初值。
/// SHA512 那一族的八个初值不同，且是 64 位字，仍在后一片里。
static SHA256_H0: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// 把 `data` 连同「`0x80` + 零填充 + 8 字节**大端**比特数」这一段一起切成 64 字节的块，
/// 逐块交给 `sink`——SHA 两族的形状（`sha1.c` 的 `SHA1Final` 与 `sha2.c` 的
/// `SHA256_SoftOrDigestFinal` 都是这一式）。MD 族的长度记小端，那一族目前走
/// `minix-crypt` 自己的填充腿；`CKSUM-HASH-DIGEST` 后一片真要接 MD4 的本地实现时，
/// 需要的是这个函数的一个小端变体，而不是在这里加一条没人走的分支。
///
/// `rem >= 56` 那一格是这段代码的全部难度所在：`0x80` 落在 `rem`，长度要占 56..64，
/// 两者一挤就得再补一整块纯零。判据写成 `>= 56` 而不是 `> 56`，因为 `rem == 56` 时
/// `0x80` 已经站到 56 号位上，长度无处可放。
fn feed_blocks(data: &[u8], sink: &mut impl FnMut(&[u8])) {
    let whole = data.len() / 64;
    let rem = data.len() % 64;
    for index in 0..whole {
        sink(&data[index * 64..(index + 1) * 64]);
    }

    let bits = (data.len() as u64).wrapping_mul(8);
    let mut tail = [0u8; 64];
    tail[..rem].copy_from_slice(&data[whole * 64..]);
    tail[rem] = 0x80;
    if rem >= 56 {
        sink(&tail);
        tail = [0u8; 64];
    }
    tail[56..].copy_from_slice(&bits.to_be_bytes());
    sink(&tail);
}

/// `sha2.c` 的 `SHA256_Transform`：64 轮，八个寄存器各自有名（`SH256R` 宏传的是
/// `a`…`h` 八个），字扩展用 `sigma0_256`/`sigma1_256` 那两个「两次右旋加一次右移」。
/// 右旋与右移各是谁（`rotate_right` 对 `>>`）不能混：`sigma0_256(x) = ROTR(7)^ROTR(18)^(x>>3)`
/// 里第三项是**移位**，写成 `ROTR(3)` 就把挪走的 3 位重新拉回来。
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

/// MD5 的 16 字节摘要：走 `minix-crypt` 已有的那份流式实现（`Md5::new`/`update`/`finalize`
/// 对位 C 的 `MD5Init`/`MD5Update`/`MD5Final`）。
fn md5(data: &[u8]) -> [u8; 16] {
    let mut context = minix_crypt::md5::Md5::new();
    context.update(data);
    context.finalize()
}

/// SHA1 的 20 字节摘要：走 `minix-crypt` 已有的那份 `sha1`（FIPS 180-1）。
fn sha1(data: &[u8]) -> [u8; 20] {
    minix_crypt::sha1::sha1(data)
}

/// SHA256 的 32 字节摘要：本模块新译的那一份（`sha2.c` 的 `SHA256_Final` 交出八个状态字
/// 按大端摊开的 32 字节）。
fn sha256(data: &[u8]) -> [u8; 32] {
    let mut state = SHA256_H0;
    feed_blocks(data, &mut |block| {
        sha256_transform(&mut state, block);
    });
    let mut digest = [0u8; 32];
    for (index, word) in state.iter().enumerate() {
        digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    digest
}

/// C 的 `End`（`mdXhl.c:44-66`、`hashhl.c` 的同名函数）排版摘要的那一步：逐字节取高
/// 四位再取低四位，查 `0123456789abcdef`，**小写**。大写的那一份只出现在报文头的算法
/// 名里（`cksum.c` 的 `hash->hashname`），别把两处混成一种。
fn hex(digest: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(digest.len() * 2);
    for byte in digest {
        text.push(DIGITS[usize::from(byte >> 4)] as char);
        text.push(DIGITS[usize::from(byte & 0x0f)] as char);
    }
    text
}

/// 已经接了摘要本体的算法。名字用 C 的 `hashes[]` 表里 `hashname` 那一列（大写）来认，
/// 因为选择半已经把用户输入归一成它了（`src/cksum.rs` 的 `crypto_hashname`）。
/// `md2`、`md4`、`rmd160`、`sha384`、`sha512` 五枚还在这个枚举之外：它们的本体在
/// `CKSUM-HASH-DIGEST` 的后一片里，薄壳拿到 `None` 才保留自己的占位报文，而不是印一枚
/// 空输入的摘要冒充结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algo {
    Md5,
    Sha1,
    Sha256,
}

impl Algo {
    /// `hashname` → 已接本体的算法；认不出的一律 `None`（包括那五枚没接的，也包括
    /// `crc`/`old1`/`old2` 这种非摘要名字——它们根本不该走到这条缝上）。
    pub fn from_hashname(hashname: &str) -> Option<Algo> {
        match hashname {
            "MD5" => Some(Algo::Md5),
            "SHA1" => Some(Algo::Sha1),
            "SHA256" => Some(Algo::Sha256),
            _ => None,
        }
    }

    /// 小写十六进制的那一行摘要正文，长度是 `2 * 摘要字节数`。
    pub fn hex_digest(self, data: &[u8]) -> String {
        match self {
            Algo::Md5 => hex(&md5(data)),
            Algo::Sha1 => hex(&sha1(data)),
            Algo::Sha256 => hex(&sha256(data)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Algo, hex, md5, sha1, sha256};

    /// 本模块的门只认两类出处：仓库里的 C 期望值文件，以及**用本仓 C 原件编出来的参照件**
    /// 的读数（配方记在 `rewrite-notes/18-stage-commands/todo.md` 批次三百八十四；参照件
    /// 先复现了下面这几枚仓库内期望值，才被用来出别的长度上的读数）。宿主 libc 或宿主
    /// OpenSSL 的任何哈希实现都不作真值。
    const ALPHA: &[u8] = b"abcdefghijklmnopqrstuvwxyz";

    /// 参照件与仓库期望值共同覆盖的输入形状：长度按 64 字节块的边界取。
    /// `(长度, md5, sha1, sha256)`，输入是 `ALPHA` 循环填充的前 `长度` 个字节。
    /// 长度 0/1/3 三行的期望同时出现在 `minix3/tests/lib/libc/hash/data/md5test-out`
    /// 与 `sha1test-out`（那三行的输入就是 `""`/`a`/`abc`），所以这张表自己就带一根
    /// 钉到仓库内的桩。
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

    /// 块边界与填充判据：长度 55/56/57 三行是 `rem >= 56` 那格的正反两面（55 时 `0x80`
    /// 与长度挤在同一块里，56 起要多补一整块纯零），63/64/65 是「满块」的三面，
    /// 119/120/127/128/129 是跨两块的同一条判据。少一条就漏一整类输入。
    /// 三族同表比对本模块那三个入口，所以这枚门同时也钉 `minix-crypt` 那两份的填充腿。
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

    /// 仓库里的期望值文件：`md5test-in` 的七行逐行去换行后对应 `md5test-out` 的七串
    /// （长度 0/1/3/14/26/62/80 一并写在这里，因为这七行的形状正是驱动 `h_hash.c` 的读法：
    /// 它的 `CHOMP` 只剥行尾那**一个**换行，所以被摘要的输入**不含**换行）。
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

    /// SHA1 的三条仓库内读数：`sha1test-in` 的两行（第二行是 56 个字符那一枚，正是
    /// 填充要多补一块的那一格）与 `sha1test2-out` 那一百万个 `a`（`t_hash.sh` 用
    /// `jot -s '' -b 'a' -n 1000000` 灌进标准输入，`regress()` 只剥掉行尾那一个换行，
    /// 所以被摘要的是**整整一百万个 `a`**）。
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

    /// MD5 与 SHA256 对同一串一百万个 `a` 的读数出自参照件（同一份 `md5c.c` 与
    /// `sha2.c`）。这枚门钉的是「跨块折叠时状态没被重置」：一百万不是 64 的整数倍，
    /// 最后一条 `sink` 走的是整块之后的独立填充块，与长度 64 整数倍那格同一条路。
    #[test]
    fn a_megabyte_input_folds_block_after_block_without_losing_state() {
        let million = alloc::vec![b'a'; 1_000_000];
        assert_eq!(hex(&md5(&million)), "7707d6ae4e027c70eea2a935c2296f21");
        assert_eq!(
            hex(&sha256(&million)),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    /// `minix3/external/bsd/bind/dist/bin/tests/hashes/t_hashes.c` 里的 56 字符那一枚：
    /// 它是仓库内**唯一**一份 MD5 的 56 字节期望值，钉的是 `md5c.c` 第二个 16 轮组
    /// （`GG`）与填充那块的一个交点。
    #[test]
    fn md5_matches_the_bind_test_vector_for_the_56_byte_string() {
        assert_eq!(
            hex(&md5(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "8215ef0796a20bcaaae116d3876c664a"
        );
    }

    /// 排版的两条：小写、且长度是摘要字节数的两倍（C 的 `End` 给 `malloc(33)`/
    /// `HASH_STRLEN`，即 `2 * 字节数 + 1`）。
    #[test]
    fn the_rendering_is_lowercase_and_double_width() {
        assert_eq!(hex(&[0x0a, 0xff, 0x00]), "0aff00");
        assert_eq!(Algo::Md5.hex_digest(b"abc").len(), 32);
        assert_eq!(Algo::Sha1.hex_digest(b"abc").len(), 40);
        assert_eq!(Algo::Sha256.hex_digest(b"abc").len(), 64);
    }

    /// 名字表只认已经接了本体的三枚；其余五枚（`MD2`/`MD4`/`RMD160`/`SHA384`/`SHA512`）
    /// 与两个非摘要名字都认不出。小写的 `md5` 也认不出是**契约的一部分**：归一大小写是
    /// 选择半（`src/cksum.rs` 的 `crypto_hashname`）的活，两道缝各管一段，这里再归一一次
    /// 就等于允许调用方把两个口径都走一遍。
    #[test]
    fn only_the_ported_families_answer_and_the_rest_decline() {
        assert_eq!(
            Algo::from_hashname("MD5")
                .map(|algo| algo.hex_digest(b"abc"))
                .as_deref(),
            Some("900150983cd24fb0d6963f7d28e17f72")
        );
        for name in [
            "MD2", "MD4", "RMD160", "SHA384", "SHA512", "CRC", "OLD1", "md5", "SHA224", "",
        ] {
            assert!(
                Algo::from_hashname(name).is_none(),
                "{name} 的本体没接或不是摘要名，不该给出算法"
            );
        }
    }
}
