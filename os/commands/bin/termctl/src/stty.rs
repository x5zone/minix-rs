//! `stty` argument parsing into operations.
//!
//! Ground truth: `minix3/bin/stty/stty.c` (operand handling around line
//! 137), flag tables in `modes.c` (`cmodes`, `imodes`, `lmodes`, `omodes`
//! with their `specialmodes` companions at lines 65 to 175; `echo`/`ECHO`
//! at line 121), and the search order in `modeset` (line 208: control,
//! input, local, output). An operand is one of:
//!
//! - A speed (`9600`, `115200`, ...): sets both directions.
//! - A control character assignment (`intr ^C`, `erase undef`, ...): the
//!   name must be known, the value parsed by caret rules.
//! - A flag word (`echo`, `-echo`, `icanon`, ...): set or clear one flag.
//!
//! Anything else is an error. Applying operations to a real terminal stays
//! with the execution layer; the flag identities below (which group each
//! name belongs to) mirror the four C tables.

use crate::TermError;
use crate::baud::parse_speed;
use crate::cchar::{lookup, parse_value};

/// Which flag group a name belongs to (mirroring the four C tables).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlagGroup {
    /// Control modes (`cmodes`).
    Control,
    /// Input modes (`imodes`).
    Input,
    /// Local modes (`lmodes`).
    Local,
    /// Output modes (`omodes`).
    Output,
}

/// One parsed `stty` operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SttyOp<'a> {
    /// Set both directions to `baud`.
    Speed(u32),
    /// Assign control character `slot` the value `value`.
    ControlChar(u8, u8),
    /// Set (`true`) or clear (`false`) the named flag in its group.
    Flag(FlagGroup, &'a str, bool),
    /// A whole `gfmt1:...` grep-form operand (C `stty.c:143-146` 的
    /// `strncmp` 前缀支路，交给 [`grep_read`] 落值).
    Grep(&'a str),
}

/// (Name, group) pairs for the flag words this module recognises: the
/// common subset across the four C tables (flow control, canonical mode,
/// echo and its variants, signal and extension switches, output
/// postprocessing). The full tables hold hundreds of entries (baud
/// variants, character sizes, exotic switches); the execution layer owns
/// the complete generated table, this core covers the words scripts
/// actually use.
pub const FLAGS: [(&str, FlagGroup); 16] = [
    ("parenb", FlagGroup::Control),
    ("parodd", FlagGroup::Control),
    ("cs8", FlagGroup::Control),
    ("hupcl", FlagGroup::Control),
    ("istrip", FlagGroup::Input),
    ("ixon", FlagGroup::Input),
    ("ixoff", FlagGroup::Input),
    ("icrnl", FlagGroup::Input),
    ("icanon", FlagGroup::Local),
    ("echo", FlagGroup::Local),
    ("echoe", FlagGroup::Local),
    ("echok", FlagGroup::Local),
    ("isig", FlagGroup::Local),
    ("iexten", FlagGroup::Local),
    ("opost", FlagGroup::Output),
    ("onlcr", FlagGroup::Output),
];

/// Parse `stty` operands into operations.
///
/// `args` are the words after the program name. A word is tried as a speed
/// first (all digits), then as `name value` for control characters (the
/// name must be known), then as a flag word with optional leading dash.
/// Unknown words are an error, matching the C tool refusing the whole
/// command line.
pub fn parse_args<'a>(args: &[&'a str]) -> Result<([SttyOp<'a>; 16], usize), TermError> {
    let mut ops: [SttyOp<'a>; 16] = [SttyOp::Speed(9600); 16];
    let mut count = 0;
    let mut index = 0;
    let push = |ops: &mut [SttyOp<'a>; 16],
                    count: &mut usize,
                    op: SttyOp<'a>|
     -> Result<(), TermError> {
        if *count >= ops.len() {
            return Err(TermError::InvalidArgument);
        }
        ops[*count] = op;
        *count += 1;
        Ok(())
    };
    while index < args.len() {
        let word = args[index];
        if let Some(rest) = word.strip_prefix("gfmt1") {
            // C 的 main 循环先 `strncmp("gfmt1")` 命中即 gread（stty.c:143-146），
            // 不过关键字表。这里在解析侧做语法预验（坏操作数在解析时报警，
            // 与 C 边读边 errx 的时机等值），apply 侧再落值。
            grep_read(&mut minix_sys::Termios::new(), rest)?;
            push(&mut ops, &mut count, SttyOp::Grep(word))?;
            index += 1;
            continue;
        }
        if !word.is_empty() && word.bytes().all(|b| b.is_ascii_digit()) {
            push(&mut ops, &mut count, SttyOp::Speed(parse_speed(word)?))?;
            index += 1;
            continue;
        }
        if lookup(word).is_ok() {
            index += 1;
            let value = args.get(index).ok_or(TermError::InvalidArgument)?;
            let (slot, _) = lookup(word).map_err(|_| TermError::InvalidArgument)?;
            push(
                &mut ops,
                &mut count,
                SttyOp::ControlChar(slot, parse_value(value)?),
            )?;
            index += 1;
            continue;
        }
        let (negated, name) = match word.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, word),
        };
        match FLAGS.iter().find(|(known, _)| *known == name) {
            Some((_, group)) => {
                push(&mut ops, &mut count, SttyOp::Flag(*group, name, !negated))?;
                index += 1;
            }
            None => return Err(TermError::InvalidArgument),
        }
    }
    Ok((ops, count))
}

/// (name, group, bit) 三元组：`FLAGS` 里每个字的落位（`termios.h`
/// 各旗标 define；`cs8` 特殊——置位时先清 `CSIZE` 掩码再上 `CS8`）。
const FLAG_BITS: [(&str, FlagGroup, u32); 16] = [
    ("parenb", FlagGroup::Control, minix_sys::PARENB),
    ("parodd", FlagGroup::Control, minix_sys::PARODD),
    ("cs8", FlagGroup::Control, minix_sys::CS8),
    ("hupcl", FlagGroup::Control, minix_sys::HUPCL),
    ("istrip", FlagGroup::Input, minix_sys::ISTRIP),
    ("ixon", FlagGroup::Input, minix_sys::IXON),
    ("ixoff", FlagGroup::Input, minix_sys::IXOFF),
    ("icrnl", FlagGroup::Input, minix_sys::ICRNL),
    ("icanon", FlagGroup::Local, minix_sys::ICANON),
    ("echo", FlagGroup::Local, minix_sys::ECHO),
    ("echoe", FlagGroup::Local, minix_sys::ECHOE),
    ("echok", FlagGroup::Local, minix_sys::ECHOK),
    ("isig", FlagGroup::Local, minix_sys::ISIG),
    ("iexten", FlagGroup::Local, minix_sys::IEXTEN),
    ("opost", FlagGroup::Output, minix_sys::OPOST),
    ("onlcr", FlagGroup::Output, minix_sys::ONLCR),
];

/// Apply parsed operations to a terminal attribute record (`modeset`,
/// `stty.c:208` 的搜索次序在解析层已定，这里只做落位）。
///
/// `cs8` 的置位先清 `CSIZE` 掩码（字符大小是域不是位）；清除则只清
/// `CS8` 位值。速度同时写两个方向（C 的 `cfsetispeed`/`cfsetospeed`
/// 成对面）。
pub fn apply_ops(ops: &[SttyOp], count: usize, t: &mut minix_sys::Termios) {
    for op in &ops[..count.min(ops.len())] {
        match *op {
            SttyOp::Speed(baud) => {
                t.c_ispeed = baud as i32;
                t.c_ospeed = baud as i32;
            }
            SttyOp::ControlChar(slot, value) => {
                if (slot as usize) < minix_sys::NCCS {
                    t.c_cc[slot as usize] = value;
                }
            }
            SttyOp::Grep(word) => {
                // 解析侧已验过语法，这里不可能再失败；坏输入在 C 里
                // errx 直接终止，报错时机归调用层的 parse。
                if let Some(rest) = word.strip_prefix("gfmt1") {
                    let _ = grep_read(t, rest);
                }
            }
            SttyOp::Flag(group, name, set) => {
                let Some((_, _, bit)) =
                    FLAG_BITS.iter().find(|(known, g, _)| *g == group && *known == name)
                else {
                    continue;
                };
                let word = match group {
                    FlagGroup::Control => &mut t.c_cflag,
                    FlagGroup::Input => &mut t.c_iflag,
                    FlagGroup::Local => &mut t.c_lflag,
                    FlagGroup::Output => &mut t.c_oflag,
                };
                if name == "cs8" && set {
                    // 置八位：清大小域再上 CS8（termios.h:131-135）。
                    *word &= !minix_sys::CSIZE;
                    *word |= minix_sys::CS8;
                } else if set {
                    *word |= bit;
                } else {
                    *word &= !bit;
                }
            }
        }
    }
}

/// `-a` 显示面：速度行、四组旗标（清除的名字带负号）、控制字符行。
///
/// C 的 `stty -a` 还有 rows/columns 与 line discipline 两个域——本模型
/// 的属性记录没有这两个面，显示里省略（13-terminal-termios.md §5 声明）。
/// 旗标清单就是决定半认识的十六个字。
pub fn display_a(t: &minix_sys::Termios, out: &mut [u8]) -> Result<usize, TermError> {
    let mut at = 0usize;
    let put = |out: &mut [u8], at: &mut usize, bytes: &[u8]| -> Result<(), TermError> {
        if *at + bytes.len() > out.len() {
            return Err(TermError::InvalidArgument);
        }
        out[*at..*at + bytes.len()].copy_from_slice(bytes);
        *at += bytes.len();
        Ok(())
    };
    let number = |out: &mut [u8], at: &mut usize, value: u32| -> Result<(), TermError> {
        let mut digits = [0u8; 10];
        let mut count = 0;
        let mut v = value;
        if v == 0 {
            digits[0] = b'0';
            count = 1;
        }
        while v > 0 {
            digits[count] = b'0' + (v % 10) as u8;
            count += 1;
            v /= 10;
        }
        while count > 0 {
            count -= 1;
            put(out, at, &digits[count..count + 1])?;
        }
        Ok(())
    };

    put(out, &mut at, b"speed ")?;
    number(out, &mut at, t.c_ospeed as u32)?;
    put(out, &mut at, b" baud;\n")?;

    // 控制字符行：恒显的七个槽位（intr/quit/erase/kill/eof/eol 对齐
    // C 的显示序；undef 打 `<undef>`）。
    let named = [
        ("intr = ", VINTR_SLOT),
        ("quit = ", VQUIT_SLOT),
        ("erase = ", VERASE_SLOT),
        ("kill = ", VKILL_SLOT),
        ("eof = ", VEOF_SLOT),
        ("eol = ", VEOL_SLOT),
    ];
    for (index, (label, slot)) in named.iter().enumerate() {
        if index > 0 {
            put(out, &mut at, b"; ")?;
        }
        put(out, &mut at, label.as_bytes())?;
        let value = t.c_cc[*slot];
        if value == crate::cchar::DISABLED || value == 0 {
            // 0 即 `_POSIX_VDISABLE`（Minix 的禁用值）——同 `<undef>`。
            put(out, &mut at, b"<undef>")?;
        } else {
            put(out, &mut at, b"^")?;
            let letter = if value >= 128 {
                value & 0x1f
            } else {
                value ^ 0x40
            };
            put(out, &mut at, &[letter])?;
        }
    }
    put(out, &mut at, b";\n")?;

    // 四组旗标：置位裸名、清除带负号（C `stty -a` 的分组打印序）。
    for (group, gap) in [
        (FlagGroup::Control, true),
        (FlagGroup::Input, false),
        (FlagGroup::Local, false),
        (FlagGroup::Output, false),
    ] {
        if gap {
            put(out, &mut at, b"\n")?;
        }
        let mut first = true;
        for (name, g, bit) in FLAG_BITS.iter() {
            if *g != group {
                continue;
            }
            let word = match group {
                FlagGroup::Control => t.c_cflag,
                FlagGroup::Input => t.c_iflag,
                FlagGroup::Local => t.c_lflag,
                FlagGroup::Output => t.c_oflag,
            };
            let set = word & bit != 0;
            if !first {
                put(out, &mut at, b" ")?;
            }
            first = false;
            if !set {
                put(out, &mut at, b"-")?;
            }
            put(out, &mut at, name.as_bytes())?;
        }
        put(out, &mut at, b";\n")?;
    }
    Ok(at)
}

/// `-g` 单行格式输出——终端属性的机器可读形态。
///
/// C: `gprint` (`minix3/bin/stty/gfmt.c:61-71`)：四个旗标字按小写十六进制
/// 打印（printf `%x`：无前导零，0 打 `0`），随后按 `cchars1` 表
/// (`cchar.c:59-79`) 的 18 个主名同格式逐一打印，行尾两个速度按十进制。
pub fn grep_print(t: &minix_sys::Termios, out: &mut [u8]) -> Result<usize, TermError> {
    let mut at = 0usize;
    put(out, &mut at, b"gfmt1:cflag=")?;
    hex(out, &mut at, t.c_cflag)?;
    put(out, &mut at, b":iflag=")?;
    hex(out, &mut at, t.c_iflag)?;
    put(out, &mut at, b":lflag=")?;
    hex(out, &mut at, t.c_lflag)?;
    put(out, &mut at, b":oflag=")?;
    hex(out, &mut at, t.c_oflag)?;
    put(out, &mut at, b":")?;
    // 前 18 项是 cchars1 主名；表尾三项（brk/flush/rprnt）是旧别名，
    // C 的 cchars1 里没有，不打印。
    for (name, slot, _) in crate::cchar::CONTROL_CHARS.iter().take(18) {
        put(out, &mut at, name.as_bytes())?;
        put(out, &mut at, b"=")?;
        hex(out, &mut at, t.c_cc[*slot as usize] as u32)?;
        put(out, &mut at, b":")?;
    }
    put(out, &mut at, b"ispeed=")?;
    number(out, &mut at, t.c_ispeed as u32)?;
    put(out, &mut at, b":ospeed=")?;
    number(out, &mut at, t.c_ospeed as u32)?;
    put(out, &mut at, b"\n")?;
    Ok(at)
}

/// `gfmt1:...` 操作数输入面——从 grep 格式回填旗标与控制字符。
///
/// C: `gread` (`minix3/bin/stty/gfmt.c:73-131`)，逐段对位：`gfmt1` 前缀已
/// 由调用方剥去，余文必须含冒号（C `strchr`），冒号后按 `name=value`
/// 冒号分段。除 `min`/`time`（VMIN/VTIME 槽，C `sscanf` `%ld` 十进制）
/// 外都按十六进制（`%lx`）取最长合法前缀；不认识的名称报错。
///
/// 两处对 C 事实的等值复刻，测试钉死：C 的 `ispeed`/`ospeed` 名称分支
/// 在 `#ifdef BSD4_4` 内而 minix3 未定义该宏（全树 grep 零命中），所以
/// `-g` 自己行尾打印的两段在输入侧按"不认识名称"拒绝——C 的
/// `stty $(stty -g)` 往返在这台机器上本就不成立。C 的 `sscanf` 在无
/// 合法数字时留下未初始化变量（未定义行为），这里统一报错。
pub fn grep_read(t: &mut minix_sys::Termios, rest: &str) -> Result<(), TermError> {
    let Some(start) = rest.find(':').map(|i| &rest[i + 1..]) else {
        return Err(TermError::InvalidArgument);
    };
    let mut s = start;
    loop {
        let (segment, remainder) = match s.find(':') {
            Some(i) => (&s[..i], Some(&s[i + 1..])),
            None => (s, None),
        };
        if segment.is_empty() {
            break;
        }
        let Some((name, value)) = segment.split_once('=') else {
            return Err(TermError::InvalidArgument);
        };
        match name {
            "cflag" => t.c_cflag = parse_hex(value)? as u32,
            "iflag" => t.c_iflag = parse_hex(value)? as u32,
            "lflag" => t.c_lflag = parse_hex(value)? as u32,
            "oflag" => t.c_oflag = parse_hex(value)? as u32,
            _ => {
                let Some((_, slot, _)) = crate::cchar::CONTROL_CHARS
                    .iter()
                    .take(18)
                    .find(|(known, _, _)| *known == name)
                else {
                    return Err(TermError::InvalidArgument);
                };
                let raw = if *slot == minix_sys::VMIN as u8 || *slot == minix_sys::VTIME as u8 {
                    parse_dec(value)?
                } else {
                    parse_hex(value)?
                };
                t.c_cc[*slot as usize] = raw as u8;
            }
        }
        let Some(next) = remainder else { break };
        s = next;
    }
    Ok(())
}

/// 向缓冲追加字节（越界即整行报错，同 `display_a` 的失败语义）。
fn put(out: &mut [u8], at: &mut usize, bytes: &[u8]) -> Result<(), TermError> {
    if *at + bytes.len() > out.len() {
        return Err(TermError::InvalidArgument);
    }
    out[*at..*at + bytes.len()].copy_from_slice(bytes);
    *at += bytes.len();
    Ok(())
}

/// C printf `%x` 形态：小写十六进制、无前导零、0 打单个 `0`。
fn hex(out: &mut [u8], at: &mut usize, value: u32) -> Result<(), TermError> {
    if value == 0 {
        return put(out, at, b"0");
    }
    let mut digits = [0u8; 8];
    let mut count = 0;
    let mut v = value;
    while v > 0 {
        let d = (v & 0xf) as u8;
        digits[count] = if d < 10 { b'0' + d } else { b'a' + d - 10 };
        count += 1;
        v >>= 4;
    }
    while count > 0 {
        count -= 1;
        put(out, at, &digits[count..count + 1])?;
    }
    Ok(())
}

/// 十进制整数（C printf `%d` 形态，同 `display_a` 的速度打印）。
fn number(out: &mut [u8], at: &mut usize, value: u32) -> Result<(), TermError> {
    if value == 0 {
        return put(out, at, b"0");
    }
    let mut digits = [0u8; 10];
    let mut count = 0;
    let mut v = value;
    while v > 0 {
        digits[count] = b'0' + (v % 10) as u8;
        count += 1;
        v /= 10;
    }
    while count > 0 {
        count -= 1;
        put(out, at, &digits[count..count + 1])?;
    }
    Ok(())
}

/// C `sscanf` `%lx` 的可辩护近亲：跳过空白与可选 `0x` 前缀，取最长合法
/// 十六进制前缀（溢出按 wrapping 折算，C 为未定义行为），无合法数字报错。
fn parse_hex(text: &str) -> Result<u64, TermError> {
    let body = text.trim_start();
    let body = body
        .strip_prefix("0x")
        .or(body.strip_prefix("0X"))
        .unwrap_or(body);
    let mut acc = 0u64;
    let mut seen = false;
    for ch in body.chars() {
        let Some(d) = ch.to_digit(16) else { break };
        acc = acc.wrapping_shl(4) | d as u64;
        seen = true;
    }
    if seen {
        Ok(acc)
    } else {
        Err(TermError::InvalidArgument)
    }
}

/// C `sscanf` `%ld` 的同型面：最长十进制前缀。
fn parse_dec(text: &str) -> Result<u64, TermError> {
    let body = text.trim_start();
    let mut acc = 0u64;
    let mut seen = false;
    for ch in body.chars() {
        let Some(d) = ch.to_digit(10) else { break };
        acc = acc.wrapping_mul(10) + d as u64;
        seen = true;
    }
    if seen {
        Ok(acc)
    } else {
        Err(TermError::InvalidArgument)
    }
}

/// `CONTROL_CHARS` 表里的槽位常量名对齐（`termios.h:44-74` 的槽位号）。
const VINTR_SLOT: usize = 8;
const VQUIT_SLOT: usize = 9;
const VERASE_SLOT: usize = 3;
const VKILL_SLOT: usize = 5;
const VEOF_SLOT: usize = 0;
const VEOL_SLOT: usize = 1;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cchar::DISABLED;

    #[test]
    fn test_speed_operand() {
        let (ops, count) = parse_args(&["9600"]).unwrap();
        assert_eq!(count, 1);
        assert_eq!(ops[0], SttyOp::Speed(9600));
    }

    #[test]
    fn test_control_char_assignment() {
        let (ops, count) = parse_args(&["intr", "^C"]).unwrap();
        assert_eq!(count, 1);
        assert_eq!(ops[0], SttyOp::ControlChar(8, 0x03));
        let (ops, _) = parse_args(&["erase", "undef"]).unwrap();
        assert_eq!(ops[0], SttyOp::ControlChar(3, DISABLED));
    }

    #[test]
    fn test_flag_set_and_clear() {
        let (ops, count) = parse_args(&["echo", "-icanon"]).unwrap();
        assert_eq!(count, 2);
        assert_eq!(ops[0], SttyOp::Flag(FlagGroup::Local, "echo", true));
        assert_eq!(ops[1], SttyOp::Flag(FlagGroup::Local, "icanon", false));
    }

    #[test]
    fn test_mixed_command_line() {
        let (ops, count) = parse_args(&["9600", "intr", "^C", "-echo"]).unwrap();
        assert_eq!(count, 3);
        assert_eq!(ops[0], SttyOp::Speed(9600));
    }

    #[test]
    fn test_unknown_words_rejected() {
        assert_eq!(parse_args(&["bogus"]), Err(TermError::InvalidArgument));
        assert_eq!(parse_args(&["intr"]), Err(TermError::InvalidArgument));
        assert_eq!(parse_args(&["-bogus"]), Err(TermError::InvalidArgument));
    }

    #[test]
    fn test_grep_print_bytes_exact() {
        // C gprint 的逐字节形态（gfmt.c:66-70）：四旗标 %x、18 段控制字符、
        // 十进制速度收尾。构造值全部取非零单 nibble 以上，防前导零漂移。
        let mut t = minix_sys::Termios::new();
        t.c_cflag = 0x1c;
        t.c_iflag = 0x500;
        t.c_lflag = 0x8a;
        t.c_oflag = 0;
        t.c_cc[minix_sys::VEOF] = 0x04;
        t.c_cc[minix_sys::VINTR] = 0x03;
        t.c_cc[minix_sys::VMIN] = 1;
        t.c_ispeed = 9600;
        t.c_ospeed = 9600;
        let mut out = [0u8; 512];
        let used = grep_print(&t, &mut out).unwrap();
        let text = core::str::from_utf8(&out[..used]).unwrap();
        assert_eq!(
            text,
            "gfmt1:cflag=1c:iflag=500:lflag=8a:oflag=0:\
             discard=0:dsusp=0:eof=4:eol=0:eol2=0:erase=0:intr=3:kill=0:\
             lnext=0:min=1:quit=0:reprint=0:start=0:status=0:stop=0:susp=0:\
             time=0:werase=0:ispeed=9600:ospeed=9600\n"
        );
    }

    #[test]
    fn test_grep_read_flags_and_chars() {
        // 四旗标 + 十六进制控制字符段 + min/time 十进制段（C %ld 支路）。
        let mut t = minix_sys::Termios::new();
        grep_read(
            &mut t,
            ":cflag=1c:iflag=500:lflag=8a:oflag=2:eof=4:min=7:time=9:",
        )
        .unwrap();
        assert_eq!(t.c_cflag, 0x1c);
        assert_eq!(t.c_iflag, 0x500);
        assert_eq!(t.c_lflag, 0x8a);
        assert_eq!(t.c_oflag, 2);
        assert_eq!(t.c_cc[minix_sys::VEOF], 4);
        assert_eq!(t.c_cc[minix_sys::VMIN], 7, "VMIN 段按十进制取 7 而非 0x7");
        assert_eq!(t.c_cc[minix_sys::VTIME], 9);
    }

    #[test]
    fn test_grep_read_min_decimal_not_hex() {
        // C gread 对 min/time 用 %ld：`min=16` 落 16 而非 0x16=22。
        let mut t = minix_sys::Termios::new();
        grep_read(&mut t, ":min=16").unwrap();
        assert_eq!(t.c_cc[minix_sys::VMIN], 16);
    }

    #[test]
    fn test_grep_read_rejects_ispeed_segment_as_c_does() {
        // C !BSD4_4 事实（gfmt.c:100-121 的 ifdef 未启用）：-g 自己打印的
        // ispeed/ospeed 段在输入侧按不认识名称拒绝——往返不成立于 C，照实复刻。
        let mut t = minix_sys::Termios::new();
        assert_eq!(
            grep_read(&mut t, ":cflag=1c:ispeed=9600:ospeed=9600:"),
            Err(TermError::InvalidArgument)
        );
        assert_eq!(t.c_cflag, 0x1c, "拒绝发生在 ispeed 段，之前的段落已落值");
    }

    #[test]
    fn test_grep_read_rejects_malformed() {
        // 无冒号 / 段无等号 / 非法名称 / 无合法数字：全部报错。
        let mut t = minix_sys::Termios::new();
        assert!(grep_read(&mut t, "cflag=1c").is_err());
        assert!(grep_read(&mut t, ":cflag").is_err());
        assert!(grep_read(&mut t, ":bogus=1").is_err());
        assert!(grep_read(&mut t, ":cflag=zz").is_err());
    }

    #[test]
    fn test_grep_operand_flows_through_parse_and_apply() {
        // `gfmt1:` 前缀词绕开关键字表成为 Grep op，与其他操作数按命令行
        // 顺序混排生效（C 主循环逐操作数语义）。
        let (ops, count) = parse_args(&["echo", "gfmt1:lflag=8a:"]).unwrap();
        assert_eq!(count, 2);
        assert_eq!(ops[0], SttyOp::Flag(FlagGroup::Local, "echo", true));
        assert_eq!(ops[1], SttyOp::Grep("gfmt1:lflag=8a:"));
        let mut t = minix_sys::Termios::new();
        apply_ops(&ops, count, &mut t);
        // echo 位先置（lflag |= 0x8），gfmt1 的 lflag=8a 随后整体覆盖。
        assert_eq!(t.c_lflag, 0x8a);
    }

    #[test]
    fn test_grep_print_buffer_too_small() {
        let t = minix_sys::Termios::new();
        let mut tiny = [0u8; 8];
        assert_eq!(grep_print(&t, &mut tiny), Err(TermError::InvalidArgument));
    }
}
    #[test]
    fn test_apply_flag_bits_and_cs8_domain() {
        use super::*;
        let mut t = minix_sys::Termios::new();
        // 四组各一位：置位落对字，清除清对位。
        let (ops, count) = parse_args(&["echo", "-icrnl", "parenb", "-onlcr"]).unwrap();
        apply_ops(&ops, count, &mut t);
        assert!(t.c_lflag & minix_sys::ECHO != 0);
        assert!(t.c_iflag & minix_sys::ICRNL == 0);
        assert!(t.c_cflag & minix_sys::PARENB != 0);
        assert!(t.c_oflag & minix_sys::ONLCR == 0);
        // cs8 置位：清 CSIZE 域再上 CS8（termios.h:131-135）。
        t.c_cflag |= 0x100; // 人为置 CS7 位（CSIZE 域内）
        let (ops, count) = parse_args(&["cs8"]).unwrap();
        apply_ops(&ops, count, &mut t);
        assert!(t.c_cflag & minix_sys::CSIZE == minix_sys::CS8);
    }

    #[test]
    fn test_apply_speed_and_control_char() {
        use super::*;
        let mut t = minix_sys::Termios::new();
        let (ops, count) = parse_args(&["115200", "intr", "^C"]).unwrap();
        apply_ops(&ops, count, &mut t);
        assert_eq!(t.c_ispeed, 115200);
        assert_eq!(t.c_ospeed, 115200);
        assert_eq!(t.c_cc[8], 0x03, "VINTR 槽位 8 得 ^C");
    }

    #[test]
    fn test_display_a_shape() {
        use super::*;
        let mut t = minix_sys::Termios::new();
        t.c_ospeed = 9600;
        t.c_ispeed = 9600;
        t.c_lflag |= minix_sys::ICANON | minix_sys::ECHO;
        t.c_cflag |= minix_sys::CS8;
        t.c_cc[minix_sys::VINTR] = 0x03;
        let mut out = [0u8; 512];
        let used = display_a(&t, &mut out).unwrap();
        let text = core::str::from_utf8(&out[..used]).unwrap();
        assert!(text.starts_with("speed 9600 baud;\n"), "{text:?}");
        assert!(text.contains("icanon echo"), "置位裸名并列");
        assert!(text.contains("-opost -onlcr;"), "清除带负号");
        assert!(text.contains("intr = ^C"), "控制字符 caret 记法");
    }
