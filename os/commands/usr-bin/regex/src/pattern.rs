//! Pattern compilation (Thompson construction) and matching (Pike virtual
//! machine).
//!
//! Ground truth for the accepted spellings: the POSIX `regcomp` interface
//! the C tools program against (`minix3/minix/usr.bin/grep/grep.h:66`,
//! `util.c:205`, `grep.c:480`), with two spellings selected by flags:
//! basic (default, plus `-G`) and extended (`-E`). The compiler below
//! accepts the same two spellings.
//!
//! # Why a virtual machine instead of backtracking
//!
//! A naive backtracking matcher can take exponential time on patterns like
//! `(a*)*$`, hanging the command on ordinary input. The Thompson
//! construction plus Pike virtual machine used here runs in time
//! proportional to program size times input length: every input byte is
//! processed once, against a bounded set of active states. This is the
//! same structural choice Russ Cox's articles popularised, and the same
//! reason modern search tools favour automata over backtracking. Bounded
//! fixed size tables keep the whole engine heap free (`no_std`).
//!
//! Two deliberate simplifications are documented, not hidden:
//!
//! - A leading repetition operator (`*`, `+`, `?` with no atom before it)
//!   is read as a literal character, matching long standing Unix practice.
//! - `$` matches only at the very end of the given text, not before a
//!   trailing newline. The command layer feeds the engine one line at a
//!   time without its newline, so the difference never surfaces there.
//! - Numeric back references (`\1` to `\9`) inside patterns are rejected at
//!   compile time. They are not regular: supporting them would force the
//!   engine back into backtracking and reintroduce exponential behaviour.
//!   Back references in `sed` *replacements* (which only read already
//!   captured spans) work fully. POSIX character classes (`[:alpha:]` and
//!   friends) are a second documented follow-up; ranges (`a-z`), negation
//!   (`[^...]`), and backslash escapes inside classes work.

use crate::RegexError;

/// Maximum instructions per compiled program.
pub const MAX_INSTS: usize = 192;
/// Maximum character classes per pattern.
pub const MAX_CLASSES: usize = 16;
/// Capture slots: slot 0 is the whole match, slots 1 to 9 are groups.
pub const MAX_GROUPS: usize = 10;
/// Maximum simultaneously active threads in the virtual machine.
pub const MAX_THREADS: usize = 128;

/// One byte range inside a character class; a single byte is `(b, b)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    /// First byte of the range, inclusive.
    pub low: u8,
    /// Last byte of the range, inclusive.
    pub high: u8,
}

/// A `[...]` character class: up to 12 ranges plus optional negation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharClass {
    /// Ranges that (when `negated` is false) make up the class.
    pub ranges: [ByteRange; 12],
    /// How many of `ranges` are used.
    pub range_count: u8,
    /// True for `[^...]`: every byte outside the ranges matches.
    pub negated: bool,
}

impl CharClass {
    /// Decide whether `byte` is a member of the class.
    pub fn contains(self, byte: u8) -> bool {
        let mut inside = false;
        for range in self.ranges[..self.range_count as usize].iter() {
            if range.low <= byte && byte <= range.high {
                inside = true;
                break;
            }
        }
        if self.negated {
            !inside
        } else {
            inside
        }
    }
}

/// One virtual machine instruction.
///
/// Every fragment built by the compiler ends in exactly one open jump
/// whose target is filled in when the fragment's successor is known, so
/// concatenation and alternation never rewrite already emitted code.
/// Save and assertion instructions are always emitted immediately before
/// their closing jump, so the machine continues at the next slot after
/// executing them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Inst {
    /// Array filler; kills any thread that reaches it (never emitted).
    Empty,
    /// Match one exact byte, then continue at the next slot.
    Byte(u8),
    /// Match any single byte except newline, then continue.
    Any,
    /// Match one byte from a class (index into the class table).
    Class(u8),
    /// Epsilon fork: enter `x` first (priority), then `y`.
    Split(u8, u8),
    /// Epsilon jump to the target.
    Jump(u8),
    /// Record the current position for a group boundary, then continue.
    /// Even ids open a group (`2 * number`), odd ids close it.
    Save(u8),
    /// Succeed only at the start of the text, then continue.
    AssertStart,
    /// Succeed only at the end of the text, then continue.
    AssertEnd,
    /// Accept: the input matched.
    Match,
}

/// A compiled pattern: instruction program plus class table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    insts: [Inst; MAX_INSTS],
    inst_count: u8,
    classes: [CharClass; MAX_CLASSES],
    class_count: u8,
    start: u8,
    /// How many numbered groups the pattern holds (0 to 9).
    pub group_count: u8,
    /// True for the extended spelling, false for the basic spelling.
    pub extended: bool,
}

impl Default for Pattern {
    fn default() -> Self {
        Pattern {
            insts: [Inst::Empty; MAX_INSTS],
            inst_count: 0,
            classes: [
                CharClass {
                    ranges: [ByteRange { low: 0, high: 0 }; 12],
                    range_count: 0,
                    negated: false,
                };
                MAX_CLASSES
            ],
            class_count: 0,
            start: 0,
            group_count: 0,
            extended: false,
        }
    }
}

/// Captured spans: slot 0 is the whole match, slots 1 to 9 are groups.
/// Positions are byte offsets into the searched text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Captures {
    /// Start and end offset per slot; `None` means the group did not
    /// participate in the match.
    pub spans: [Option<(u32, u32)>; MAX_GROUPS],
}

impl Captures {
    /// An empty capture set (nothing captured yet).
    pub fn empty() -> Self {
        Captures {
            spans: [None; MAX_GROUPS],
        }
    }
}

/// Compile `text` with the basic spelling (default and `-G`).
pub fn compile_basic(text: &str) -> Result<Pattern, RegexError> {
    compile(text, false)
}

/// Compile `text` with the extended spelling (`-E`).
pub fn compile_extended(text: &str) -> Result<Pattern, RegexError> {
    compile(text, true)
}

// ---------------------------------------------------------------------------
// Compiler.
// ---------------------------------------------------------------------------

/// A partially built program piece: entry instruction plus the one open
/// jump waiting for a successor.
struct Frag {
    start: u8,
    open: u8,
}

struct Compiler<'a> {
    bytes: &'a [u8],
    pos: usize,
    pattern: Pattern,
    /// How many groups are currently open: `\)` closes one only above zero.
    group_depth: u8,
    /// The atom just read, when it is a single instruction that `{m,n}` can
    /// repeat in place (a byte, `.` or a class). Anything assembled from several
    /// instructions — a group — is `None`: repeating that needs a copy of the
    /// subgraph that shares the original's capture slots, and this compiler
    /// numbers slots monotonically. Those spellings are refused rather than
    /// quietly read as ordinary text.
    repeatable: Option<Inst>,
}

fn compile(text: &str, extended: bool) -> Result<Pattern, RegexError> {
    if text.is_empty() {
        return Err(RegexError::InvalidPattern);
    }
    let mut compiler = Compiler {
        bytes: text.as_bytes(),
        pos: 0,
        pattern: Pattern::default(),
        group_depth: 0,
        repeatable: None,
    };
    compiler.pattern.extended = extended;
    let frag = compiler.parse_alt()?;
    if compiler.pos != compiler.bytes.len() {
        return Err(RegexError::InvalidPattern);
    }
    let accept = compiler.emit(Inst::Match)?;
    compiler.close(frag.open, accept);
    compiler.pattern.start = frag.start;
    Ok(compiler.pattern)
}

impl Compiler<'_> {
    fn emit(&mut self, inst: Inst) -> Result<u8, RegexError> {
        if self.pattern.inst_count as usize >= MAX_INSTS {
            return Err(RegexError::TooComplex);
        }
        let index = self.pattern.inst_count;
        self.pattern.insts[index as usize] = inst;
        self.pattern.inst_count += 1;
        Ok(index)
    }

    /// Point the fragment's open jump at `target`.
    fn close(&mut self, open: u8, target: u8) {
        debug_assert_eq!(self.pattern.insts[open as usize], Inst::Jump(255));
        self.pattern.insts[open as usize] = Inst::Jump(target);
    }

    /// A consuming or boundary instruction followed by its open jump.
    fn atom(&mut self, inst: Inst) -> Result<Frag, RegexError> {
        let start = self.emit(inst)?;
        let open = self.emit(Inst::Jump(255))?;
        Ok(Frag { start, open })
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn next_byte(&mut self) -> Option<u8> {
        let byte = self.bytes.get(self.pos).copied()?;
        self.pos += 1;
        Some(byte)
    }

    fn parse_alt(&mut self) -> Result<Frag, RegexError> {
        let mut left = self.parse_concat()?;
        while self.pattern.extended && self.peek() == Some(b'|') {
            self.pos += 1;
            let right = self.parse_concat()?;
            let split = self.emit(Inst::Split(left.start, right.start))?;
            let open = self.emit(Inst::Jump(255))?;
            self.close(left.open, open);
            self.close(right.open, open);
            left = Frag { start: split, open };
        }
        Ok(left)
    }

    fn parse_concat(&mut self) -> Result<Frag, RegexError> {
        let mut head: Option<Frag> = None;
        while let Some(frag) = self.parse_quantified()? {
            head = Some(match head {
                Some(left) => {
                    self.close(left.open, frag.start);
                    Frag {
                        start: left.start,
                        open: frag.open,
                    }
                }
                None => frag,
            });
        }
        head.map_or_else(
            || {
                // Empty branch (empty alternative side): a jump filled in
                // by whoever follows.
                let open = self.emit(Inst::Jump(255))?;
                Ok(Frag { start: open, open })
            },
            Ok,
        )
    }

    fn parse_quantified(&mut self) -> Result<Option<Frag>, RegexError> {
        let mut frag = match self.parse_atom()? {
            Some(frag) => frag,
            None => return Ok(None),
        };
        // Only a single-instruction atom can be copied by the `{m,n}` expansion
        // below, and `frag.start` is exactly that instruction when it is one.
        self.repeatable = match self.pattern.insts[usize::from(frag.start)] {
            inst @ (Inst::Byte(_) | Inst::Any | Inst::Class(_)) => Some(inst),
            _ => None,
        };
        loop {
            match self.peek() {
                Some(b'*') => {
                    self.pos += 1;
                    frag = self.star(frag)?;
                    self.repeatable = None;
                }
                Some(b'+') if self.pattern.extended => {
                    self.pos += 1;
                    frag = self.plus(frag)?;
                    self.repeatable = None;
                }
                Some(b'?') if self.pattern.extended => {
                    self.pos += 1;
                    frag = self.quest(frag)?;
                    self.repeatable = None;
                }
                // The interval's opener also decides its closer: `{m,n}` in the
                // extended dialect (`regcomp.c:512-529`), `\{m,n\}` in the basic
                // one (`:705-721`). Both need a digit after the brace — a `{`
                // followed by anything else stays ordinary text there
                // (`regcomp.c:473`) — and both are read only where a repetition
                // operator may stand, i.e. right after an atom.
                Some(b'{')
                    if self.pattern.extended
                        && matches!(self.bytes.get(self.pos + 1), Some(&digit) if digit.is_ascii_digit()) =>
                {
                    self.pos += 1;
                    let (least, greatest) = self.parse_bounds(false)?;
                    frag = self.expand(least, greatest)?;
                }
                Some(b'\\')
                    if !self.pattern.extended && self.bytes.get(self.pos + 1) == Some(&b'{') =>
                {
                    self.pos += 2;
                    let (least, greatest) = self.parse_bounds(true)?;
                    frag = self.expand(least, greatest)?;
                }
                _ => break,
            }
        }
        Ok(Some(frag))
    }

    /// Read the counts and the closer of an interval, after its opener was eaten.
    ///
    /// C's `p_count` takes the digit run and the two shapes after it: a plain
    /// `{m}` repeats exactly `m` times, `{m,}` is open-ended, and `{m,n}`
    /// requires `m <= n` (`regcomp.c:513-521`, and the basic dialect's
    /// `:706-714` is the same with a `\}` closer). A missing closer or a
    /// reversed range is malformed; C's own recovery scans ahead for the
    /// closer just to report the failure, which this compiler has no reason to
    /// imitate since both outcomes are one refusal.
    fn parse_bounds(&mut self, escaped: bool) -> Result<(u32, Option<u32>), RegexError> {
        let least = self.parse_count()?;
        let greatest = if self.peek() == Some(b',') {
            self.pos += 1;
            if matches!(self.peek(), Some(digit) if digit.is_ascii_digit()) {
                let high = self.parse_count()?;
                if high < least {
                    return Err(RegexError::InvalidPattern);
                }
                Some(high)
            } else {
                None
            }
        } else {
            Some(least)
        };
        if escaped {
            if self.next_byte() != Some(b'\\') || self.next_byte() != Some(b'}') {
                return Err(RegexError::InvalidPattern);
            }
        } else if self.next_byte() != Some(b'}') {
            return Err(RegexError::InvalidPattern);
        }
        Ok((least, greatest))
    }

    /// A digit run, as C's `p_count` reads it (`regcomp.c:733-760`).
    fn parse_count(&mut self) -> Result<u32, RegexError> {
        let mut count: u32 = 0;
        let mut digits = 0u32;
        while let Some(&digit @ b'0'..=b'9') = self.bytes.get(self.pos) {
            self.pos += 1;
            digits += 1;
            count = count
                .checked_mul(10)
                .and_then(|value| value.checked_add(u32::from(digit - b'0')))
                .ok_or(RegexError::InvalidPattern)?;
        }
        if digits == 0 {
            return Err(RegexError::InvalidPattern);
        }
        Ok(count)
    }

    /// `x{m,n}` for a single-instruction atom.
    ///
    /// C hands the atom's code range to `repeat()` (`regcomp.c:522`, `:716`) and
    /// its engine counts passes while matching. This compiler has no counting
    /// state, so the same language is assembled from the pieces it does have:
    /// `m` mandatory copies, then one `quest`-wrapped copy per remaining
    /// repetition — or a `star` when the upper bound is open. The greedy order is
    /// preserved because `quest` and `star` both try their body first, and a
    /// pattern beyond the instruction array simply runs into `TooComplex`, the
    /// same way an over-long literal does.
    fn expand(&mut self, least: u32, greatest: Option<u32>) -> Result<Frag, RegexError> {
        let inst = self.repeatable.ok_or(RegexError::TooComplex)?;
        // One repetition operator per atom: C's parser reports `REG_BADRPT` when a
        // second one follows a quantified atom (`regcomp.c:531-537`), so the
        // expandable slot is spent by this operator like the others spend it.
        self.repeatable = None;
        let mut head: Option<Frag> = None;
        for _ in 0..least {
            let copy = self.atom(inst)?;
            head = Some(self.append(head, copy)?);
        }
        match greatest {
            // `{m,}`: the tail is an unbounded repeat of one more copy.
            None => {
                let copy = self.atom(inst)?;
                let around = self.star(copy)?;
                head = Some(self.append(head, around)?);
            }
            // `{m,n}`: `n - m` further copies, each optional on its own, which is
            // the same `x?x?x?` chain C's counted loop admits and rejects.
            Some(high) => {
                for _ in least..high {
                    let copy = self.atom(inst)?;
                    let piece = self.quest(copy)?;
                    head = Some(self.append(head, piece)?);
                }
            }
        }
        Ok(match head {
            Some(built) => built,
            // `{0}` and `{0,0}` contribute no copy at all, which is what C's
            // `repeat()` does too: the atom disappears and the line matches
            // around it. A bare open jump is the same placeholder an empty
            // alternative gets, so whoever follows simply wires past it.
            None => {
                let jump = self.emit(Inst::Jump(255))?;
                Frag {
                    start: jump,
                    open: jump,
                }
            }
        })
    }

    /// Append `right` after the piece built so far, patching the open jump.
    /// `None` on the left means this is the first copy, so the result is just
    /// `right`.
    fn append(&mut self, left: Option<Frag>, right: Frag) -> Result<Frag, RegexError> {
        Ok(match left {
            Some(prev) => {
                self.close(prev.open, right.start);
                Frag {
                    start: prev.start,
                    open: right.open,
                }
            }
            None => right,
        })
    }

    /// `body*`: split before the body looping back, exit past it.
    fn star(&mut self, body: Frag) -> Result<Frag, RegexError> {
        let exit = self.emit(Inst::Jump(255))?;
        let split = self.emit(Inst::Split(body.start, exit))?;
        self.close(body.open, split);
        Ok(Frag { start: split, open: exit })
    }

    /// `body+`: one mandatory pass, then the star loop over the same body.
    fn plus(&mut self, body: Frag) -> Result<Frag, RegexError> {
        let exit = self.emit(Inst::Jump(255))?;
        let split = self.emit(Inst::Split(body.start, exit))?;
        self.close(body.open, split);
        Ok(Frag {
            start: body.start,
            open: exit,
        })
    }

    /// `body?`: split to the body or past it.
    fn quest(&mut self, body: Frag) -> Result<Frag, RegexError> {
        let exit = self.emit(Inst::Jump(255))?;
        let split = self.emit(Inst::Split(body.start, exit))?;
        self.close(body.open, exit);
        Ok(Frag { start: split, open: exit })
    }

    fn parse_atom(&mut self) -> Result<Option<Frag>, RegexError> {
        let byte = match self.peek() {
            None => return Ok(None),
            Some(byte) => byte,
        };
        // Only a character that can actually end the current run stops the scan.
        //
        // In the basic dialect groups open with `\(` and close with `\)`:
        // `regcomp.c:570-597`'s loop stops on `SEETWO(end1, end2)`, which for a
        // group body is the **two-byte** pair `\` `)`, and the atom switch
        // (`regcomp.c:628-690`) has no case for a bare `)` — it falls to
        // `default: ordinary(...)`. So an unescaped `)` is ordinary text at any
        // depth there, and the closer itself is consumed by the escape path
        // below.
        //
        // In the extended dialect a bare `)` closes a group. Outside any group it
        // is still ordinary, because Minix builds this library with
        // `-DPOSIX_MISTAKE` (`minix3/lib/libc/regex/Makefile.inc:7`), which
        // compiles out `regcomp.c:428-438`'s "unmatched )" error.
        if byte == b')' && self.pattern.extended && self.group_depth > 0 {
            return Ok(None);
        }
        if byte == b'|' && self.pattern.extended {
            // `p_ere` breaks its concatenation loop on `|` at every depth
            // (`regcomp.c:361`), including inside a group.
            return Ok(None);
        }
        if byte == b'*' || (self.pattern.extended && (byte == b'+' || byte == b'?')) {
            // Leading repetition operator: literal, per long standing
            // Unix practice.
            self.pos += 1;
            return Ok(Some(self.atom(Inst::Byte(byte))?));
        }
        match byte {
            b'^' => {
                self.pos += 1;
                Ok(Some(self.atom(Inst::AssertStart)?))
            }
            b'$' => {
                self.pos += 1;
                Ok(Some(self.atom(Inst::AssertEnd)?))
            }
            b'.' => {
                self.pos += 1;
                Ok(Some(self.atom(Inst::Any)?))
            }
            b'[' => {
                let class = self.parse_class()?;
                Ok(Some(self.atom(Inst::Class(class))?))
            }
            b'(' if self.pattern.extended => {
                self.pos += 1;
                self.parse_group_body(false).map(Some)
            }
            b'\\' => self.parse_escape(),
            _ => {
                self.pos += 1;
                Ok(Some(self.atom(Inst::Byte(byte))?))
            }
        }
    }

    fn parse_group_body(&mut self, basic: bool) -> Result<Frag, RegexError> {
        if basic {
            // The basic spelling opens groups with `\(`; consume both
            // bytes here (the extended path consumes `(` in `parse_atom`).
            if self.next_byte() != Some(b'\\') || self.next_byte() != Some(b'(') {
                return Err(RegexError::InvalidPattern);
            }
        }
        if self.pattern.group_count >= 9 {
            return Err(RegexError::TooComplex);
        }
        self.pattern.group_count += 1;
        let slot = self.pattern.group_count;
        let enter = self.atom(Inst::Save(slot * 2))?;
        self.group_depth += 1;
        let body = self.parse_alt()?;
        self.group_depth -= 1;
        let expect_close = if basic {
            self.next_byte() == Some(b'\\') && self.next_byte() == Some(b')')
        } else {
            self.next_byte() == Some(b')')
        };
        if !expect_close {
            return Err(RegexError::InvalidPattern);
        }
        let exit = self.atom(Inst::Save(slot * 2 + 1))?;
        self.close(enter.open, body.start);
        self.close(body.open, exit.start);
        Ok(Frag {
            start: enter.start,
            open: exit.open,
        })
    }

    fn parse_escape(&mut self) -> Result<Option<Frag>, RegexError> {
        debug_assert_eq!(self.bytes[self.pos], b'\\');
        self.pos += 1;
        let byte = self.next_byte().ok_or(RegexError::InvalidPattern)?;
        if !self.pattern.extended {
            match byte {
                b'(' => {
                    // Rewind onto the backslash: the body parser consumes
                    // the two byte opener itself.
                    self.pos -= 2;
                    return self.parse_group_body(true).map(Some);
                }
                b')' if self.group_depth > 0 => {
                    // Group closer: rewind so the body parser sees `\)`.
                    self.pos -= 2;
                    return Ok(None);
                }
                b'1'..=b'9' => {
                    // Numeric back references are rejected: they are not
                    // regular and would force the engine back into
                    // backtracking. See the module documentation.
                    return Err(RegexError::InvalidPattern);
                }
                _ => {}
            }
        }
        self.atom(Inst::Byte(byte)).map(Some)
    }

    fn parse_class(&mut self) -> Result<u8, RegexError> {
        debug_assert_eq!(self.bytes[self.pos], b'[');
        self.pos += 1;
        let mut class = CharClass {
            ranges: [ByteRange { low: 0, high: 0 }; 12],
            range_count: 0,
            negated: false,
        };
        if self.peek() == Some(b'^') {
            self.pos += 1;
            class.negated = true;
        }
        let mut first = true;
        loop {
            let byte = self.next_byte().ok_or(RegexError::InvalidPattern)?;
            if byte == b']' && !first {
                break;
            }
            first = false;
            let low = if byte == b'\\' {
                self.next_byte().ok_or(RegexError::InvalidPattern)?
            } else {
                byte
            };
            if self.peek() == Some(b'-') && self.bytes.get(self.pos + 1) != Some(&b']') {
                self.pos += 1;
                let high = match self.next_byte() {
                    Some(b'\\') => self.next_byte().ok_or(RegexError::InvalidPattern)?,
                    Some(byte) => byte,
                    None => return Err(RegexError::InvalidPattern),
                };
                if high < low {
                    return Err(RegexError::InvalidPattern);
                }
                Self::push_range(&mut class, low, high)?;
            } else {
                Self::push_range(&mut class, low, low)?;
                // A dash that cannot open a range is a literal member.
                if self.peek() == Some(b'-') {
                    self.pos += 1;
                    Self::push_range(&mut class, b'-', b'-')?;
                }
            }
        }
        if class.range_count == 0 {
            return Err(RegexError::InvalidPattern);
        }
        if self.pattern.class_count as usize >= MAX_CLASSES {
            return Err(RegexError::TooComplex);
        }
        let index = self.pattern.class_count;
        self.pattern.classes[index as usize] = class;
        self.pattern.class_count += 1;
        Ok(index)
    }

    fn push_range(class: &mut CharClass, low: u8, high: u8) -> Result<(), RegexError> {
        if class.range_count as usize >= class.ranges.len() {
            return Err(RegexError::TooComplex);
        }
        class.ranges[class.range_count as usize] = ByteRange { low, high };
        class.range_count += 1;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Virtual machine.
// ---------------------------------------------------------------------------

/// One live path through the program: instruction plus the captures
/// gathered along this path. The input position is carried by the driver
/// loop, not by the thread: every thread in one list shares it.
#[derive(Debug, Clone, Copy)]
struct Thread {
    pc: u8,
    caps: Captures,
}

impl Pattern {
    /// Leftmost match anywhere in `text` (longest end at that start), or
    /// `None`. Captures describe the winning path.
    pub fn find(&self, text: &str) -> Option<(usize, usize, Captures)> {
        self.find_bytes(text.as_bytes(), 0)
    }

    /// Byte based leftmost match at or after `from`, or `None`. Positions
    /// are always byte valid (no string slicing inside), so command layers
    /// walking a line match by match cannot panic on multibyte text.
    pub fn find_bytes(
        &self,
        bytes: &[u8],
        from: usize,
    ) -> Option<(usize, usize, Captures)> {
        let mut start = from.min(bytes.len());
        while start <= bytes.len() {
            if let Some((end, caps)) = self.anchored_longest(bytes, start) {
                return Some((start, end, caps));
            }
            start += char_len_at(bytes, start);
        }
        None
    }

    /// Leftmost match whose neighbours are not word bytes — BSD grep `-w`
    /// (`minix3/minix/usr.bin/grep/grep.c:85`, applied at `grep.c:186`). Walks
    /// match-by-match (advancing past each non-word-delimited match's start)
    /// so a later word-delimited occurrence still selects the line. Positions
    /// are byte offsets into `text`; byte-slicing avoids multibyte panics.
    pub fn find_word(&self, text: &str) -> Option<(usize, usize)> {
        let bytes = text.as_bytes();
        let mut from = 0usize;
        loop {
            let (start, end, _) = self.find_bytes(bytes, from)?;
            let left_ok = start == 0 || !crate::matcher::is_word_byte(bytes[start - 1]);
            let right_ok = end >= bytes.len() || !crate::matcher::is_word_byte(bytes[end]);
            if left_ok && right_ok {
                return Some((start, end));
            }
            // Non-word-boundary hit: step past this match's first
            // character and look for the next candidate (`from` strictly
            // increases, so the loop terminates).
            from = start + char_len_at(bytes, start);
        }
    }

    /// True when the pattern matches anywhere in `text`. Single pass: the
    /// start state joins the active set at every position, so the first
    /// accept found anywhere ends the search.
    pub fn is_match(&self, text: &str) -> bool {
        let bytes = text.as_bytes();
        let mut current = [Thread {
            pc: 0,
            caps: Captures::empty(),
        }; MAX_THREADS];
        let mut next = current;
        let mut current_len = 0;
        let mut next_len = 0;
        let mut visited = [0u64; MAX_INSTS];
        let mut pos = 0;
        Self::add_start(
            self,
            &mut current,
            &mut current_len,
            pos,
            &mut visited,
            1,
            bytes.len(),
        );
        loop {
            for thread in current[..current_len].iter() {
                if self.insts[thread.pc as usize] == Inst::Match {
                    return true;
                }
            }
            if pos >= bytes.len() {
                return false;
            }
            let byte = bytes[pos];
            for thread in current[..current_len].iter() {
                match self.insts[thread.pc as usize] {
                    Inst::Byte(want) if want == byte => {
                        Self::add_thread(
                            self,
                            &mut next,
                            &mut next_len,
                            thread.pc + 1,
                            pos as u32 + 1,
                            thread.caps,
                            &mut visited,
                            pos as u64 * 2 + 3,
                            bytes.len(),
                        );
                    }
                    Inst::Any if byte != b'\n' => {
                        Self::add_thread(
                            self,
                            &mut next,
                            &mut next_len,
                            thread.pc + 1,
                            pos as u32 + 1,
                            thread.caps,
                            &mut visited,
                            pos as u64 * 2 + 3,
                            bytes.len(),
                        );
                    }
                    Inst::Class(index)
                        if self.classes[index as usize].contains(byte) =>
                    {
                        Self::add_thread(
                            self,
                            &mut next,
                            &mut next_len,
                            thread.pc + 1,
                            pos as u32 + 1,
                            thread.caps,
                            &mut visited,
                            pos as u64 * 2 + 3,
                            bytes.len(),
                        );
                    }
                    _ => {}
                }
            }
            pos += 1;
            core::mem::swap(&mut current, &mut next);
            current_len = next_len;
            next_len = 0;
            Self::add_start(
            self,
            &mut current,
            &mut current_len,
            pos,
            &mut visited,
            pos as u64 * 2 + 2,
            bytes.len(),
        );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn add_start(
        pattern: &Pattern,
        list: &mut [Thread; MAX_THREADS],
        len: &mut usize,
        pos: usize,
        visited: &mut [u64; MAX_INSTS],
        stamp: u64,
        text_len: usize,
    ) {
        Self::add_thread(
            pattern,
            list,
            len,
            pattern.start,
            pos as u32,
            Captures::empty(),
            visited,
            stamp,
            text_len,
        );
    }

    /// True when the pattern matches the whole of `text`.
    pub fn is_full_match(&self, text: &str) -> bool {
        let bytes = text.as_bytes();
        match self.anchored_longest(bytes, 0) {
            Some((end, _)) => end == bytes.len(),
            None => false,
        }
    }

    /// Longest match starting exactly at `start`, or `None`.
    fn anchored_longest(&self, bytes: &[u8], start: usize) -> Option<(usize, Captures)> {
        let mut current = [Thread {
            pc: 0,
            caps: Captures::empty(),
        }; MAX_THREADS];
        let mut next = current;
        let mut current_len = 0;
        let mut next_len = 0;
        let mut visited = [0u64; MAX_INSTS];
        let mut best: Option<(usize, Captures)> = None;
        let mut pos = start;
        Self::add_thread(
            self,
            &mut current,
            &mut current_len,
            self.start,
            pos as u32,
            Captures::empty(),
            &mut visited,
            1,
            bytes.len(),
        );
        loop {
            for thread in current[..current_len].iter() {
                if self.insts[thread.pc as usize] == Inst::Match {
                    // Threads are priority ordered, but every accept at
                    // this position is recorded and the longest end wins:
                    // leftmost start, longest end (POSIX leaning).
                    best = Some((pos, thread.caps));
                    break;
                }
            }
            if pos >= bytes.len() {
                break;
            }
            let byte = bytes[pos];
            for thread in current[..current_len].iter() {
                match self.insts[thread.pc as usize] {
                    Inst::Byte(want) if want == byte => {
                        Self::add_thread(
                            self,
                            &mut next,
                            &mut next_len,
                            thread.pc + 1,
                            pos as u32 + 1,
                            thread.caps,
                            &mut visited,
                            pos as u64 * 2 + 3,
                            bytes.len(),
                        );
                    }
                    Inst::Any if byte != b'\n' => {
                        Self::add_thread(
                            self,
                            &mut next,
                            &mut next_len,
                            thread.pc + 1,
                            pos as u32 + 1,
                            thread.caps,
                            &mut visited,
                            pos as u64 * 2 + 3,
                            bytes.len(),
                        );
                    }
                    Inst::Class(index)
                        if self.classes[index as usize].contains(byte) =>
                    {
                        Self::add_thread(
                            self,
                            &mut next,
                            &mut next_len,
                            thread.pc + 1,
                            pos as u32 + 1,
                            thread.caps,
                            &mut visited,
                            pos as u64 * 2 + 3,
                            bytes.len(),
                        );
                    }
                    _ => {}
                }
            }
            pos += 1;
            core::mem::swap(&mut current, &mut next);
            current_len = next_len;
            next_len = 0;
        }
        // Stamp the whole match span onto slot 0 for the winner.
        best.map(|(end, mut caps)| {
            caps.spans[0] = Some((start as u32, end as u32));
            (end, caps)
        })
    }

    /// Epsilon closure: follow jumps, splits (priority first), saves, and
    /// assertions from `pc`, appending consuming instructions (and `Match`)
    /// to the thread list. The first thread to reach an instruction wins,
    /// which preserves program priority order. `text_len` answers the end
    /// of text assertion on the spot.
    #[allow(clippy::too_many_arguments)]
    fn add_thread(
        pattern: &Pattern,
        list: &mut [Thread; MAX_THREADS],
        len: &mut usize,
        mut pc: u8,
        pos: u32,
        mut caps: Captures,
        visited: &mut [u64; MAX_INSTS],
        stamp: u64,
        text_len: usize,
    ) {
        let mut stack = [0u8; 64];
        stack[0] = pc;
        let mut top = 1;
        while top > 0 {
            top -= 1;
            pc = stack[top];
            if visited[pc as usize] == stamp {
                continue;
            }
            visited[pc as usize] = stamp;
            match pattern.insts[pc as usize] {
                Inst::Empty => {}
                Inst::Jump(target) => {
                    if top < stack.len() {
                        stack[top] = target;
                        top += 1;
                    }
                }
                Inst::Split(first, second) => {
                    if top + 1 < stack.len() {
                        // Push the fallback second so the priority branch
                        // is processed first.
                        stack[top] = second;
                        stack[top + 1] = first;
                        top += 2;
                    }
                }
                Inst::Save(slot) => {
                    let group = (slot / 2) as usize;
                    if slot % 2 == 0 {
                        // Group entry: the latest pass wins, so repeated
                        // groups report their last iteration.
                        caps.spans[group] = Some((pos, pos));
                    } else {
                        let (start, _) = caps.spans[group].unwrap_or((pos, pos));
                        caps.spans[group] = Some((start, pos));
                    }
                    if top < stack.len() {
                        stack[top] = pc + 1;
                        top += 1;
                    }
                }
                Inst::AssertStart => {
                    if pos == 0 && top < stack.len() {
                        stack[top] = pc + 1;
                        top += 1;
                    }
                }
                Inst::AssertEnd => {
                    if pos as usize == text_len && top < stack.len() {
                        stack[top] = pc + 1;
                        top += 1;
                    }
                }
                consuming => {
                    if *len < list.len() {
                        list[*len] = Thread { pc, caps };
                        *len += 1;
                    }
                    let _ = consuming;
                }
            }
        }
    }
}

/// Length in bytes of the character starting at `pos` (multibyte sequence
/// leads counted, 1 past the end or on continuation bytes).
fn char_len_at(bytes: &[u8], pos: usize) -> usize {
    if pos >= bytes.len() {
        return 1;
    }
    let lead = bytes[pos];
    if lead < 0x80 {
        1
    } else if lead >> 5 == 0b110 {
        2.min(bytes.len() - pos)
    } else if lead >> 4 == 0b1110 {
        3.min(bytes.len() - pos)
    } else if lead >> 3 == 0b11110 {
        4.min(bytes.len() - pos)
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_literal_find() {
        let pattern = compile_basic("hello").unwrap();
        assert_eq!(
            pattern.find("say hello there").map(|(s, e, _)| (s, e)),
            Some((4, 9))
        );
        assert_eq!(pattern.find("nothing here"), None);
    }

    #[test]
    fn test_dot_and_star() {
        let pattern = compile_basic("a.c").unwrap();
        assert!(pattern.is_match("abc"));
        assert!(!pattern.is_match("ac"));
        let star = compile_basic("ab*c").unwrap();
        assert!(star.is_match("ac"));
        assert!(star.is_match("abbbbc"));
        // Greedy backtrack across the star: the classic case a two pass
        // matcher gets wrong.
        let back = compile_basic("a*aab").unwrap();
        assert!(back.is_match("aaab"));
    }

    #[test]
    fn test_anchors() {
        let pattern = compile_basic("^hello$").unwrap();
        assert!(pattern.is_full_match("hello"));
        assert!(!pattern.is_full_match("hello!"));
        assert_eq!(
            pattern.find("hello").map(|(s, e, _)| (s, e)),
            Some((0, 5))
        );
        assert!(!compile_basic("^hello$").unwrap().is_match("say hello"));
    }

    #[test]
    fn test_class_and_negation() {
        // `+` is literal in the basic spelling.
        let literal_plus = compile_basic("[a-z]+").unwrap();
        assert!(literal_plus.is_match("x+"));
        assert!(!literal_plus.is_match("xyz"));
        let class = compile_basic("[a-z][0-9]").unwrap();
        assert!(class.is_match("x7"));
        assert!(!class.is_match("X7"));
        let negated = compile_basic("[^0-9][^0-9]*").unwrap();
        assert!(negated.is_match("abc"));
    }

    #[test]
    fn test_extended_alt_plus_quest() {
        let pattern = compile_extended("(cat|dog)s?").unwrap();
        assert!(pattern.is_match("cats"));
        assert!(pattern.is_match("dog"));
        assert!(!pattern.is_match("cow"));
        let plus = compile_extended("a+").unwrap();
        assert!(!plus.is_match("bbb"));
        assert!(plus.is_match("baab"));
        // Longest end at the leftmost start.
        let alt = compile_extended("a|aa").unwrap();
        assert_eq!(alt.find("aa").map(|(s, e, _)| (s, e)), Some((0, 2)));
    }

    #[test]
    fn test_basic_group_captures() {
        let pattern = compile_basic("\\(ab\\)c").unwrap();
        let (_, _, caps) = pattern.find("zabcab").unwrap();
        assert_eq!(caps.spans[0], Some((1, 4)));
        assert_eq!(caps.spans[1], Some((1, 3)));
    }

    #[test]
    fn test_pattern_backref_rejected() {
        assert_eq!(
            compile_basic("\\(ab\\)c\\1"),
            Err(RegexError::InvalidPattern)
        );
    }

    #[test]
    fn test_leading_star_is_literal() {
        let pattern = compile_basic("*ello").unwrap();
        assert!(pattern.is_match("*ello"));
        assert!(!pattern.is_match("hello"));
    }

    #[test]
    fn test_unclosed_class_rejected() {
        assert_eq!(compile_basic("[abc"), Err(RegexError::InvalidPattern));
    }

    #[test]
    fn test_empty_pattern_rejected() {
        assert_eq!(compile_basic(""), Err(RegexError::InvalidPattern));
    }

    #[test]
    fn test_pathological_pattern_is_fast() {
        // The classic exponential case for backtrackers finishes in
        // linear time here. `(a*)*b$` cannot match (no `b`), so the
        // engine must explore and reject without hanging.
        let pattern = compile_extended("(a*)*b$").unwrap();
        let long = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa!";
        assert!(!pattern.is_match(long));
    }

    #[test]
    fn test_interval_repeats_a_single_instruction_atom() {
        // `{m}` / `{m,n}` / `{m,}` in the extended spelling and `\{m\}` & friends
        // in the basic one (`regcomp.c:512-529`, `:705-721`), over the atoms that
        // are one instruction each.
        let exactly = compile_extended("a{2}b").unwrap();
        assert!(exactly.find("aab").is_some());
        assert!(exactly.find("ab").is_none());
        let ranged = compile_extended("[ab]{1,2}z").unwrap();
        assert!(ranged.find("az").is_some());
        assert!(ranged.find("abz").is_some());
        assert!(
            ranged.find("z").is_none(),
            "one or two of the class come first"
        );
        assert!(ranged.find("zz").is_none(), "no class byte at all");
        let open = compile_extended(".{2,}!").unwrap();
        assert!(open.find("xy!").is_some());
        assert!(open.find("x!").is_none());
        // The basic dialect needs the backslashed braces, and its closer is `\}`.
        let basic = compile_basic("a\\{2\\}b").unwrap();
        assert!(basic.find("aab").is_some());
        assert!(basic.find("ab").is_none());
        assert!(compile_basic("a\\{1,2\\}b").unwrap().find("ab").is_some());
        assert!(compile_basic("a\\{2,\\}b").unwrap().find("aaab").is_some());
        // A zero count makes the atom disappear, which is what C's `repeat()`
        // gives: `a{0}b` is `b` with `a` allowed to sit in front of it.
        let nothing = compile_extended("a{0}b").unwrap();
        assert!(nothing.find("b").is_some());
        assert!(nothing.find("ab").is_some());
        assert!(nothing.find("aab").is_some());
        // Escaped bytes repeat the same way.
        assert!(compile_extended(r"\.{2}x").unwrap().find("..x").is_some());
    }

    #[test]
    fn test_interval_shape_errors_follow_c() {
        // Reversed bounds are refused by both parsers (`REQUIRE(count <= count2,
        // REG_BADBR)`, `regcomp.c:517` / `:710`), and so is a missing closer.
        assert!(compile_extended("a{2,1}b").is_err());
        assert!(compile_basic("a\\{2,1\\}b").is_err());
        assert!(compile_extended("a{2b").is_err(), "no closer at all");
        assert!(compile_extended("a{2").is_err());
        assert!(compile_basic("a\\{2\\}b").is_ok());
        assert!(
            compile_basic("a\\{2}b").is_err(),
            "the basic dialect closes with a backslash brace"
        );
        // A brace that does not start an interval is ordinary text in the
        // extended dialect (`regcomp.c:473`), so these never reach the bounds
        // parser at all.
        assert!(compile_extended("a{x}").unwrap().find("a{x}").is_some());
        assert!(compile_extended("a{").unwrap().find("a{").is_some());
        assert!(compile_extended("{2}x").unwrap().find("{2}x").is_some());
    }

    #[test]
    fn test_group_intervals_are_refused_rather_than_read_as_text() {
        // C repeats a subexpression by copying its code range with `dupl`, and the
        // copies share the group's capture slots so the last iteration wins
        // (`regcomp.c:522`'s `repeat`, and POSIX's rule for `(x){n}` captures).
        // This compiler numbers slots monotonically, so the honest answer is a
        // refusal: reading the brace as ordinary text would silently match
        // `ab{2}` where C matches `abab`.
        assert_eq!(
            compile_extended("(ab){2}").err(),
            Some(RegexError::TooComplex)
        );
        assert_eq!(
            compile_basic("\\(ab\\)\\{2\\}").err(),
            Some(RegexError::TooComplex)
        );
        // A group without a following brace is unaffected.
        assert!(compile_extended("(ab)c").unwrap().find("abc").is_some());
        // Two repetition operators in a row die the same way C's parser does them
        // (`regcomp.c:531-537`: the next operator after a quantified atom is
        // `REG_BADRPT`), which is also why the repeatable slot is cleared as soon
        // as any operator has been applied.
        assert!(compile_extended("a*{2}").is_err());
        assert!(compile_extended("a+{2}").is_err());
        assert!(compile_extended("a{2}{3}").is_err());
    }

    #[test]
    fn test_basic_dialect_reads_unescaped_parentheses_as_text() {
        // C's BRE atom switch has no case for a bare `)`, and a group body stops
        // only on the **two-byte** pair `\` `)` (`regcomp.c:570-597`'s
        // `SEETWO(end1, end2)`, `:628-690`'s `default: ordinary`), so both
        // parenthesis bytes are ordinary text — outside a group and inside one.
        let pattern = compile_basic("a(b)").unwrap();
        assert_eq!(pattern.group_count, 0);
        assert!(pattern.find("a(b)").is_some());
        assert!(pattern.find("ab").is_none());
        // A lone closer at the top level is text too.
        let closer = compile_basic("x)").unwrap();
        assert!(closer.find("x)").is_some());
        assert!(closer.find("x").is_none());
        // The other bytes the extended dialect made operators are ordinary here
        // as well, which is what the same `default: ordinary` arm gives
        // (`regcomp.c:628-690`): alternation, the two repetition letters, and the
        // braces. Each row is (pattern, the text it looks for, the same text with
        // the metacharacter bytes dropped).
        for (pattern, literal, stripped) in [
            ("x|y", "x|y", "xy"),
            ("p+q", "p+q", "pq"),
            ("r{2}", "r{2}", "r2"),
        ] {
            let compiled = compile_basic(pattern)
                .unwrap_or_else(|error| panic!("基本式 {pattern:?} 该能编译：{error:?}"));
            assert!(
                compiled.find(literal).is_some(),
                "{pattern:?} 要找的是字面 {literal:?}"
            );
            assert!(
                compiled.find(stripped).is_none(),
                "{pattern:?} 不该把 {stripped:?} 当运算符读"
            );
        }
        // The body of `\(a)b\)` is `a)b`: the bare `)` does not end the group,
        // the closing pair does.
        let group = compile_basic("\\(a)b\\)").unwrap();
        assert_eq!(group.group_count, 1);
        assert!(group.find("a)b").is_some());
        assert!(group.find("ab").is_none());
    }

    #[test]
    fn test_extended_dialect_closes_groups_and_lets_a_stray_paren_through() {
        // The extended dialect does use bare parentheses: `regcomp.c:417-427`'s
        // `case '('` recurses with `stop = ')'`, and the body must end on it.
        let group = compile_extended("a(b)c").unwrap();
        assert_eq!(group.group_count, 1);
        assert!(group.find("abc").is_some());
        assert!(group.find("a(b)c").is_none());
        // A `)` with no group open is ordinary text here, because Minix builds the
        // library with `-DPOSIX_MISTAKE` (`minix3/lib/libc/regex/Makefile.inc:7`),
        // which compiles out `regcomp.c:428-438`'s unmatched-`)` error.
        let stray = compile_extended("x)").unwrap();
        assert!(stray.find("x)").is_some());
        assert!(stray.find("x").is_none());
        // An escaped one is text in this dialect too.
        let escaped = compile_extended("\\)").unwrap();
        assert_eq!(escaped.group_count, 0);
        assert!(escaped.find(")").is_some());
    }

    #[test]
    fn test_group_count_reported() {
        assert_eq!(compile_basic("a").unwrap().group_count, 0);
        assert_eq!(compile_basic("\\(a\\)\\(b\\)").unwrap().group_count, 2);
    }

    #[test]
    fn test_find_word_boundary() {
        // BSD grep -w: a word hit must have non-word bytes on both sides.
        let cat = compile_basic("cat").unwrap();
        // "a cat sat": spaces on both sides of cat -> word hit.
        assert_eq!(cat.find_word("a cat sat"), Some((2, 5)));
        // "concatenate": cat sits inside a word, so no candidate
        // satisfies the word boundary -> none.
        assert_eq!(cat.find_word("concatenate"), None);
        // Start-of-line/end-of-line edges also count as word boundaries.
        assert_eq!(cat.find_word("cat"), Some((0, 3)));
        assert_eq!(cat.find_word("the cat"), Some((4, 7)));
        // Underscore is a word byte: cat in "_cat" has _ on its left ->
        // not a word boundary.
        assert_eq!(cat.find_word("_cat"), None);
        // Several candidates: the first hit is a non-word boundary (the
        // cat inside concat), the second is a word boundary -> take the
        // latter.
        assert_eq!(cat.find_word("concat cat"), Some((7, 10)));
    }
}
