//! Production cipher and hash: AES-256 block encryption and SHA-256.
//!
//! C correspondence: `rijndael_ecb_encrypt` under a 32-byte key
//! (`random.c:90,198`) and SHA-256 (`random.c:26,51,216-232`). Both are
//! implemented here per the published specifications (FIPS-197,
//! FIPS-180-4) and pinned by their standard test vectors — not ported
//! from the C rijndael copy, whose structure (S-box tables as C arrays,
//! pointer-walking rounds) would be a translate. Decision record:
//! `16-stage-drivers/todo.md` G4 (vendored crates rejected to keep the
//! no_std footprint and supply chain self-contained; vectors give the
//! audit trail).
//!
//! ECB single-block encryption is safe here by construction: the input
//! is a 64-bit counter (never repeating under one key), which is the
//! CTR construction the C driver uses.

use crate::core::{BlockCipher, BLOCK_SIZE, KEY_SIZE};
use crate::pool::PoolHash;

/// AES-256 block cipher (`rijndael` with a 32-byte key).
#[derive(Clone)]
pub struct Aes256 {
    round_keys: [[u8; 16]; 15],
}

const SBOX: [u8; 256] = [
    0x63, 0x7C, 0x77, 0x7B, 0xF2, 0x6B, 0x6F, 0xC5, 0x30, 0x01, 0x67, 0x2B, 0xFE, 0xD7, 0xAB, 0x76,
    0xCA, 0x82, 0xC9, 0x7D, 0xFA, 0x59, 0x47, 0xF0, 0xAD, 0xD4, 0xA2, 0xAF, 0x9C, 0xA4, 0x72, 0xC0,
    0xB7, 0xFD, 0x93, 0x26, 0x36, 0x3F, 0xF7, 0xCC, 0x34, 0xA5, 0xE5, 0xF1, 0x71, 0xD8, 0x31, 0x15,
    0x04, 0xC7, 0x23, 0xC3, 0x18, 0x96, 0x05, 0x9A, 0x07, 0x12, 0x80, 0xE2, 0xEB, 0x27, 0xB2, 0x75,
    0x09, 0x83, 0x2C, 0x1A, 0x1B, 0x6E, 0x5A, 0xA0, 0x52, 0x3B, 0xD6, 0xB3, 0x29, 0xE3, 0x2F, 0x84,
    0x53, 0xD1, 0x00, 0xED, 0x20, 0xFC, 0xB1, 0x5B, 0x6A, 0xCB, 0xBE, 0x39, 0x4A, 0x4C, 0x58, 0xCF,
    0xD0, 0xEF, 0xAA, 0xFB, 0x43, 0x4D, 0x33, 0x85, 0x45, 0xF9, 0x02, 0x7F, 0x50, 0x3C, 0x9F, 0xA8,
    0x51, 0xA3, 0x40, 0x8F, 0x92, 0x9D, 0x38, 0xF5, 0xBC, 0xB6, 0xDA, 0x21, 0x10, 0xFF, 0xF3, 0xD2,
    0xCD, 0x0C, 0x13, 0xEC, 0x5F, 0x97, 0x44, 0x17, 0xC4, 0xA7, 0x7E, 0x3D, 0x64, 0x5D, 0x19, 0x73,
    0x60, 0x81, 0x4F, 0xDC, 0x22, 0x2A, 0x90, 0x88, 0x46, 0xEE, 0xB8, 0x14, 0xDE, 0x5E, 0x0B, 0xDB,
    0xE0, 0x32, 0x3A, 0x0A, 0x49, 0x06, 0x24, 0x5C, 0xC2, 0xD3, 0xAC, 0x62, 0x91, 0x95, 0xE4, 0x79,
    0xE7, 0xC8, 0x37, 0x6D, 0x8D, 0xD5, 0x4E, 0xA9, 0x6C, 0x56, 0xF4, 0xEA, 0x65, 0x7A, 0xAE, 0x08,
    0xBA, 0x78, 0x25, 0x2E, 0x1C, 0xA6, 0xB4, 0xC6, 0xE8, 0xDD, 0x74, 0x1F, 0x4B, 0xBD, 0x8B, 0x8A,
    0x70, 0x3E, 0xB5, 0x66, 0x48, 0x03, 0xF6, 0x0E, 0x61, 0x35, 0x57, 0xB9, 0x86, 0xC1, 0x1D, 0x9E,
    0xE1, 0xF8, 0x98, 0x11, 0x69, 0xD9, 0x8E, 0x94, 0x9B, 0x1E, 0x87, 0xE9, 0xCE, 0x55, 0x28, 0xDF,
    0x8C, 0xA1, 0x89, 0x0D, 0xBF, 0xE6, 0x42, 0x68, 0x41, 0x99, 0x2D, 0x0F, 0xB0, 0x54, 0xBB, 0x16,
];

const RCON: [u8; 7] = [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40];

impl Aes256 {
    /// Expand a 32-byte key into the fifteen round keys
    /// (FIPS-197 §5.3.5, `Nk = 8`, `Nr = 14`).
    pub fn new(key: &[u8; KEY_SIZE]) -> Self {
        // The 240-byte expansion lives as eight-word columns.
        let mut w = [[0u8; 4]; 60];
        for (i, chunk) in key.chunks_exact(4).enumerate() {
            w[i].copy_from_slice(chunk);
        }
        let mut rcon_index = 0;
        for i in 8..60 {
            let mut temp = w[i - 1];
            if i % 8 == 0 {
                temp.rotate_left(1);
                for byte in temp.iter_mut() {
                    *byte = SBOX[*byte as usize];
                }
                temp[0] ^= RCON[rcon_index];
                rcon_index += 1;
            } else if i % 8 == 4 {
                for byte in temp.iter_mut() {
                    *byte = SBOX[*byte as usize];
                }
            }
            for (byte, prev) in temp.iter_mut().zip(w[i - 8].iter()) {
                *byte ^= prev;
            }
            w[i] = temp;
        }
        let mut round_keys = [[0u8; 16]; 15];
        for (round, key_word) in round_keys.iter_mut().enumerate() {
            for word in 0..4 {
                key_word[word * 4..word * 4 + 4]
                    .copy_from_slice(&w[round * 4 + word]);
            }
        }
        Aes256 { round_keys }
    }

    /// Encrypt one sixteen-byte block in place (FIPS-197 §5.1).
    fn encrypt(&self, block: &mut [u8; 16]) {
        add_round_key(block, &self.round_keys[0]);
        for round in 1..14 {
            sub_bytes(block);
            shift_rows(block);
            mix_columns(block);
            add_round_key(block, &self.round_keys[round]);
        }
        sub_bytes(block);
        shift_rows(block);
        add_round_key(block, &self.round_keys[14]);
    }
}

impl BlockCipher for Aes256 {
    fn encrypt_block(&self, key: &[u8; KEY_SIZE], input: &[u8; BLOCK_SIZE]) -> [u8; BLOCK_SIZE] {
        // The key is fixed at construction; `Aes256::new` is the key
        // schedule. This adapter keeps the trait shape the core drives.
        let _ = key;
        let mut block = *input;
        self.encrypt(&mut block);
        block
    }
}

fn add_round_key(block: &mut [u8; 16], key: &[u8; 16]) {
    for (byte, key_byte) in block.iter_mut().zip(key.iter()) {
        *byte ^= key_byte;
    }
}

fn sub_bytes(block: &mut [u8; 16]) {
    for byte in block.iter_mut() {
        *byte = SBOX[*byte as usize];
    }
}

fn shift_rows(block: &mut [u8; 16]) {
    // Row r = bytes r, r+4, r+8, r+12 rotated left by r.
    let old = *block;
    for r in 1..4usize {
        for c in 0..4usize {
            block[r + 4 * c] = old[r + 4 * ((c + r) % 4)];
        }
    }
}

fn xtime(a: u8) -> u8 {
    let doubled = (a as u16) << 1;
    if doubled & 0x100 != 0 {
        (doubled ^ 0x1B) as u8
    } else {
        doubled as u8
    }
}

fn mix_columns(block: &mut [u8; 16]) {
    // FIPS-197 §5.1.3: multiply each column by the fixed matrix over
    // GF(2^8) — 2·x = xtime(x), 3·x = xtime(x) ^ x.
    for column in block.chunks_exact_mut(4) {
        let a = [column[0], column[1], column[2], column[3]];
        let x = |b: u8| xtime(b);
        let x3 = |b: u8| xtime(b) ^ b;
        column[0] = x(a[0]) ^ x3(a[1]) ^ a[2] ^ a[3];
        column[1] = a[0] ^ x(a[1]) ^ x3(a[2]) ^ a[3];
        column[2] = a[0] ^ a[1] ^ x(a[2]) ^ x3(a[3]);
        column[3] = x3(a[0]) ^ a[1] ^ a[2] ^ x(a[3]);
    }
}

/// SHA-256 incremental hasher (`SHA256_CTX`, FIPS-180-4).
#[derive(Clone)]
pub struct Sha256 {
    state: [u32; 8],
    buffer: [u8; 64],
    buffered: usize,
    length_bits: u64,
}

impl Sha256 {
    /// Fresh hasher with the standard initial state.
    pub fn new() -> Self {
        Sha256 {
            state: [
                0x6A09E667, 0xBB67AE85, 0x3C6EF372, 0xA54FF53A, 0x510E527F, 0x9B05688C, 0x1F83D9AB,
                0x5BE0CD19,
            ],
            buffer: [0; 64],
            buffered: 0,
            length_bits: 0,
        }
    }

    fn compress(&mut self, block: &[u8; 64]) {
        let mut w = [0u32; 64];
        for (i, word) in w.iter_mut().take(16).enumerate() {
            *word = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut h = self.state;
        for i in 0..64 {
            let s1 = h[4].rotate_right(6) ^ h[4].rotate_right(11) ^ h[4].rotate_right(25);
            let ch = (h[4] & h[5]) ^ (!h[4] & h[6]);
            let temp1 = h[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = h[0].rotate_right(2) ^ h[0].rotate_right(13) ^ h[0].rotate_right(22);
            let maj = (h[0] & h[1]) ^ (h[0] & h[2]) ^ (h[1] & h[2]);
            let temp2 = s0.wrapping_add(maj);
            h[7] = h[6];
            h[6] = h[5];
            h[5] = h[4];
            h[4] = h[3].wrapping_add(temp1);
            h[3] = h[2];
            h[2] = h[1];
            h[1] = h[0];
            h[0] = temp1.wrapping_add(temp2);
        }
        for (word, h_word) in self.state.iter_mut().zip(h.iter()) {
            *word = word.wrapping_add(*h_word);
        }
    }
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

impl PoolHash for Sha256 {
    fn absorb(&mut self, bytes: &[u8]) {
        self.length_bits = self.length_bits.wrapping_add((bytes.len() as u64) * 8);
        let mut offset = 0;
        while self.buffered + (bytes.len() - offset) >= 64 {
            let take = 64 - self.buffered;
            self.buffer[self.buffered..64].copy_from_slice(&bytes[offset..offset + take]);
            let block = self.buffer;
            self.compress(&block);
            self.buffered = 0;
            offset += take;
        }
        let rest = &bytes[offset..];
        self.buffer[self.buffered..self.buffered + rest.len()].copy_from_slice(rest);
        self.buffered += rest.len();
    }

    fn snapshot_reset(&mut self, out: &mut [u8; 32]) {
        // Padding: 0x80, zeros, then the 64-bit big-endian bit length.
        let bits = self.length_bits;
        let pad = [(0x80u8)];
        self.absorb(&pad);
        self.length_bits = bits.wrapping_add(8); // absorb counted the 0x80
        while self.buffered != 56 {
            self.absorb(&[0]);
        }
        let tail = bits.to_be_bytes();
        self.absorb(&tail);
        for (word, out_chunk) in self.state.iter().zip(out.chunks_exact_mut(4)) {
            out_chunk.copy_from_slice(&word.to_be_bytes());
        }
        *self = Sha256::new();
    }
}

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aes256_fips197_appendix_c3() {
        // FIPS-197 Appendix C.3: AES-256 known answer.
        let key: [u8; KEY_SIZE] = core::array::from_fn(|i| i as u8);
        let plaintext: [u8; BLOCK_SIZE] =
            [0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD,
             0xEE, 0xFF];
        let cipher = Aes256::new(&key);
        let out = BlockCipher::encrypt_block(&cipher, &key, &plaintext);
        assert_eq!(
            out,
            [0x8E, 0xA2, 0xB7, 0xCA, 0x51, 0x67, 0x45, 0xBF, 0xEA, 0xFC, 0x49, 0x90, 0x4B, 0x49,
             0x60, 0x89]
        );
    }

    #[test]
    fn test_sha256_abc_vector() {
        // FIPS-180-4 test vector: SHA256("abc").
        let mut hash = Sha256::new();
        hash.absorb(b"abc");
        let mut out = [0u8; 32];
        hash.snapshot_reset(&mut out);
        let expected = [
            0xBA, 0x78, 0x16, 0xBF, 0x8F, 0x01, 0xCF, 0xEA, 0x41, 0x41, 0x40, 0xDE, 0x5D, 0xAE,
            0x22, 0x23, 0xB0, 0x03, 0x61, 0xA3, 0x96, 0x17, 0x7A, 0x9C, 0xB4, 0x10, 0xFF, 0x61,
            0xF2, 0x00, 0x15, 0xAD,
        ];
        assert_eq!(out, expected);
    }

    #[test]
    fn test_sha256_absorbs_spanning_block_boundaries() {
        // 64+ bytes across multiple absorb calls must match one-shot.
        let data: alloc::vec::Vec<u8> = (0..100u8).collect();
        let mut one_shot = Sha256::new();
        one_shot.absorb(&data);
        let mut want = [0u8; 32];
        one_shot.snapshot_reset(&mut want);

        let mut split = Sha256::new();
        split.absorb(&data[..40]);
        split.absorb(&data[40..70]);
        split.absorb(&data[70..]);
        let mut got = [0u8; 32];
        split.snapshot_reset(&mut got);
        assert_eq!(got, want);
    }
}
