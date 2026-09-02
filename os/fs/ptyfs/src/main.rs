//! FS server (ptyfs) 入口（占位）。

fn main() {
    // TODO: 实装为真实服务进程（事件循环 + RS 启动协议）。
    minix_fs_ptyfs::init();
    loop {}
}
