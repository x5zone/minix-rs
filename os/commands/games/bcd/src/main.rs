//! Minix-RS bcd — 占位游戏。
//!
//! C 对应: `minix3/games/bcd/`
//! 状态: 占位（stub），待实装。

use minix_rt::*;
use minix_sys::*;

fn main() {
    init();

    // TODO: 实装游戏逻辑。
    // 纯 stdio 类（factor/primes/bcd/morse/...）只需 exec + stdio + exit；
    // 终端控制类（worm/rain/colorbars/...）需 termios/转义序列落地后再实装。
    exit(0);
}
