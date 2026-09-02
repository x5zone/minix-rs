//! driver (storage/at_wini) 入口（占位）。

fn main() {
    // TODO: 实装为真实服务进程（事件循环 + RS 启动协议）。
    minix_driver_at_wini::init();
    loop {}
}
