//! Bitmap implementation.
//!
//! Provides fixed-size bitmap data structures for process table slot management, etc.
//!
//! # Features
//!
//! - Fixed size, determined at compile time.
//! - Zero dependencies, `no_std` compatible.
//! - Efficient bit operation implementation.
//! - Single cache line optimization (for 256-bit bitmap).
//!
//! # About Generic Size
//!
//! Current implementation uses fixed maximum capacity (512 bits) + runtime size,
//! because **Stable Rust** does not support using generic parameters in array sizes.
//!
//! If you need true compile-time generic size, you can use **Nightly Rust**'s
//! `generic_const_exprs` feature:
//!
//! ```rust,ignore
//! #![feature(generic_const_exprs)]
//!
//! pub struct Bitmap<const N: usize>
//! where
//!     [(); (N + 7) / 8]:,
//! {
//!     bits: [u8; (N + 7) / 8],
//! }
//! ```
//!
//! However, considering the stability requirements of kernel development,
//! the current approach (fixed maximum capacity) is a more practical choice.

/// Maximum bitmap size (bits).
pub const MAX_BITMAP_BITS: usize = 512;
/// Maximum bitmap bytes.
pub const MAX_BITMAP_BYTES: usize = (MAX_BITMAP_BITS + 7) / 8;

/// Generic bitmap.
///
/// Uses a fixed-size internal array, with maximum capacity determined at compile time.
///
/// # Examples
///
/// ```
/// use minix_types::{Bitmap, Bitmap256};
///
/// // 256-bit bitmap, 32 bytes (VM/PM default)
/// let mut bm256: Bitmap256 = Bitmap::new(256);
///
/// // Set and get
/// bm256.set(5, true);
/// assert!(bm256.get(5));
///
/// // Find first zero bit
/// assert_eq!(bm256.find_first_zero(), Some(0));
/// bm256.set(0, true);
/// assert_eq!(bm256.find_first_zero(), Some(1));
/// ```
///
/// # Memory Layout
///
/// | Size | Bits | Bytes | Cache Line (64B) |
/// |------|------|-------|------------------|
/// | `Bitmap256` | 256 | 32 | **1 line** |
/// | `Bitmap512` | 512 | 64 | 1 line |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bitmap {
    bits: [u8; MAX_BITMAP_BYTES],
    size: usize,
}

impl Bitmap {
    /// Creates a new empty bitmap (all bits are 0).
    ///
    /// # Panics
    ///
    /// Panics if `size > MAX_BITMAP_BITS`.
    ///
    /// # Examples
    ///
    /// ```
    /// use minix_types::Bitmap;
    ///
    /// let bm = Bitmap::new(256);
    /// assert_eq!(bm.count_ones(), 0);
    /// ```
    pub const fn new(size: usize) -> Self {
        assert!(size <= MAX_BITMAP_BITS, "bitmap size too large");
        Self {
            bits: [0; MAX_BITMAP_BYTES],
            size,
        }
    }

    /// Gets the bitmap size (bits).
    ///
    /// # Examples
    ///
    /// ```
    /// use minix_types::Bitmap;
    ///
    /// let bm = Bitmap::new(256);
    /// assert_eq!(bm.size(), 256);
    /// ```
    pub const fn size(&self) -> usize {
        self.size
    }

    /// Gets the bitmap byte count.
    ///
    /// # Examples
    ///
    /// ```
    /// use minix_types::Bitmap;
    ///
    /// let bm = Bitmap::new(256);
    /// assert_eq!(bm.byte_size(), 32);
    /// ```
    pub const fn byte_size(&self) -> usize {
        (self.size + 7) / 8
    }

    /// Gets the bit value at the specified position.
    ///
    /// # Panics
    ///
    /// Panics if `index >= size`.
    ///
    /// # Examples
    ///
    /// ```
    /// use minix_types::Bitmap;
    ///
    /// let mut bm = Bitmap::new(256);
    /// bm.set(10, true);
    /// assert!(bm.get(10));
    /// assert!(!bm.get(11));
    /// ```
    #[inline]
    pub fn get(&self, index: usize) -> bool {
        assert!(index < self.size, "index {} out of bounds (size: {})", index, self.size);
        (self.bits[index / 8] >> (index % 8)) & 1 != 0
    }

    /// Sets the bit value at the specified position.
    ///
    /// # Panics
    ///
    /// Panics if `index >= size`.
    ///
    /// # Examples
    ///
    /// ```
    /// use minix_types::Bitmap;
    ///
    /// let mut bm = Bitmap::new(256);
    /// bm.set(5, true);
    /// assert!(bm.get(5));
    /// bm.set(5, false);
    /// assert!(!bm.get(5));
    /// ```
    #[inline]
    pub fn set(&mut self, index: usize, value: bool) {
        assert!(index < self.size, "index {} out of bounds (size: {})", index, self.size);
        if value {
            self.bits[index / 8] |= 1 << (index % 8);
        } else {
            self.bits[index / 8] &= !(1 << (index % 8));
        }
    }

    /// Finds the first zero bit.
    ///
    /// Returns the index of the first bit with value `false`, or `None` if all bits are `true`.
    ///
    /// # Performance
    ///
    /// Uses `trailing_ones()` optimization, only 1 operation per byte.
    ///
    /// # Examples
    ///
    /// ```
    /// use minix_types::Bitmap;
    ///
    /// let mut bm = Bitmap::new(256);
    /// assert_eq!(bm.find_first_zero(), Some(0));
    ///
    /// bm.set(0, true);
    /// assert_eq!(bm.find_first_zero(), Some(1));
    ///
    /// // Fill first 8 bits
    /// for i in 0..8 {
    ///     bm.set(i, true);
    /// }
    /// assert_eq!(bm.find_first_zero(), Some(8));
    /// ```
    #[inline]
    pub fn find_first_zero(&self) -> Option<usize> {
        let byte_count = self.byte_size();
        for i in 0..byte_count {
            let byte = self.bits[i];
            if byte != 0xFF {
                let bit_pos = byte.trailing_ones() as usize;
                let result = i * 8 + bit_pos;
                if result < self.size {
                    return Some(result);
                }
            }
        }
        None
    }

    /// Finds the first one bit.
    ///
    /// Returns the index of the first bit with value `true`, or `None` if all bits are `false`.
    ///
    /// # Examples
    ///
    /// ```
    /// use minix_types::Bitmap;
    ///
    /// let mut bm = Bitmap::new(256);
    /// assert_eq!(bm.find_first_one(), None);
    ///
    /// bm.set(10, true);
    /// assert_eq!(bm.find_first_one(), Some(10));
    /// ```
    #[inline]
    pub fn find_first_one(&self) -> Option<usize> {
        let byte_count = self.byte_size();
        for i in 0..byte_count {
            let byte = self.bits[i];
            if byte != 0 {
                let bit_pos = byte.trailing_zeros() as usize;
                let result = i * 8 + bit_pos;
                if result < self.size {
                    return Some(result);
                }
            }
        }
        None
    }

    /// Counts the number of bits set to 1.
    ///
    /// # Examples
    ///
    /// ```
    /// use minix_types::Bitmap;
    ///
    /// let mut bm = Bitmap::new(256);
    /// assert_eq!(bm.count_ones(), 0);
    ///
    /// bm.set(0, true);
    /// bm.set(5, true);
    /// assert_eq!(bm.count_ones(), 2);
    /// ```
    pub fn count_ones(&self) -> usize {
        let byte_count = self.byte_size();
        self.bits[..byte_count]
            .iter()
            .map(|&b| b.count_ones() as usize)
            .sum()
    }

    /// Counts the number of bits set to 0.
    ///
    /// # Examples
    ///
    /// ```
    /// use minix_types::Bitmap;
    ///
    /// let mut bm = Bitmap::new(256);
    /// bm.set(0, true);
    /// bm.set(5, true);
    /// assert_eq!(bm.count_zeros(), 254);
    /// ```
    pub fn count_zeros(&self) -> usize {
        self.size - self.count_ones()
    }

    /// Clears all bits.
    ///
    /// # Examples
    ///
    /// ```
    /// use minix_types::Bitmap;
    ///
    /// let mut bm = Bitmap::new(256);
    /// bm.set(0, true);
    /// bm.clear();
    /// assert_eq!(bm.count_ones(), 0);
    /// ```
    pub fn clear(&mut self) {
        let byte_count = self.byte_size();
        self.bits[..byte_count].fill(0);
    }

    /// Fills all bits with 1.
    ///
    /// # Examples
    ///
    /// ```
    /// use minix_types::Bitmap;
    ///
    /// let mut bm = Bitmap::new(256);
    /// bm.fill();
    /// assert_eq!(bm.count_ones(), 256);
    /// ```
    pub fn fill(&mut self) {
        let byte_count = self.byte_size();
        self.bits[..byte_count].fill(0xFF);
    }

    /// Checks if the bitmap is empty (all bits are 0).
    ///
    /// # Examples
    ///
    /// ```
    /// use minix_types::Bitmap;
    ///
    /// let mut bm = Bitmap::new(256);
    /// assert!(bm.is_empty());
    ///
    /// bm.set(0, true);
    /// assert!(!bm.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        let byte_count = self.byte_size();
        self.bits[..byte_count].iter().all(|&b| b == 0)
    }

    /// Checks if the bitmap is full (all bits are 1).
    ///
    /// # Examples
    ///
    /// ```
    /// use minix_types::Bitmap;
    ///
    /// let mut bm = Bitmap::new(256);
    /// assert!(!bm.is_full());
    ///
    /// bm.fill();
    /// assert!(bm.is_full());
    /// ```
    pub fn is_full(&self) -> bool {
        let byte_count = self.byte_size();
        self.bits[..byte_count].iter().all(|&b| b == 0xFF)
    }
}

impl Default for Bitmap {
    fn default() -> Self {
        Self::new(MAX_BITMAP_BITS)
    }
}

/// 256-bit bitmap type alias.
///
/// Suitable for NR_PROCS = 256 process table management.
pub type Bitmap256 = Bitmap;

/// 512-bit bitmap type alias.
///
/// Suitable for larger process tables.
pub type Bitmap512 = Bitmap;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bitmap_basic() {
        let mut bm = Bitmap::new(256);

        assert!(!bm.get(0));
        assert!(!bm.get(127));
        assert!(!bm.get(255));

        bm.set(0, true);
        assert!(bm.get(0));

        bm.set(127, true);
        assert!(bm.get(127));

        bm.set(255, true);
        assert!(bm.get(255));

        bm.set(0, false);
        assert!(!bm.get(0));
    }

    #[test]
    fn test_bitmap_find_first_zero() {
        let mut bm = Bitmap::new(256);

        assert_eq!(bm.find_first_zero(), Some(0));

        bm.set(0, true);
        assert_eq!(bm.find_first_zero(), Some(1));

        for i in 0..8 {
            bm.set(i, true);
        }
        assert_eq!(bm.find_first_zero(), Some(8));

        bm.fill();
        assert_eq!(bm.find_first_zero(), None);
    }

    #[test]
    fn test_bitmap_find_first_one() {
        let mut bm = Bitmap::new(256);

        assert_eq!(bm.find_first_one(), None);

        bm.set(10, true);
        assert_eq!(bm.find_first_one(), Some(10));

        bm.set(5, true);
        assert_eq!(bm.find_first_one(), Some(5));
    }

    #[test]
    fn test_bitmap_count() {
        let mut bm = Bitmap::new(256);

        assert_eq!(bm.count_ones(), 0);
        assert_eq!(bm.count_zeros(), 256);

        bm.set(0, true);
        bm.set(100, true);
        bm.set(255, true);

        assert_eq!(bm.count_ones(), 3);
        assert_eq!(bm.count_zeros(), 253);
    }

    #[test]
    fn test_bitmap_clear_fill() {
        let mut bm = Bitmap::new(256);

        bm.set(0, true);
        bm.set(127, true);

        assert!(!bm.is_empty());

        bm.clear();
        assert!(bm.is_empty());
        assert!(!bm.is_full());

        bm.fill();
        assert!(!bm.is_empty());
        assert!(bm.is_full());
    }

    #[test]
    fn test_bitmap_size() {
        let bm32 = Bitmap::new(32);
        assert_eq!(bm32.size(), 32);
        assert_eq!(bm32.byte_size(), 4);

        let bm256 = Bitmap::new(256);
        assert_eq!(bm256.size(), 256);
        assert_eq!(bm256.byte_size(), 32);

        let bm512 = Bitmap::new(512);
        assert_eq!(bm512.size(), 512);
        assert_eq!(bm512.byte_size(), 64);
    }

    #[test]
    #[should_panic(expected = "out of bounds")]
    fn test_bitmap_out_of_bounds() {
        let bm = Bitmap::new(256);
        bm.get(256);
    }

    #[test]
    fn test_bitmap_default() {
        let bm = Bitmap::default();
        assert_eq!(bm.size(), MAX_BITMAP_BITS);
    }
}
