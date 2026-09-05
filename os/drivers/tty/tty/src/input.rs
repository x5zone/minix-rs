//! Input queue: the 256-cell ring with line-break accounting.
//!
//! C correspondence: `tty_inbuf[TTY_IN_BYTES]` with `tty_inhead`,
//! `tty_intail`, `tty_incount`, `tty_eotct` (`tty.h`), the character marks
//! `IN_EOT`, `IN_EOF`, `IN_ESC` (`tty.h`), and the canonical branch of
//! `in_process` (`tty.c:1012-1177`): erase, kill, end-of-file, escaped
//! characters, and carriage-return mappings.

use super::termios::{ControlChars, LineFlags};
use minix_types::OK;

/// Capacity of the input ring.
///
/// C: `TTY_IN_BYTES 256` (`tty.h:12`).
pub const INPUT_RING: usize = 256;

/// Marks stored alongside each queued character.
///
/// C: the high bits above `IN_CHAR 0x00FF` (`tty.h`): length echo bits are
/// a display concern and stay in the service crate; end-of-line,
/// end-of-file, and escaped marks are policy and live here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharMark {
    /// Line break (newline or end-of-file character).
    pub end_of_line: bool,
    /// End of file (never returned to the reader).
    pub end_of_file: bool,
    /// Quoted by the literal-next character: no interpretation.
    pub escaped: bool,
}

impl CharMark {
    /// Plain character with no marks.
    pub const fn plain() -> CharMark {
        CharMark {
            end_of_line: false,
            end_of_file: false,
            escaped: false,
        }
    }
}

/// One queued input character plus its marks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueuedChar {
    /// The character itself (low eight bits significant).
    pub value: u8,
    /// Marks attached at queue time.
    pub mark: CharMark,
}

/// Ring of typed characters waiting to be read.
///
/// C: the head/tail/count/eotct quartet (`tty.h`). The ring refuses input
/// past capacity (the C code drops with an audible bell); accounting of
/// line breaks lets canonical reads finish at a newline without scanning.
#[derive(Debug, Clone)]
pub struct InputQueue {
    cells: [Option<QueuedChar>; INPUT_RING],
    head: usize,
    tail: usize,
    count: usize,
    breaks: usize,
}

impl InputQueue {
    /// Empty queue.
    pub const fn new() -> InputQueue {
        InputQueue {
            cells: [None; INPUT_RING],
            head: 0,
            tail: 0,
            count: 0,
            breaks: 0,
        }
    }

    /// Characters currently stored.
    pub fn len(&self) -> usize {
        self.count
    }

    /// True when nothing is stored.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Line breaks currently stored.
    pub fn break_count(&self) -> usize {
        self.breaks
    }

    /// Append one character; false (dropped) when full.
    pub fn push(&mut self, cell: QueuedChar) -> bool {
        if self.count >= INPUT_RING {
            return false;
        }
        self.cells[self.head] = Some(cell);
        self.head = (self.head + 1) % INPUT_RING;
        self.count += 1;
        if cell.mark.end_of_line {
            self.breaks += 1;
        }
        true
    }

    /// Remove the oldest character; `None` when empty.
    pub fn pop(&mut self) -> Option<QueuedChar> {
        if self.count == 0 {
            return None;
        }
        let cell = self.cells[self.tail].take();
        self.tail = (self.tail + 1) % INPUT_RING;
        self.count -= 1;
        if cell.map(|cell| cell.mark.end_of_line).unwrap_or(false) {
            self.breaks -= 1;
        }
        cell
    }

    /// Remove the newest character (erase processing); `None` when empty.
    ///
    /// C: `back_over` (`tty.c:1251-...`): rubbing out the last character.
    pub fn pop_last(&mut self) -> Option<QueuedChar> {
        if self.count == 0 {
            return None;
        }
        self.head = (self.head + INPUT_RING - 1) % INPUT_RING;
        let cell = self.cells[self.head].take();
        self.count -= 1;
        if cell.map(|cell| cell.mark.end_of_line).unwrap_or(false) {
            self.breaks -= 1;
        }
        cell
    }

    /// Drop the whole current line (kill processing).
    ///
    /// C: the kill branch of `in_process`: characters vanish up to (but
    /// excluding) the previous line break. Returns how many were dropped.
    pub fn drop_line(&mut self) -> usize {
        let mut dropped = 0;
        while self.count > 0 {
            let is_break = self.cells[(self.head + INPUT_RING - 1) % INPUT_RING]
                .map(|cell| cell.mark.end_of_line)
                .unwrap_or(false);
            if is_break {
                break;
            }
            self.pop_last();
            dropped += 1;
        }
        dropped
    }
}

impl Default for InputQueue {
    fn default() -> Self {
        InputQueue::new()
    }
}

/// Outcome of feeding one raw byte through the line discipline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedOutcome {
    /// Stored (possibly with marks).
    Stored,
    /// Erased one character (the byte itself is not stored).
    Erased,
    /// Killed the current line (the byte itself is not stored).
    Killed,
    /// Dropped: queue full.
    Dropped,
    /// Ignored by mapping (carriage return with ignore set).
    Ignored,
    /// Reprint requested (the byte itself is not stored).
    Reprint,
}

/// Canonical/non-canonical line processor (the testable half of
/// `in_process`).
///
/// C: `in_process` (`tty.c:1012-1177`). Echoing stays in the service crate
/// (it needs the device); this type owns the queue decisions: mappings,
/// erase, kill, end-of-file, escape. One call per byte keeps the
/// state machine explicit.
pub struct LineProcessor {
    escaped: bool,
}

impl LineProcessor {
    /// Fresh processor (no pending escape).
    pub const fn new() -> LineProcessor {
        LineProcessor { escaped: false }
    }

    /// Feed one byte; returns what happened to it.
    pub fn feed(
        &mut self,
        queue: &mut InputQueue,
        byte: u8,
        flags: &LineFlags,
        controls: &ControlChars,
    ) -> FeedOutcome {
        let mut byte = byte;
        let mut mark = CharMark::plain();

        if flags.strip_to_seven_bits {
            byte &= 0x7F;
        }
        if self.escaped {
            self.escaped = false;
            mark.escaped = true;
            return store(queue, byte, mark);
        }
        if flags.extended_functions && byte == controls.literal_next {
            self.escaped = true;
            return FeedOutcome::Stored;
        }
        if flags.extended_functions && byte == controls.reprint {
            return FeedOutcome::Reprint;
        }
        if byte == b'\r' {
            if flags.ignore_cr {
                return FeedOutcome::Ignored;
            }
            if flags.map_cr_to_nl {
                byte = b'\n';
            }
        } else if byte == b'\n' && flags.map_nl_to_cr {
            byte = b'\r';
        }
        if flags.canonical {
            if byte == controls.erase {
                return match queue.pop_last() {
                    Some(_) => FeedOutcome::Erased,
                    None => FeedOutcome::Ignored,
                };
            }
            if byte == controls.kill {
                queue.drop_line();
                return FeedOutcome::Killed;
            }
            if byte == controls.eof {
                mark.end_of_line = true;
                mark.end_of_file = true;
                return store(queue, byte, mark);
            }
            if byte == b'\n' || byte == controls.eol {
                mark.end_of_line = true;
            }
        }
        store(queue, byte, mark)
    }
}

impl Default for LineProcessor {
    fn default() -> Self {
        LineProcessor::new()
    }
}

fn store(queue: &mut InputQueue, byte: u8, mark: CharMark) -> FeedOutcome {
    if queue.push(QueuedChar { value: byte, mark }) {
        FeedOutcome::Stored
    } else {
        FeedOutcome::Dropped
    }
}

/// Success marker for call sites that only need the code.
pub const SUCCESS: i32 = OK;

#[cfg(test)]
mod tests {
    use super::super::termios::{ControlChars, LineConfig};
    use super::*;

    fn configured() -> (LineFlags, ControlChars) {
        let config = LineConfig::defaults();
        (config.flags, config.controls)
    }

    #[test]
    fn test_plain_characters_queue_in_order() {
        let (flags, controls) = configured();
        let mut queue = InputQueue::new();
        let mut processor = LineProcessor::new();
        for byte in [b'h', b'i', b'\n'] {
            assert_eq!(
                processor.feed(&mut queue, byte, &flags, &controls),
                FeedOutcome::Stored
            );
        }
        assert_eq!(queue.len(), 3);
        assert_eq!(queue.break_count(), 1);
        assert_eq!(queue.pop().unwrap().value, b'h');
    }

    #[test]
    fn test_erase_removes_last_character() {
        let (flags, controls) = configured();
        let mut queue = InputQueue::new();
        let mut processor = LineProcessor::new();
        processor.feed(&mut queue, b'a', &flags, &controls);
        processor.feed(&mut queue, b'b', &flags, &controls);
        assert_eq!(
            processor.feed(&mut queue, controls.erase, &flags, &controls),
            FeedOutcome::Erased
        );
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn test_kill_drops_line_but_keeps_previous_break() {
        let (flags, controls) = configured();
        let mut queue = InputQueue::new();
        let mut processor = LineProcessor::new();
        for byte in [b'a', b'\n', b'b', b'c'] {
            processor.feed(&mut queue, byte, &flags, &controls);
        }
        assert_eq!(
            processor.feed(&mut queue, controls.kill, &flags, &controls),
            FeedOutcome::Killed
        );
        assert_eq!(queue.len(), 2);
        assert_eq!(queue.break_count(), 1);
    }

    #[test]
    fn test_eof_marks_break_and_file_end() {
        let (flags, controls) = configured();
        let mut queue = InputQueue::new();
        let mut processor = LineProcessor::new();
        assert_eq!(
            processor.feed(&mut queue, controls.eof, &flags, &controls),
            FeedOutcome::Stored
        );
        let cell = queue.pop().unwrap();
        assert!(cell.mark.end_of_line);
        assert!(cell.mark.end_of_file);
    }

    #[test]
    fn test_literal_next_quotes_erase_character() {
        let (flags, controls) = configured();
        let mut queue = InputQueue::new();
        let mut processor = LineProcessor::new();
        processor.feed(&mut queue, b'a', &flags, &controls);
        processor.feed(&mut queue, controls.literal_next, &flags, &controls);
        assert_eq!(
            processor.feed(&mut queue, controls.erase, &flags, &controls),
            FeedOutcome::Stored
        );
        assert_eq!(queue.len(), 2);
        assert!(queue.pop_last().unwrap().mark.escaped);
    }

    #[test]
    fn test_cr_mapping_and_ignore() {
        let (mut flags, controls) = configured();
        let mut queue = InputQueue::new();
        let mut processor = LineProcessor::new();
        processor.feed(&mut queue, b'\r', &flags, &controls);
        assert_eq!(queue.pop_last().unwrap().value, b'\n');
        flags.ignore_cr = true;
        flags.map_cr_to_nl = false;
        assert_eq!(
            processor.feed(&mut queue, b'\r', &flags, &controls),
            FeedOutcome::Ignored
        );
        assert!(queue.is_empty());
    }

    #[test]
    fn test_full_queue_drops_with_signal() {
        let (flags, controls) = configured();
        let mut queue = InputQueue::new();
        let mut processor = LineProcessor::new();
        let mut flags = flags;
        flags.canonical = false;
        for _ in 0..INPUT_RING {
            assert_eq!(
                processor.feed(&mut queue, b'x', &flags, &controls),
                FeedOutcome::Stored
            );
        }
        assert_eq!(
            processor.feed(&mut queue, b'x', &flags, &controls),
            FeedOutcome::Dropped
        );
    }
}
