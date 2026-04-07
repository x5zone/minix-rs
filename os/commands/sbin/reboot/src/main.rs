//! Minix-RS reboot - 重启系统

use minix_rt::*;
use minix_sys::*;

fn main() {
    init();

    // TODO: 实现 reboot
    // 1. 同步文件系统
    // 2. 通知内核重启

    exit(0);
}
