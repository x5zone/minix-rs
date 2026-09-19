//! `struct flock` 的 LP64 线上布局与锁类常量。
//!
//! C correspondence: `minix3/sys/sys/fcntl.h:253-259`（结构体）、
//! `:203-205`（`F_RDLCK`/`F_UNLCK`/`F_WRLCK`）。消费方是 VFS 的
//! fcntl 锁类命令（`F_GETLK`/`F_SETLK`/`F_SETLKW`/`F_FREESP`）——
//! 这四个命令都要在用户内存与 VFS 之间整块拷这个结构。

/// `struct flock` 的 LP64 总长：两个 `off_t`（8+8）、`pid_t`（4）、两个
/// `short`（2+2），尾部对齐到 8 → 24 字节。
pub const FLOCK_SIZE: usize = 24;

/// `struct flock` 的域偏移（LP64；声明序是 start/len/pid/type/whence，
/// **不是**直觉的 type 在前——按声明序逐格钉）。
pub mod flock_off {
    /// `off_t l_start`（区间起点）。
    pub const START: usize = 0;
    /// `off_t l_len`（0 = 到文件尾）。
    pub const LEN: usize = 8;
    /// `pid_t l_pid`（锁属主；GETLK 报告冲突者时填）。
    pub const PID: usize = 16;
    /// `short l_type`。
    pub const TYPE: usize = 20;
    /// `short l_whence`。
    pub const WHENCE: usize = 22;
}

/// 共享（读）锁（`fcntl.h:203`）。
pub const F_RDLCK: i32 = 1;
/// 解锁（`fcntl.h:204`）。
pub const F_UNLCK: i32 = 2;
/// 独占（写）锁（`fcntl.h:205`）。
pub const F_WRLCK: i32 = 3;

#[cfg(test)]
mod tests {
    use super::*;

    /// 布局逐格钉死（fcntl.h:253-259）：错一格，锁类型就会被当成
    /// 长度读出去。
    #[test]
    fn test_flock_layout_matches_c_struct() {
        assert_eq!(FLOCK_SIZE, 24);
        assert_eq!(flock_off::START, 0);
        assert_eq!(flock_off::LEN, 8);
        assert_eq!(flock_off::PID, 16);
        assert_eq!(flock_off::TYPE, 20);
        assert_eq!(flock_off::WHENCE, 22);
    }

    /// 锁类常量（fcntl.h:203-205）。
    #[test]
    fn test_lock_type_constants() {
        assert_eq!(F_RDLCK, 1);
        assert_eq!(F_UNLCK, 2);
        assert_eq!(F_WRLCK, 3);
    }
}
