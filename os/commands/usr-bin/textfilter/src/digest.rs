//! `cksum -a` 的摘要本体：八枚算法里已经接上六枚——MD4、MD5、SHA1、SHA256、SHA384、SHA512。
//!
//! 其中两族**不在这里重写**，因为本仓已经有它们：`os/libs/minix-crypt` 的
//! [`md5`](minix_crypt::md5) 与 [`sha1`](minix_crypt::sha1)，两份的真源同样是本仓 C 原件
//! `minix3/common/lib/libc/md/md5c.c`（`MD5Transform`）与
//! `minix3/common/lib/libc/hash/sha1/sha1.c`（`SHA1Transform`）。同一个工作区里放两份
//! MD5 是净损失——它会让人去问「哪一份是真的」，而这个问题本来不该存在。本模块因此只做
//! 两件事：**接上已经有的**，以及**补上确实没有的**。剩下四枚在本模块新译，出处逐一对位：
//!
//! * MD4 —— `minix3/common/lib/libc/md/md4c.c` 的 `MD4Transform`；
//! * SHA256、SHA384、SHA512 —— `minix3/common/lib/libc/hash/sha2/sha2.c` 的
//!   `SHA256_Transform` 与 `SHA512_Transform`（SHA384 与 SHA512 共用后一颗芯，只换初值
//!   并截去尾部 16 字节，`sha2.c` 自己就是这么写的）。
//!
//! 还差的只有 `md2` 与 `rmd160`（真源 `minix3/lib/libc/hash/md2/md2.c` 与
//! `minix3/common/lib/libc/hash/rmd160/rmd160.c`），在 `CKSUM-HASH-DIGEST` 的第三片里。
//!
//! 把多族接进同一道缝的收益不只是少几份实现：**向量门从此一次覆盖全部已接的族**，而
//! `minix-crypt` 那两份自带的判据并不等重——`sha1.rs` 有 `test_sha1_vectors`，`md5.rs` 一枚
//! 测试也没有（现查 `grep -c "#\[test\]" os/libs/minix-crypt/src/md5.rs` → 0），且两处的
//! 向量都是短输入。本模块末尾那些按 64/128 字节块边界取的长度，再加一百万字节那一枚，
//! 覆盖的正是「跨块折叠 + 填充换块」这一类短向量测不到的形状。
//!
//! 各族的骨架相同：把输入按整块切好，末尾补上「一个 `0x80`、若干 `0x00`、末尾的比特数」，
//! 逐块折叠进一个状态向量。次序在这里是承重的——`0x80` 必须紧跟最后一个数据字节，比特数
//! 必须落在最后一块的最末尾。把长度写成「补完之后的总长」，或者把 `0x80` 放到零填充的
//! 另一头，摘要值不会报错，只会让每个输入都变成另一个摘要；这类缺陷只能靠已知输入→已知
//! 摘要的对照钉住。族与族真正不一致的地方只有三处，全在「末尾那个字段」和「状态怎么摊成
//! 字节」上：
//!
//! * 块长：MD 族与 SHA-256 是 64 字节，SHA-384/512 是 128 字节。
//! * 比特数字段：MD 族记 8 字节**小端**（`md5c.c`/`md4c.c` 的 `Encode` 从低位字节起写），
//!   SHA-1/256 记 8 字节大端（`SHA1Final` 那句注释自称 `Endian independent`，它按
//!   `(3-(i&3))*8` 位移取字节），SHA-512 记 **16 字节**大端（`sha2.c` 把 128 位计数器拆成
//!   两个 `u64`，短输入时高 8 字节恒零）。
//! * 摘要摊成字节：MD 族按小端摊，SHA 族按大端摊；SHA384 取 SHA512 那 64 字节的前 48 字节。
//!
//! 也就是说同一份输入在 x86-64/AArch64/RISC-V 上必须得到同一串十六进制——字节序是
//! **算法规定的**，不是机器决定的，所以本模块只走显式的 `to_be_bytes`/`from_le_bytes`，
//! 一处也不交给宿主决定。
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

/// `md4c.c` 的 48 条 `FF`/`GG`/`HH` 行里的消息字下标；机械摘取自源文件（同 `SHA256_K` 的理由）。
static MD4_WORD: [usize; 48] = [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
    0x00, 0x04, 0x08, 0x0c, 0x01, 0x05, 0x09, 0x0d, 0x02, 0x06, 0x0a, 0x0e, 0x03, 0x07, 0x0b, 0x0f,
    0x00, 0x08, 0x04, 0x0c, 0x02, 0x0a, 0x06, 0x0e, 0x01, 0x09, 0x05, 0x0d, 0x03, 0x0b, 0x07, 0x0f,
];

/// 同上 48 条行的旋转量（`md4c.c` 的 `S11`…`S34`）。
static MD4_SHIFT: [u32; 48] = [
    0x03, 0x07, 0x0b, 0x13, 0x03, 0x07, 0x0b, 0x13, 0x03, 0x07, 0x0b, 0x13, 0x03, 0x07, 0x0b, 0x13,
    0x03, 0x05, 0x09, 0x0d, 0x03, 0x05, 0x09, 0x0d, 0x03, 0x05, 0x09, 0x0d, 0x03, 0x05, 0x09, 0x0d,
    0x03, 0x09, 0x0b, 0x0f, 0x03, 0x09, 0x0b, 0x0f, 0x03, 0x09, 0x0b, 0x0f, 0x03, 0x09, 0x0b, 0x0f,
];

/// `md4c.c` 的 `MD4Init` 四枚初值。它与 MD5 的四枚同值，两份 C 源各自写了一遍，这里也写两份。
static MD4_H0: [u32; 4] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476];

/// `sha2.c` 的 `K512[80]`。
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

/// `sha2.c` 的 `sha512_initial_hash_value[8]`。
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

/// `sha2.c` 的 `sha384_initial_hash_value[8]`：与 SHA512 同芯，只有这颗初值和输出长度不同。
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

/// 末尾那个「比特数」字段的形状。三族的差异全在这里，而这一处错了不报错、只换摘要：
/// MD 族写 8 字节小端（`md4c.c`/`md5c.c` 的 `Encode`），SHA-1/256 写 8 字节大端
///（`sha1.c` 的 `SHA1Final`），SHA-384/512 写 **16 字节**大端（`sha2.c` 把计数器拆成
/// 两个 `u64`，高那半在本仓会遇到的输入长度下恒零，所以这里只写低半、高半留给零初始化的缓冲）。
#[derive(Clone, Copy)]
enum Counter {
    Le64,
    Be64,
    Be128,
}

impl Counter {
    /// 这个字段占几个字节——填充是否还挤得进当前块就靠它。
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

/// 把 `data` 连同「`0x80` + 零填充 + 末尾的比特数」这一段一起切成 `block_len` 字节的块，
/// 逐块交给 `sink`。`block_len` 取 64（MD 族与 SHA-256）或 128（SHA-384/512）。
///
/// 「挤不挤得下」是这段代码的全部难度：`0x80` 站在 `rem`，比特数要占块尾那
/// `counter.len()` 格，于是放得下的条件是 `rem + counter.len() + 1 <= block_len`；
/// 64 字节块配 8 字节字段就是 `rem <= 55`，128 字节块配 16 字节字段就是 `rem <= 111`。
/// 边界写成 `>`（不满足就先把这一块交出去、再补一整块纯零），而不是「`>= 56`」这种把
/// 两个常数焊在一起的写法——`rem` 恰为 56（或 112）时 `0x80` 已经站到字段的位置上，
/// 这时候还往同一块里写长度就会把填充覆盖掉，摘要不报错、只是每份输入都变成另一个值。
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

/// `md4c.c` 的 `MD4Transform`：48 轮，三个 16 轮的组各换一个布尔函数与一个常数
///（第一组加零、第二组加 `0x5a827999`、第三组加 `0x6ed9eba1`——**第三组那个数不是**
/// MD5 的族常数，也不是常见教科书里那个 `0x6d70756d`；它逐字取自 `md4c.c` 的 `HH` 宏）。
/// 与 MD5 的差别只在每轮末尾：MD4 的 `FF` 宏没有 `(a) += (b)` 那一句，所以轮次里
/// 交回寄存器的是旋转后的值本身。
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

/// `sha2.c` 的 `SHA512_Transform`：80 轮、128 字节块、`u64` 字。四个复合函数里的
/// 右旋位数逐一对 `sha2.c` 的 `Sigma0_512`/`Sigma1_512`/`sigma0_512`/`sigma1_512`：
/// `28/34/39`、`14/18/41`、`1/8` 加**右移** `7`、`19/61` 加**右移** `6`。
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
    feed_blocks(data, 64, Counter::Be64, &mut |block| {
        sha256_transform(&mut state, block);
    });
    let mut digest = [0u8; 32];
    for (index, word) in state.iter().enumerate() {
        digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    digest
}

/// SHA-512 那 64 字节摘要的共用芯：`sha2.c` 的 `SHA512_Data`/`SHA384_Data` 也是同一颗芯
/// 换初值（`SHA384_Init` 装 `sha384_initial_hash_value`，`SHA512_Init` 装
/// `sha512_initial_hash_value`）。
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

/// SHA512 的 64 字节摘要。
fn sha512(data: &[u8]) -> [u8; 64] {
    sha512_with(data, SHA512_H0)
}

/// SHA384 的 48 字节摘要：`sha2.c` 的 `SHA384_Final` 只把前六个状态字写出去，
/// 也就是取 SHA512 那 64 字节的前 48 字节——不是「另算一个 384 位摘要」。
fn sha384(data: &[u8]) -> [u8; 48] {
    let full = sha512_with(data, SHA384_H0);
    let mut digest = [0u8; 48];
    digest.copy_from_slice(&full[..48]);
    digest
}

/// MD4 的 16 字节摘要（本模块新译的那一颗芯；`minix-crypt` 没有 MD4）。
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
/// 只剩 `md2` 与 `rmd160` 两枚还在这个枚举之外（`CKSUM-HASH-DIGEST` 的第三片）：薄壳拿到
/// `None` 才保留自己的占位报文，而不是印一枚空输入的摘要冒充结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algo {
    Md4,
    Md5,
    Sha1,
    Sha256,
    Sha384,
    Sha512,
}

impl Algo {
    /// `hashname` → 已接本体的算法；认不出的一律 `None`（包括那五枚没接的，也包括
    /// `crc`/`old1`/`old2` 这种非摘要名字——它们根本不该走到这条缝上）。
    pub fn from_hashname(hashname: &str) -> Option<Algo> {
        match hashname {
            "MD4" => Some(Algo::Md4),
            "MD5" => Some(Algo::Md5),
            "SHA1" => Some(Algo::Sha1),
            "SHA256" => Some(Algo::Sha256),
            "SHA384" => Some(Algo::Sha384),
            "SHA512" => Some(Algo::Sha512),
            _ => None,
        }
    }

    /// 小写十六进制的那一行摘要正文，长度是 `2 * 摘要字节数`。
    pub fn hex_digest(self, data: &[u8]) -> String {
        match self {
            Algo::Md4 => hex(&md4(data)),
            Algo::Md5 => hex(&md5(data)),
            Algo::Sha1 => hex(&sha1(data)),
            Algo::Sha256 => hex(&sha256(data)),
            Algo::Sha384 => hex(&sha384(data)),
            Algo::Sha512 => hex(&sha512(data)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Algo, hex, md4, md5, sha1, sha256, sha384, sha512};

    /// 本模块的门只认两类出处：仓库里的 C 期望值文件，以及**用本仓 C 原件编出来的参照件**
    /// 的读数（配方记在 `rewrite-notes/18-stage-commands/todo.md` 批次三百八十四与三百八十五；
    /// 参照件先复现了下面这些仓库内期望值——`md5test-out`/`sha1test-out`/`sha1test2-out`、
    /// `md4test.c` 那七条、`t_sha2.c` 那两张表——才被用来出别的长度上的读数）。
    /// 宿主 libc 或宿主 OpenSSL 的任何哈希实现都不作真值。
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

    /// 块边界与填充判据（MD5/SHA1/SHA256 那三族，块长都是 64）：长度 55/56/57 三行是
    /// 「放不放得下」那格的正反两面（55 时 `0x80` 与长度挤在同一块里，56 起要多补一整块
    /// 纯零），63/64/65 是「满块」的三面，119/120/127/128/129 是跨两块的同一条判据。
    /// 少一条就漏一整类输入。这枚门跨在两道缝上：MD5/SHA1 走 `minix-crypt` 那两份的填充腿，
    /// SHA256 走本模块那一份。
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

    /// 新接的三族按同一套规则比对待照件的读数：`md4` 与 `sha384`/`sha512`。长度除了 64 字节块
    /// 的边界（55/56/57、63/64/65），还加了 **128 字节块**的边界（111/112/119/120/127/128/129、
    /// 247/248）与整块数（1024/1025、1152）——SHA-512 那族的「放不放得下」阈值是 `rem <= 111`
    /// （块尾要让给 16 字节的比特数），与 64 字节族那条同源但数值不同；少这一组就等于
    /// 没测那半套填充。`(长度, md4, sha384, sha512)`，输入仍是字母表循环填充。
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

    /// 仓库里的 MD4 期望值只有一份，且不在 Minix 自己的测试目录里：
    /// `minix3/crypto/external/bsd/openssl/dist/crypto/md4/md4test.c:75-94` 那七条 RSA 套件
    /// ——输入与 `md5test-in` 那七行逐字同一串，正好让两族横向对照。参照件在出别的读数之前
    /// 先把这七条复现了一遍（`tmp/cksum-384/gen_vectors2.py` 的第一段，七条全合）。
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

    /// `minix3/tests/lib/libc/hash/t_sha2.c` 的两张表各取六条短的：它们钉的是**初值与截断**
    ///（两族同一颗芯，只有那颗初值和输出宽度不同），跨块的形状由上面那张边界表钉。
    /// `hello, world` 排在表首是因为它是 `sha2.c` 作者自己留的对照串，不在标准套件里。
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

    /// 一百万个 `a` 在新三族上的读数（参照件出，与 `sha1test2-out` 那枚同一条输入）：
    /// `md4` 走 64 字节块、另两族走 128 字节块，一百万对两者都不是整数倍，所以这一枚钉的是
    /// 「折叠到最后那块填充」在两种块长下都没丢状态。
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
        assert_eq!(Algo::Md4.hex_digest(b"abc").len(), 32);
        assert_eq!(Algo::Sha256.hex_digest(b"abc").len(), 64);
        assert_eq!(Algo::Sha384.hex_digest(b"abc").len(), 96);
        assert_eq!(Algo::Sha512.hex_digest(b"abc").len(), 128);
    }

    /// 名字表认六枚已接的，`MD2` 与 `RMD160` 还认不出，非摘要名字也认不出。
    /// 小写的 `md5` 也认不出是**契约的一部分**：归一大小写是
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
        for name in ["MD2", "RMD160", "CRC", "OLD1", "md5", "md4", "SHA224", ""] {
            assert!(
                Algo::from_hashname(name).is_none(),
                "{name} 的本体没接或不是摘要名，不该给出算法"
            );
        }
        for (name, width) in [
            ("MD4", 32),
            ("MD5", 32),
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
