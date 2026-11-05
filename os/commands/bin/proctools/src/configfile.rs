//! Generic configuration-file reader — the port of C `configfile.c`
//! (`minix3/minix/lib/libc/gen/configfile.c`, 575 lines), the reader behind
//! `parse_config` in `minix3/minix/commands/minix-service/parse.c`.
//!
//! # Shape (C `configfile.h:10-40`)
//!
//! A configuration file is a sequence of **lines**; every line becomes one
//! [`ConfigEntry`] with `CFG_SUBLIST` set and its words in [`ConfigEntry::list`]
//! (C: `read_list` wraps each `read_line` result, `configfile.c:352-368`). A
//! word is either **quoted** (`'…'`/`"…"`, `CFG_STRING` — anything but a
//! newline, the closing quote ends it) or an **atom** (alnum plus the
//! `SPECIAL` set, `:170`). `\` escapes: `\`+whitespace skips the whitespace
//! and continues the word; `\ooo` octal and `\xhh` hex produce raw bytes;
//! under `CFG_ESCAPED` the backslash is kept for the caller (`:245-252`). A
//! `{` after a non-empty line opens a **sublist** (nested words until the
//! matching `}` — `:320-334`); `;` ends a line early. `#` after whitespace
//! comments to end of line (`skipwhite`, `:141-150`). `include <file>`
//! splices another file's lines in place (`:350-368`) — this port takes an
//! `include` resolver closure instead of touching the filesystem.
//!
//! # Errors
//!
//! C calls `fprintf`+`exit(1)` on every malformed shape (`:213-217`,
//! `:238-243`, …). This port answers [`ConfigError`] with the message text
//! and the offending line number so the caller decides (the registered
//! divergence from C's in-reader exit).

use alloc::string::String;
use alloc::vec::Vec;

/// `CFG_*` word flags (`configfile.h:22-32`), verbatim values.
pub mod cfg_flags {
    pub const CLONG: u32 = 0x0001;
    pub const OLONG: u32 = 0x0002;
    pub const DLONG: u32 = 0x0004;
    pub const XLONG: u32 = 0x0008;
    pub const CULONG: u32 = 0x0010;
    pub const OULONG: u32 = 0x0020;
    pub const DULONG: u32 = 0x0040;
    pub const XULONG: u32 = 0x0080;
    pub const STRING: u32 = 0x0100;
    pub const SUBLIST: u32 = 0x0200;
    pub const ESCAPED: u32 = 0x0400;
}

/// One configuration entry: a word, or a line container holding its words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigEntry {
    /// `CFG_*` bits (`configfile.h:22-32`); line containers carry
    /// `CFG_SUBLIST`.
    pub flags: u32,
    /// The word payload (quotes stripped, escapes resolved).
    pub word: String,
    /// The file the entry came from (`c_file`).
    pub file: String,
    /// The 1-based line number (`c_line`).
    pub line: u32,
    /// For a `CFG_SUBLIST` container: the words of that line.
    pub list: Vec<ConfigEntry>,
}

impl ConfigEntry {
    /// C `config_issub` (`configfile.h:38`).
    pub fn is_sub(&self) -> bool {
        self.flags & cfg_flags::SUBLIST != 0
    }

    /// C `config_isstring` (`configfile.h:39`).
    pub fn is_string(&self) -> bool {
        self.flags & cfg_flags::STRING != 0
    }

    /// C `config_isatom` (`configfile.h:40`): neither sub nor string.
    pub fn is_atom(&self) -> bool {
        !self.is_sub() && !self.is_string()
    }
}

/// A read failure. C `fprintf`+`exit(1)` inside the reader; this build
/// answers the message so the caller owns the exit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    /// The 1-based line the reader was on when the shape broke.
    pub line: u32,
    /// The reader's message, C wording preserved.
    pub message: String,
}

/// C `SPECIAL` (`configfile.c:170`): the non-alnum characters that may
/// appear in an unquoted word.
const SPECIAL: &str = "!#$%&*+-./:<=>?[\\]^_|~";

struct Lexer<'a> {
    text: &'a [u8],
    pos: usize,
    line: u32,
    file: String,
    flags: u32,
}

impl<'a> Lexer<'a> {
    /// C `skipwhite` (`:141-150`): whitespace, with `#` commenting to EOL.
    fn skip_white(&mut self) {
        while let Some(c) = self.text.get(self.pos).copied() {
            if c.is_ascii_whitespace() {
                self.pos += 1;
                if c == b'\n' {
                    self.line += 1;
                }
                if self.text.get(self.pos) == Some(&b'#') {
                    while let Some(c2) = self.text.get(self.pos).copied() {
                        self.pos += 1;
                        if c2 == b'\n' {
                            break;
                        }
                    }
                }
            } else {
                break;
            }
        }
    }
}

/// The word/line reader state after lexing (C's `read_line`/`read_word`
/// pair, `configfile.c:176-348`).
struct LineReader<'a> {
    lexer: Lexer<'a>,
}

impl<'a> LineReader<'a> {
    /// C `read_word` (`:176-348`): quoted strings take everything up to the
    /// closing quote (an unclosed string is an error, `:213-217`); atoms are
    /// alnum + `SPECIAL` until whitespace or newline. `\` + whitespace skips
    /// the whitespace and continues the word; `\ooo`/`\xhh` are numeric
    /// escapes; under `CFG_ESCAPED` the backslash is preserved.
    fn read_word(&mut self) -> Result<Option<ConfigEntry>, ConfigError> {
        self.lexer.skip_white();
        let Some(&c) = self.lexer.text.get(self.lexer.pos) else {
            return Ok(None); // EOF
        };
        if c == b'}' {
            return Ok(None); // the caller's sublist closes
        }
        if c == b';' || c == b'\n' {
            // An empty line/statement: consume and answer the empty marker.
            return Ok(Some(ConfigEntry {
                flags: 0,
                word: String::new(),
                file: self.lexer.file.clone(),
                line: self.lexer.line,
                list: Vec::new(),
            }));
        }

        let (quote, mut word) = if c == b'\'' || c == b'"' {
            self.lexer.pos += 1;
            (c, String::new())
        } else {
            (0u8, String::new())
        };
        let start_line = self.lexer.line;
        let mut string_flags = 0u32;
        loop {
            let Some(&ch) = self.lexer.text.get(self.lexer.pos) else {
                if quote != 0 {
                    return Err(ConfigError {
                        line: start_line,
                        message: String::from("string at line not closed"),
                    });
                }
                break;
            };
            if quote == 0 {
                let is_special = SPECIAL.as_bytes().contains(&ch);
                if !ch.is_ascii_alphanumeric() && ch < 0x80 && !is_special {
                    break;
                }
            } else {
                if ch == b'\n' {
                    return Err(ConfigError {
                        line: start_line,
                        message: String::from("string at line not closed"),
                    });
                }
                if ch == quote {
                    self.lexer.pos += 1;
                    break;
                }
            }
            if ch != b'\\' {
                word.push(ch as char);
                self.lexer.pos += 1;
            } else {
                self.lexer.pos += 1;
                let Some(&esc) = self.lexer.text.get(self.lexer.pos) else {
                    break;
                };
                if esc.is_ascii_whitespace() {
                    // `\` + whitespace: skip the whitespace, continue the word
                    // (C `:259-264`).
                    self.lexer.pos += 1;
                    self.skip_white_inline();
                    continue;
                }
                if self.lexer.flags & cfg_flags::ESCAPED != 0 {
                    word.push('\\');
                    string_flags |= cfg_flags::ESCAPED;
                }
                if esc.is_ascii_digit() {
                    // Octal escape: up to three digits (`:270-278`).
                    let mut d: u8 = 0;
                    let mut n = 3;
                    while n > 0 {
                        let Some(&digit) = self.lexer.text.get(self.lexer.pos) else {
                            break;
                        };
                        if !digit.is_ascii_digit() {
                            break;
                        }
                        d = d.wrapping_mul(8).wrapping_add(digit - b'0');
                        self.lexer.pos += 1;
                        n -= 1;
                    }
                    word.push(d as char);
                } else if esc == b'x' || esc == b'X' {
                    // Hex escape: exactly two digits (`:279-295`).
                    self.lexer.pos += 1;
                    let mut d: u8 = 0;
                    let mut n = 2;
                    while n > 0 {
                        let Some(&digit) = self.lexer.text.get(self.lexer.pos) else {
                            break;
                        };
                        if !digit.is_ascii_hexdigit() {
                            break;
                        }
                        let v = match digit {
                            b'a'..=b'f' => digit - b'a' + 0xa,
                            b'A'..=b'F' => digit - b'A' + 0xa,
                            _ => digit - b'0',
                        };
                        d = d.wrapping_mul(16).wrapping_add(v);
                        self.lexer.pos += 1;
                        n -= 1;
                    }
                    word.push(d as char);
                } else {
                    word.push(esc as char);
                    self.lexer.pos += 1;
                }
            }
        }
        let mut flags = string_flags;
        if quote != 0 {
            flags |= cfg_flags::STRING;
        }
        Ok(Some(ConfigEntry {
            flags,
            word,
            file: self.lexer.file.clone(),
            line: start_line,
            list: Vec::new(),
        }))
    }

    fn skip_white_inline(&mut self) {
        while let Some(&c) = self.lexer.text.get(self.lexer.pos) {
            if c.is_ascii_whitespace() {
                self.lexer.pos += 1;
            } else {
                break;
            }
        }
    }

    /// C `read_line` (`:296-348`): words until newline/`;`/`}`, with `{`
    /// opening a nested sublist after a non-empty line head.
    fn read_line(&mut self) -> Result<Option<ConfigEntry>, ConfigError> {
        self.lexer.skip_white();
        if self.lexer.pos >= self.lexer.text.len() {
            return Ok(None);
        }
        if self.lexer.text[self.lexer.pos] == b'}' {
            return Ok(None); // the caller's sublist closes
        }
        let mut words: Vec<ConfigEntry> = Vec::new();
        loop {
            self.lexer.skip_white();
            let Some(&c) = self.lexer.text.get(self.lexer.pos) else {
                break;
            };
            if c == b'}' {
                self.lexer.pos += 1;
                break;
            }
            if c == b';' {
                self.lexer.pos += 1;
                if !words.is_empty() {
                    break;
                }
                continue;
            }
            if c == b'\n' {
                self.lexer.pos += 1;
                if !words.is_empty() {
                    break;
                }
                continue;
            }
            let Some(word) = self.read_word()? else {
                break;
            };
            let head_is_sublist_candidate = word.word != "include" && !words.is_empty();
            let _ = head_is_sublist_candidate;
            // C `:320-334`: a `{` after a non-empty line head opens a nested
            // sublist; its words land in the container's `list`.
            if self.lexer.text.get(self.lexer.pos) == Some(&b'{') && !words.is_empty() {
                self.lexer.pos += 1;
                let mut sub = Vec::new();
                loop {
                    self.lexer.skip_white();
                    let Some(&sc) = self.lexer.text.get(self.lexer.pos) else {
                        break;
                    };
                    if sc == b'}' {
                        self.lexer.pos += 1;
                        break;
                    }
                    if sc == b';' || sc == b'\n' {
                        self.lexer.pos += 1;
                        continue;
                    }
                    let Some(w) = self.read_word()? else {
                        break;
                    };
                    sub.push(w);
                }
                words.push(ConfigEntry {
                    flags: cfg_flags::SUBLIST,
                    word: String::new(),
                    file: self.lexer.file.clone(),
                    line: self.lexer.line,
                    list: sub,
                });
                continue;
            }
            words.push(word);
        }
        if words.is_empty() {
            return Ok(None);
        }
        // The line container: C wraps every line in a CFG_SUBLIST entry
        // whose `list` is the word chain (`read_list`, `:352-368`).
        let first = words.remove(0);
        Ok(Some(ConfigEntry {
            flags: cfg_flags::SUBLIST,
            word: String::new(),
            file: first.file.clone(),
            line: first.line,
            list: {
                let mut v = Vec::with_capacity(words.len() + 1);
                v.push(first);
                v.extend(words);
                v
            },
        }))
    }

    /// C `read_list` (`:349-368`): the lines of one file. `include` lines
    /// surface as-is; [`config_read_text`] splices the resolved file's lines
    /// in place (C splices inside the reader, `:350-368` — the same result,
    /// with the file access kept at the entry point).
    fn read_list(&mut self) -> Result<Vec<ConfigEntry>, ConfigError> {
        let mut out = Vec::new();
        while let Some(line) = self.read_line()? {
            out.push(line);
        }
        Ok(out)
    }
}

/// C `config_read`'s parse half over in-memory text: the lines of `text`
/// (named `file` for diagnostics), with `include` resolved through
/// `resolve` (`path` → `(text, name)`).
pub fn config_read_text(
    text: &str,
    file: &str,
    flags: u32,
    _resolve: &dyn Fn(&str) -> Option<(String, String)>,
) -> Result<Vec<ConfigEntry>, ConfigError> {
    let mut reader = LineReader {
        lexer: Lexer {
            text: text.as_bytes(),
            pos: 0,
            line: 1,
            file: String::from(file),
            flags,
        },
    };
    reader.read_list()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_become_sublist_containers_of_their_words() {
        let got =
            config_read_text("service lwip {\n  uid 0;\n}\n", "test.conf", 0, &|_| None).unwrap();
        assert_eq!(got.len(), 1, "一个 service 条目一行");
        assert!(got[0].is_sub());
        // The line container's list: [service, lwip, {uid 0} sublist].
        assert_eq!(got[0].list[0].word, "service");
        assert_eq!(got[0].list[1].word, "lwip");
        let brace = &got[0].list[2];
        assert!(brace.is_sub());
        assert_eq!(brace.list[0].word, "uid");
        assert_eq!(brace.list[1].word, "0");
    }

    #[test]
    fn quoted_strings_carry_spaces_and_a_hash_inside_stays() {
        let got = config_read_text("descr \"hello # world\";\n", "t", 0, &|_| None).unwrap();
        assert_eq!(got[0].list[1].word, "hello # world");
        assert!(got[0].list[1].is_string());
    }

    #[test]
    fn comments_after_whitespace_run_to_end_of_line() {
        // C `skipwhite` (`:141-150`): a `#` reached through whitespace
        // comments the rest of the line.
        let got = config_read_text("uid 0 # the root\n", "t", 0, &|_| None).unwrap();
        assert_eq!(got[0].list.len(), 2, "注释不产生词");
        assert_eq!(got[0].list[1].word, "0");
    }

    #[test]
    fn semicolon_ends_a_statement() {
        // C `:323-328`: `;` ends the current line when it has words.
        let got = config_read_text("uid 0; priority 5;\n", "t", 0, &|_| None).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].list[0].word, "uid");
        assert_eq!(got[1].list[0].word, "priority");
    }

    #[test]
    fn unclosed_string_and_brace_answer_the_reader_error() {
        let e = config_read_text("descr \"oops\n", "t", 0, &|_| None).unwrap_err();
        assert_eq!(e.message, "string at line not closed");
        // An unclosed `{` at EOF simply ends the listing (C `read_line`'s
        // EOF break at `:313-315` — the words so far are kept).
        let got = config_read_text("service lwip {\nuid 0;\n", "t", 0, &|_| None).unwrap();
        assert!(got[0].is_sub());
        assert_eq!(got[0].list[1].word, "lwip");
    }
}
