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

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// 预置模板。C 用四个宏加 `TIME_FORMAT` 表达（`stat.c:94-115`）；此处原样抄成常量，
/// 好让「选了哪条预置」这一件事在宿主测里可直接断言。
pub const DEF_FORMAT: &str =
    "%d %i %Sp %l %Su %Sg %r %z \"%Sa\" \"%Sm\" \"%Sc\" \"%SB\" %k %b %#Xf %N";
/// 预置：`-r`（`stat.c:97-98`）。
pub const RAW_FORMAT: &str = "%d %i %#p %l %u %g %r %z %a %m %c %B %k %b %f %N";
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

/// 一个时间字段：整秒加纳秒（C 的 `st_*time` 与 `st_*timensec`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeField {
    pub secs: i64,
    pub nsecs: i64,
}

impl TimeField {
    /// 组装一个时间字段。
    pub const fn new(secs: i64, nsecs: i64) -> Self {
        TimeField { secs, nsecs }
    }
}

/// 一份纯字段的文件状态。薄壳把内核交回的状态结构逐字段拷进来，本层因此
/// 不碰任何系统调用面（分层边界：库侧只用 `minix_sys` 的薄壳才许读句柄）。
///
/// 宽度按 C 的字段宽度给定，因为 `format1()` 的 `small` 判据（`stat.c:606`、
/// `:636`、`:684` 一类 `sizeof(...) == 4`）真的会改变非十进制形态下的取值：
/// 32 位字段在八进制/无符号/十六进制形里先截到 32 位（`stat.c:1126-1128`）。
#[derive(Debug, Clone, Copy)]
pub struct StatData {
    /// `st_dev`（`dev_t`，64 位）。
    pub dev: u64,
    /// `st_mode`（`mode_t`，32 位）。
    pub mode: u32,
    /// `st_ino`（64 位）。
    pub ino: u64,
    /// `st_nlink`（32 位）。
    pub nlink: u32,
    /// `st_uid`（32 位）。
    pub uid: u32,
    /// `st_gid`（32 位）。
    pub gid: u32,
    /// `st_rdev`（64 位）。
    pub rdev: u64,
    /// `st_atim`。
    pub atime: TimeField,
    /// `st_mtim`。
    pub mtime: TimeField,
    /// `st_ctim`。
    pub ctime: TimeField,
    /// `st_birthtim`。
    pub btime: TimeField,
    /// `st_size`（64 位，有符号）。
    pub size: i64,
    /// `st_blocks`（64 位，512 字节块数）。
    pub blocks: i64,
    /// `st_blksize`（32 位）。
    pub blksize: i32,
    /// `st_flags`（32 位）。
    pub flags: u32,
    /// `st_gen`（32 位）。字段名不叫 `gen`：那是 2024 版语言的保留字。
    pub generation: u32,
}

/// 形态允许集，位值与 C 的 `FMTF_*` 逐位相同（`stat.c:147-152`）。
mod fmtf {
    pub const DECIMAL: u8 = 0x01;
    pub const OCTAL: u8 = 0x02;
    pub const UNSIGNED: u8 = 0x04;
    pub const HEX: u8 = 0x08;
    pub const FLOAT: u8 = 0x10;
    pub const STRING: u8 = 0x20;
}

/// 渲染一条件所需的外部信息。符号链内容由薄壳一次读好交进来：C 也是在
/// `format1()` 里就地 `readlink()`（`stat.c:844-861`），把这条腿留在薄壳侧
/// 才能让本层的渲染完全可测。
pub struct Render<'a> {
    /// 文件状态。
    pub data: &'a StatData,
    /// 操作数名；`None` 表示标准输入（C 的 `file == NULL`，打 `(stdin)`）。
    pub name: Option<&'a str>,
    /// 符号链目标。只有 `mode` 确为符链时才有意义。
    pub link: Option<&'a str>,
    /// 时间模板（`-t`，或 `-s`/`-x` 的默认，或 `TIME_FORMAT`）。
    pub timefmt: &'a str,
    /// `%@` 用的操作数序号，C 从 1 起（`stat.c:278` 的 `fn = 1`）。
    pub file_number: usize,
}

/// 一轮渲染的可观察副作用，薄壳据此决定退出码与末尾换行。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Report {
    /// C 的全局 `linkfail`：`%Y` 读不到目标或 `%R` 解析失败时置起
    /// （`stat.c:850`、`:829`、`:859`），`readlink` 人格用它当退出码（`stat.c:354`）。
    pub linkfail: bool,
    /// 本轮输出是否以换行结尾（`stat.c:559-560` 据此决定是否补一个换行）。
    pub ends_with_newline: bool,
}

/// 渲染阶段的失败。C 在这两处都是 `errx(1, "%.*s: bad format", flen, fmt)`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FaceError {
    /// 子片段字母或形态字母与该字段不合（`stat.c:994-996`），或字符串形
    /// 却没有字符串可打（`stat.c:1116-1117`）。带着那条子格式串的原文。
    BadFormat(String),
    /// `%R` 需要 realpath 那条腿（逐段解析符号链的路径解析器），本树尚无封装。
    /// 显式拒绝而不是悄悄打空串，免得把未接线的形当成渲染错了。
    RealpathUnported,
}

/// 一条字段的取值结果。`num` 总在（数值形用），`text` 只在有字符串形时在，
/// `nsecs` 只有时间字段带得动 `F` 形态。
struct Field {
    num: u64,
    text: Option<String>,
    allowed: u8,
    default: OutFmt,
    small: bool,
    shift: u32,
    linkfail: bool,
    /// 只有时间字段带得动它：`F` 形态打的是 `秒.纳秒`（`stat.c:1029-1099`）。
    nsecs: i64,
    /// `%Z` 在字符/块设备上走的那一支：C 直接把两个片段各自渲染后用逗号接起来
    /// （`stat.c:966-978`），数值形与字符串形的选择只作用于这两个片段。
    pair_of_rdev: bool,
}

impl Field {
    fn numeric(num: u64, allowed: u8, default: OutFmt, small: bool) -> Field {
        Field {
            num,
            text: None,
            allowed,
            default,
            small,
            shift: 0,
            linkfail: false,
            nsecs: 0,
            pair_of_rdev: false,
        }
    }
}

/// 取一条字段的值。对应 `format1()` 开头那一大段 `switch (what)`
/// （`stat.c:600-990`）。`hilo` 在这里就折掉：能被某字段消费的子片段字母
/// 会被吃掉并置 `None`，消费不掉的留着，由调用方按 C 判坏格式。
fn take_field(spec: &Spec<'_>, ctx: &Render<'_>) -> Result<(Field, Option<Hilo>), FaceError> {
    let d = ctx.data;
    let mut hilo = spec.hilo;
    let field = match spec.datum {
        Datum::Dev | Datum::Rdev => {
            let raw = if spec.datum == Datum::Dev {
                d.dev
            } else {
                d.rdev
            };
            // C 的片段取法：`major()` 取高位、`minor()` 取低位（`stat.c:618-625`）。
            let num = match hilo {
                Some(Hilo::High) => {
                    hilo = None;
                    major_of(raw)
                }
                Some(Hilo::Low) => {
                    hilo = None;
                    minor_of(raw)
                }
                _ => raw,
            };
            let mut f = Field::numeric(
                num,
                fmtf::DECIMAL | fmtf::OCTAL | fmtf::UNSIGNED | fmtf::HEX | fmtf::STRING,
                OutFmt::Unsigned,
                false,
            );
            // `devname(3)` 需要扫 `/dev` 找配对项，本树没有这条腿，取 C 找不到时
            // 的那一支：打 `???`（`stat.c:616` 的 `sdata = "???"`）。
            f.text = Some("???".to_string());
            f
        }
        Datum::Ino => Field::numeric(
            d.ino,
            fmtf::DECIMAL | fmtf::OCTAL | fmtf::UNSIGNED | fmtf::HEX,
            OutFmt::Unsigned,
            false,
        ),
        Datum::Mode => {
            // `strmode` 的十一个字符里去掉末尾填充位，再按片段切
            // （`stat.c:636-665`：高位取用户三位、中位取组三位、低位取其他三位）。
            let mut smode = crate::mode::strmode(d.mode).to_vec();
            if smode.last() == Some(&b' ') {
                smode.pop();
            }
            let (num, slice) = match hilo {
                Some(Hilo::High) => {
                    // 数值折成类型位加用户三位（`stat.c:652`：`data >>= 12`）。
                    hilo = None;
                    ((d.mode >> 12) as u64, &smode[1..4])
                }
                Some(Hilo::Middle) => {
                    hilo = None;
                    (((d.mode >> 9) & 0o7) as u64, &smode[4..7])
                }
                Some(Hilo::Low) => {
                    hilo = None;
                    ((d.mode & 0o777) as u64, &smode[7..10])
                }
                _ => (d.mode as u64, smode.as_slice()),
            };
            Field {
                num,
                text: Some(String::from_utf8_lossy(slice).into_owned()),
                allowed: fmtf::DECIMAL | fmtf::OCTAL | fmtf::UNSIGNED | fmtf::HEX | fmtf::STRING,
                default: OutFmt::Octal,
                small: true,
                shift: 0,
                linkfail: false,
                nsecs: 0,
                pair_of_rdev: false,
            }
        }
        Datum::Nlink => Field::numeric(
            d.nlink as u64,
            fmtf::DECIMAL | fmtf::OCTAL | fmtf::UNSIGNED | fmtf::HEX,
            OutFmt::Unsigned,
            true,
        ),
        // 口令库在本树尚无封装（同 `chown` 的前置），因此永远走 C 的「查不到」支：
        // 打 `(123)` 这一形（`stat.c:685-690`、`:699-704`），不是裸数字。
        Datum::Uid => named_id(d.uid as u64, true),
        Datum::Gid => named_id(d.gid as u64, false),
        Datum::Size => {
            let mut f = Field::numeric(
                d.size as u64,
                fmtf::DECIMAL | fmtf::OCTAL | fmtf::UNSIGNED | fmtf::HEX,
                OutFmt::Unsigned,
                false,
            );
            // 大/中/小片段在 `stat.c:767-782` 是「按 2 的幂换算到 G/M/K」，
            // 打印前再「右移一位加一后取整」（`stat.c:1144-1148`），即四舍五入。
            f.shift = match hilo {
                Some(Hilo::High) => {
                    hilo = None;
                    30
                }
                Some(Hilo::Middle) => {
                    hilo = None;
                    20
                }
                Some(Hilo::Low) => {
                    hilo = None;
                    10
                }
                _ => 0,
            };
            f
        }
        Datum::Blocks => Field::numeric(
            d.blocks as u64,
            fmtf::DECIMAL | fmtf::OCTAL | fmtf::UNSIGNED | fmtf::HEX,
            OutFmt::Unsigned,
            false,
        ),
        Datum::Blksize => Field::numeric(
            d.blksize as u64,
            fmtf::DECIMAL | fmtf::OCTAL | fmtf::UNSIGNED | fmtf::HEX,
            OutFmt::Unsigned,
            true,
        ),
        Datum::Flags => Field::numeric(
            d.flags as u64,
            fmtf::DECIMAL | fmtf::OCTAL | fmtf::UNSIGNED | fmtf::HEX,
            OutFmt::Unsigned,
            true,
        ),
        Datum::Gen => Field::numeric(
            d.generation as u64,
            fmtf::DECIMAL | fmtf::OCTAL | fmtf::UNSIGNED | fmtf::HEX,
            OutFmt::Unsigned,
            true,
        ),
        Datum::Atime | Datum::Mtime | Datum::Ctime | Datum::Btime => {
            let t = match spec.datum {
                Datum::Atime => d.atime,
                Datum::Mtime => d.mtime,
                Datum::Ctime => d.ctime,
                _ => d.btime,
            };
            time_field(t, ctx.timefmt)
        }
        Datum::Realpath => return Err(FaceError::RealpathUnported),
        Datum::Symlink => {
            // C 的次序要留着：默认形态在指针算好之后才补上（`stat.c:855-856`），
            // 所以裸 `%Y` 不带 ` -> ` 前缀，显式 `%SY` 才带（`stat.c:443-446` 的注释）。
            let explicit_string = spec.ofmt == Some(OutFmt::String);
            if (d.mode & crate::cp::S_IFMT) == crate::cp::S_IFLNK {
                let body = ctx.link.unwrap_or_default().to_string();
                let text = if explicit_string {
                    format!(" -> {body}")
                } else {
                    body
                };
                Field {
                    num: 0,
                    text: Some(text),
                    allowed: fmtf::STRING,
                    default: OutFmt::String,
                    small: false,
                    shift: 0,
                    linkfail: ctx.link.is_none(),
                    nsecs: 0,
                    pair_of_rdev: false,
                }
            } else {
                Field {
                    num: 0,
                    text: Some(String::new()),
                    allowed: fmtf::STRING,
                    default: OutFmt::String,
                    small: false,
                    shift: 0,
                    linkfail: true,
                    nsecs: 0,
                    pair_of_rdev: false,
                }
            }
        }
        Datum::Filetype => {
            let (name, mark) = type_pair(d.mode);
            match hilo {
                None | Some(Hilo::Low) => {
                    hilo = None;
                    text_field(mark.to_string())
                }
                Some(Hilo::High) => {
                    hilo = None;
                    text_field(name.to_string())
                }
                _ => Field::numeric(0, fmtf::STRING, OutFmt::String, false),
            }
        }
        Datum::Filename => {
            let Some(name) = ctx.name else {
                let f = text_field("(stdin)".to_string());
                // `(stdin)` 没有路径可切，C 在此直接丢掉高/低片段（`stat.c:919-926`）。
                if hilo == Some(Hilo::High) || hilo == Some(Hilo::Low) {
                    hilo = None;
                }
                return Ok((f, hilo));
            };
            if spec.hilo.is_none() {
                text_field(name.to_string())
            } else {
                let trimmed = name.trim_end_matches('/');
                match hilo {
                    Some(Hilo::High) => {
                        hilo = None;
                        let dir = trimmed.rsplit_once('/').map_or(".", |(head, _)| head);
                        text_field(dir.to_string())
                    }
                    Some(Hilo::Low) => {
                        hilo = None;
                        let base = trimmed.rsplit_once('/').map_or(trimmed, |(_, tail)| tail);
                        text_field(base.to_string())
                    }
                    _ => text_field(name.to_string()),
                }
            }
        }
        Datum::SizeRdev => {
            // `%Z`：字符/块设备走 `rdev` 的高低片段，其余走 `size`
            // （`stat.c:961-989`，C 用两次递归调用拼出 `major,minor`）。
            let is_dev = (d.mode & crate::cp::S_IFMT) == crate::cp::S_IFCHR
                || (d.mode & crate::cp::S_IFMT) == crate::cp::S_IFBLK;
            if is_dev {
                let mut f = Field::numeric(
                    0,
                    fmtf::DECIMAL | fmtf::OCTAL | fmtf::UNSIGNED | fmtf::HEX,
                    OutFmt::Unsigned,
                    false,
                );
                f.pair_of_rdev = true;
                f
            } else {
                let mut f = take_field(
                    &Spec {
                        flags: spec.flags,
                        size: spec.size,
                        prec: spec.prec,
                        ofmt: spec.ofmt,
                        hilo: None,
                        datum: Datum::Size,
                        source: spec.source,
                    },
                    ctx,
                )?;
                f.1 = None;
                f.0
            }
        }
    };
    Ok((field, hilo))
}

/// `%Su`/`%Sg`：查得到名字打名字，查不到打 `(数字)`。本树没有口令库，
/// 所以恒走后者；这一支与 C 的 `getpwuid()` 返回 `NULL` 时同形（`stat.c:685-690`）。
fn named_id(value: u64, is_user: bool) -> Field {
    let mut f = Field::numeric(
        value,
        fmtf::DECIMAL | fmtf::OCTAL | fmtf::UNSIGNED | fmtf::HEX | fmtf::STRING,
        OutFmt::Unsigned,
        true,
    );
    f.text = Some(format!("({})", value));
    let _ = is_user;
    f
}

fn text_field(text: String) -> Field {
    Field {
        num: 0,
        text: Some(text),
        allowed: fmtf::STRING,
        default: OutFmt::String,
        small: false,
        shift: 0,
        linkfail: false,
        nsecs: 0,
        pair_of_rdev: false,
    }
}

/// 时间字段：数值形给整秒，字符串形走 `strftime` 子集，浮点形另带纳秒
/// （`stat.c:741-758`）。C 用 `localtime()`；本层与 `date`/`who` 同一口径——
/// 无时区库，分解即 UTC。
fn time_field(t: TimeField, timefmt: &str) -> Field {
    let mut text = String::new();
    if t.secs >= 0 || t.secs.checked_add(0).is_some() {
        let broken = minix_proctools::dates::breakdown(t.secs);
        let mut buf = [0u8; 256];
        // 交回秒数本身，`%s` 才有得打（`-s` 的默认模板就是它）。
        if let Ok(used) =
            minix_proctools::dates::format_at(timefmt, &broken, Some(t.secs), &mut buf)
        {
            text = String::from_utf8_lossy(&buf[..used]).into_owned();
        }
    }
    Field {
        num: t.secs as u64,
        text: Some(text),
        allowed: fmtf::DECIMAL
            | fmtf::OCTAL
            | fmtf::UNSIGNED
            | fmtf::HEX
            | fmtf::FLOAT
            | fmtf::STRING,
        default: OutFmt::Decimal,
        small: false,
        shift: 0,
        linkfail: false,
        nsecs: t.nsecs,
        pair_of_rdev: false,
    }
}

/// 主设备号。Minix3 给 NetBSD 派生工具用的那对宏在
/// `minix3/tools/compat/compat_defs.h:1254`：`(x & 0x000fff00) >> 8`。
fn major_of(dev: u64) -> u64 {
    (dev & 0x000f_ff00) >> 8
}

/// 次设备号。同一处 `compat_defs.h:1259-1260`：高段 `0xfff00000 >> 12` 或上低八位。
fn minor_of(dev: u64) -> u64 {
    ((dev & 0xfff0_0000) >> 12) | (dev & 0x0000_00ff)
}

/// `%T` 的两副面孔：高位是名字，低位/缺省是 `ls` 那一类记号
/// （`stat.c:867-912`）。未知类型两副都落 `???`。
fn type_pair(mode: u32) -> (&'static str, &'static str) {
    const S_IFMT: u32 = crate::cp::S_IFMT;
    match mode & S_IFMT {
        crate::cp::S_IFIFO => ("Fifo File", "|"),
        crate::cp::S_IFCHR => ("Character Device", "c"),
        crate::cp::S_IFDIR => ("Directory", "/"),
        crate::cp::S_IFBLK => ("Block Device", "b"),
        crate::cp::S_IFREG => ("Regular File", if mode & 0o111 != 0 { "*" } else { "-" }),
        crate::cp::S_IFLNK => ("Symbolic Link", "@"),
        crate::cp::S_IFSOCK => ("Socket", "="),
        crate::mode::S_IFWHT => ("Whiteout File", "%"),
        _ => ("???", "?"),
    }
}

/// 把件表渲染成文本（`output()` 的输出腿 + `format1()` 的全部形态选择）。
///
/// 连续的普通字节合并成一段再落进 `out`，这样模板里的非 ASCII 文字（UTF-8 的
/// 多字节序列被拆成逐字节件）不会被拆散。
pub fn render(
    pieces: &[Piece<'_>],
    ctx: &Render<'_>,
    out: &mut String,
) -> Result<Report, FaceError> {
    let mut linkfail = false;
    let mut pending: Vec<u8> = Vec::new();
    for piece in pieces {
        match piece {
            Piece::Literal(b) => pending.push(*b),
            Piece::Simple(s) => {
                flush(&mut pending, out);
                out.push(match s {
                    Simple::Newline => '\n',
                    Simple::Tab => '\t',
                    Simple::Percent => '%',
                    Simple::FileNumber => {
                        // C 把 `fn` 用 `%d` 打出来（`stat.c:417-424`）。
                        pending.clear();
                        out.push_str(&itoa(ctx.file_number as i64));
                        continue;
                    }
                });
            }
            Piece::Spec(spec) => {
                flush(&mut pending, out);
                linkfail |= render_spec(spec, ctx, out)?;
            }
        }
    }
    flush(&mut pending, out);
    Ok(Report {
        linkfail,
        ends_with_newline: out.ends_with('\n'),
    })
}

fn flush(pending: &mut Vec<u8>, out: &mut String) {
    if !pending.is_empty() {
        out.push_str(&String::from_utf8_lossy(pending));
        pending.clear();
    }
}

/// 渲染一条字段件，返回 C 的 `linkfail` 是否该置起。
fn render_spec(spec: &Spec<'_>, ctx: &Render<'_>, out: &mut String) -> Result<bool, FaceError> {
    let (field, leftover_hilo) = take_field(spec, ctx)?;
    let form = match spec.ofmt {
        Some(f) => f,
        None => field.default,
    };
    // `stat.c:994-996`：子片段没人吃掉、或形态不在该字段的允许集里，都是坏格式。
    if leftover_hilo.is_some() || !allowed_has(field.allowed, form) {
        return Err(FaceError::BadFormat(spec.source.to_string()));
    }
    if field.pair_of_rdev {
        // C 在这里递归渲染主次两段再用逗号接起来，宽度与精度作用于每一段。
        let head = render_number(
            spec.flags,
            spec.size,
            spec.prec,
            form,
            major_of(ctx.data.rdev),
        );
        let tail = render_number(
            spec.flags,
            spec.size,
            spec.prec,
            form,
            minor_of(ctx.data.rdev),
        );
        out.push_str(&format!("{head},{tail}"));
        return Ok(field.linkfail);
    }
    if form == OutFmt::String {
        let Some(text) = &field.text else {
            // `stat.c:1116-1117`：要字符串形却没有字符串可打。
            return Err(FaceError::BadFormat(spec.source.to_string()));
        };
        let body = if spec.flags & FLAG_POUND != 0 {
            vis(text)
        } else {
            text.clone()
        };
        out.push_str(&pad(&body, spec.size, spec.prec, spec.flags));
        return Ok(field.linkfail);
    }
    if form == OutFmt::Float {
        out.push_str(&render_float(
            spec.flags,
            spec.size,
            spec.prec,
            field.num,
            field.nsecs,
        ));
        return Ok(field.linkfail);
    }
    let mut value = field.num;
    if field.small && form != OutFmt::Decimal {
        // `stat.c:1128-1130`：32 位字段在非十进制形里先截到 32 位，免得符号扩展
        // 把输出打成一片 f。
        value &= 0xffff_ffff;
    }
    if field.shift > 0 {
        // `stat.c:1144-1148`：先右移一位、加一、再右移一位，即除以 2 的幂后四舍五入。
        value = (value >> (field.shift - 1)) + 1;
        value >>= 1;
    }
    out.push_str(&render_number(
        spec.flags, spec.size, spec.prec, form, value,
    ));
    Ok(field.linkfail)
}

fn allowed_has(allowed: u8, form: OutFmt) -> bool {
    let bit = match form {
        OutFmt::Decimal => fmtf::DECIMAL,
        OutFmt::Octal => fmtf::OCTAL,
        OutFmt::Unsigned => fmtf::UNSIGNED,
        OutFmt::Hex => fmtf::HEX,
        OutFmt::Float => fmtf::FLOAT,
        OutFmt::String => fmtf::STRING,
    };
    allowed & bit != 0
}

/// 数值形：等价于 C 拼出的 `%[flags][width][.prec]ll[doxu]`。
fn render_number(flags: u32, width: i32, prec: i32, form: OutFmt, value: u64) -> String {
    let signed = form == OutFmt::Decimal;
    let negative = signed && (value as i64) < 0;
    let magnitude = if negative {
        (value as i64).unsigned_abs()
    } else {
        value
    };
    let digits = match form {
        OutFmt::Hex => format!("{:x}", magnitude),
        OutFmt::Octal => format!("{:o}", magnitude),
        // 无符号形要把整 64 位打出来（`%z` 对一个负 `st_size` 就打回绕后的巨数）。
        OutFmt::Unsigned => format!("{magnitude}"),
        _ => itoa(magnitude as i64),
    };
    let digits = if prec >= 0 {
        pad_left(&digits, prec as usize, b'0' as char)
    } else {
        digits
    };

    let mut prefix = String::new();
    if flags & FLAG_POUND != 0 {
        match form {
            OutFmt::Hex => prefix.push_str("0x"),
            OutFmt::Octal if !digits.starts_with('0') => prefix.push('0'),
            _ => {}
        }
    }
    let mut sign = String::new();
    if negative {
        sign.push('-');
    } else if signed {
        if flags & FLAG_PLUS != 0 {
            sign.push('+');
        } else if flags & FLAG_SPACE != 0 {
            sign.push(' ');
        }
    }

    let body = format!("{sign}{prefix}{digits}");
    let zero_pad = flags & FLAG_ZERO != 0 && prec < 0 && flags & FLAG_MINUS == 0;
    if width > 0 && body.chars().count() < width as usize {
        let fill = width as usize - body.chars().count();
        if zero_pad {
            // 零填在符号与前缀之后、数字之前（`%#08x` 打 `0x0000ff`）。
            return format!("{}{}{}{}", sign, prefix, "0".repeat(fill), digits);
        }
        if flags & FLAG_MINUS != 0 {
            return format!("{body}{}", " ".repeat(fill));
        }
        return format!("{}{body}", " ".repeat(fill));
    }
    body
}

/// `F` 形态：`秒.纳秒`。C 把宽度让给秒（`size -= prec + 1`，`stat.c:1056-1062`），
/// 精度超过九位就用零补足（`stat.c:1088-1093`）。
fn render_float(flags: u32, width: i32, prec: i32, secs: u64, nsecs: i64) -> String {
    // 精度为 0 就是「只要秒」，小数点也不打（`stat.c:1031-1042`）。
    if prec == 0 {
        return render_number(flags, width, -1, OutFmt::Decimal, secs);
    }
    // 未给精度 = 有几位打几位（九位），给了就照给（`stat.c:1045-1048`）。
    let want = if prec < 0 { 9 } else { prec };
    let shown = want.min(9) as usize;
    // 宽度让给秒：C 先扣掉小数点与小数位（`stat.c:1050-1062`）。
    let secs_width = if width > 0 { width - (want + 1) } else { width };
    let head = render_number(flags, secs_width, -1, OutFmt::Decimal, secs);
    let mut frac = nsecs.unsigned_abs();
    for _ in shown..9 {
        frac /= 10;
    }
    let mut body = format!("{head}.{frac}");
    if let Some(pos) = body.find('.') {
        let digits = body.len() - pos - 1;
        if digits < shown {
            body.insert_str(pos + 1, &"0".repeat(shown - digits));
        }
    }
    // 超过九位的部分只能补零（`stat.c:1088-1093`）。
    if want > 9 {
        body.push_str(&"0".repeat((want - 9) as usize));
    }
    body
}

/// 十进制整数字面量（C 各处 `%d`/`%lld` 的最小实现）。
fn itoa(value: i64) -> String {
    format!("{value}")
}

/// 左补到 `width` 个字符，不足才补。
fn pad_left(text: &str, width: usize, fill: char) -> String {
    if text.chars().count() >= width {
        return text.to_string();
    }
    format!(
        "{}{text}",
        fill.to_string().repeat(width - text.chars().count())
    )
}

/// 字符串形：精度截断、宽度填充。`0` 标志对字符串不生效——C 那侧取决于
/// libc 是否按 glibc 那样把零填用于 `%s`，本树按 POSIX 的保守读法用空格，
/// 五条预置模板都不涉及这一形。
fn pad(text: &str, width: i32, prec: i32, flags: u32) -> String {
    let mut body: String = if prec >= 0 {
        text.chars().take(prec as usize).collect()
    } else {
        text.to_string()
    };
    if width > 0 && body.chars().count() < width as usize {
        let fill = " ".repeat(width as usize - body.chars().count());
        if flags & FLAG_MINUS != 0 {
            body.push_str(&fill);
        } else {
            body = format!("{fill}{body}");
        }
    }
    body
}

/// `vis(3)` 的一个子集：`VIS_CSTYLE | VIS_OCTAL | VIS_WHITE`（`stat.c:1000-1006`
/// 在 `#` 与字符串形并现时用它）。可打印 ASCII 原样出，有名字的七控制字符
/// 加 `NUL` 走反斜杠形，其余（含空格与所有非 ASCII 字节）走三位八进制。
fn vis(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        match ch {
            '\0' => out.push_str("\\0"),
            '\u{7}' => out.push_str("\\a"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{b}' => out.push_str("\\v"),
            '\\' => out.push_str("\\\\"),
            c if c.is_ascii_graphic() => out.push(c),
            // VIS_WHITE 把空格也算作要转义的空白；非 ASCII 字节在 C 语言环境里
            // 不是可打印字符，两者都落三位八进制。
            c => {
                for byte in c.encode_utf8(&mut [0u8; 4]).as_bytes() {
                    if *byte == b' ' || !byte.is_ascii_graphic() {
                        out.push_str(&format!("\\{:03o}", *byte));
                    } else {
                        out.push(*byte as char);
                    }
                }
            }
        }
    }
    out
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

#[cfg(test)]
mod face_tests {
    use super::*;

    fn sample() -> StatData {
        StatData {
            dev: 2,
            mode: 0o100644,
            ino: 42,
            nlink: 1,
            uid: 0,
            gid: 0,
            rdev: 0,
            atime: TimeField::new(1_000_000_000, 123_456_789),
            mtime: TimeField::new(1_000_000_001, 123_456_789),
            ctime: TimeField::new(1_000_000_002, 0),
            btime: TimeField::new(1_000_000_003, 999_000_000),
            size: 4096,
            blocks: 8,
            blksize: 4096,
            flags: 0x21,
            generation: 7,
        }
    }

    fn face(
        fmt: &str,
        data: &StatData,
        name: Option<&str>,
        link: Option<&str>,
        timefmt: &str,
    ) -> String {
        let pieces = parse_format(fmt).unwrap_or_else(|e| panic!("{fmt} 应可解析: {}", e.span));
        let ctx = Render {
            data,
            name,
            link,
            timefmt,
            file_number: 3,
        };
        let mut out = String::new();
        render(&pieces, &ctx, &mut out).unwrap_or_else(|e| panic!("{fmt} 应可渲染: {e:?}"));
        out
    }

    #[test]
    fn the_default_template_lays_out_every_field_in_order() {
        let mut d = sample();
        d.generation = 7;
        assert_eq!(
            face(DEF_FORMAT, &d, Some("/etc/rc"), None, TIME_FORMAT),
            "2 42 -rw-r--r-- 1 (0) (0) 0 4096 \"Sep  9 01:46:40 2001\" \
             \"Sep  9 01:46:41 2001\" \"Sep  9 01:46:42 2001\" \"Sep  9 01:46:43 2001\" \
             4096 8 0x21 /etc/rc"
        );
    }

    #[test]
    fn the_raw_template_leaves_the_quotes_off_and_uses_numbers() {
        assert_eq!(
            face(RAW_FORMAT, &sample(), Some("/etc/rc"), None, TIME_FORMAT),
            "2 42 0100644 1 0 0 0 4096 1000000000 1000000001 1000000002 1000000003 4096 8 33 /etc/rc"
        );
    }

    #[test]
    fn the_ls_like_templates_mark_type_and_follow_the_link() {
        let mut d = sample();
        d.mode = crate::cp::S_IFLNK | 0o777;
        // `%SY` 显式字符串形才带 ` -> `（stat.c:855-856 的指针先于默认形态算好）。
        assert_eq!(
            face(
                LS_FORMAT,
                &d,
                Some("/tmp/link"),
                Some("/etc/rc"),
                TIME_FORMAT
            ),
            "lrwxrwxrwx 1 (0) (0) 4096 Sep  9 01:46:41 2001 /tmp/link -> /etc/rc"
        );
        assert_eq!(
            face(
                LSF_FORMAT,
                &d,
                Some("/tmp/link"),
                Some("/etc/rc"),
                TIME_FORMAT
            ),
            "lrwxrwxrwx 1 (0) (0) 4096 Sep  9 01:46:41 2001 /tmp/link@ -> /etc/rc"
        );
    }

    #[test]
    fn the_shell_and_linux_templates_print_assignments_and_blocks() {
        assert!(
            face(SHELL_FORMAT, &sample(), Some("f"), None, "%s").starts_with(
                "st_dev=2 st_ino=42 st_mode=0100644 st_nlink=1 st_uid=0 st_gid=0 st_rdev=0 \
             st_size=4096 st_atime=1000000000 st_mtime=1000000001 st_ctime=1000000002 \
             st_birthtime=1000000003 st_blksize=4096 st_blocks=8 st_flags=33"
            )
        );
        let linux = face(LINUX_FORMAT, &sample(), Some("f"), None, "%c");
        assert!(
            linux.starts_with("  File: \"f\"\n  Size: 4096"),
            "实际：{linux:?}"
        );
        assert!(linux.contains("FileType: Regular File"), "实际：{linux:?}");
        assert!(linux.contains("Mode: (0644/-rw-r--r--)"), "实际：{linux:?}");
        assert!(linux.contains("Device: 0,2   Inode: 42"), "实际：{linux:?}");
    }

    #[test]
    fn mode_string_form_drops_the_padding_and_the_octal_default_keeps_the_type_bits() {
        assert_eq!(
            face("%Sp", &sample(), None, None, TIME_FORMAT),
            "-rw-r--r--"
        );
        assert_eq!(face("%p", &sample(), None, None, TIME_FORMAT), "100644");
        assert_eq!(face("%#p", &sample(), None, None, TIME_FORMAT), "0100644");
        assert_eq!(face("%Dp", &sample(), None, None, TIME_FORMAT), "33188");
    }

    #[test]
    fn the_three_subfield_letters_slice_the_permission_groups() {
        let mut d = sample();
        d.mode = 0o100754;
        // 字符串形才切权限组（`stat.c:650-666` 的 `stmp += …; stmp[3] = 0`）。
        assert_eq!(face("%SHp", &d, None, None, TIME_FORMAT), "rwx");
        assert_eq!(face("%SMp", &d, None, None, TIME_FORMAT), "r-x");
        assert_eq!(face("%SLp", &d, None, None, TIME_FORMAT), "r--");
        // 裸片段仍是数值形：高位折成类型位（`stat.c:652`），低位取那九位（`:664`），
        // 中位取 `(data >> 9) & 07`（`:658`）。
        assert_eq!(face("%Hp", &d, None, None, TIME_FORMAT), "10");
        assert_eq!(face("%Lp", &d, None, None, TIME_FORMAT), "754");
        // 中位是 C 自己的一处不一致：数值那一支折的是粘滞/设组/设身三位，字符串那一支
        // 切的是组权限（`stmp += 4`，`:659`）。0o100754 的特殊三位全空，所以数值形
        // 打 0 而字符串形打 `r-x`。照抄不「修正」——外部行为优先。
        assert_eq!(face("%Mp", &d, None, None, TIME_FORMAT), "0");
        assert_eq!(face("%SMp", &d, None, None, TIME_FORMAT), "r-x");
    }

    #[test]
    fn hex_with_pound_and_zero_padding_put_the_zeros_after_the_prefix() {
        // 宽度算的是整个字段（含 `0x`），所以零填两位而不是四位。
        assert_eq!(
            face("%#08Xz", &sample(), None, None, TIME_FORMAT),
            "0x001000"
        );
        assert_eq!(face("%8Xz", &sample(), None, None, TIME_FORMAT), "    1000");
        assert_eq!(
            face("%-8z|", &sample(), None, None, TIME_FORMAT),
            "4096    |"
        );
    }

    #[test]
    fn sign_flags_only_ride_the_decimal_form() {
        // 默认是无符号形，`+` 与空格旗标在它身上不落发（C 的 `%+llu` 同理）。
        assert_eq!(face("%+z", &sample(), None, None, TIME_FORMAT), "4096");
        assert_eq!(face("%+Dz", &sample(), None, None, TIME_FORMAT), "+4096");
        assert_eq!(face("% Dz", &sample(), None, None, TIME_FORMAT), " 4096");
        assert_eq!(face("%+Xz", &sample(), None, None, TIME_FORMAT), "1000");
        let mut neg = sample();
        neg.size = -1;
        assert_eq!(face("%Dz", &neg, None, None, TIME_FORMAT), "-1");
        // 默认是无符号形，负大小就打成回绕后的巨数（同 C 的 `%llu`）。
        assert_eq!(
            face("%z", &neg, None, None, TIME_FORMAT),
            "18446744073709551615"
        );
    }

    #[test]
    fn precision_pads_the_digits_while_width_pads_the_field() {
        assert_eq!(face("%.8i", &sample(), None, None, TIME_FORMAT), "00000042");
        assert_eq!(face("%6i", &sample(), None, None, TIME_FORMAT), "    42");
        assert_eq!(
            face("%6.8i", &sample(), None, None, TIME_FORMAT),
            "00000042"
        );
        // 字符串形的精度是截断。
        assert_eq!(
            face("%.4SN", &sample(), Some("/etc/rc"), None, TIME_FORMAT),
            "/etc"
        );
    }

    #[test]
    fn size_subfields_divide_by_powers_of_two_and_round() {
        assert_eq!(face("%Lz", &sample(), None, None, TIME_FORMAT), "4");
        assert_eq!(face("%Mz", &sample(), None, None, TIME_FORMAT), "0");
        assert_eq!(face("%Hz", &sample(), None, None, TIME_FORMAT), "0");
        let mut big = sample();
        big.size = 4 * 1024 * 1024;
        assert_eq!(face("%Mz", &big, None, None, TIME_FORMAT), "4");
        assert_eq!(face("%Hz", &big, None, None, TIME_FORMAT), "0");
    }

    #[test]
    fn times_default_to_seconds_and_the_string_form_uses_the_time_template() {
        assert_eq!(face("%m", &sample(), None, None, TIME_FORMAT), "1000000001");
        assert_eq!(
            face("%Sm", &sample(), None, None, TIME_FORMAT),
            "Sep  9 01:46:41 2001"
        );
        // 时间模板换成 `%s` 之后，字符串形就退化成整秒。
        assert_eq!(face("%Sm", &sample(), None, None, "%s"), "1000000001");
    }

    #[test]
    fn the_float_form_lays_the_nanoseconds_behind_the_decimal_point() {
        assert_eq!(
            face("%Fm", &sample(), None, None, TIME_FORMAT),
            "1000000001.123456789"
        );
        assert_eq!(
            face("%.2Fm", &sample(), None, None, TIME_FORMAT),
            "1000000001.12"
        );
        assert_eq!(
            face("%.0Fm", &sample(), None, None, TIME_FORMAT),
            "1000000001"
        );
        // 请求超过九位时，多余的部分补零（stat.c:1088-1093）。
        assert_eq!(
            face("%.12Fc", &sample(), None, None, TIME_FORMAT),
            "1000000002.000000000000"
        );
        assert_eq!(
            face("%.6FB", &sample(), None, None, TIME_FORMAT),
            "1000000003.999000"
        );
    }

    #[test]
    fn the_link_arrow_appears_only_for_the_explicit_string_form() {
        let mut d = sample();
        d.mode = crate::cp::S_IFLNK | 0o777;
        assert_eq!(
            face("%Y", &d, Some("l"), Some("/tmp/x"), TIME_FORMAT),
            "/tmp/x"
        );
        assert_eq!(
            face("%SY", &d, Some("l"), Some("/tmp/x"), TIME_FORMAT),
            " -> /tmp/x"
        );
        // 非符链上 `%Y` 打空串，并把 linkfail 置起（stat.c:858-861）。
        let pieces = parse_format("%Y").unwrap();
        let ctx = Render {
            data: &sample(),
            name: Some("f"),
            link: None,
            timefmt: TIME_FORMAT,
            file_number: 1,
        };
        let mut out = String::new();
        let report = render(&pieces, &ctx, &mut out).unwrap();
        assert_eq!(out, "");
        assert!(report.linkfail);
    }

    #[test]
    fn the_readlink_personality_reports_a_missing_target_through_the_report() {
        let mut d = sample();
        d.mode = crate::cp::S_IFLNK | 0o777;
        let pieces = parse_format("%Y").unwrap();
        let ctx = Render {
            data: &d,
            name: Some("l"),
            link: None,
            timefmt: TIME_FORMAT,
            file_number: 1,
        };
        let mut out = String::new();
        assert!(render(&pieces, &ctx, &mut out).unwrap().linkfail);
    }

    #[test]
    fn filename_subfields_split_the_directory_and_the_basename() {
        assert_eq!(
            face("%N", &sample(), Some("/etc/rc"), None, TIME_FORMAT),
            "/etc/rc"
        );
        assert_eq!(
            face("%HN", &sample(), Some("/etc/rc"), None, TIME_FORMAT),
            "/etc"
        );
        assert_eq!(
            face("%LN", &sample(), Some("/etc/rc"), None, TIME_FORMAT),
            "rc"
        );
        // 尾部斜杠先剥掉再切（stat.c:929-937）。
        assert_eq!(
            face("%HN", &sample(), Some("/etc/rc//"), None, TIME_FORMAT),
            "/etc"
        );
        // 没有斜杠的路径，高位片段打成 `.`（stat.c:939-941）。
        assert_eq!(face("%HN", &sample(), Some("rc"), None, TIME_FORMAT), ".");
        assert_eq!(face("%N", &sample(), None, None, TIME_FORMAT), "(stdin)");
    }

    #[test]
    fn filetype_names_itself_in_the_high_form_and_marks_it_in_the_low_form() {
        assert_eq!(
            face("%HT", &sample(), None, None, TIME_FORMAT),
            "Regular File"
        );
        assert_eq!(face("%LT", &sample(), None, None, TIME_FORMAT), "-");
        let mut exec = sample();
        exec.mode = 0o100755;
        assert_eq!(face("%T", &exec, None, None, TIME_FORMAT), "*");
        let mut dir = sample();
        dir.mode = crate::cp::S_IFDIR | 0o755;
        assert_eq!(face("%HT", &dir, None, None, TIME_FORMAT), "Directory");
        assert_eq!(face("%LT", &dir, None, None, TIME_FORMAT), "/");
    }

    #[test]
    fn device_datums_split_major_and_minor_and_sizerdev_folds_to_the_pair() {
        let mut dev = sample();
        dev.mode = crate::cp::S_IFCHR | 0o666;
        // `makedev(3, 1)` 按同一文件 :1264 的编码是 0x301。
        dev.rdev = 0x301;
        assert_eq!(face("%Hr", &dev, None, None, TIME_FORMAT), "3");
        assert_eq!(face("%Lr", &dev, None, None, TIME_FORMAT), "1");
        assert_eq!(face("%Z", &dev, None, None, TIME_FORMAT), "3,1");
        // 非设备走 size 一支（stat.c:979-988）。
        assert_eq!(face("%Z", &sample(), None, None, TIME_FORMAT), "4096");
    }

    #[test]
    fn the_devname_branch_prints_the_marks_c_prints_when_it_finds_nothing() {
        assert_eq!(face("%Sd", &sample(), None, None, TIME_FORMAT), "???");
    }

    #[test]
    fn ids_print_parenthesised_without_a_password_database_behind_them() {
        assert_eq!(face("%Su", &sample(), None, None, TIME_FORMAT), "(0)");
        assert_eq!(face("%u", &sample(), None, None, TIME_FORMAT), "0");
        let mut d = sample();
        d.gid = 1000;
        assert_eq!(face("%Sg", &d, None, None, TIME_FORMAT), "(1000)");
        assert_eq!(face("%g", &d, None, None, TIME_FORMAT), "1000");
    }

    #[test]
    fn the_pound_flag_on_a_string_vis_encodes_control_and_space() {
        assert_eq!(
            face("%#SN", &sample(), Some("a\tb c"), None, TIME_FORMAT),
            "a\\tb\\040c"
        );
        // 反斜杠自己也要被_escape_，非 ASCII 字节走三位八进制。
        assert_eq!(
            face("%#SN", &sample(), Some("a\\b"), None, TIME_FORMAT),
            "a\\\\b"
        );
        assert_eq!(
            face("%#SN", &sample(), Some("é"), None, TIME_FORMAT),
            "\\303\\251"
        );
    }

    #[test]
    fn unsupported_subfield_or_form_is_the_same_bad_format_c_reports() {
        let pieces = parse_format("%Mf").unwrap();
        let ctx = Render {
            data: &sample(),
            name: Some("f"),
            link: None,
            timefmt: TIME_FORMAT,
            file_number: 1,
        };
        let mut out = String::new();
        assert_eq!(
            render(&pieces, &ctx, &mut out).unwrap_err(),
            FaceError::BadFormat("%Mf".to_string())
        );
        // 节点号没有字符串形（stat.c:639-641 的允许集里没有 FMTF_STRING）。
        let pieces = parse_format("%Si").unwrap();
        assert_eq!(
            render(&pieces, &ctx, &mut String::new()).unwrap_err(),
            FaceError::BadFormat("%Si".to_string())
        );
    }

    #[test]
    fn the_realpath_letter_is_refused_instead_of_rendering_empty() {
        let pieces = parse_format("%R").unwrap();
        let ctx = Render {
            data: &sample(),
            name: Some("f"),
            link: None,
            timefmt: TIME_FORMAT,
            file_number: 1,
        };
        assert_eq!(
            render(&pieces, &ctx, &mut String::new()).unwrap_err(),
            FaceError::RealpathUnported
        );
    }

    #[test]
    fn literals_and_simple_forms_reach_the_output_in_order() {
        let pieces = parse_format("%@ of %@: %z%n").unwrap();
        let ctx = Render {
            data: &sample(),
            name: None,
            link: None,
            timefmt: TIME_FORMAT,
            file_number: 2,
        };
        let mut out = String::new();
        let report = render(&pieces, &ctx, &mut out).unwrap();
        assert_eq!(out, "2 of 2: 4096\n");
        assert!(report.ends_with_newline);
        assert!(!report.linkfail);
    }
}
