//! unifdef — selectively remove C preprocessor conditionals
//! (minix3/usr.bin/unifdef/unifdef.c).
//!
//! Deciding half: the ten-state #if machine (trans_table indexed by
//! ifstate × linetype), the C comment scanner with backslash-newline
//! continuations, the four-level precedence evaluator for `#if`
//! expressions (`defined()`, `-D` values, hex and octal literals),
//! and the keyword rewrites that keep nesting balanced when a
//! known-false group meets an unknown `#elif`. The DODGY variants
//! (directives with comments spanning the newline) are not modeled;
//! such lines parse as their plain counterparts (registered corner).
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

pub const MAXDEPTH: usize = 64;

/// A command-line symbol: `-Dsym[=val]` (value None for `-U`; an
/// empty value means `-Dsym` without `=val`).
#[derive(Debug, Clone, PartialEq)]
pub struct Sym {
    pub name: String,
    pub value: Option<String>,
    pub ignore: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub symbols: Vec<Sym>,
    /// `-c`: complement — `-D` acts as `-U` and vice versa.
    pub complement: bool,
    /// `-l`: blank deleted lines instead of omitting them.
    pub lnblank: bool,
    /// `-s`: output the controlling symbol list instead of text.
    pub symlist: bool,
    /// `-t`: input is text; don't parse C comments.
    pub text: bool,
    /// `-k`: process constant `#if` expressions.
    pub killconsts: bool,
    /// `-o`: write the result to this path instead of standard output
    /// (`unifdef.c:311-312` 的 `ofilename = optarg`).
    pub output: Option<String>,
}

/// `addsym`（`unifdef.c:965-991`）的两种失败：形不合（C 直接 `usage()`）与符号表满
/// （`unifdef.c:971-972` 的 `errx(2, "too many symbols")`）。
#[derive(Debug, PartialEq)]
pub enum SymError {
    Malformed,
    TooMany,
}

/// C 的 `skipsym`（`unifdef.c:947-952`）：吃一串 `[A-Za-z0-9_]`，**空串合法**（它一个字符
/// 也不吃就返回原点，于是 `addsym` 看到的是 `*val == '\0'`）。
fn symbol_run(spec: &str) -> usize {
    let mut end = 0usize;
    while end < spec.len()
        && matches!(spec.as_bytes()[end], b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_')
    {
        end += 1;
    }
    end
}

/// Parses `-Dsym[=val]` / `-Usym` (and the `-iD/-iU` ignore variants).
///
/// 判据逐条对着 `addsym` 走，不自创句子：`-D` 那一支允许 `sym`、`sym=`、`sym=val`，
/// 名字之后既不是结尾也不是 `=` 就交回 [`SymError::Malformed`]（C 那边是一句 `usage()`）；
/// `-U` 那一支名字之后必须是结尾，带 `=` 就坏（参照件现量：`unifdef -UFOO=1` 只打 usage 一行）。
/// 名字重复时 C 用 `findsym` 找到同一格再覆写 `value`/`ignore`，这里同形。
pub fn add_symbol(
    options: &mut Options,
    spec: &str,
    defined: bool,
    ignore: bool,
) -> Result<(), SymError> {
    let cut = symbol_run(spec);
    let (name, rest) = spec.split_at(cut);
    let value = if defined {
        if rest.is_empty() {
            Some(String::new())
        } else if let Some(tail) = rest.strip_prefix('=') {
            Some(tail.to_string())
        } else {
            return Err(SymError::Malformed);
        }
    } else if rest.is_empty() {
        None
    } else {
        return Err(SymError::Malformed);
    };
    for sym in options.symbols.iter_mut() {
        if sym.name == name {
            sym.value = if defined { value } else { None };
            sym.ignore = ignore;
            return Ok(());
        }
    }
    // `MAXSYMS`（`unifdef.c:197`）是 C 那张静态表的容量，溢出那句 `too many symbols`
    // 走的是 `errx(2, …)` 而不是 usage。
    if options.symbols.len() >= 4096 {
        return Err(SymError::TooMany);
    }
    options.symbols.push(Sym {
        name: name.to_string(),
        value,
        ignore,
    });
    Ok(())
}

/// 前门环的出口（`unifdef.c:273` 的 `getopt(argc, argv, "i:D:U:I:o:cdeklst")`）。
#[derive(Debug, PartialEq)]
pub enum UnifdefArgError {
    /// 集外字母或缺值字母：libc 先印一行，`unifdef.c:322-323` 的 `default: usage()` 接第二行。
    Getopt(crate::GetoptReport),
    /// `-i` 后面不是 `D`/`U`（`unifdef.c:283-287`），或符号名不合 `addsym` 的形——
    /// 两处 C 都只 `usage()`，**不打任何自拟句子**。
    Usage,
    /// 符号表满：`errx(2, "too many symbols")` 单独一行。
    TooManySymbols,
}

/// 选项环的产物：旗标与**按原序**交回的操数（C 只接受 0 或 1 个，多的在壳里判）。
pub struct UnifdefArgs<'a> {
    pub options: Options,
    pub operands: Vec<&'a str>,
}

/// `unifdef.c:273-323` 的选项环。
///
/// 五个带值项（`D`/`U`/`i`/`I`/`o`）与七个不带值的字母（`c`/`d`/`e`/`k`/`l`/`s`/`t`），
/// 大小写混排——`D` 与 `d`、`U` 与 `u`（后者不在串里）是两回事，集外的判定不能按字母族猜。
/// `-i` 的取值先吃掉紧随其后的**一个字符**再看剩下那段（`opt = *optarg++`，`unifdef.c:278-287`）：
/// `-iDFOO` 里的符号是 `FOO`，`-iD` 的符号是空串（合法），`-iX` 与 `-i` 配空串都直接 usage。
/// `-I` 收下值却什么都不做（`unifdef.c:289-291` 的 cpp 兼容位）。
///
/// 环**不重排**：第一个非选项 token 一到，`getopt.c:74-78` 当场结束环且不推进 `optind`，
/// 于是 `unifdef -DFOO in.c -c` 里的 `-c` 是第二个操数（参照件现量：`unifdef: can only do one file`），
/// 而不是又点起一面旗标。裸 `-` 留在操数里，且 `unifdef.c:333` 的 `strcmp(*argv, "-") != 0`
/// 把它读成标准输入。
pub fn parse_unifdef_args<'a>(args: &'a [&'a str]) -> Result<UnifdefArgs<'a>, UnifdefArgError> {
    let mut options = Options::default();
    let mut index = 0usize;
    let operands = loop {
        let Some(&arg) = args.get(index) else {
            break Vec::new();
        };
        let bytes = arg.as_bytes();
        if arg == "--" {
            // `getopt.c:81-85`：`++optind` 之后才返回 -1，所以 `--` 自身不是操数。
            break args[index + 1..].to_vec();
        }
        if bytes.len() < 2 || bytes[0] != b'-' {
            // `getopt.c:74-78`：非选项 token 一到就结束，不推进 `optind`。
            break args[index..].to_vec();
        }
        let mut pos = 1usize;
        while pos < bytes.len() {
            match bytes[pos] {
                b'c' => options.complement = true,
                b'k' => options.killconsts = true,
                b'l' => options.lnblank = true,
                b's' => options.symlist = true,
                b't' => options.text = true,
                // `-d`（调试报告）与 `-e`（宽容畸形行）在本壳是无操作的合法字母：
                // 它们门控的是 DODGY 表那几列，已另登记。
                b'd' | b'e' => {}
                ch @ (b'D' | b'U' | b'i' | b'I' | b'o') => {
                    // 值是「本串去掉 `-` 与字母之后的那一段」，没有剩余就取下一个 token
                    // （`getopt.c:118-141`）；两种取法都把本串吃到尽头。
                    let value = if pos + 1 < bytes.len() {
                        &arg[pos + 1..]
                    } else {
                        match args.get(index + 1) {
                            Some(next) => {
                                index += 1;
                                next
                            }
                            None => {
                                return Err(UnifdefArgError::Getopt(
                                    crate::GetoptReport::MissingValue(ch as char),
                                ));
                            }
                        }
                    };
                    match ch {
                        b'D' => {
                            add_symbol(&mut options, value, true, false).map_err(usage_or_full)?
                        }
                        b'U' => {
                            add_symbol(&mut options, value, false, false).map_err(usage_or_full)?
                        }
                        b'I' => {}
                        b'o' => options.output = Some(value.to_string()),
                        _ => {
                            // `-i` 的第一个字符选边，剩下那段才是符号名。
                            let mut rest = value.bytes();
                            match rest.next() {
                                Some(b'D') => add_symbol(&mut options, &value[1..], true, true)
                                    .map_err(usage_or_full)?,
                                Some(b'U') => add_symbol(&mut options, &value[1..], false, true)
                                    .map_err(usage_or_full)?,
                                _ => return Err(UnifdefArgError::Usage),
                            }
                        }
                    }
                    pos = bytes.len();
                }
                _ => {
                    // 报的是**这一处的第一个字节**：C 的 `optopt` 是 `char`，`%c` 打的就是
                    // 那一个字节；游标 `pos` 是字节下标，非 ASCII 字母时它会落进多字节字符
                    // 中间，所以按字节取而不切字符串（切串当场 panic）。
                    return Err(UnifdefArgError::Getopt(crate::GetoptReport::UnknownOption(
                        char::from(bytes[pos]),
                    )));
                }
            }
            pos += 1;
        }
        index += 1;
    };
    Ok(UnifdefArgs { options, operands })
}

/// `addsym` 的两种失败在 C 里落在两个不同的出口上：形不合是 `usage()`，表满是 `errx`。
fn usage_or_full(err: SymError) -> UnifdefArgError {
    match err {
        SymError::Malformed => UnifdefArgError::Usage,
        SymError::TooMany => UnifdefArgError::TooManySymbols,
    }
}

/// Errors mirroring unifdef.c's `error()` exits (status 2).
#[derive(Debug, PartialEq)]
pub enum UnifdefError {
    EofInComment,
    InappropriateElif {
        line: usize,
        if_line: usize,
        depth: usize,
    },
    InappropriateElse {
        line: usize,
    },
    InappropriateEndif {
        line: usize,
    },
    PrematureEof,
    ObfuscatedControlLine {
        line: usize,
    },
    TooDeep {
        line: usize,
    },
}

/// Success: filtered text plus the exit status (0 = unchanged input,
/// 1 = some lines dropped).
#[derive(Debug, Default, PartialEq)]
pub struct Outcome {
    pub output: Vec<u8>,
    pub exitstat: i32,
    /// `-s` mode: the controlling symbols in order of appearance.
    pub symbol_list: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LT {
    TrueI,
    FalseI,
    If,
    True,
    False,
    Elif,
    ElTrue,
    ElFalse,
    Else,
    EndIf,
    Plain,
    Eof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IfState {
    Outside,
    FalsePrefix,
    TruePrefix,
    PassMiddle,
    FalseMiddle,
    TrueMiddle,
    PassElse,
    FalseElse,
    TrueElse,
    FalseTrailer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommentState {
    No,
    C,
    Cxx,
    Starting,
    Finishing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineState {
    Start,
    Hash,
    Dirty,
}

pub struct Unifdef {
    options: Options,
    output: Vec<u8>,
    symbol_list: Vec<String>,
    exitstat: i32,
    keepthis: bool,
    incomment: CommentState,
    linestate: LineState,
    linenum: usize,
    depth: usize,
    ifstate: [IfState; MAXDEPTH],
    ignoring: [bool; MAXDEPTH],
    stifline: [usize; MAXDEPTH],
    /// Byte offset of the directive keyword within the current line
    /// (the C's `keyword` pointer).
    keyword_at: usize,
}

impl Unifdef {
    pub fn new(options: Options) -> Unifdef {
        Unifdef {
            options,
            output: Vec::new(),
            symbol_list: Vec::new(),
            exitstat: 0,
            keepthis: false,
            incomment: CommentState::No,
            linestate: LineState::Start,
            linenum: 0,
            depth: 0,
            ifstate: [IfState::Outside; MAXDEPTH],
            ignoring: [false; MAXDEPTH],
            stifline: [0; MAXDEPTH],
            keyword_at: 0,
        }
    }

    /// Runs the machine over the whole input; EOF transitions are
    /// applied like the C's `done()`/`Eeof`.
    pub fn run(mut self, input: &str) -> Result<Outcome, UnifdefError> {
        for line in split_lines(input) {
            self.linenum += 1;
            let lineval = self.get_line(line)?;
            self.transition(lineval, line)?;
        }
        // EOF transition.
        if self.ifstate[self.depth] != IfState::Outside {
            return Err(UnifdefError::PrematureEof);
        }
        if self.incomment != CommentState::No {
            return Err(UnifdefError::EofInComment);
        }
        Ok(Outcome {
            output: self.output,
            exitstat: self.exitstat,
            symbol_list: self.symbol_list,
        })
    }

    // ---- state machine utilities (unifdef.c:509-568) ----

    fn nest(&mut self) -> Result<(), UnifdefError> {
        self.depth += 1;
        if self.depth >= MAXDEPTH {
            return Err(UnifdefError::TooDeep { line: self.linenum });
        }
        self.stifline[self.depth] = self.linenum;
        Ok(())
    }

    fn state(&mut self, is: IfState) {
        self.ifstate[self.depth] = is;
    }

    fn ignoreoff(&mut self) {
        self.ignoring[self.depth] = self.ignoring[self.depth - 1];
    }

    fn ignoreon(&mut self) {
        self.ignoring[self.depth] = true;
    }

    /// flushline: write the line, blank it, or drop it.
    fn flushline(&mut self, keep: bool, line: &str) {
        if self.options.symlist {
            return;
        }
        if keep ^ self.options.complement {
            self.output.extend_from_slice(line.as_bytes());
        } else {
            if self.options.lnblank {
                self.output.push(b'\n');
            }
            self.exitstat = 1;
        }
    }

    fn print(&mut self, line: &str) {
        self.flushline(true, line);
    }

    fn drop(&mut self, line: &str) {
        self.flushline(false, line);
    }

    /// keywordedit: replace from the directive keyword onward, then
    /// print (the C's keywordedit ends in print()).
    fn keywordedit(&mut self, replacement: &str, line: &str) {
        let end = self.keyword_at.min(line.len());
        let head: String = line[..end].to_string();
        let rewritten = format!("{}{}", head, replacement);
        self.print(&rewritten);
    }

    /// Mpass rewrites `#elif` into `#if  ` keeping the condition.
    fn mpass_rewrite(line: &str, keyword_at: usize) -> String {
        let mut chars: Vec<char> = line.chars().collect();
        let repl: [char; 4] = ['i', 'f', ' ', ' '];
        for (i, r) in repl.iter().enumerate() {
            if keyword_at + i < chars.len() {
                chars[keyword_at + i] = *r;
            }
        }
        chars.into_iter().collect()
    }

    // ---- transitions (unifdef.c:412-440) ----

    fn transition(&mut self, lineval: LT, line: &str) -> Result<(), UnifdefError> {
        use IfState::*;
        match self.ifstate[self.depth] {
            Outside => match lineval {
                LT::TrueI => self.itrue(line)?,
                LT::FalseI => self.ifalse(line)?,
                LT::If => self.fpass(line)?,
                LT::True => self.ftrue(line)?,
                LT::False => self.ffalse(line)?,
                LT::Elif | LT::ElTrue | LT::ElFalse => {
                    let (l, d) = (self.linenum, 0);
                    return Err(UnifdefError::InappropriateElif {
                        line: l,
                        if_line: d,
                        depth: 0,
                    });
                }
                LT::Else => return Err(UnifdefError::InappropriateElse { line: self.linenum }),
                LT::EndIf => return Err(UnifdefError::InappropriateEndif { line: self.linenum }),
                LT::Plain => self.print(line),
                LT::Eof => {}
            },
            FalsePrefix => match lineval {
                LT::TrueI | LT::FalseI => self.idrop(line)?,
                LT::If | LT::True | LT::False => self.fdrop(line)?,
                LT::Elif => {
                    // Mpass: rewrite #elif into #if and pass the block.
                    let rewritten = Self::mpass_rewrite(line, self.keyword_at);
                    self.print(&rewritten);
                    self.ignoreoff();
                    self.state(PassMiddle);
                }
                LT::ElTrue => self.strue(line),
                LT::ElFalse => self.sfalse(line),
                LT::Else => self.selse(line),
                LT::EndIf => self.dendif(line),
                LT::Plain => self.drop(line),
                LT::Eof => return Err(UnifdefError::PrematureEof),
            },
            TruePrefix => match lineval {
                LT::TrueI => self.itrue(line)?,
                LT::FalseI => self.ifalse(line)?,
                LT::If => self.fpass(line)?,
                LT::True => self.ftrue(line)?,
                LT::False => self.ffalse(line)?,
                LT::Elif | LT::ElTrue | LT::ElFalse => self.dfalse(line),
                LT::Else => self.delse(line),
                LT::EndIf => self.dendif(line),
                LT::Plain => self.print(line),
                LT::Eof => return Err(UnifdefError::PrematureEof),
            },
            PassMiddle => match lineval {
                LT::TrueI => self.itrue(line)?,
                LT::FalseI => self.ifalse(line)?,
                LT::If => self.fpass(line)?,
                LT::True => self.ftrue(line)?,
                LT::False => self.ffalse(line)?,
                LT::Elif => {
                    // Pelif
                    self.print(line);
                    self.ignoreoff();
                    self.state(PassMiddle);
                }
                LT::ElTrue => {
                    // Mtrue
                    self.keywordedit("else\n", line);
                    self.state(TrueMiddle);
                }
                LT::ElFalse => {
                    // Delif
                    self.drop(line);
                    self.ignoreoff();
                    self.state(FalseMiddle);
                }
                LT::Else => {
                    // Pelse
                    self.print(line);
                    self.state(PassElse);
                }
                LT::EndIf => {
                    // Pendif
                    self.print(line);
                    self.depth -= 1;
                }
                LT::Plain => self.print(line),
                LT::Eof => return Err(UnifdefError::PrematureEof),
            },
            FalseMiddle => match lineval {
                LT::TrueI | LT::FalseI => self.idrop(line)?,
                LT::If | LT::True | LT::False => self.fdrop(line)?,
                LT::Elif => {
                    // Pelif
                    self.print(line);
                    self.ignoreoff();
                    self.state(PassMiddle);
                }
                LT::ElTrue => {
                    self.keywordedit("else\n", line);
                    self.state(TrueMiddle);
                }
                LT::ElFalse => {
                    self.drop(line);
                    self.ignoreoff();
                    self.state(FalseMiddle);
                }
                LT::Else => {
                    self.print(line);
                    self.state(PassElse);
                }
                LT::EndIf => {
                    self.print(line);
                    self.depth -= 1;
                }
                LT::Plain => self.drop(line),
                LT::Eof => return Err(UnifdefError::PrematureEof),
            },
            TrueMiddle => match lineval {
                LT::TrueI => self.itrue(line)?,
                LT::FalseI => self.ifalse(line)?,
                LT::If => self.fpass(line)?,
                LT::True => self.ftrue(line)?,
                LT::False => self.ffalse(line)?,
                LT::Elif | LT::ElTrue | LT::ElFalse => {
                    // Melif
                    self.keywordedit("endif\n", line);
                    self.state(FalseTrailer);
                }
                LT::Else => {
                    self.keywordedit("endif\n", line);
                    self.state(FalseElse);
                }
                LT::EndIf => {
                    self.print(line);
                    self.depth -= 1;
                }
                LT::Plain => self.print(line),
                LT::Eof => return Err(UnifdefError::PrematureEof),
            },
            PassElse => match lineval {
                LT::TrueI => self.itrue(line)?,
                LT::FalseI => self.ifalse(line)?,
                LT::If => self.fpass(line)?,
                LT::True => self.ftrue(line)?,
                LT::False => self.ffalse(line)?,
                LT::Elif | LT::ElTrue | LT::ElFalse => {
                    let (l, ifl, d) = (self.linenum, self.stifline[self.depth], self.depth);
                    return Err(UnifdefError::InappropriateElif {
                        line: l,
                        if_line: ifl,
                        depth: d,
                    });
                }
                LT::Else => return Err(UnifdefError::InappropriateElse { line: self.linenum }),
                LT::EndIf => {
                    self.print(line);
                    self.depth -= 1;
                }
                LT::Plain => self.print(line),
                LT::Eof => return Err(UnifdefError::PrematureEof),
            },
            FalseElse => match lineval {
                LT::TrueI | LT::FalseI => self.idrop(line)?,
                LT::If | LT::True | LT::False => self.fdrop(line)?,
                LT::Elif | LT::ElTrue | LT::ElFalse => {
                    let (l, ifl, d) = (self.linenum, self.stifline[self.depth], self.depth);
                    return Err(UnifdefError::InappropriateElif {
                        line: l,
                        if_line: ifl,
                        depth: d,
                    });
                }
                LT::Else => return Err(UnifdefError::InappropriateElse { line: self.linenum }),
                LT::EndIf => self.dendif(line),
                LT::Plain => self.drop(line),
                LT::Eof => return Err(UnifdefError::PrematureEof),
            },
            TrueElse => match lineval {
                LT::TrueI => self.itrue(line)?,
                LT::FalseI => self.ifalse(line)?,
                LT::If => self.fpass(line)?,
                LT::True => self.ftrue(line)?,
                LT::False => self.ffalse(line)?,
                LT::Elif | LT::ElTrue | LT::ElFalse => {
                    let (l, ifl, d) = (self.linenum, self.stifline[self.depth], self.depth);
                    return Err(UnifdefError::InappropriateElif {
                        line: l,
                        if_line: ifl,
                        depth: d,
                    });
                }
                LT::Else => return Err(UnifdefError::InappropriateElse { line: self.linenum }),
                LT::EndIf => self.dendif(line),
                LT::Plain => self.print(line),
                LT::Eof => return Err(UnifdefError::PrematureEof),
            },
            FalseTrailer => match lineval {
                LT::TrueI | LT::FalseI => self.idrop(line)?,
                LT::If | LT::True | LT::False => self.fdrop(line)?,
                LT::Elif | LT::ElTrue | LT::ElFalse => self.dfalse(line),
                LT::Else => self.delse(line),
                LT::EndIf => self.dendif(line),
                LT::Plain => self.drop(line),
                LT::Eof => return Err(UnifdefError::PrematureEof),
            },
        }
        Ok(())
    }

    // Transition helpers.

    fn itrue(&mut self, line: &str) -> Result<(), UnifdefError> {
        self.ftrue(line)?;
        self.ignoreon();
        Ok(())
    }

    fn ifalse(&mut self, line: &str) -> Result<(), UnifdefError> {
        self.ffalse(line)?;
        self.ignoreon();
        Ok(())
    }

    fn idrop(&mut self, line: &str) -> Result<(), UnifdefError> {
        self.fdrop(line)?;
        self.ignoreon();
        Ok(())
    }

    fn ftrue(&mut self, line: &str) -> Result<(), UnifdefError> {
        // Ftrue = { nest(); Strue(); }
        self.nest()?;
        self.drop(line);
        self.ignoreoff();
        self.state(IfState::TruePrefix);
        Ok(())
    }

    fn ffalse(&mut self, line: &str) -> Result<(), UnifdefError> {
        self.nest()?;
        self.drop(line);
        self.ignoreoff();
        self.state(IfState::FalsePrefix);
        Ok(())
    }

    fn fpass(&mut self, line: &str) -> Result<(), UnifdefError> {
        self.nest()?;
        self.print(line);
        self.ignoreoff();
        self.state(IfState::PassMiddle);
        Ok(())
    }

    fn fdrop(&mut self, line: &str) -> Result<(), UnifdefError> {
        self.nest()?;
        self.drop(line);
        self.ignoreoff();
        self.state(IfState::FalseTrailer);
        Ok(())
    }

    fn strue(&mut self, line: &str) {
        self.drop(line);
        self.ignoreoff();
        self.state(IfState::TruePrefix);
    }

    fn sfalse(&mut self, line: &str) {
        self.drop(line);
        self.ignoreoff();
        self.state(IfState::FalsePrefix);
    }

    fn selse(&mut self, line: &str) {
        self.drop(line);
        self.state(IfState::TrueElse);
    }

    fn dfalse(&mut self, line: &str) {
        self.drop(line);
        self.ignoreoff();
        self.state(IfState::FalseTrailer);
    }

    fn delse(&mut self, line: &str) {
        self.drop(line);
        self.state(IfState::FalseElse);
    }

    fn dendif(&mut self, line: &str) {
        self.drop(line);
        self.depth -= 1;
    }

    // ---- line parsing (get_line, unifdef.c:576-672) ----

    fn get_line(&mut self, line: &str) -> Result<LT, UnifdefError> {
        let b = line.as_bytes();
        let wascomment = self.incomment;
        let mut cp = self.skipcomment(b, 0);
        if self.linestate == LineState::Start {
            if cp < b.len() && b[cp] == b'#' {
                self.linestate = LineState::Hash;
                cp = self.skipcomment(b, cp + 1);
            } else if cp < b.len() {
                self.linestate = LineState::Dirty;
            }
        }
        let mut retval = LT::Plain;
        if self.incomment == CommentState::No && self.linestate == LineState::Hash {
            self.keyword_at = cp;
            let kw_end = skip_sym(b, cp);
            let keyword = &line[cp..kw_end.min(line.len())];
            cp = kw_end;
            // No continuation inside a keyword (Eioccc).
            if cp + 2 <= b.len() && &b[cp..cp + 2] == b"\\\n" {
                self.linestate = LineState::Dirty;
                return Err(UnifdefError::ObfuscatedControlLine { line: self.linenum });
            }
            if keyword == "ifdef" || keyword == "ifndef" {
                cp = self.skipcomment(b, cp);
                retval = match self.find_sym(b, cp) {
                    None => LT::If,
                    Some((symidx, _)) => {
                        let base = if keyword.as_bytes().get(2) == Some(&b'n') {
                            LT::False
                        } else {
                            LT::True
                        };
                        let sym = &self.options.symbols[symidx];
                        let mut r = base;
                        if sym.value.is_none() {
                            r = match r {
                                LT::True => LT::False,
                                _ => LT::True,
                            };
                        }
                        if sym.ignore {
                            r = match r {
                                LT::True => LT::TrueI,
                                _ => LT::FalseI,
                            };
                        }
                        r
                    }
                };
                cp = skip_sym(b, cp);
            } else if keyword == "if" {
                retval = match self.ifeval(b, cp) {
                    Some((v, end)) => {
                        cp = end;
                        if v != 0 { LT::True } else { LT::False }
                    }
                    None => LT::If,
                };
            } else if keyword == "elif" {
                retval = match self.ifeval(b, cp) {
                    Some((v, end)) => {
                        cp = end;
                        if v != 0 { LT::ElTrue } else { LT::ElFalse }
                    }
                    None => LT::Elif,
                };
            } else if keyword == "else" {
                retval = LT::Else;
            } else if keyword == "endif" {
                retval = LT::EndIf;
            } else {
                self.linestate = LineState::Dirty;
                retval = LT::Plain;
            }
            cp = self.skipcomment(b, cp);
            if cp < b.len() && b[cp] != b'\n' {
                self.linestate = LineState::Dirty;
                retval = match retval {
                    LT::True | LT::False | LT::TrueI | LT::FalseI => LT::If,
                    LT::ElTrue | LT::ElFalse => LT::Elif,
                    other => other,
                };
            }
            if retval != LT::Plain
                && (wascomment != CommentState::No || self.incomment != CommentState::No)
            {
                // DODGY (not modeled; see module note): the C shifts
                // into the DODGY table columns here.
                if self.incomment != CommentState::No {
                    self.linestate = LineState::Dirty;
                }
            }
        }
        if self.linestate == LineState::Dirty {
            while cp < b.len() {
                cp = self.skipcomment(b, cp + 1);
            }
        }
        Ok(retval)
    }

    // ---- expression evaluation (unifdef.c:676-838) ----

    /// Evaluates the `#if`/`#elif` expression; `None` when unknown.
    fn ifeval(&mut self, b: &[u8], pos: usize) -> Option<(i32, usize)> {
        self.keepthis = !self.options.killconsts;
        let result = self.eval_table(0, b, pos)?;
        if self.keepthis { None } else { Some(result) }
    }

    fn eval_table(&mut self, level: usize, b: &[u8], pos: usize) -> Option<(i32, usize)> {
        let (mut val, mut p) = if level == 3 {
            self.eval_unary(b, pos)?
        } else {
            self.eval_table(level + 1, b, pos)?
        };
        loop {
            p = self.skipcomment(b, p);
            let op: Option<(&str, fn(i32, i32) -> i32)> = match level {
                0 if starts_with(b, p, "||") => Some(("||", |a, c| (a != 0 || c != 0) as i32)),
                1 if starts_with(b, p, "&&") => Some(("&&", |a, c| (a != 0 && c != 0) as i32)),
                2 if starts_with(b, p, "==") => Some(("==", |a, c| (a == c) as i32)),
                2 if starts_with(b, p, "!=") => Some(("!=", |a, c| (a != c) as i32)),
                3 if starts_with(b, p, "<=") => Some(("<=", |a, c| (a <= c) as i32)),
                3 if starts_with(b, p, ">=") => Some((">=", |a, c| (a >= c) as i32)),
                3 if starts_with(b, p, "<") => Some(("<", |a, c| (a < c) as i32)),
                3 if starts_with(b, p, ">") => Some((">", |a, c| (a > c) as i32)),
                _ => None,
            };
            let (opstr, f) = match op {
                Some(x) => x,
                None => break,
            };
            let (rhs, p3) = if level == 3 {
                self.eval_unary(b, p + opstr.len())?
            } else {
                self.eval_table(level + 1, b, p + opstr.len())?
            };
            val = f(val, rhs);
            p = p3;
        }
        Some((val, p))
    }

    fn eval_unary(&mut self, b: &[u8], pos: usize) -> Option<(i32, usize)> {
        let mut p = self.skipcomment(b, pos);
        if p < b.len() && b[p] == b'!' {
            let (v, p2) = self.eval_unary(b, p + 1)?;
            return Some(((v == 0) as i32, p2));
        }
        if p < b.len() && b[p] == b'(' {
            let (v, p2) = self.eval_table(0, b, p + 1)?;
            let p3 = self.skipcomment(b, p2);
            if p3 < b.len() && b[p3] == b')' {
                return Some((v, p3 + 1));
            }
            return None;
        }
        if p < b.len() && b[p].is_ascii_digit() {
            let (v, end) = strtol0(b, p);
            return Some((v, skip_sym(b, end)));
        }
        if p + 8 <= b.len() && &b[p..p + 7] == b"defined" && ends_sym(b.get(p + 7).copied()) {
            p = self.skipcomment(b, p + 7);
            if p < b.len() && b[p] == b'(' {
                p = self.skipcomment(b, p + 1);
                let (symidx, p2) = self.find_sym(b, p)?;
                let defined = self.options.symbols[symidx].value.is_some();
                let p3 = skip_sym(b, p2);
                let p4 = self.skipcomment(b, p3);
                if p4 < b.len() && b[p4] == b')' {
                    self.keepthis = false;
                    return Some((defined as i32, p4 + 1));
                }
            }
            return None;
        }
        if p < b.len() && !ends_sym(Some(b[p])) {
            let (symidx, p2) = self.find_sym(b, p)?;
            let sym = self.options.symbols[symidx].clone();
            let v = match &sym.value {
                None => 0,
                Some(vs) => {
                    let bytes = vs.as_bytes();
                    let (n, used) = strtol0(bytes, 0);
                    if used != bytes.len() || used == 0 {
                        return None;
                    }
                    n
                }
            };
            self.keepthis = false;
            return Some((v, p2));
        }
        None
    }

    // ---- scanning helpers ----

    /// skipcomment: advance over whitespace/comments, tracking the
    /// comment and line states (unifdef.c:840-914).
    fn skipcomment(&mut self, b: &[u8], mut cp: usize) -> usize {
        if self.options.text || self.ignoring[self.depth] {
            while cp < b.len() && (b[cp] as char).is_ascii_whitespace() {
                if b[cp] == b'\n' {
                    self.linestate = LineState::Start;
                }
                cp += 1;
            }
            return cp;
        }
        while cp < b.len() {
            if b[cp] == b'\\' && cp + 1 < b.len() && b[cp + 1] == b'\n' {
                // don't reset to LS_START after a line continuation
                cp += 2;
                continue;
            }
            match self.incomment {
                CommentState::No => {
                    if b[cp] == b'/' && cp + 2 < b.len() && b[cp + 1] == b'\\' && b[cp + 2] == b'\n'
                    {
                        self.incomment = CommentState::Starting;
                        cp += 3;
                    } else if cp + 1 < b.len() && b[cp] == b'/' && b[cp + 1] == b'*' {
                        self.incomment = CommentState::C;
                        cp += 2;
                    } else if cp + 1 < b.len() && b[cp] == b'/' && b[cp + 1] == b'/' {
                        self.incomment = CommentState::Cxx;
                        cp += 2;
                    } else if b[cp] == b'\n' {
                        self.linestate = LineState::Start;
                        cp += 1;
                    } else if b[cp] == b' ' || b[cp] == b'\t' {
                        cp += 1;
                    } else {
                        return cp;
                    }
                }
                CommentState::Cxx => {
                    if b[cp] == b'\n' {
                        self.incomment = CommentState::No;
                        self.linestate = LineState::Start;
                    }
                    cp += 1;
                }
                CommentState::C => {
                    if b[cp] == b'*' && cp + 2 < b.len() && b[cp + 1] == b'\\' && b[cp + 2] == b'\n'
                    {
                        self.incomment = CommentState::Finishing;
                        cp += 3;
                    } else if cp + 1 < b.len() && b[cp] == b'*' && b[cp + 1] == b'/' {
                        self.incomment = CommentState::No;
                        cp += 2;
                    } else {
                        cp += 1;
                    }
                }
                CommentState::Starting => {
                    if b[cp] == b'*' {
                        self.incomment = CommentState::C;
                        cp += 1;
                    } else if b[cp] == b'/' {
                        self.incomment = CommentState::Cxx;
                        cp += 1;
                    } else {
                        self.incomment = CommentState::No;
                        self.linestate = LineState::Dirty;
                    }
                }
                CommentState::Finishing => {
                    if b[cp] == b'/' {
                        self.incomment = CommentState::No;
                        cp += 1;
                    } else {
                        self.incomment = CommentState::C;
                    }
                }
            }
        }
        cp
    }

    /// findsym: match the identifier at `cp` against the table.
    /// Returns (index, end-of-symbol).
    fn find_sym(&mut self, b: &[u8], cp: usize) -> Option<(usize, usize)> {
        let end = skip_sym(b, cp);
        if end == cp {
            return None;
        }
        let name = core::str::from_utf8(&b[cp..end]).ok()?;
        if self.options.symlist {
            self.symbol_list.push(name.to_string());
        }
        for (i, sym) in self.options.symbols.iter().enumerate() {
            if sym.name == name {
                return Some((i, end));
            }
        }
        None
    }
}

fn starts_with(b: &[u8], pos: usize, s: &str) -> bool {
    b.len() >= pos + s.len() && &b[pos..pos + s.len()] == s.as_bytes()
}

fn ends_sym(c: Option<u8>) -> bool {
    match c {
        None => true,
        Some(c) => !(c.is_ascii_alphanumeric() || c == b'_'),
    }
}

fn skip_sym(b: &[u8], mut cp: usize) -> usize {
    while cp < b.len() && !ends_sym(Some(b[cp])) {
        cp += 1;
    }
    cp
}

/// strtol with base 0: hex `0x`, octal `0`, decimal.
fn strtol0(b: &[u8], mut cp: usize) -> (i32, usize) {
    let neg = if cp < b.len() && (b[cp] == b'+' || b[cp] == b'-') {
        let n = b[cp] == b'-';
        cp += 1;
        n
    } else {
        false
    };
    let (radix, digits_at) = if cp + 1 < b.len() && b[cp] == b'0' && (b[cp + 1] | 0x20) == b'x' {
        (16u32, cp + 2)
    } else if cp < b.len() && b[cp] == b'0' {
        (8, cp)
    } else {
        (10, cp)
    };
    let mut end = if radix == 16 { digits_at } else { digits_at };
    let mut value: i64 = 0;
    let mut any = false;
    for &c in &b[end..] {
        let d = match (c as char).to_digit(radix) {
            Some(d) => d as i64,
            None => break,
        };
        value = value * radix as i64 + d;
        end += 1;
        any = true;
    }
    let _ = any;
    let v = if neg { -(value as i32) } else { value as i32 };
    (v, end)
}

/// Splits input into lines, keeping the newline (like fgets).
fn split_lines(input: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let bytes = input.as_bytes();
    let mut start = 0usize;
    for i in 0..bytes.len() {
        if bytes[i] == b'\n' {
            out.push(&input[start..=i]);
            start = i + 1;
        }
    }
    if start < bytes.len() {
        out.push(&input[start..]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 集外字母与缺值都交回 libc 那两个形状；大小写算集合的一部分（`u` 与 `E` 都不在串里）。
    #[test]
    fn an_unreachable_option_shape_is_reported_by_the_libc_line() {
        for (args, report) in [
            (vec!["-z"], crate::GetoptReport::UnknownOption('z')),
            (vec!["-cz"], crate::GetoptReport::UnknownOption('z')),
            (vec!["--x"], crate::GetoptReport::UnknownOption('-')),
            (vec!["-uD"], crate::GetoptReport::UnknownOption('u')),
            (vec!["-tE"], crate::GetoptReport::UnknownOption('E')),
            (vec!["-cD"], crate::GetoptReport::MissingValue('D')),
            (vec!["-lI"], crate::GetoptReport::MissingValue('I')),
        ] {
            assert_eq!(
                parse_unifdef_args(&args).err().unwrap(),
                UnifdefArgError::Getopt(report),
                "`{args:?}` 的出口"
            );
        }
    }

    /// 环**不重排**：`-DA in.c -c` 里的 `-c` 留在操数里（参照件那句 `can only do one file`），
    /// 而裸 `-` 也是操数；`--` 结束环且自身不算操数。
    #[test]
    fn the_ring_ends_at_the_first_operand_and_keeps_everything_after_it() {
        let parsed = parse_unifdef_args(&["-DA", "in.c", "-c"]).expect("环在 in.c 处结束");
        assert_eq!(parsed.operands, ["in.c", "-c"]);
        assert!(!parsed.options.complement, "环后的 `-c` 不该再点旗标");
        let parsed = parse_unifdef_args(&["-DA", "-"]).expect("裸 `-` 是操数");
        assert_eq!(parsed.operands, ["-"]);
        let parsed = parse_unifdef_args(&["--", "-z"]).expect("`--` 之后全是操数");
        assert_eq!(parsed.operands, ["-z"]);
        assert!(parse_unifdef_args(&["--"]).unwrap().operands.is_empty());
    }

    /// `-i` 的取值先吃一个字母：`-iDFOO` 的符号是 `FOO` 且带 ignore；后随字母不是 `D`/`U`
    /// 就是 `Usage`（C 直接 `usage()`，没有自己的句子）。
    #[test]
    fn the_ignore_letter_splits_its_value() {
        let parsed = parse_unifdef_args(&["-iDFOO", "-iUBAR"]).expect("两个 ignore 符号");
        assert_eq!(
            parsed.options.symbols,
            [
                Sym {
                    name: "FOO".into(),
                    value: Some(String::new()),
                    ignore: true,
                },
                Sym {
                    name: "BAR".into(),
                    value: None,
                    ignore: true,
                },
            ]
        );
        assert_eq!(
            parse_unifdef_args(&["-iX"]).err().unwrap(),
            UnifdefArgError::Usage
        );
        assert_eq!(
            parse_unifdef_args(&["-i", ""]).err().unwrap(),
            UnifdefArgError::Usage,
            "空值时 `*optarg` 当场是 `\\0`"
        );
    }

    /// 空符号名合法（`skipsym` 原地返回），而名字之后既不是结尾也不是 `=` 就交回 `Usage`；
    /// 重复给同一个符号覆写同一格而不是叠两份。
    #[test]
    fn a_symbol_name_is_a_leading_identifier_run_and_may_be_empty() {
        let parsed = parse_unifdef_args(&["-D", ""]).expect("空符号名是合法的");
        assert_eq!(parsed.options.symbols.len(), 1);
        assert_eq!(parsed.options.symbols[0].value, Some(String::new()));
        for args in [
            vec!["-DFOO."],
            vec!["-UFOO=1"],
            vec!["-D", "FOO BAR"],
            vec!["-U", "FOO."],
        ] {
            assert_eq!(
                parse_unifdef_args(&args).err().unwrap(),
                UnifdefArgError::Usage,
                "`{args:?}` 的符号形"
            );
        }
        let parsed = parse_unifdef_args(&["-DA", "-DA=2"]).expect("同一个符号覆写同一格");
        assert_eq!(parsed.options.symbols.len(), 1);
        assert_eq!(parsed.options.symbols[0].value.as_deref(), Some("2"));
    }

    fn run_with(input: &str, setup: impl FnOnce(&mut Options)) -> Result<Outcome, UnifdefError> {
        let mut options = Options::default();
        setup(&mut options);
        Unifdef::new(options).run(input)
    }

    #[test]
    fn test_dropped_ifdef_body() {
        let out = run_with("#ifdef FOO\nkept\n#endif\nafter\n", |o| {
            add_symbol(o, "FOO", false, false).unwrap();
        })
        .unwrap();
        // The decided #ifdef/#endif pair itself is deleted too.
        assert_eq!(String::from_utf8(out.output).unwrap(), "after\n");
        assert_eq!(out.exitstat, 1);
    }

    #[test]
    fn test_kept_ifndef_body() {
        let out = run_with("#ifndef FOO\nkept\n#endif\n", |o| {
            add_symbol(o, "FOO", false, false).unwrap();
        })
        .unwrap();
        assert_eq!(String::from_utf8(out.output).unwrap(), "kept\n");
        assert_eq!(out.exitstat, 1);
    }

    #[test]
    fn test_else_branch_dropped_when_if_kept() {
        let out = run_with("#ifdef FOO\nyes\n#else\nno\n#endif\n", |o| {
            add_symbol(o, "FOO", true, false).unwrap();
        })
        .unwrap();
        assert_eq!(String::from_utf8(out.output).unwrap(), "yes\n");
    }

    #[test]
    fn test_else_branch_kept_when_if_dropped() {
        let out = run_with("#ifdef FOO\nyes\n#else\nno\n#endif\n", |o| {
            add_symbol(o, "FOO", false, false).unwrap()
        })
        .unwrap();
        assert_eq!(String::from_utf8(out.output).unwrap(), "no\n");
    }

    #[test]
    fn test_nested_conditionals() {
        let src = "#ifdef A\nouter\n#ifdef B\nboth\n#endif\n#endif\n";
        let out = run_with(src, |o| {
            add_symbol(o, "A", true, false).unwrap();
            add_symbol(o, "B", false, false).unwrap();
        })
        .unwrap();
        // Both groups were decidable, so no directives survive.
        assert_eq!(String::from_utf8(out.output).unwrap(), "outer\n");
    }

    #[test]
    fn test_unknown_if_passed_through() {
        let out = run_with("#if UNKNOWN > 2\nbody\n#endif\n", |_| {}).unwrap();
        assert_eq!(
            String::from_utf8(out.output).unwrap(),
            "#if UNKNOWN > 2\nbody\n#endif\n"
        );
    }

    #[test]
    fn test_constant_if_evaluated_with_k() {
        let out = run_with("#if 1 > 2\nno\n#else\nyes\n#endif\n", |o| {
            o.killconsts = true;
        })
        .unwrap();
        assert_eq!(String::from_utf8(out.output).unwrap(), "yes\n");
    }

    #[test]
    fn test_dash_d_value_used_in_if() {
        let out = run_with("#if FOO == 42\nyes\n#else\nno\n#endif\n", |o| {
            add_symbol(o, "FOO=42", true, false).unwrap();
            o.killconsts = true;
        })
        .unwrap();
        assert_eq!(String::from_utf8(out.output).unwrap(), "yes\n");
    }

    #[test]
    fn test_ignore_symbols_treat_block_as_text() {
        let out = run_with("#ifdef FOO\ntext\n#endif\n", |o| {
            add_symbol(o, "FOO", true, true).unwrap();
        })
        .unwrap();
        assert_eq!(String::from_utf8(out.output).unwrap(), "text\n");
        assert_eq!(out.exitstat, 1);
    }

    #[test]
    fn test_complement_inverts() {
        let out = run_with("#ifdef FOO\nyes\n#endif\n", |o| {
            add_symbol(o, "FOO", true, false).unwrap();
            o.complement = true;
        })
        .unwrap();
        // The complement re-emits the directive skeleton and drops
        // what would have been kept.
        assert_eq!(
            String::from_utf8(out.output).unwrap(),
            "#ifdef FOO\n#endif\n"
        );
        assert_eq!(out.exitstat, 1);
    }

    #[test]
    fn test_lnblank_blanks_dropped_lines() {
        let out = run_with("#ifdef FOO\nyes\nno\n#endif\n", |o| {
            add_symbol(o, "FOO", true, false).unwrap();
            o.lnblank = true;
        })
        .unwrap();
        // Dropped directive lines become blank lines.
        assert_eq!(String::from_utf8(out.output).unwrap(), "\nyes\nno\n\n");
    }

    #[test]
    fn test_comments_around_directives() {
        // A comment closed on the directive line does not make it
        // DODGY: the directive still parses.
        let out = run_with("#ifdef FOO /* x */\nbody\n#endif\n", |o| {
            add_symbol(o, "FOO", false, false).unwrap();
        })
        .unwrap();
        // -UFOO drops the whole group including the directive lines.
        assert_eq!(String::from_utf8(out.output).unwrap(), "");
    }

    #[test]
    fn test_text_mode_leaves_comments_alone() {
        let out = run_with("#ifdef FOO\n// not a directive marker\n#endif\n", |o| {
            add_symbol(o, "FOO", false, false).unwrap();
            o.text = true;
        })
        .unwrap();
        // The // line is plain text INSIDE the dropped group.
        assert_eq!(String::from_utf8(out.output).unwrap(), "");
    }

    #[test]
    fn test_inappropriate_endif_is_error() {
        let err = run_with("#endif\n", |_| {}).unwrap_err();
        assert_eq!(err, UnifdefError::InappropriateEndif { line: 1 });
    }

    #[test]
    fn test_premature_eof_is_error() {
        let err = run_with("#ifdef FOO\nbody\n", |o| {
            add_symbol(o, "FOO", false, false).unwrap();
        })
        .unwrap_err();
        assert_eq!(err, UnifdefError::PrematureEof);
    }

    #[test]
    fn test_symbol_list_mode() {
        let out = run_with("#ifdef FOO\nx\n#elif BAR\ny\n#endif\n", |o| {
            o.symlist = true;
        })
        .unwrap();
        assert_eq!(out.symbol_list, vec!["FOO", "BAR"]);
        assert!(out.output.is_empty());
    }

    #[test]
    fn test_strtol0_bases() {
        assert_eq!(strtol0(b"42", 0), (42, 2));
        assert_eq!(strtol0(b"0x2a", 0), (42, 4));
        assert_eq!(strtol0(b"052", 0), (42, 3));
        assert_eq!(strtol0(b"-7", 0), (-7, 2));
    }
}
