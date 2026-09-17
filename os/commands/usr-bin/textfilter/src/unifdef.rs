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
use alloc::vec::Vec;
use alloc::string::{String, ToString};
use alloc::format;

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
}

/// Parses `-Dsym[=val]` / `-Usym` (and the `-iD/-iU` ignore variants).
pub fn add_symbol(options: &mut Options, spec: &str, defined: bool, ignore: bool) -> Result<(), String> {
    let (name, value) = match spec.find('=') {
        Some(eq) => (&spec[..eq], Some(spec[eq + 1..].to_string())),
        None => (spec, None),
    };
    if name.is_empty()
        || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        || (!defined && value.is_some())
    {
        return Err(format!("bad symbol spec {}", spec));
    }
    for sym in options.symbols.iter_mut() {
        if sym.name == name {
            sym.value = if defined {
                Some(value.unwrap_or_default())
            } else {
                None
            };
            sym.ignore = ignore;
            return Ok(());
        }
    }
    options.symbols.push(Sym {
        name: name.to_string(),
        value: if defined { Some(value.unwrap_or_default()) } else { None },
        ignore,
    });
    Ok(())
}

/// Errors mirroring unifdef.c's `error()` exits (status 2).
#[derive(Debug, PartialEq)]
pub enum UnifdefError {
    EofInComment,
    InappropriateElif { line: usize, if_line: usize, depth: usize },
    InappropriateElse { line: usize },
    InappropriateEndif { line: usize },
    PrematureEof,
    ObfuscatedControlLine { line: usize },
    TooDeep { line: usize },
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
                    return Err(UnifdefError::InappropriateElif { line: l, if_line: d, depth: 0 });
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
                    return Err(UnifdefError::InappropriateElif { line: l, if_line: ifl, depth: d });
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
                    return Err(UnifdefError::InappropriateElif { line: l, if_line: ifl, depth: d });
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
                    return Err(UnifdefError::InappropriateElif { line: l, if_line: ifl, depth: d });
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
                        if v != 0 {
                            LT::True
                        } else {
                            LT::False
                        }
                    }
                    None => LT::If,
                };
            } else if keyword == "elif" {
                retval = match self.ifeval(b, cp) {
                    Some((v, end)) => {
                        cp = end;
                        if v != 0 {
                            LT::ElTrue
                        } else {
                            LT::ElFalse
                        }
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
            if retval != LT::Plain && (wascomment != CommentState::No || self.incomment != CommentState::No) {
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
        if self.keepthis {
            None
        } else {
            Some(result)
        }
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
                    if b[cp] == b'/' && cp + 2 < b.len() && b[cp + 1] == b'\\' && b[cp + 2] == b'\n' {
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
                    if b[cp] == b'*' && cp + 2 < b.len() && b[cp + 1] == b'\\' && b[cp + 2] == b'\n' {
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
        assert_eq!(String::from_utf8(out.output).unwrap(), "#if UNKNOWN > 2\nbody\n#endif\n");
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
        assert_eq!(String::from_utf8(out.output).unwrap(), "#ifdef FOO\n#endif\n");
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
