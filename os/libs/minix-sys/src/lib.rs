//! Minix-RS System Call Library
//!
//! 系统调用封装，供用户态程序使用

#![no_std]

use minix_ipc::{Endpoint, Message, SyscallNum};
pub use minix_types::{Gid, Pid, Uid};

/// 文件描述符
pub type Fd = i32;

/// 创建子进程
pub fn fork() -> Result<Pid, Errno> {
    todo!("fork syscall")
}

/// 执行新程序
pub fn exec(path: &str, argv: &[&str]) -> Result<(), Errno> {
    todo!("exec syscall")
}

/// 进程退出
pub fn exit(status: i32) -> ! {
    loop {}
}

/// 等待子进程
pub fn waitpid(pid: Pid, status: &mut i32, options: i32) -> Result<Pid, Errno> {
    todo!("waitpid syscall")
}

/// 发送信号
pub fn kill(pid: Pid, sig: i32) -> Result<(), Errno> {
    todo!("kill syscall")
}

/// 打开文件
pub fn open(path: &str, flags: i32, mode: u32) -> Result<Fd, Errno> {
    todo!("open syscall")
}

/// 关闭文件
pub fn close(fd: Fd) -> Result<(), Errno> {
    todo!("close syscall")
}

/// 读取文件
pub fn read(fd: Fd, buf: &mut [u8]) -> Result<usize, Errno> {
    todo!("read syscall")
}

/// 写入文件
pub fn write(fd: Fd, buf: &[u8]) -> Result<usize, Errno> {
    todo!("write syscall")
}

/// 内存映射
pub fn mmap(
    addr: *mut u8,
    len: usize,
    prot: i32,
    flags: i32,
    fd: Fd,
    offset: i64,
) -> Result<*mut u8, Errno> {
    todo!("mmap syscall")
}

/// 错误码
#[derive(Debug, Clone, Copy)]
#[repr(i32)]
pub enum Errno {
    Eperm = 1,
    Enoent = 2,
    Esrch = 3,
    Eintr = 4,
    Eio = 5,
    Enxio = 6,
    E2big = 7,
    Enoexec = 8,
    Ebadf = 9,
    Echild = 10,
    Eagain = 11,
    Enomem = 12,
    Eacces = 13,
    Efault = 14,
    Ebusy = 16,
    Eexist = 17,
    Exdev = 18,
    Enodev = 19,
    Enotdir = 20,
    Eisdir = 21,
    Einval = 22,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_errno_values() {
        assert_eq!(Errno::Eperm as i32, 1);
        assert_eq!(Errno::Enoent as i32, 2);
    }
}
