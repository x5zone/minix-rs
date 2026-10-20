//! `yes` 的端到端门：命令行上那串字节就是每一行的内容。
//!
//! 真源是 `minix3/usr.bin/yes/yes.c:50-56`——`yes = (argc > 1) ? argv[1] : "y"`
//! 存的是 `const char *`，`while (puts(yes) >= 0)` 每轮把那一串字节原样发出再加换行。
//! 这一支没有选项环也没有档名：参数既不是旗标也不是路径，它就是被无限重复的**输出内容**。
//! 所以参数缝上的任何折叠，改的不是某一行报文，而是每一行的字节。
//!
//! 门读数的形状受两件事约束：其一，`yes` 是无限循环，夹具只能取前若干字节再杀掉子进程；
//! 其二，循环的退出条件是写失败（C 的 `puts < 0` ⇒ `EXIT_FAILURE`），所以「读够了就 kill」
//! 是本夹具的正常收尾，不是被测代码的缺陷。

use std::ffi::OsString;
use std::io::Read as _;
use std::os::unix::ffi::OsStringExt as _;
use std::process::{Command, Stdio};

const YES: &str = env!("CARGO_BIN_EXE_yes");

/// 起 `yes`、取回前 `want` 个字节、再杀掉它。argv 逐字节交回（`OsString::from_vec`
/// 不经 UTF-8 校验），因为这几枚门要钉的正是「裸字节能不能原样被重复打出来」。
fn head_bytes(args: &[Vec<u8>], want: usize) -> Vec<u8> {
    let mut child = Command::new(YES)
        .args(args.iter().map(|raw| OsString::from_vec(raw.clone())))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the yes binary should be runnable");
    let mut got: Vec<u8> = Vec::new();
    {
        let mut pipe = child.stdout.take().expect("stdout is piped");
        let mut byte = [0u8; 1];
        while got.len() < want {
            match pipe.read(&mut byte) {
                Ok(1) => got.push(byte[0]),
                _ => break,
            }
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    got
}

/// 一枚裸 `ff` 作为参数时，每一行就是 `ff` 加一个换行：改前的本壳先把 argv 折成
/// `ef bf bd` 再打，于是四字节里三字节都是那个替换字符（现量见
/// `tmp/argv-442/before-yes.log`）。
#[test]
fn the_repeated_word_is_the_raw_bytes_of_the_argument() {
    let arg: Vec<u8> = b"\xff".to_vec();
    assert_eq!(head_bytes(&[arg], 4), b"\xff\n\xff\n");
}

/// 一串里混着合法与非法字节时，非法的那枚不许把整串换掉：一行是四字节（`a`、`ff`、`b`、换行）。
#[test]
fn a_nonutf8_byte_inside_the_word_does_not_disturb_its_neighbours() {
    let arg: Vec<u8> = [b"a".as_slice(), b"\xff", b"b"].concat();
    assert_eq!(head_bytes(&[arg], 8), b"a\xffb\na\xffb\n");
}

/// 没有参数时默认那枚 `y` 照旧（`yes.c:52` 的三元式另一支）。
#[test]
fn no_argument_repeats_a_lone_y() {
    assert_eq!(head_bytes(&[], 3), b"y\ny");
}
