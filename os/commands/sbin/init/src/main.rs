//! Minix-RS init - 系统初始化进程

use minix_rt::*;
use minix_sys::*;

fn main() {
    init();

    // TODO: 实现 init
    // 1. 启动系统服务器
    // 2. 启动 shell
    // 3. 等待子进程

    loop {
        // 等待子进程
    }
}
