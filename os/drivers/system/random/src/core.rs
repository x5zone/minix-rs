//! Generator core: keystream counter, rekeying, and reseed wiring.
//!
//! C correspondence: `random_getbytes` and `data_block`
//! (`random.c:81-113,181-204`) plus the reseed assembly in `reseed`
//! (`random.c:206-236`).
//!
//! The C code encrypts a sixty-four-bit counter with the current key for
//! each output block, then derives a fresh key from two more blocks
//! (backtracking resistance: a leaked key does not reveal past output).
//! The block cipher itself stays behind the [`BlockCipher`] trait:
//! production wires the platform cipher, tests use the folding cipher
//! below. Counter arithmetic, block chunking, and the reseed mix order are
//! pure and fully testable.

/// Cipher block width in bytes (the platform cipher width).
///
/// The C code uses the cipher block size throughout (`random.c`); sixteen
/// matches the deployed cipher.
pub const BLOCK_SIZE: usize = 16;

/// Key width in bytes: two blocks.
///
/// C: `random_key[2*AES_BLOCKSIZE]` (`random.c:29`).
pub const KEY_SIZE: usize = 2 * BLOCK_SIZE;

/// Block cipher behavior for the keystream: encrypt one zero-padded
/// counter block.
///
/// C: `rijndael_ecb_encrypt` over the counter input in `data_block`
/// (`random.c:198`). Reversible ciphers and one-way mixers both fit;
/// the core only needs determinism per (key, counter).
pub trait BlockCipher {
    /// Encrypt the sixteen-byte input block under this key.
    fn encrypt_block(&self, key: &[u8; KEY_SIZE], input: &[u8; BLOCK_SIZE]) -> [u8; BLOCK_SIZE];
}

/// Folding test cipher: rotates key and input together. Deterministic and
/// invertible-looking, but not cryptographic; only proves the core
/// plumbing (chunking, counter advance, rekey mixing).
#[derive(Debug, Default, Clone, Copy)]
pub struct FoldCipher;

impl BlockCipher for FoldCipher {
    fn encrypt_block(&self, key: &[u8; KEY_SIZE], input: &[u8; BLOCK_SIZE]) -> [u8; BLOCK_SIZE] {
        let mut out = [0u8; BLOCK_SIZE];
        for i in 0..BLOCK_SIZE {
            out[i] = input[i]
                .wrapping_add(key[i])
                .wrapping_add(key[KEY_SIZE - 1 - i])
                .rotate_left(3);
        }
        out
    }
}

/// Constant test cipher: every block encrypts to the key's first block.
///
/// Behavior differs from [`FoldCipher`] (which mixes input): pairs of
/// generator states that must stay distinguishable by input use different
/// ciphers in tests.
#[derive(Debug, Default, Clone, Copy)]
pub struct ConstCipher;

impl BlockCipher for ConstCipher {
    fn encrypt_block(&self, key: &[u8; KEY_SIZE], _input: &[u8; BLOCK_SIZE]) -> [u8; BLOCK_SIZE] {
        let mut out = [0u8; BLOCK_SIZE];
        out.copy_from_slice(&key[..BLOCK_SIZE]);
        out
    }
}

/// Generator state: key, sixty-four-bit counter, reseed count, seed flag.
///
/// C: `random_key`, `count_lo`, `count_hi`, `reseed_count`, `got_seeded`
/// (`random.c:29-31`).
#[derive(Debug, Clone)]
pub struct GeneratorCore {
    key: [u8; KEY_SIZE],
    counter_lo: u32,
    counter_hi: u32,
    reseeds: u64,
    seeded: bool,
}

impl GeneratorCore {
    /// Fresh core: zero key, zero counter, unseeded.
    ///
    /// C: the zeroing half of `random_init` (`random.c:52-54`).
    pub const fn new() -> GeneratorCore {
        GeneratorCore {
            key: [0; KEY_SIZE],
            counter_lo: 0,
            counter_hi: 0,
            reseeds: 0,
            seeded: false,
        }
    }

    /// True once a reseed has completed at least once.
    ///
    /// C: `random_isseeded` (`random.c:57-62`).
    pub const fn is_seeded(&self) -> bool {
        self.seeded
    }

    /// Reseeds completed so far (drives the pool walk).
    pub const fn reseed_count(&self) -> u64 {
        self.reseeds
    }

    /// Current counter as one sixty-four-bit value (test inspection).
    pub const fn counter(&self) -> u64 {
        ((self.counter_hi as u64) << 32) | self.counter_lo as u64
    }

    /// Fill the output with keystream, then rekey from two fresh blocks.
    ///
    /// C: `random_getbytes` (`random.c:81-113`): full blocks encrypt
    /// straight into the caller area, a short tail encrypts aside and
    /// copies, and the key refreshes from two blocks afterwards. Copies
    /// stay in the service crate; this method fills the provided buffer.
    pub fn generate<C: BlockCipher>(&mut self, cipher: &C, out: &mut [u8]) {
        let mut offset = 0;
        while offset < out.len() {
            let block = self.next_block(cipher);
            let take = (out.len() - offset).min(BLOCK_SIZE);
            out[offset..offset + take].copy_from_slice(&block[..take]);
            offset += take;
        }
        let first = self.next_block(cipher);
        let second = self.next_block(cipher);
        self.key[..BLOCK_SIZE].copy_from_slice(&first);
        self.key[BLOCK_SIZE..].copy_from_slice(&second);
    }

    /// Encrypt the current counter block and advance (wrapping).
    ///
    /// C: `data_block` (`random.c:181-204`): counter laid little-endian
    /// into the block head, low word advance with carry into the high
    /// word.
    pub fn next_block<C: BlockCipher>(&mut self, cipher: &C) -> [u8; BLOCK_SIZE] {
        let mut input = [0u8; BLOCK_SIZE];
        input[..4].copy_from_slice(&self.counter_lo.to_ne_bytes());
        input[4..8].copy_from_slice(&self.counter_hi.to_ne_bytes());
        let out = cipher.encrypt_block(&self.key, &input);
        let (next, carried) = self.counter_lo.overflowing_add(1);
        self.counter_lo = next;
        if carried {
            self.counter_hi = self.counter_hi.wrapping_add(1);
        }
        out
    }

    /// Mix pool digests into a fresh key following the C order.
    ///
    /// C: `reseed` (`random.c:206-236`): when already seeded, the old key
    /// joins first; pool zero always joins; further pools join per
    /// [`super::pool::reseed_extra_pools`]; every joined pool resets. The
    /// digests arrive precomputed (hashing stays behind the pool trait);
    /// this method folds them in order, marks seeded, and bumps the
    /// reseed count.
    pub fn reseed(&mut self, digests: &[&[u8; 32]]) {
        let mut mixed = self.key;
        if self.seeded {
            fold_in(&mut mixed, &self.key_as_digest());
        }
        for digest in digests {
            fold_in(&mut mixed, digest);
        }
        self.key[..32].copy_from_slice(&mixed[..32]);
        self.reseeds += 1;
        self.seeded = true;
    }

    fn key_as_digest(&self) -> [u8; 32] {
        let mut out = [0u8; 32];
        out.copy_from_slice(&self.key[..32]);
        out
    }
}

impl Default for GeneratorCore {
    fn default() -> Self {
        GeneratorCore::new()
    }
}

fn fold_in(state: &mut [u8; KEY_SIZE], digest: &[u8; 32]) {
    for i in 0..KEY_SIZE {
        state[i] ^= digest[i % 32].rotate_left(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fresh_core_is_unseeded_at_zero() {
        let core = GeneratorCore::new();
        assert!(!core.is_seeded());
        assert_eq!(core.counter(), 0);
        assert_eq!(core.reseed_count(), 0);
    }

    #[test]
    fn test_counter_advances_one_per_block_with_carry() {
        let mut core = GeneratorCore::new();
        let cipher = FoldCipher;
        core.next_block(&cipher);
        core.next_block(&cipher);
        assert_eq!(core.counter(), 2);
        core.counter_lo = u32::MAX;
        core.next_block(&cipher);
        assert_eq!(core.counter(), 1 << 32);
    }

    #[test]
    fn test_generate_chunks_and_rekeys() {
        let mut core = GeneratorCore::new();
        let cipher = FoldCipher;
        let mut out = [0u8; 40];
        core.generate(&cipher, &mut out);
        assert_eq!(core.counter(), 5);
        let mut second = [0u8; 40];
        core.generate(&cipher, &mut second);
        assert_ne!(out, second);
    }

    #[test]
    fn test_reseed_marks_seeded_and_counts() {
        let mut core = GeneratorCore::new();
        let digest = [7u8; 32];
        core.reseed(&[&digest]);
        assert!(core.is_seeded());
        assert_eq!(core.reseed_count(), 1);
        let before = core.key;
        core.reseed(&[&digest]);
        assert_ne!(core.key, before);
    }

    #[test]
    fn test_const_cipher_ignores_input() {
        let cipher = ConstCipher;
        let key = [3u8; KEY_SIZE];
        let first = cipher.encrypt_block(&key, &[0; BLOCK_SIZE]);
        let second = cipher.encrypt_block(&key, &[9; BLOCK_SIZE]);
        assert_eq!(first, second);
    }

    #[test]
    fn test_widths_match_c_layout() {
        assert_eq!(BLOCK_SIZE, 16);
        assert_eq!(KEY_SIZE, 32);
    }
}
