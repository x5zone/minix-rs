//! MD5 message digest (RFC 1321).
//!
//! Standard reference implementation of the RSA MD5 algorithm — the same
//! algorithm C carries in `minix3/common/lib/libc/md/md5c.c` and the
//! `<md5.h>` API (`MD5Init`/`MD5Update`/`MD5Final`) that
//! `md5crypt.c:28-33` consumes. Pure functions over a fixed context; no
//! allocation.

/// Digest length in bytes (`MD5_DIGEST_LENGTH`, sys/md5.h:33).
pub const DIGEST_LENGTH: usize = 16;

/// MD5 context (`MD5_CTX`, sys/md5.h:38-44).
#[derive(Debug, Clone)]
pub struct Md5 {
    state: [u32; 4],
    count: u64,
    buffer: [u8; 64],
    buffered: usize,
}

impl Md5 {
    /// `MD5Init`.
    pub fn new() -> Self {
        Self {
            state: [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476],
            count: 0,
            buffer: [0; 64],
            buffered: 0,
        }
    }

    /// `MD5Update`: absorb bytes.
    pub fn update(&mut self, mut data: &[u8]) {
        self.count = self.count.wrapping_add(data.len() as u64);
        if self.buffered > 0 {
            let take = (64 - self.buffered).min(data.len());
            self.buffer[self.buffered..self.buffered + take].copy_from_slice(&data[..take]);
            self.buffered += take;
            data = &data[take..];
            if self.buffered == 64 {
                let block = self.buffer;
                self.transform(&block);
                self.buffered = 0;
            }
        }
        while data.len() >= 64 {
            let (block, rest) = data.split_at(64);
            let mut owned = [0u8; 64];
            owned.copy_from_slice(block);
            self.transform(&owned);
            data = rest;
        }
        if !data.is_empty() {
            self.buffer[..data.len()].copy_from_slice(data);
            self.buffered = data.len();
        }
    }

    /// `MD5Final`: pad, transform, and emit the 16-byte digest.
    pub fn finalize(mut self) -> [u8; DIGEST_LENGTH] {
        let bit_count = self.count.wrapping_mul(8);
        // Pad to 56 mod 64, then the little-endian bit count.
        self.update(&[0x80]);
        while self.buffered != 56 {
            self.update(&[0]);
        }
        self.update(&bit_count.to_le_bytes());
        let mut digest = [0u8; DIGEST_LENGTH];
        for (i, word) in self.state.iter().enumerate() {
            digest[i * 4..i * 4 + 4].copy_from_slice(&word.to_le_bytes());
        }
        digest
    }

    /// One 64-byte block transform (`MD5Transform`).
    fn transform(&mut self, block: &[u8; 64]) {
        let mut x = [0u32; 16];
        for (i, word) in x.iter_mut().enumerate() {
            *word = u32::from_le_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }

        const S: [[u32; 4]; 4] = [
            [7, 12, 17, 22],
            [5, 9, 14, 20],
            [4, 11, 16, 23],
            [6, 10, 15, 21],
        ];
        // K[i] = floor(2^32 * abs(sin(i + 1))) (RFC 1321, table T).
        const K: [u32; 64] = [
            0xd76a_a478, 0xe8c7_b756, 0x2420_70db, 0xc1bd_ceee, 0xf57c_0faf, 0x4787_c62a,
            0xa830_4613, 0xfd46_9501, 0x6980_98d8, 0x8b44_f7af, 0xffff_5bb1, 0x895c_d7be,
            0x6b90_1122, 0xfd98_7193, 0xa679_438e, 0x49b4_0821, 0xf61e_2562, 0xc040_b340,
            0x265e_5a51, 0xe9b6_c7aa, 0xd62f_105d, 0x0244_1453, 0xd8a1_e681, 0xe7d3_fbc8,
            0x21e1_cde6, 0xc337_07d6, 0xf4d5_0d87, 0x455a_14ed, 0xa9e3_e905, 0xfcef_a3f8,
            0x676f_02d9, 0x8d2a_4c8a, 0xfffa_3942, 0x8771_f681, 0x6d9d_6122, 0xfde5_380c,
            0xa4be_ea44, 0x4bde_cfa9, 0xf6bb_4b60, 0xbebf_bc70, 0x289b_7ec6, 0xeaa1_27fa,
            0xd4ef_3085, 0x0488_1d05, 0xd9d4_d039, 0xe6db_99e5, 0x1fa2_7cf8, 0xc4ac_5665,
            0xf429_2244, 0x432a_ff97, 0xab94_23a7, 0xfc93_a039, 0x655b_59c3, 0x8f0c_cc92,
            0xffef_f47d, 0x8584_5dd1, 0x6fa8_7e4f, 0xfe2c_e6e0, 0xa301_4314, 0x4e08_11a1,
            0xf753_7e82, 0xbd3a_f235, 0x2ad7_d2bb, 0xeb86_d391,
        ];

        let [mut a, mut b, mut c, mut d] = self.state;
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let rotated = (a.wrapping_add(f).wrapping_add(K[i]).wrapping_add(x[g]))
                .rotate_left(S[i / 16][i % 4]);
            let temp = b.wrapping_add(rotated);
            a = d;
            d = c;
            c = b;
            b = temp;
        }
        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
    }
}

impl Default for Md5 {
    fn default() -> Self {
        Self::new()
    }
}
