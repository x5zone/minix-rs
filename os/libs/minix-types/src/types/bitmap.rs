//! 位图实现
//!
//! 提供固定大小的位图数据结构，用于进程表槽位管理等场景。
//!
//! # 特点
//!
//! - 固定大小，编译期确定
//! - 零依赖，`no_std` 兼容
//! - 高效的位运算实现
//! - 单 cache line 优化（对于 256 位位图）
//!
//! # 关于泛型大小
//!
//! 当前实现使用固定最大容量（512 位）+ 运行时大小的方案，原因是 **Stable Rust**
//! 不支持在数组大小中使用泛型参数。
//!
//! 如果需要真正的编译期泛型大小，可以使用 **Nightly Rust** 的 `generic_const_exprs` feature：
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
//! 但考虑到内核开发对稳定性的要求，当前方案（固定最大容量）是更实用的选择。

/// 最大位图大小（位数）
pub const MAX_BITMAP_BITS: usize = 512;
/// 最大位图字节数
pub const MAX_BITMAP_BYTES: usize = (MAX_BITMAP_BITS + 7) / 8;

/// 泛型位图
///
/// 使用固定大小的内部数组，在编译期确定最大容量。
///
/// # 示例
///
/// ```
/// use minix_types::{Bitmap, Bitmap256};
///
/// // 256 位位图，32 字节（VM/PM 默认）
/// let mut bm256: Bitmap256 = Bitmap::new(256);
///
/// // 设置和获取
/// bm256.set(5, true);
/// assert!(bm256.get(5));
///
/// // 查找第一个零位
/// assert_eq!(bm256.find_first_zero(), Some(0));
/// bm256.set(0, true);
/// assert_eq!(bm256.find_first_zero(), Some(1));
/// ```
///
/// # 内存布局
///
/// | 大小 | 位数 | 字节数 | Cache Line (64B) |
/// |------|------|--------|------------------|
/// | `Bitmap256` | 256 | 32 | **1 行** |
/// | `Bitmap512` | 512 | 64 | 1 行 |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bitmap {
    bits: [u8; MAX_BITMAP_BYTES],
    size: usize,
}

impl Bitmap {
    /// 创建新的空位图（所有位为 0）
    ///
    /// # Panics
    ///
    /// 如果 `size > MAX_BITMAP_BITS`，会触发 panic
    ///
    /// # 示例
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

    /// 获取位图大小（位数）
    ///
    /// # 示例
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

    /// 获取位图字节数
    ///
    /// # 示例
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

    /// 获取指定位置的位值
    ///
    /// # Panics
    ///
    /// 如果 `index >= size`，会触发 panic
    ///
    /// # 示例
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

    /// 设置指定位置的位值
    ///
    /// # Panics
    ///
    /// 如果 `index >= size`，会触发 panic
    ///
    /// # 示例
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

    /// 查找第一个为零的位
    ///
    /// 返回第一个值为 `false` 的位的索引，如果所有位都为 `true` 则返回 `None`。
    ///
    /// # 性能
    ///
    /// 使用 `trailing_ones()` 优化，每个字节只需 1 次操作。
    ///
    /// # 示例
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
    /// // 填满前 8 位
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

    /// 查找第一个为一的位
    ///
    /// 返回第一个值为 `true` 的位的索引，如果所有位都为 `false` 则返回 `None`。
    ///
    /// # 示例
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

    /// 统计值为 1 的位数
    ///
    /// # 示例
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

    /// 统计值为 0 的位数
    ///
    /// # 示例
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

    /// 清空所有位
    ///
    /// # 示例
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

    /// 填充所有位为 1
    ///
    /// # 示例
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

    /// 检查位图是否为空（所有位为 0）
    ///
    /// # 示例
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

    /// 检查位图是否已满（所有位为 1）
    ///
    /// # 示例
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

/// 256 位位图类型别名
///
/// 适用于 NR_PROCS = 256 的进程表管理
pub type Bitmap256 = Bitmap;

/// 512 位位图类型别名
///
/// 适用于更大的进程表
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
