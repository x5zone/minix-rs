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
    /// The symbol name is argv's **raw bytes**: C's `addsym` stores the pointer into
    /// `optarg` (`unifdef.c:975`'s `symname[symind] = sym`), looks it up through
    /// `strlcmp` (`:951`) and never validates the encoding.
    pub name: Vec<u8>,
    pub value: Option<Vec<u8>>,
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
    /// (`unifdef.c:311-312`'s `ofilename = optarg`). Bytes: the same
    /// `argv` word later reaches `err(2, "can't open %s", ofilename)`
    /// (`unifdef.c:365`) and `errx(2, "%s unchanged", ofilename)` (`:509`).
    pub output: Option<Vec<u8>>,
}

/// The two failures of `addsym` (`unifdef.c:965-991`): a malformed shape (C calls
/// `usage()` straight away) and a full symbol table (`unifdef.c:971-972`'s
/// `errx(2, "too many symbols")`).
#[derive(Debug, PartialEq)]
pub enum SymError {
    Malformed,
    TooMany,
}

/// C's `skipsym` (`unifdef.c:928-933`, whose test is `:262`'s `endsym`): consumes a
/// run of `[A-Za-z0-9_]`, and **the empty string is valid** (it consumes no character
/// and returns to its start, so `addsym` sees `*val == '\0'`).
fn symbol_run(spec: &[u8]) -> usize {
    let mut end = 0usize;
    while end < spec.len() && matches!(spec[end], b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_') {
        end += 1;
    }
    end
}

/// Parses `-Dsym[=val]` / `-Usym` (and the `-iD/-iU` ignore variants).
///
/// Every test follows `addsym`; no sentences are invented: the `-D` branch allows
/// `sym`, `sym=` and `sym=val`, and anything after the name that is neither the end
/// nor `=` returns [`SymError::Malformed`] (C calls `usage()` there); the `-U` branch
/// requires the end right after the name, so a trailing `=` is malformed (the
/// reference's `unifdef -UFOO=1` prints the usage line alone). A repeated name makes
/// C's `findsym` find the same slot and overwrite its `value`/`ignore`; this matches.
pub fn add_symbol(
    options: &mut Options,
    spec: &[u8],
    defined: bool,
    ignore: bool,
) -> Result<(), SymError> {
    let cut = symbol_run(spec);
    let (name, rest) = spec.split_at(cut);
    // C's `addsym` looks at `*val` first: `'\0'` and `'='` are the two legal
    // branches, anything else goes straight to `usage()` (`unifdef.c:977-990`). The
    // whole run after `=` becomes the value, with no further character-set check.
    let value = if defined {
        if rest.is_empty() {
            Some(Vec::new())
        } else if rest[0] == b'=' {
            Some(rest[1..].to_vec())
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
    // `MAXSYMS` (`unifdef.c:197`) is the capacity of C's static table, and the
    // overflow sentence `too many symbols` goes through `errx(2, …)` rather than
    // usage.
    if options.symbols.len() >= 4096 {
        return Err(SymError::TooMany);
    }
    options.symbols.push(Sym {
        name: name.to_vec(),
        value,
        ignore,
    });
    Ok(())
}

/// The exits of the front-end ring (`unifdef.c:273`'s
/// `getopt(argc, argv, "i:D:U:I:o:cdeklst")`).
#[derive(Debug, PartialEq)]
pub enum UnifdefArgError {
    /// An off-letter or a valued letter without its value: libc prints one line,
    /// then `unifdef.c:322-323`'s `default: usage()` adds the second.
    Getopt(crate::GetoptReport),
    /// `-i` not followed by `D`/`U` (`unifdef.c:283-287`), or a symbol name that does
    /// not fit `addsym`'s shape — C only calls `usage()` in both places, **with no
    /// invented sentence**.
    Usage,
    /// The symbol table is full: a single `errx(2, "too many symbols")` line.
    TooManySymbols,
}

/// What the option ring produces: the flags and the operands handed back **in their
/// original order** (C accepts 0 or 1; more is judged in the shell).
pub struct UnifdefArgs<'a> {
    pub options: Options,
    /// Operands are handed back as argv's raw bytes: C passes them to `fopen`
    /// (`unifdef.c:335`), and on failure prints them in
    /// `err(2, "can't open %s", filename)` (`:337`).
    pub operands: Vec<&'a [u8]>,
}

/// The option ring of `unifdef.c:273-323`.
///
/// Five valued options (`D`/`U`/`i`/`I`/`o`) and seven valueless letters
/// (`c`/`d`/`e`/`k`/`l`/`s`/`t`), mixed case — `D` and `d`, `U` and `u` (the latter
/// not in the string) are different things, so an off-set decision cannot be guessed
/// from the letter family. `-i` consumes the **one character** that follows before
/// looking at the rest (`opt = *optarg++`, `unifdef.c:278-287`): the symbol in
/// `-iDFOO` is `FOO`, `-iD`'s symbol is the empty string (valid), and `-iX` and `-i`
/// with an empty string both go straight to usage. `-I` takes a value and does
/// nothing with it (the cpp-compatibility slot at `unifdef.c:289-291`).
///
/// The ring **does not rearrange**: on the first non-option token, `getopt.c:74-78`
/// ends the ring on the spot without advancing `optind`, so the `-c` in
/// `unifdef -DFOO in.c -c` is a second operand (the reference gives
/// `unifdef: can only do one file`) rather than another flag. A bare `-` stays among
/// the operands, and `unifdef.c:333`'s `strcmp(*argv, "-") != 0` reads it as
/// standard input.
pub fn parse_unifdef_args<'a>(args: &'a [&'a [u8]]) -> Result<UnifdefArgs<'a>, UnifdefArgError> {
    let mut options = Options::default();
    let mut index = 0usize;
    let operands = loop {
        let Some(&arg) = args.get(index) else {
            break Vec::new();
        };
        let bytes = arg;
        if arg == b"--" {
            // `getopt.c:81-85`: -1 is returned only after `++optind`, so `--` itself
            // is not an operand.
            break args[index + 1..].to_vec();
        }
        if bytes.len() < 2 || bytes[0] != b'-' {
            // `getopt.c:74-78`: a non-option token ends the ring without advancing
            // `optind`.
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
                // `-d` (debug report) and `-e` (tolerate malformed lines) are valid
                // no-op letters in this shell: they gate the DODGY table columns,
                // which are registered separately.
                b'd' | b'e' => {}
                ch @ (b'D' | b'U' | b'i' | b'I' | b'o') => {
                    // The value is "the rest of this token after the `-` and the
                    // letter", or the next token when nothing remains
                    // (`getopt.c:118-141`); both readers consume the token to its end.
                    let value = if pos + 1 < bytes.len() {
                        &bytes[pos + 1..]
                    } else {
                        match args.get(index + 1) {
                            Some(next) => {
                                index += 1;
                                next
                            }
                            None => {
                                return Err(UnifdefArgError::Getopt(
                                    crate::GetoptReport::MissingValue(char::from(ch)),
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
                        b'o' => options.output = Some(value.to_vec()),
                        _ => {
                            // The first **byte** of `-i`'s value picks the side
                            // (`opt = *optarg++`); the rest is the symbol name.
                            match value.first() {
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
                    // What is reported is **the first byte at this point**: C's
                    // `optopt` is a `char` and `%c` prints exactly that byte; the
                    // cursor `pos` is a byte index, which can land inside a multibyte
                    // character for a non-ASCII letter, so the byte is taken directly
                    // instead of slicing the string (slicing panics on the spot).
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

/// `addsym`'s two failures land on two different exits in C: a malformed shape is
/// `usage()`, a full table is `errx`.
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
                    let bytes = vs.as_slice();
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
        // `skip_sym` reads the same way as C's `skipsym` (`unifdef.c:262`'s `endsym`
        // accepts only letters, digits and `_`), so this run is always ASCII and
        // `from_utf8` cannot fail.
        let name = core::str::from_utf8(&b[cp..end]).ok()?;
        if self.options.symlist {
            self.symbol_list.push(name.to_string());
        }
        for (i, sym) in self.options.symbols.iter().enumerate() {
            // Names are compared as bytes (C uses `strlcmp`, `unifdef.c:951`).
            if sym.name == name.as_bytes() {
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

    /// The fixture's argv: `std::env::args_os()` hands back bytes. The `symbol_list`
    /// expectations are still spelled as text: its entries hold only `[A-Za-z0-9_]`
    /// (`endsym` reads both ends the same way), so no byte-versus-character difference
    /// can arise.
    fn argv<'a>(words: &[&'a str]) -> Vec<&'a [u8]> {
        words.iter().map(|word| word.as_bytes()).collect()
    }

    /// An off-letter and a missing value both hand back libc's two shapes; case is
    /// part of the set (`u` and `E` are both absent from the string).
    #[test]
    fn an_unreachable_option_shape_is_reported_by_the_libc_line() {
        for (args, report) in [
            (
                vec![b"-z".as_slice()],
                crate::GetoptReport::UnknownOption('z'),
            ),
            (
                vec![b"-cz".as_slice()],
                crate::GetoptReport::UnknownOption('z'),
            ),
            (
                vec![b"--x".as_slice()],
                crate::GetoptReport::UnknownOption('-'),
            ),
            (
                vec![b"-uD".as_slice()],
                crate::GetoptReport::UnknownOption('u'),
            ),
            (
                vec![b"-tE".as_slice()],
                crate::GetoptReport::UnknownOption('E'),
            ),
            (
                vec![b"-cD".as_slice()],
                crate::GetoptReport::MissingValue('D'),
            ),
            (
                vec![b"-lI".as_slice()],
                crate::GetoptReport::MissingValue('I'),
            ),
        ] {
            assert_eq!(
                parse_unifdef_args(&args).err().unwrap(),
                UnifdefArgError::Getopt(report),
                "`{args:?}` 的出口"
            );
        }
    }

    /// The ring **does not rearrange**: the `-c` in `-DA in.c -c` stays among the
    /// operands (the reference's `can only do one file`), a bare `-` is an operand
    /// too, and `--` ends the ring without counting itself.
    #[test]
    fn the_ring_ends_at_the_first_operand_and_keeps_everything_after_it() {
        let w_parsed = argv(&["-DA", "in.c", "-c"]);
        let parsed = parse_unifdef_args(&w_parsed).expect("环在 in.c 处结束");
        assert_eq!(parsed.operands, [b"in.c".as_slice(), b"-c".as_slice()]);
        assert!(!parsed.options.complement, "环后的 `-c` 不该再点旗标");
        let w_parsed = argv(&["-DA", "-"]);
        let parsed = parse_unifdef_args(&w_parsed).expect("裸 `-` 是操数");
        assert_eq!(parsed.operands, [b"-".as_slice()]);
        let w_parsed = argv(&["--", "-z"]);
        let parsed = parse_unifdef_args(&w_parsed).expect("`--` 之后全是操数");
        assert_eq!(parsed.operands, [b"-z".as_slice()]);
        assert!(
            parse_unifdef_args(&argv(&["--"]))
                .unwrap()
                .operands
                .is_empty()
        );
    }

    /// `-i`'s value consumes one letter first: `-iDFOO`'s symbol is `FOO` and it is
    /// marked ignore; a following letter that is not `D`/`U` yields `Usage` (C calls
    /// `usage()` directly, with no sentence of its own).
    #[test]
    fn the_ignore_letter_splits_its_value() {
        let w_parsed = argv(&["-iDFOO", "-iUBAR"]);
        let parsed = parse_unifdef_args(&w_parsed).expect("两个 ignore 符号");
        assert_eq!(
            parsed.options.symbols,
            [
                Sym {
                    name: "FOO".into(),
                    value: Some(Vec::new()),
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
            parse_unifdef_args(&argv(&["-iX"])).err().unwrap(),
            UnifdefArgError::Usage
        );
        assert_eq!(
            parse_unifdef_args(&argv(&["-i", ""])).err().unwrap(),
            UnifdefArgError::Usage,
            "空值时 `*optarg` 当场是 `\\0`"
        );
    }

    /// An empty symbol name is valid (`skipsym` returns in place); anything after the
    /// name that is neither the end nor `=` returns `Usage`; repeating a symbol
    /// overwrites the same slot rather than stacking two entries.
    #[test]
    fn a_symbol_name_is_a_leading_identifier_run_and_may_be_empty() {
        let w_parsed = argv(&["-D", ""]);
        let parsed = parse_unifdef_args(&w_parsed).expect("空符号名是合法的");
        assert_eq!(parsed.options.symbols.len(), 1);
        assert_eq!(parsed.options.symbols[0].value, Some(Vec::new()));
        for args in [
            vec![b"-DFOO.".as_slice()],
            vec![b"-UFOO=1".as_slice()],
            vec![b"-D".as_slice(), b"FOO BAR".as_slice()],
            vec![b"-U".as_slice(), b"FOO.".as_slice()],
        ] {
            assert_eq!(
                parse_unifdef_args(&args).err().unwrap(),
                UnifdefArgError::Usage,
                "`{args:?}` 的符号形"
            );
        }
        let w_parsed = argv(&["-DA", "-DA=2"]);
        let parsed = parse_unifdef_args(&w_parsed).expect("同一个符号覆写同一格");
        assert_eq!(parsed.options.symbols.len(), 1);
        assert_eq!(
            parsed.options.symbols[0].value.as_deref(),
            Some(b"2".as_slice())
        );
    }

    fn run_with(input: &str, setup: impl FnOnce(&mut Options)) -> Result<Outcome, UnifdefError> {
        let mut options = Options::default();
        setup(&mut options);
        Unifdef::new(options).run(input)
    }

    #[test]
    fn test_dropped_ifdef_body() {
        let out = run_with("#ifdef FOO\nkept\n#endif\nafter\n", |o| {
            add_symbol(o, b"FOO", false, false).unwrap();
        })
        .unwrap();
        // The decided #ifdef/#endif pair itself is deleted too.
        assert_eq!(String::from_utf8(out.output).unwrap(), "after\n");
        assert_eq!(out.exitstat, 1);
    }

    #[test]
    fn test_kept_ifndef_body() {
        let out = run_with("#ifndef FOO\nkept\n#endif\n", |o| {
            add_symbol(o, b"FOO", false, false).unwrap();
        })
        .unwrap();
        assert_eq!(String::from_utf8(out.output).unwrap(), "kept\n");
        assert_eq!(out.exitstat, 1);
    }

    #[test]
    fn test_else_branch_dropped_when_if_kept() {
        let out = run_with("#ifdef FOO\nyes\n#else\nno\n#endif\n", |o| {
            add_symbol(o, b"FOO", true, false).unwrap();
        })
        .unwrap();
        assert_eq!(String::from_utf8(out.output).unwrap(), "yes\n");
    }

    #[test]
    fn test_else_branch_kept_when_if_dropped() {
        let out = run_with("#ifdef FOO\nyes\n#else\nno\n#endif\n", |o| {
            add_symbol(o, b"FOO", false, false).unwrap()
        })
        .unwrap();
        assert_eq!(String::from_utf8(out.output).unwrap(), "no\n");
    }

    #[test]
    fn test_nested_conditionals() {
        let src = "#ifdef A\nouter\n#ifdef B\nboth\n#endif\n#endif\n";
        let out = run_with(src, |o| {
            add_symbol(o, b"A", true, false).unwrap();
            add_symbol(o, b"B", false, false).unwrap();
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
            add_symbol(o, b"FOO=42", true, false).unwrap();
            o.killconsts = true;
        })
        .unwrap();
        assert_eq!(String::from_utf8(out.output).unwrap(), "yes\n");
    }

    #[test]
    fn test_ignore_symbols_treat_block_as_text() {
        let out = run_with("#ifdef FOO\ntext\n#endif\n", |o| {
            add_symbol(o, b"FOO", true, true).unwrap();
        })
        .unwrap();
        assert_eq!(String::from_utf8(out.output).unwrap(), "text\n");
        assert_eq!(out.exitstat, 1);
    }

    #[test]
    fn test_complement_inverts() {
        let out = run_with("#ifdef FOO\nyes\n#endif\n", |o| {
            add_symbol(o, b"FOO", true, false).unwrap();
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
            add_symbol(o, b"FOO", true, false).unwrap();
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
            add_symbol(o, b"FOO", false, false).unwrap();
        })
        .unwrap();
        // -UFOO drops the whole group including the directive lines.
        assert_eq!(String::from_utf8(out.output).unwrap(), "");
    }

    #[test]
    fn test_text_mode_leaves_comments_alone() {
        let out = run_with("#ifdef FOO\n// not a directive marker\n#endif\n", |o| {
            add_symbol(o, b"FOO", false, false).unwrap();
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
            add_symbol(o, b"FOO", false, false).unwrap();
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
    /// The symbol name, `-o`'s value and the operands all hand back argv's **raw
    /// bytes**. All three C sites work on bytes: `addsym` stores the pointer into
    /// `optarg` (`unifdef.c:975`), looks names up by comparing bytes with `strlcmp`
    /// (`:951`, defined at `:998`), and takes the tail's start from `skipsym`
    /// (`:977`); after `ofilename = optarg` (`:314`) the name reaches `fopen`
    /// (`:364`), `err(2, "can't open %s", ofilename)` (`:365`) and
    /// `errx(2, "%s unchanged", ofilename)` (`:509`), and the operand behaves the same
    /// (`:335-337`). The `-i` branch consumes **one byte** to pick the side
    /// (`opt = *optarg++`, `:281`), so `-i\xff` takes the usage path rather than the
    /// symbol arm.
    #[test]
    fn the_symbol_and_path_legs_hand_back_the_argv_bytes() {
        // The name itself must stay within `[A-Za-z0-9_]` (C's `skipsym`), so a bare
        // `ff` inside a name yields `usage()` rather than "a symbol carrying a byte";
        // the places a raw byte can reach alive are **the value after `=`** (C does
        // not check the character set there) and the two file-name sites.
        assert_eq!(
            parse_unifdef_args(&[b"-D\xff"]).err(),
            Some(UnifdefArgError::Usage),
            "名字里的 ff 让 `*val` 停在 ff 上，C 那一支就是 usage"
        );
        assert_eq!(
            parse_unifdef_args(&[b"-DA\xff"]).err(),
            Some(UnifdefArgError::Usage),
            "名字吃到 `A` 为止，ff 是剩下的尾巴——`addsym` 判的就是这根尾巴为空"
        );
        let w_val: Vec<&[u8]> = vec![b"-DA=\xff".as_slice()];
        let parsed = parse_unifdef_args(&w_val).expect("`=` 之后 C 不再校验字符集");
        assert_eq!(parsed.options.symbols[0].name, b"A");
        assert_eq!(
            parsed.options.symbols[0].value.as_deref(),
            Some(b"\xff".as_slice()),
            "值位是原字节：它随后进的是 `strtol0` 的比较而不是报文"
        );
        let w_o: Vec<&[u8]> = vec![b"-o".as_slice(), b"out\xff.c".as_slice(), b"-DA".as_slice()];
        let parsed = parse_unifdef_args(&w_o).expect("-o 的值是档名，按字节留着");
        assert_eq!(
            parsed.options.output.as_deref(),
            Some(b"out\xff.c".as_slice()),
            "报文与 fopen 用的都是这一串"
        );
        // After `-o` consumes the run right behind it, the ring keeps going: `-DA`
        // still starts with `-`, so it is a **second option**, not an operand
        // (`getopt.c:74-78` returns -1 only on a non-option token).
        assert!(parsed.operands.is_empty(), "环没停，`-DA` 被当选项吃了");
        assert_eq!(parsed.options.symbols.len(), 1);
        let w_dash: Vec<&[u8]> = vec![b"-DA".as_slice(), b"in\xff.c".as_slice()];
        assert_eq!(
            parse_unifdef_args(&w_dash).unwrap().operands,
            vec![b"in\xff.c".as_slice()]
        );
        // The letter slot must be pinned in the library too: when the first version
        // of the "read by character" negative control was injected into this arm, the
        // in-library tests stayed all green, which showed this face covered only the
        // two `usage()` branches and not the letter arm (the rule: an all-green run
        // is a reason to suspect a hole in coverage). With this block added, the same
        // injection bites one in-library test.
        assert_eq!(
            parse_unifdef_args(&[b"-\xff"]).err(),
            Some(UnifdefArgError::Getopt(crate::GetoptReport::UnknownOption(
                '\u{00ff}'
            ))),
            "短横后第一枚字节 ff 交回 U+00FF，不是折过的 U+FFFD"
        );
        assert_eq!(
            parse_unifdef_args(&[b"-i\xff"]).err(),
            Some(UnifdefArgError::Usage),
            "-i 选边吃的是第一枚字节，ff 既不是 D 也不是 U"
        );
    }
}
