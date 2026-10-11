//! `stat` 的选项面与格式串解析（决定半第一包）。
//!
//! 真源是 `minix3/usr.bin/stat/stat.c`（NetBSD 1.38）。这个命令的骨架是一台
//! 小型格式机：一串自带字段码的模板，加上一批把模板换成预置模板的开关。本包只做
//! 「读命令行走哪条预置」与「把模板拆成可渲染的件」这两件事；把件换成文字（字段取值、
//! 进制与宽度、时间渲染、符号链尾巴）在下一包，薄壳与系统调用腿在其后。
//!
//! 之所以能这样切：C 里 `output()`（`stat.c:368-566`）扫模板时只产出一段段的
//! 「子格式串 + 解析出的参数」，再交给 `format1()`（`stat.c:568-1151`）去取值渲染。
//! 两段之间没有共享状态，解析形可以单独钉住测试。

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// 预置模板。C 用四个宏加 `TIME_FORMAT` 表达（`stat.c:94-115`）；此处原样抄成常量，
/// 好让「选了哪条预置」这一件事在宿主测里可直接断言。
pub const DEF_FORMAT: &str =
    "%d %i %Sp %l %Su %Sg %r %z \"%Sa\" \"%Sm\" \"%Sc\" \"%SB\" %k %b %#Xf %N";
/// 预置：`-r`（`stat.c:97-98`）。
pub const RAW_FORMAT: &str = "%d %i %#p %l %u %g %r %z %a %m %c %B %k %b %#f %N";
/// 预置：`-l`（`stat.c:99`）。
pub const LS_FORMAT: &str = "%Sp %l %Su %Sg %Z %Sm %N%SY";
/// 预置：`-l -F`（`stat.c:100`）。
pub const LSF_FORMAT: &str = "%Sp %l %Su %Sg %Z %Sm %N%T%SY";
/// 预置：`-s`（`stat.c:101-105`）。
pub const SHELL_FORMAT: &str = "st_dev=%d st_ino=%i st_mode=%#p st_nlink=%l \
    st_uid=%u st_gid=%g st_rdev=%r st_size=%z st_atime=%Sa st_mtime=%Sm \
    st_ctime=%Sc st_birthtime=%SB st_blksize=%k st_blocks=%b st_flags=%f";
/// 预置：`-x`（`stat.c:106-113`）。
pub const LINUX_FORMAT: &str = "  File: \"%N\"%n  Size: %-11z  FileType: %HT%n  \
    Mode: (%04OLp/%.10Sp)         Uid: (%5u/%8Su)  Gid: (%5g/%8Sg)%nDevice: %Hd,%Ld   \
    Inode: %i    Links: %l%nAccess: %Sa%nModify: %Sm%nChange: %Sc";
/// 默认时间模板（`stat.c:115`）；`-s` 用 `%s`、`-x` 用 `%c`（`stat.c:302-311`）。
pub const TIME_FORMAT: &str = "%b %e %T %Y";

/// 字段码。`stat.c:158-178` 的 `SHOW_*`，逐个对应一个字母。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Datum {
    /// `%R`：realpath（`stat.c:158`）。
    Realpath,
    /// `%d`：`st_dev`。
    Dev,
    /// `%i`：`st_ino`。
    Ino,
    /// `%p`：`st_mode`。
    Mode,
    /// `%l`：`st_nlink`。
    Nlink,
    /// `%u`：`st_uid`。
    Uid,
    /// `%g`：`st_gid`。
    Gid,
    /// `%r`：`st_rdev`。
    Rdev,
    /// `%a`：`st_atime`。
    Atime,
    /// `%m`：`st_mtime`。
    Mtime,
    /// `%c`：`st_ctime`。
    Ctime,
    /// `%B`：`st_birthtime`。
    Btime,
    /// `%z`：`st_size`。
    Size,
    /// `%b`：`st_blocks`。
    Blocks,
    /// `%k`：`st_blksize`。
    Blksize,
    /// `%f`：`st_flags`。
    Flags,
    /// `%v`：`st_generation`。
    Gen,
    /// `%Y`：符号链的内容。
    Symlink,
    /// `%T`：文件类型名。
    Filetype,
    /// `%N`：文件名。
    Filename,
    /// `%Z`：`sizerdev`——字符/块设备给 `rdev`，其余给 `size`（`stat.c:447-448` 的注释）。
    SizeRdev,
}

impl Datum {
    /// 解析字段码字母（`stat.c:515-537` 的那一长串 `fmtcase`）。
    fn from_char(c: u8) -> Option<Datum> {
        Some(match c {
            b'R' => Datum::Realpath,
            b'd' => Datum::Dev,
            b'i' => Datum::Ino,
            b'p' => Datum::Mode,
            b'l' => Datum::Nlink,
            b'u' => Datum::Uid,
            b'g' => Datum::Gid,
            b'r' => Datum::Rdev,
            b'a' => Datum::Atime,
            b'm' => Datum::Mtime,
            b'c' => Datum::Ctime,
            b'B' => Datum::Btime,
            b'z' => Datum::Size,
            b'b' => Datum::Blocks,
            b'k' => Datum::Blksize,
            b'f' => Datum::Flags,
            b'v' => Datum::Gen,
            b'Y' => Datum::Symlink,
            b'T' => Datum::Filetype,
            b'N' => Datum::Filename,
            b'Z' => Datum::SizeRdev,
            _ => return None,
        })
    }
}

/// 进制/形态选择字母 `D O U X F S`（`stat.c:140-145`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutFmt {
    Decimal,
    Octal,
    Unsigned,
    Hex,
    Float,
    String,
}

impl OutFmt {
    /// 解析输出形态字母（`stat.c:494-503`）。
    fn from_char(c: u8) -> Option<OutFmt> {
        Some(match c {
            b'D' => OutFmt::Decimal,
            b'O' => OutFmt::Octal,
            b'U' => OutFmt::Unsigned,
            b'X' => OutFmt::Hex,
            b'F' => OutFmt::Float,
            b'S' => OutFmt::String,
            _ => return None,
        })
    }
}

/// 子字段选择 `H M L`（`stat.c:154-156`）：取高位/中位/低位片段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hilo {
    High,
    Middle,
    Low,
}

impl Hilo {
    /// 解析子字段字母（`stat.c:505-513`）。
    fn from_char(c: u8) -> Option<Hilo> {
        Some(match c {
            b'H' => Hilo::High,
            b'M' => Hilo::Middle,
            b'L' => Hilo::Low,
            _ => return None,
        })
    }
}

/// 格式参数位。`stat.c:117-121` 的 `FLAG_POUND`…`FLAG_MINUS`，取值逐位照抄。
pub const FLAG_POUND: u32 = 0x01;
/// 见 `FLAG_POUND`（`stat.c:118`）。
pub const FLAG_SPACE: u32 = 0x02;
/// 见 `FLAG_POUND`（`stat.c:119`）。
pub const FLAG_PLUS: u32 = 0x04;
/// 见 `FLAG_POUND`（`stat.c:120`）。
pub const FLAG_ZERO: u32 = 0x08;
/// 见 `FLAG_POUND`（`stat.c:121`）。
pub const FLAG_MINUS: u32 = 0x10;

/// 单字符简写件（`stat.c:129-132` 的 `SIMPLE_*`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Simple {
    /// `%n`：换行。
    Newline,
    /// `%t`：制表符。
    Tab,
    /// `%%`：一个百分号。
    Percent,
    /// `%@`：本文件是第几个操作数（`stat.c:417-423`，`fn` 从 1 起）。
    FileNumber,
}

/// 一条完整字段件，带 C 用来报错的那段子格式串。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec<'a> {
    /// `FLAG_*` 的按位或（`stat.c:451-462`）。
    pub flags: u32,
    /// 字段宽度；`-1` 表示没写（C 的初值，`stat.c:468`）。
    pub size: i32,
    /// 精度；`-1` 表示没写（C 的初值，`stat.c:479`）。
    pub prec: i32,
    /// 输出形态，没写则 `None`（C 置 `ofmt = 0`，`stat.c:501`）。
    pub ofmt: Option<OutFmt>,
    /// 子字段，没写则 `None`（C 置 `hilo = 0`，`stat.c:512`）。
    pub hilo: Option<Hilo>,
    /// 字段码。
    pub datum: Datum,
    /// 本子格式串的原文（含起始的 `%`，到字段码为止）。C 把它连同长度一并交给
    /// `format1()`（`stat.c:570`），两处报错都用它做 `"%.*s: bad format"` 的回显：
    /// 扫描阶段读不出字段码在 `stat.c:555-556`，取值阶段发现该字段不接受所请求的
    /// 形态在 `stat.c:989`、`:997`、`:1117`。
    pub source: &'a str,
}

/// 模板拆开后的件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece<'a> {
    /// 普通字节，原样输出。
    Literal(u8),
    /// `%n` / `%t` / `%%` / `%@` 四条形同简写的写法。
    Simple(Simple),
    /// 一条字段件。
    Spec(Spec<'a>),
}

/// 坏格式。C 一律 `errx(1, "%.*s: bad format", …)`（`stat.c:554-556`），
/// 这里把那段子格式串交给调用方去渲染，纯逻辑层不必知道退出码。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadFormat {
    /// 报错文本里回显的那段子格式串（从 `%` 起，到读坏的那个字符止，含之）。
    pub span: String,
}

/// 拆模板（C 的 `output()` 主循环，`stat.c:380-556`）。
///
/// 与 C 同形的几处：`%` 后先试四个简写字母；再连吃五个参数位字母；然后是最多十进制
/// 宽度、可选 `.` 与最多十进制精度、可选形态字母、可选子字段字母，最后必须是字段码，
/// 否则整条子格式串（含读坏的那一字符）进 `BadFormat`。
pub fn parse_format(fmt: &str) -> Result<Vec<Piece<'_>>, BadFormat> {
    let bytes = fmt.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] != b'%' {
            out.push(Piece::Literal(bytes[i]));
            i += 1;
            continue;
        }
        // 子格式串从这里的 '%' 起算（C 的 `subfmt = statfmt`，`stat.c:398`）。
        let start = i;
        i += 1;

        // 四个单字符简写（`stat.c:402-425`）。
        if i < bytes.len() {
            let simple = match bytes[i] {
                b'n' => Some(Simple::Newline),
                b't' => Some(Simple::Tab),
                b'%' => Some(Simple::Percent),
                b'@' => Some(Simple::FileNumber),
                _ => None,
            };
            if let Some(s) = simple {
                out.push(Piece::Simple(s));
                i += 1;
                continue;
            }
        }

        let mut flags = 0u32;
        while i < bytes.len() {
            let f = match bytes[i] {
                b'#' => FLAG_POUND,
                b' ' => FLAG_SPACE,
                b'+' => FLAG_PLUS,
                b'0' => FLAG_ZERO,
                b'-' => FLAG_MINUS,
                _ => break,
            };
            flags |= f;
            i += 1;
        }

        let mut size = -1i32;
        if i < bytes.len() && bytes[i].is_ascii_digit() {
            size = 0;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                let next = size
                    .checked_mul(10)
                    .and_then(|v| v.checked_add((bytes[i] - b'0') as i32));
                match next {
                    Some(v) if v >= 0 => {
                        size = v;
                        i += 1;
                    }
                    // C 判的是乘加后为负（`stat.c:469-475`）；溢出到负与超出可表示
                    // 范围都落进同一支坏格式，报错文本按同一规则取。
                    _ => return Err(bad(fmt, start, i)),
                }
            }
        }

        let mut prec = -1i32;
        if i < bytes.len() && bytes[i] == b'.' {
            i += 1;
            prec = 0;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                let next = prec
                    .checked_mul(10)
                    .and_then(|v| v.checked_add((bytes[i] - b'0') as i32));
                match next {
                    Some(v) if v >= 0 => {
                        prec = v;
                        i += 1;
                    }
                    _ => return Err(bad(fmt, start, i)),
                }
            }
        }

        // 形态字母与子字段字母各最多一个（`stat.c:494-513`）。
        let ofmt = if i < bytes.len() {
            OutFmt::from_char(bytes[i])
        } else {
            None
        };
        if ofmt.is_some() {
            i += 1;
        }
        let hilo = if i < bytes.len() {
            Hilo::from_char(bytes[i])
        } else {
            None
        };
        if hilo.is_some() {
            i += 1;
        }

        let datum = match bytes.get(i).copied().and_then(Datum::from_char) {
            Some(d) => d,
            None => return Err(bad(fmt, start, i)),
        };
        i += 1;

        out.push(Piece::Spec(Spec {
            flags,
            size,
            prec,
            ofmt,
            hilo,
            datum,
            source: &fmt[start..i],
        }));
    }

    Ok(out)
}

/// 坏格式的报错文本：C 用 `statfmt - subfmt + 1` 的长度截子格式串
/// （`stat.c:554-556`）。这里的 `i` 停在读坏的位置（可能等于串尾），
/// 因此跨度是 `[start, i]` 闭区间；串尾时 C 会连 `'\0'` 一起计入长度，
/// 实际打出来的仍是到串尾为止的原文。
fn bad(fmt: &str, start: usize, i: usize) -> BadFormat {
    let end = (i + 1).min(fmt.len());
    BadFormat {
        span: fmt[start..end].to_string(),
    }
}

/// 命令行走哪条预置模板。C 用 `fmtchar` 一个整数表达（`stat.c:204`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FmtChar {
    /// 自定义 `-f`（或默认模板）。
    Format,
    /// `-l`（`-F` 时换成 `LSF_FORMAT`）。
    Ls,
    /// `-r`。
    Raw,
    /// `-s`。
    Shell,
    /// `-x`。
    Linux,
}

/// 同一条二进制按 `argv[0]` 换人格：`readlink` 是 `stat` 的另一张脸
/// （`stat.c:219-232`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Personality {
    Stat,
    Readlink,
}

/// 选项面的解析结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan<'a> {
    /// 人格。
    pub personality: Personality,
    /// 选到的模板：预置常量或 `-f` 自带。
    pub format: &'a str,
    /// 时间模板（`-t`），未给则为 `TIME_FORMAT`，`-s`/`-x` 各有默认（`stat.c:292-318`）。
    pub timefmt: &'a str,
    /// `-F`：`-l` 用带 `%T` 的那条形。
    pub ls_f: bool,
    /// `-L`：先 `stat()`，失败且 `ENOENT` 再退到 `lstat()`（`stat.c:327-334`）。
    pub use_stat: bool,
    /// `-n`：末尾不换行（`stat.c:559-560`）。
    pub no_nl: bool,
    /// `-q`：取值失败时不出声。
    pub quiet: bool,
    /// 操作数起点（C 的 `optind`）。
    pub operands: usize,
}

/// 选项面的失败。`Usage` 对应 C 的 `usage()` 退出（`stat.c:357-365`），
/// `Conflict` 对应两处 `errx`（`stat.c:261-263`、`stat.c:289-291`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    /// `usage: <prog> <synopsis>` 那条路。
    Usage,
    /// `can't use format '%c' with '%c'`：两个模板开关撞了（`stat.c:261-263`）。
    FormatConflict { first: char, second: char },
    /// `can't use format '%c' with -F`（`stat.c:289-291`）。
    NotWithDashF { fmtchar: char },
}

/// 扫参数（C 的 `getopt` 串在 `stat.c:219-232`：stat 是 `"f:FlLnqrst:x"`，
/// readlink 是 `"fnqsv"`）。
///
/// 两条人格的差别都照 C：readlink 默认模板 `%Y`、默认 `quiet`、`-f` 意为 `%R`、
/// `-s` 只是把 `quiet` 再置一次、`-v` 取消 `quiet`，而 stat 的 `-F -L -l -r -t -x`
/// 在它那里都是不认识的开关（不在选项串里）。
pub fn parse_args<'a>(args: &[&'a str], prog: &str) -> Result<Plan<'a>, PlanError> {
    let readlink = prog.ends_with("readlink");
    let mut format: Option<&'a str> = None;
    let mut timefmt: Option<&'a str> = None;
    let mut fmtchar: Option<char> = None;
    let mut ls_f = false;
    let mut use_stat = false;
    let mut no_nl = false;
    let mut quiet = readlink;

    let mut i = 0usize;
    while i < args.len() {
        let a = args[i];
        if a == "--" {
            i += 1;
            break;
        }
        let bytes = a.as_bytes();
        if bytes.len() < 2 || bytes[0] != b'-' {
            break; // BSD getopt 在第一个非选项处停下
        }
        let mut k = 1usize;
        while k < bytes.len() {
            let c = bytes[k] as char;
            match c {
                'F' if !readlink => ls_f = true,
                'L' if !readlink => use_stat = true,
                'n' => no_nl = true,
                'q' => quiet = true,
                'v' if readlink => quiet = false,
                'f' => {
                    if readlink {
                        // readlink 的选项串是 `"fnqsv"`（`stat.c:221`）：`f` 后面没有冒号，
                        // 所以 `-f` 不取值，它的意思就是「只报解析后的路径」。
                        format = Some("%R");
                    } else {
                        let value = take_value(args, &mut i, bytes, &mut k)?;
                        format = Some(value);
                        // `stat.c:245-258`：`-f` 与 `-l/-r/-s/-x` 共用 `fmtchar` 这个坑，
                        // 所以第二个模板开关会撞车。
                        claim_fmtchar(&mut fmtchar, 'f')?;
                    }
                }
                'l' | 'r' | 's' => {
                    if readlink {
                        // `stat.c:246-249`：readlink 的 `-s` 只是把 quiet 再置一次。
                        // `-l`/`-r` 不在它的选项串里，落到下面的未知分支。
                        if c == 's' {
                            quiet = true;
                        } else {
                            return Err(PlanError::Usage);
                        }
                    } else {
                        claim_fmtchar(&mut fmtchar, c)?;
                    }
                }
                'x' if !readlink => claim_fmtchar(&mut fmtchar, c)?,
                't' if !readlink => timefmt = Some(take_value(args, &mut i, bytes, &mut k)?),
                _ => return Err(PlanError::Usage),
            }
            k += 1;
        }
        i += 1;
    }

    // `stat.c:279-291`：没写模板开关时，`-F` 折成 `-l`，否则落默认模板。
    let fmtchar = match fmtchar {
        Some(c) => c,
        None if ls_f => 'l',
        None => 'f',
    };
    if ls_f && fmtchar != 'l' {
        return Err(PlanError::NotWithDashF { fmtchar });
    }

    if readlink {
        return Ok(Plan {
            personality: Personality::Readlink,
            format: format.unwrap_or("%Y"),
            timefmt: timefmt.unwrap_or(TIME_FORMAT),
            ls_f,
            use_stat,
            no_nl,
            quiet,
            operands: i,
        });
    }

    // `-s` 与 `-x` 各带自己的时间默认（`stat.c:292-318`）。
    let timefmt = match fmtchar {
        's' => timefmt.unwrap_or("%s"),
        'x' => timefmt.unwrap_or("%c"),
        _ => timefmt.unwrap_or(TIME_FORMAT),
    };
    let format: &'a str = match fmtchar {
        'f' => format.unwrap_or(DEF_FORMAT),
        'l' => {
            if ls_f {
                LSF_FORMAT
            } else {
                LS_FORMAT
            }
        }
        'r' => RAW_FORMAT,
        's' => SHELL_FORMAT,
        'x' => LINUX_FORMAT,
        _ => DEF_FORMAT,
    };

    Ok(Plan {
        personality: Personality::Stat,
        format,
        timefmt,
        ls_f,
        use_stat,
        no_nl,
        quiet,
        operands: i,
    })
}

/// 取带值开关的值：簇里还有字符就取余段（getopt 的 `-fVALUE` 形），
/// 否则吃下一个 argv；两者都没有就是缺参，C 里 getopt 返 `?` 走 usage。
fn take_value<'a>(
    args: &[&'a str],
    i: &mut usize,
    bytes: &[u8],
    k: &mut usize,
) -> Result<&'a str, PlanError> {
    if *k + 1 < bytes.len() {
        let value = &args[*i][*k + 1..];
        *k = bytes.len() - 1; // 消费整簇余段（外层还会 +1）
        return Ok(value);
    }
    let next = *i + 1;
    if next >= args.len() {
        return Err(PlanError::Usage);
    }
    *i = next;
    Ok(args[next])
}

/// `stat.c:255-257`：`fmtchar` 已被占就报 `can't use format '%c' with '%c'`。
fn claim_fmtchar(slot: &mut Option<char>, c: char) -> Result<(), PlanError> {
    if let Some(first) = *slot {
        return Err(PlanError::FormatConflict { first, second: c });
    }
    *slot = Some(c);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 字段码字母（与 `Datum::from_char` 反向，供测里拼原文）。
    fn datum_letter(datum: Datum) -> char {
        match datum {
            Datum::Realpath => 'R',
            Datum::Dev => 'd',
            Datum::Ino => 'i',
            Datum::Mode => 'p',
            Datum::Nlink => 'l',
            Datum::Uid => 'u',
            Datum::Gid => 'g',
            Datum::Rdev => 'r',
            Datum::Atime => 'a',
            Datum::Mtime => 'm',
            Datum::Ctime => 'c',
            Datum::Btime => 'B',
            Datum::Size => 'z',
            Datum::Blocks => 'b',
            Datum::Blksize => 'k',
            Datum::Flags => 'f',
            Datum::Gen => 'v',
            Datum::Symlink => 'Y',
            Datum::Filetype => 'T',
            Datum::Filename => 'N',
            Datum::SizeRdev => 'Z',
        }
    }

    /// 「无参数位、无宽度、无精度」的裸字段件，即 C 说的「每个数据都有合理的默认输出形」。
    fn bare(datum: Datum) -> Piece<'static> {
        let source = Box::leak(format!("%{}", datum_letter(datum)).into_boxed_str());
        Piece::Spec(Spec {
            flags: 0,
            size: -1,
            prec: -1,
            ofmt: None,
            hilo: None,
            datum,
            source,
        })
    }

    #[test]
    fn plain_bytes_pass_through() {
        let pieces = parse_format("ab").unwrap();
        assert_eq!(pieces, vec![Piece::Literal(b'a'), Piece::Literal(b'b')]);
    }

    #[test]
    fn the_four_simple_forms_are_one_character_each() {
        let pieces = parse_format("%n%t%%%@").unwrap();
        assert_eq!(
            pieces,
            vec![
                Piece::Simple(Simple::Newline),
                Piece::Simple(Simple::Tab),
                Piece::Simple(Simple::Percent),
                Piece::Simple(Simple::FileNumber),
            ]
        );
    }

    #[test]
    fn a_bare_datum_needs_no_flags_or_width() {
        let pieces = parse_format("%i").unwrap();
        let Piece::Spec(s) = &pieces[0] else {
            panic!("%i 应是字段件");
        };
        assert_eq!(s.datum, Datum::Ino);
        assert_eq!((s.flags, s.size, s.prec), (0, -1, -1));
        assert!(s.ofmt.is_none() && s.hilo.is_none());
        assert_eq!(s.source, "%i");
    }

    #[test]
    fn flags_are_or_ed_in_bit_order_from_the_c_macros() {
        let pieces = parse_format("%-# 0+z").unwrap();
        let Piece::Spec(s) = &pieces[0] else {
            panic!("-z 应是字段件");
        };
        assert_eq!(
            s.flags,
            FLAG_MINUS | FLAG_POUND | FLAG_SPACE | FLAG_ZERO | FLAG_PLUS
        );
        assert_eq!(s.datum, Datum::Size);
    }

    #[test]
    fn width_then_precision_then_form_then_subfield() {
        // `%04OLp`：宽 4、零填、八进制、低位片段、`st_mode`（LINUX_FORMAT 里的原件）。
        let pieces = parse_format("%04OLp").unwrap();
        let Piece::Spec(s) = &pieces[0] else {
            panic!("应是字段件");
        };
        assert_eq!((s.flags, s.size, s.prec), (FLAG_ZERO, 4, -1));
        assert_eq!(s.ofmt, Some(OutFmt::Octal));
        assert_eq!(s.hilo, Some(Hilo::Low));
        assert_eq!(s.datum, Datum::Mode);
        assert_eq!(s.source, "%04OLp");
    }

    #[test]
    fn a_dot_without_digits_still_sets_precision_zero() {
        let pieces = parse_format("%.10Sp").unwrap();
        let Piece::Spec(s) = &pieces[0] else {
            panic!("应是字段件");
        };
        assert_eq!((s.size, s.prec), (-1, 10));
        assert_eq!(s.ofmt, Some(OutFmt::String));
        let pieces = parse_format("%.p").unwrap();
        let Piece::Spec(s) = &pieces[0] else {
            panic!("应是字段件");
        };
        assert_eq!(s.prec, 0);
    }

    #[test]
    fn every_c_datum_letter_parses() {
        const LETTERS: [(u8, Datum); 21] = [
            (b'R', Datum::Realpath),
            (b'd', Datum::Dev),
            (b'i', Datum::Ino),
            (b'p', Datum::Mode),
            (b'l', Datum::Nlink),
            (b'u', Datum::Uid),
            (b'g', Datum::Gid),
            (b'r', Datum::Rdev),
            (b'a', Datum::Atime),
            (b'm', Datum::Mtime),
            (b'c', Datum::Ctime),
            (b'B', Datum::Btime),
            (b'z', Datum::Size),
            (b'b', Datum::Blocks),
            (b'k', Datum::Blksize),
            (b'f', Datum::Flags),
            (b'v', Datum::Gen),
            (b'Y', Datum::Symlink),
            (b'T', Datum::Filetype),
            (b'N', Datum::Filename),
            (b'Z', Datum::SizeRdev),
        ];
        for (letter, want) in LETTERS {
            let fmt = format!("%{}", letter as char);
            let pieces = parse_format(&fmt).unwrap_or_else(|_| panic!("{fmt} 应可解析"));
            assert_eq!(pieces.len(), 1, "{fmt} 只该有一件");
            assert_eq!(pieces[0], bare(want), "{fmt} 的解析形不匹配");
        }
    }

    #[test]
    fn unknown_letters_are_bad_format_and_the_span_includes_the_offender() {
        let err = parse_format("%Q").unwrap_err();
        assert_eq!(err.span, "%Q");
        // 参数位与宽度都吃了但字段码不对：跨度到读坏处为止。
        let err = parse_format("%-12Q").unwrap_err();
        assert_eq!(err.span, "%-12Q");
    }

    #[test]
    fn a_trailing_percent_is_bad_format_without_eating_a_real_char() {
        let err = parse_format("%").unwrap_err();
        assert_eq!(err.span, "%");
        // `%c` 本身是合法字段码（`st_ctime`），串尾的坏写法要用一个不认识的字母。
        let err = parse_format("ab%Q").unwrap_err();
        assert_eq!(err.span, "%Q");
    }

    #[test]
    fn a_hilo_needs_a_datum_after_it() {
        let err = parse_format("%H").unwrap_err();
        assert_eq!(err.span, "%H");
    }

    #[test]
    fn oversized_width_falls_into_the_same_bad_format() {
        // C 的判据是「乘加后有符号回绕成负」（`stat.c:469-475`），因此它对宽度的
        // 上限其实取决于 int 的宽度与回绕值；本实现用 checked 算术判，任何超出可
        // 表示范围的宽度都当场落坏格式。跨度停在把计算打爆的那一位数字（C 在报错
        // 前会先把游标推过一个字符，故它的回显可能多带后一个字符——这条窄差异登记
        // 在台账，不影响哪种写法会被拒掉）。
        let err = parse_format("%9999999999z").unwrap_err();
        assert_eq!(err.span, "%9999999999");
        // 精度那一串同理（`stat.c:479-488`）。
        let err = parse_format("%.9999999999z").unwrap_err();
        assert_eq!(err.span, "%.9999999999");
    }

    #[test]
    fn mixed_literal_and_specs_keep_their_order() {
        let pieces = parse_format("Size: %-11z%n").unwrap();
        // "Size: " 六个字面件 + 一条字段件 + 一个换行简写。
        assert_eq!(pieces.len(), 8);
        assert_eq!(pieces[0], Piece::Literal(b'S'));
        assert!(matches!(pieces[6], Piece::Spec(_)));
        assert!(matches!(pieces[7], Piece::Simple(Simple::Newline)));
    }

    #[test]
    fn the_default_plan_is_def_format_and_default_timefmt() {
        let plan = parse_args(&["/etc/rc"], "stat").unwrap();
        assert_eq!(plan.personality, Personality::Stat);
        assert_eq!(plan.format, DEF_FORMAT);
        assert_eq!(plan.timefmt, TIME_FORMAT);
        assert_eq!(plan.operands, 0);
        assert!(!plan.ls_f && !plan.use_stat && !plan.no_nl && !plan.quiet);
    }

    #[test]
    fn each_template_switch_picks_its_predefined_format() {
        assert_eq!(parse_args(&["-r", "f"], "stat").unwrap().format, RAW_FORMAT);
        assert_eq!(
            parse_args(&["-s", "f"], "stat").unwrap().format,
            SHELL_FORMAT
        );
        assert_eq!(
            parse_args(&["-x", "f"], "stat").unwrap().format,
            LINUX_FORMAT
        );
        assert_eq!(parse_args(&["-l", "f"], "stat").unwrap().format, LS_FORMAT);
        // -F 单用不改模板选择：C 把它折成 fmtchar='l'（stat.c:281-286）。
        assert_eq!(parse_args(&["-F", "f"], "stat").unwrap().format, LSF_FORMAT);
    }

    #[test]
    fn shell_and_linux_forms_carry_their_own_time_default() {
        assert_eq!(parse_args(&["-s"], "stat").unwrap().timefmt, "%s");
        assert_eq!(parse_args(&["-x"], "stat").unwrap().timefmt, "%c");
        // -t 给了就听它的，包括与 -s/-x 同时给（stat.c:261-263）。
        assert_eq!(
            parse_args(&["-s", "-t", "%Y"], "stat").unwrap().timefmt,
            "%Y"
        );
    }

    #[test]
    fn two_template_switches_conflict() {
        assert_eq!(
            parse_args(&["-r", "-s"], "stat").unwrap_err(),
            PlanError::FormatConflict {
                first: 'r',
                second: 's'
            }
        );
        // 同一字母写两遍也撞：C 的判据只看 fmtchar 是否已被占。
        assert!(matches!(
            parse_args(&["-x", "-x"], "stat").unwrap_err(),
            PlanError::FormatConflict { .. }
        ));
    }

    #[test]
    fn the_ls_f_flag_only_pairs_with_the_l_flag() {
        assert_eq!(
            parse_args(&["-F", "-r"], "stat").unwrap_err(),
            PlanError::NotWithDashF { fmtchar: 'r' }
        );
        assert_eq!(
            parse_args(&["-F", "-x"], "stat").unwrap_err(),
            PlanError::NotWithDashF { fmtchar: 'x' }
        );
    }

    #[test]
    fn dash_f_shares_the_template_slot_so_the_second_switch_conflicts() {
        let plan = parse_args(&["-f", "%N", "f"], "stat").unwrap();
        assert_eq!(plan.format, "%N");
        // `stat.c:245-258` 的 FALLTHROUGH 让 `-f` 自己就占下 `fmtchar='f'`，
        // 第二个模板开关（包括第二个 `-f`）都撞车。
        assert_eq!(
            parse_args(&["-f", "%N", "-r"], "stat").unwrap_err(),
            PlanError::FormatConflict {
                first: 'f',
                second: 'r'
            }
        );
        assert_eq!(
            parse_args(&["-f", "%N", "-f", "%Z"], "stat").unwrap_err(),
            PlanError::FormatConflict {
                first: 'f',
                second: 'f'
            }
        );
        // 缺值走 usage（C 里 getopt 返 `?`）。
        assert_eq!(parse_args(&["-f"], "stat"), Err(PlanError::Usage));
        // 贴在簇后面的值形（getopt 的 `-fVALUE`）也认。
        assert_eq!(parse_args(&["-f%N"], "stat").unwrap().format, "%N");
        // 同一个簇里的余段全归 `-f`，不会被再当选项扫一遍。
        let plan = parse_args(&["-nqf%N", "x"], "stat").unwrap();
        assert!(plan.no_nl && plan.quiet);
        assert_eq!(plan.format, "%N");
        assert_eq!(plan.operands, 1);
    }

    #[test]
    fn clustered_switches_and_operand_boundary() {
        let plan = parse_args(&["-Lnq", "a", "b"], "stat").unwrap();
        assert!(plan.use_stat && plan.no_nl && plan.quiet);
        assert_eq!(plan.operands, 1);
        // `-` 单个字符不是选项，是 stdin 操作数（getopt 的行为）。
        let plan = parse_args(&["-", "f"], "stat").unwrap();
        assert_eq!(plan.operands, 0);
    }

    #[test]
    fn unknown_switch_is_usage() {
        assert_eq!(parse_args(&["-Z"], "stat"), Err(PlanError::Usage));
    }

    #[test]
    fn the_readlink_personality_flips_three_things_at_once() {
        let plan = parse_args(&["/etc"], "readlink").unwrap();
        assert_eq!(plan.personality, Personality::Readlink);
        assert_eq!(plan.format, "%Y");
        assert!(plan.quiet);
        // readlink 的 -f 不取值（它的选项串里 `f` 不带冒号），意为 %R；-v 取消 quiet。
        assert_eq!(parse_args(&["-f"], "readlink").unwrap().format, "%R");
        assert!(!parse_args(&["-v"], "readlink").unwrap().quiet);
        // `-s` 在 readlink 里只是把 quiet 再置一次（`stat.c:246-249`），
        // 而 `-l`/`-r`/`-x`/`-F`/`-L`/`-t` 都不在它的选项串里。
        assert!(parse_args(&["-s"], "readlink").unwrap().quiet);
        assert_eq!(parse_args(&["-l"], "readlink"), Err(PlanError::Usage));
        assert_eq!(parse_args(&["-F"], "readlink"), Err(PlanError::Usage));
        assert_eq!(parse_args(&["-t", "%Y"], "readlink"), Err(PlanError::Usage));
        // stat 的 `-v` 同样不在选项串里。
        assert_eq!(parse_args(&["-v"], "stat"), Err(PlanError::Usage));
    }
}
