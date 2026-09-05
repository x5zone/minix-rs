//! Archive integrity checksums behind one trait.
//!
//! Every archive and compression format carries an integrity field: the
//! writer stores a checksum of the original bytes, the reader recomputes
//! and compares. Two functions cover the stage's formats (cyclic
//! redundancy checks for `gzip`/`zip`/`cksum`, Adler's checksum for `zlib`
//! streams): both answer "what is the check value of these bytes?", so
//! both implement [`Checksum`]. The program layer depends on the trait and
//! stays unchanged while formats come and go.

/// One integrity function: feed bytes in any chunking, read the check
/// value at the end.
pub trait Checksum {
    /// Feed one chunk; chunks may split anywhere.
    fn update(&mut self, chunk: &[u8]);
    /// The check value of everything fed so far.
    fn digest(&self) -> u32;
}

/// Cyclic redundancy check (ISO 3309 polynomial, reflected): the integrity
/// field of `gzip`, `zip`, and `cksum`. Bit by bit table driven (256 entry
/// table built on first use... without allocation the table is computed
/// once into a static-equivalent owned array passed by the caller — see
/// [`Crc32::new`] which builds its own table inline at construction).
pub struct Crc32 {
    table: [u32; 256],
    state: u32,
}

impl Crc32 {
    /// A new check starting from the standard initial value.
    pub fn new() -> Self {
        let mut table = [0u32; 256];
        let mut n = 0;
        while n < 256 {
            let mut value = n as u32;
            for _ in 0..8 {
                value = if value & 1 == 1 {
                    0xEDB8_8320 ^ (value >> 1)
                } else {
                    value >> 1
                };
            }
            table[n] = value;
            n += 1;
        }
        Crc32 { table, state: 0xFFFF_FFFF }
    }
}

impl Default for Crc32 {
    fn default() -> Self {
        Self::new()
    }
}

impl Checksum for Crc32 {
    fn update(&mut self, chunk: &[u8]) {
        for byte in chunk {
            let index = ((self.state ^ (*byte as u32)) & 0xFF) as usize;
            self.state = self.table[index] ^ (self.state >> 8);
        }
    }

    fn digest(&self) -> u32 {
        self.state ^ 0xFFFF_FFFF
    }
}

/// Adler's checksum (modular sums): the integrity field of `zlib` streams.
/// Cheaper than the cyclic check, weaker against short burst errors — the
/// classic speed versus strength trade the two implementations embody.
pub struct Adler32 {
    low: u32,
    high: u32,
}

impl Adler32 {
    /// A new check starting from one (the specified initial value).
    pub fn new() -> Self {
        Adler32 { low: 1, high: 0 }
    }
}

impl Default for Adler32 {
    fn default() -> Self {
        Self::new()
    }
}

impl Checksum for Adler32 {
    fn update(&mut self, chunk: &[u8]) {
        const BASE: u32 = 65521;
        for byte in chunk {
            self.low = (self.low + *byte as u32) % BASE;
            self.high = (self.high + self.low) % BASE;
        }
    }

    fn digest(&self) -> u32 {
        (self.high << 16) | self.low
    }
}

/// Hash one whole slice with any checksum (test and one shot helper).
pub fn hash<C: Checksum + Default>(bytes: &[u8]) -> u32 {
    let mut check = C::default();
    check.update(bytes);
    check.digest()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc32_known_vector() {
        // The universally agreed check value of nine digits.
        assert_eq!(hash::<Crc32>(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn test_adler32_known_vector() {
        assert_eq!(hash::<Adler32>(b"123456789"), 0x091E_01DE);
    }

    #[test]
    fn test_chunking_does_not_matter() {
        let mut whole = Crc32::new();
        whole.update(b"hello world");
        let mut split = Crc32::new();
        split.update(b"hello ");
        split.update(b"world");
        assert_eq!(whole.digest(), split.digest());
        let mut whole = Adler32::new();
        whole.update(b"hello world");
        let mut split = Adler32::new();
        split.update(b"hello ");
        split.update(b"world");
        assert_eq!(whole.digest(), split.digest());
    }

    #[test]
    fn test_empty_inputs() {
        assert_eq!(hash::<Crc32>(b""), 0);
        assert_eq!(hash::<Adler32>(b""), 1);
    }

    #[test]
    fn test_checksums_share_the_trait() {
        let mut first = Crc32::new();
        let mut second = Adler32::new();
        {
            let sums: [&mut dyn Checksum; 2] = [&mut first, &mut second];
            for sum in sums {
                sum.update(b"archive payload");
            }
        }
        // Distinct algorithms must disagree on non trivial input.
        assert_ne!(first.digest(), second.digest());
    }
}
