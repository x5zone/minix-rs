//! minix3/minix/tests Rust 腿翻译 —— 终端域(test74/77 的可钉面)。
//!
//! # 翻译映射
//!
//! C test77(termios/pty/setsid)与 test74(ioctl/select/mmap)的主体需要
//! tty/pty 服务器与真实终端设备;本文件钉**当前已存在的契约面**:
//! termios 记录的字节布局(ioctl 一次搬运的 44 字节块,C `termios.h:192-200`)
//! 与 tty ioctl 请求号(C `ttycom.h:88-89` 的 `_IOR/_IOW` 展开)。pty 会话、
//! setsid 与终端挂起唤醒链挂 tty 服务器点亮后的后续条目。
//!
//! | C 测试 | 语义归宿 |
//! |---|---|
//! | test77(termios 记录搬运) | [`termios_byte_layout_roundtrip_preserves_record`] |
//! | test77(ioctl 请求号 ABI) | [`tty_ioctl_request_numbers_match_c_ttycom`] |
//! | test77(pty 会话/setsid)、test74(设备 select/mmap) | 边界外,挂 tty 面点亮后补 |
//!
//! 交付门 = 编译;全部测试 `#[ignore]`,点亮前提见各测试属性。

use minix_sys::{TIOCGETA, TIOCSETA};

// ---------------------------------------------------------------------------
// test77 —— termios 记录字节布局
// ---------------------------------------------------------------------------

/// C 语义(`termios.h:192-200` 的内存序序列化):termios 记录整体以
/// 44 字节块搬运(ioctl 参数),序列化-反序列化后七个字段逐项还原。
#[test]
#[ignore = "点亮前提:tty 服务器 ioctl 面随载体点亮后端到端复核"]
fn termios_byte_layout_roundtrip_preserves_record() {
    use minix_types::types::termios::{
        Termios, CS8, CREAD, ECHO, ICANON, ICRNL, ISIG, OPOST, VEOF, VINTR,
    };

    let mut record = Termios::new();
    record.c_iflag = ICRNL;
    record.c_oflag = OPOST;
    record.c_cflag = CS8 | CREAD;
    record.c_lflag = ISIG | ICANON | ECHO;
    record.c_cc[VEOF] = 0x04; // Ctrl-D
    record.c_cc[VINTR] = 0x03; // Ctrl-C
    record.c_ispeed = 9600;
    record.c_ospeed = 9600;

    let mut bytes = [0u8; 64];
    let written = record.to_bytes(&mut bytes).expect("44 字节记录可写入");
    let parsed = Termios::from_bytes(&bytes[..written]).expect("字节可反序列化");

    assert_eq!(parsed.c_iflag, ICRNL, "输入标志位还原");
    assert_eq!(parsed.c_oflag, OPOST, "输出标志位还原");
    assert_eq!(parsed.c_cflag, CS8 | CREAD, "控制标志位还原");
    assert_eq!(parsed.c_lflag, ISIG | ICANON | ECHO, "本地标志位还原");
    assert_eq!(
        parsed.c_cc[VEOF], 0x04,
        "EOF 控制字符还原(C test77 的 Raw/Canonical 面)"
    );
    assert_eq!(parsed.c_cc[VINTR], 0x03, "INTR 控制字符还原");
    assert_eq!((parsed.c_ispeed, parsed.c_ospeed), (9600, 9600), "速率还原");
}

// ---------------------------------------------------------------------------
// test77 —— tty ioctl 请求号 ABI
// ---------------------------------------------------------------------------

/// C 语义(`ttycom.h:88-89`):TIOCGETA = `_IOR('t', 19, struct termios)`
/// = IOC_OUT(0x4000_0000) | 44 字节参数 | 组 't' | 序号 19;TIOCSETA 同型
/// 置 IOC_IN(0x8000_0000)、序号 20。请求号是用户态与 tty 服务器的共享
/// ABI,逐字节钉死。
#[test]
#[ignore = "点亮前提:tty 服务器 ioctl 面随载体点亮后端到端复核"]
fn tty_ioctl_request_numbers_match_c_ttycom() {
    // _IOR('t', 19, 44 字节):方向出(0x4000_0000)、长度 44(<<16)、
    // 组码 't'(0x74)、序号 19(0x13)。
    assert_eq!(
        TIOCGETA,
        0x4000_0000 | (44 << 16) | (0x74 << 8) | 19,
        "TIOCGETA 逐位对账 C ttycom.h:88"
    );
    // _IOW('t', 20, 44 字节):方向入(0x8000_0000)、序号 20(0x14)。
    assert_eq!(
        TIOCSETA,
        0x8000_0000 | (44 << 16) | (0x74 << 8) | 20,
        "TIOCSETA 逐位对账 C ttycom.h:89"
    );
    // 方向位互斥:GETA 带出方向,SETA 带入方向。
    assert_eq!(TIOCGETA & 0xC000_0000, 0x4000_0000);
    assert_eq!(TIOCSETA & 0xC000_0000, 0x8000_0000);
}
