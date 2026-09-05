//! Text storage behind one trait, with two backends.
//!
//! Ground truth: `minix3/bin/ed/buf.c` (319 lines) keeps the edited text in
//! a linked line structure with operations to append, join, move, copy, and
//! delete lines (`main.c` lines 1051 to 1242). Two access patterns dominate:
//! sequential edits near the cursor (insertions, deletions while typing)
//! and random access by line number (address evaluation). No single
//! structure is best at both, so this module offers both behind
//! [`TextStore`]:
//!
//! - [`GapStore`]: one flat byte array with a gap at the cursor.
//!   Insertions and deletions at the cursor never move the rest of the
//!   text; random line access scans from the buffer start. The classic
//!   interactive editor structure.
//! - [`LineTable`]: an offset table over an owned text block. Random access
//!   by line number is direct; edits rebuild the table. The natural fit for
//!   address arithmetic and batch scripts.
//!
//! Reads copy into caller buffers (a gap split array cannot lend a
//! contiguous slice, so borrowed reads would be a lie for one backend).
//! Capacities are fixed: 4 KiB of text, 256 lines. Overflow is an error,
//! never silent truncation.

use crate::EditorError;

/// Maximum text bytes held.
pub const MAX_TEXT: usize = 4096;
/// Maximum lines indexed.
pub const MAX_LINES: usize = 256;

/// Line oriented text storage: load once, address lines by number, splice
/// line runs. Line numbers are 1 based; line 0 is never valid (matching
/// `ed`, where address 0 is rejected everywhere except where the manual
/// allows it).
pub trait TextStore {
    /// How many lines are stored.
    fn line_count(&self) -> usize;
    /// Copy line `number` (1 based, without its newline) into `out`,
    /// returning the byte count. Out of range lines and overlong lines
    /// are errors.
    fn read_line(&self, number: usize, out: &mut [u8]) -> Result<usize, EditorError>;
    /// Insert `text` (possibly several newline separated lines) before
    /// line `number` (`number == count + 1` appends at the end). Text not
    /// ending in a newline gains one, matching `ed` append semantics.
    fn insert(&mut self, number: usize, text: &str) -> Result<(), EditorError>;
    /// Delete lines `from` through `to` inclusive.
    fn delete(&mut self, from: usize, to: usize) -> Result<(), EditorError>;
}

/// Flat text with a gap at the cursor.
///
/// The buffer holds `text[..gap_start]` then `text[gap_end..]`; the gap
/// absorbs cursor local edits without moving the rest.
pub struct GapStore {
    text: [u8; MAX_TEXT],
    gap_start: usize,
    gap_end: usize,
}

impl GapStore {
    /// An empty store: the whole array starts as gap.
    pub fn new() -> Self {
        GapStore {
            text: [0; MAX_TEXT],
            gap_start: 0,
            gap_end: MAX_TEXT,
        }
    }

    /// Logical length (gap excluded).
    pub fn len(&self) -> usize {
        self.gap_start + (MAX_TEXT - self.gap_end)
    }

    /// True when no text is stored.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Read the logical byte at `offset`.
    fn at(&self, offset: usize) -> Option<u8> {
        if offset < self.gap_start {
            Some(self.text[offset])
        } else {
            self.text.get(self.gap_end + (offset - self.gap_start)).copied()
        }
    }

    /// Move the gap so it starts at logical `offset`.
    fn move_gap(&mut self, offset: usize) {
        let offset = offset.min(self.len());
        while self.gap_start < offset {
            let byte = self.text[self.gap_end];
            self.text[self.gap_start] = byte;
            self.gap_start += 1;
            self.gap_end += 1;
        }
        while self.gap_start > offset {
            self.gap_start -= 1;
            self.gap_end -= 1;
            self.text[self.gap_end] = self.text[self.gap_start];
        }
    }

    /// Logical start offset of every line; returns the line count. An empty
    /// store holds no lines; any non empty text holds at least one (a
    /// missing final newline still ends the last line).
    fn line_starts(&self, starts: &mut [usize; MAX_LINES]) -> usize {
        let len = self.len();
        if len == 0 {
            return 0;
        }
        let mut count = 1;
        starts[0] = 0;
        for offset in 0..len {
            if self.at(offset) == Some(b'\n') && offset + 1 < len && count < MAX_LINES {
                starts[count] = offset + 1;
                count += 1;
            }
        }
        count
    }
}

impl Default for GapStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TextStore for GapStore {
    fn line_count(&self) -> usize {
        let mut starts = [0usize; MAX_LINES];
        self.line_starts(&mut starts)
    }

    fn read_line(&self, number: usize, out: &mut [u8]) -> Result<usize, EditorError> {
        let mut starts = [0usize; MAX_LINES];
        let count = self.line_starts(&mut starts);
        if number == 0 || number > count {
            return Err(EditorError::InvalidArgument);
        }
        let start = starts[number - 1];
        let mut end = if number < count {
            starts[number] - 1
        } else {
            self.len()
        };
        if end > start && self.at(end - 1) == Some(b'\n') {
            end -= 1;
        }
        if end - start > out.len() {
            return Err(EditorError::TooLong);
        }
        for (index, offset) in (start..end).enumerate() {
            out[index] = self.at(offset).unwrap_or(0);
        }
        Ok(end - start)
    }

    fn insert(&mut self, number: usize, text: &str) -> Result<(), EditorError> {
        let count = self.line_count();
        if number == 0 || number > count + 1 {
            return Err(EditorError::InvalidArgument);
        }
        let mut starts = [0usize; MAX_LINES];
        self.line_starts(&mut starts);
        let offset = if number > count {
            self.len()
        } else {
            starts[number - 1]
        };
        self.move_gap(offset);
        let mut bytes = text.bytes();
        let need_newline = !text.is_empty() && !text.ends_with('\n');
        // Reserve room for the implicit newline before writing.
        let room = self.gap_end - self.gap_start;
        let wanted = text.len() + usize::from(need_newline);
        if wanted > room {
            return Err(EditorError::TooLong);
        }
        for byte in &mut bytes {
            self.text[self.gap_start] = byte;
            self.gap_start += 1;
        }
        if need_newline {
            self.text[self.gap_start] = b'\n';
            self.gap_start += 1;
        }
        Ok(())
    }

    fn delete(&mut self, from: usize, to: usize) -> Result<(), EditorError> {
        let count = self.line_count();
        if from == 0 || to < from || to > count {
            return Err(EditorError::InvalidArgument);
        }
        let mut starts = [0usize; MAX_LINES];
        self.line_starts(&mut starts);
        let start = starts[from - 1];
        let end = if to < count { starts[to] } else { self.len() };
        self.move_gap(start);
        self.gap_end += end - start;
        Ok(())
    }
}

/// Offset table over an owned text block.
///
/// The block holds the whole text contiguously; `offsets[i]` is the byte
/// offset where line `i + 1` starts, with `offsets[count]` as the end
/// sentinel. Loads and edits rebuild the table in place.
pub struct LineTable {
    text: [u8; MAX_TEXT],
    len: usize,
    offsets: [u32; MAX_LINES],
    count: usize,
}

impl LineTable {
    /// An empty table.
    pub fn new() -> Self {
        LineTable {
            text: [0; MAX_TEXT],
            len: 0,
            offsets: [0; MAX_LINES],
            count: 0,
        }
    }

    /// Load `text`, replacing any previous content.
    pub fn load(&mut self, text: &str) -> Result<(), EditorError> {
        if text.len() > MAX_TEXT {
            return Err(EditorError::TooLong);
        }
        self.text[..text.len()].copy_from_slice(text.as_bytes());
        self.len = text.len();
        self.reindex()
    }

    /// Rebuild the offset table over the current block.
    fn reindex(&mut self) -> Result<(), EditorError> {
        self.count = 0;
        if self.len == 0 {
            return Ok(());
        }
        self.offsets[0] = 0;
        self.count = 1;
        for offset in 0..self.len {
            if self.text[offset] == b'\n' && offset + 1 < self.len {
                if self.count >= MAX_LINES {
                    return Err(EditorError::TooLong);
                }
                self.offsets[self.count] = (offset + 1) as u32;
                self.count += 1;
            }
        }
        Ok(())
    }

    /// Byte range of line `number` (1 based), newline excluded.
    fn span(&self, number: usize) -> Result<(usize, usize), EditorError> {
        if number == 0 || number > self.count {
            return Err(EditorError::InvalidArgument);
        }
        let start = self.offsets[number - 1] as usize;
        let mut end = if number < self.count {
            self.offsets[number] as usize - 1
        } else {
            self.len
        };
        if end > start && self.text[end - 1] == b'\n' {
            end -= 1;
        }
        Ok((start, end))
    }
}

impl Default for LineTable {
    fn default() -> Self {
        Self::new()
    }
}

impl TextStore for LineTable {
    fn line_count(&self) -> usize {
        self.count
    }

    fn read_line(&self, number: usize, out: &mut [u8]) -> Result<usize, EditorError> {
        let (start, end) = self.span(number)?;
        if end - start > out.len() {
            return Err(EditorError::TooLong);
        }
        out[..end - start].copy_from_slice(&self.text[start..end]);
        Ok(end - start)
    }

    fn insert(&mut self, number: usize, text: &str) -> Result<(), EditorError> {
        if number == 0 || number > self.count + 1 {
            return Err(EditorError::InvalidArgument);
        }
        let mut owned = [0u8; MAX_TEXT + 256];
        let mut len = 0;
        // Copy a stored line with its newline when one follows.
        let mut copy_line = |owned: &mut [u8; MAX_TEXT + 256],
                             len: &mut usize,
                             line: usize,
                             table: &LineTable|
         -> Result<(), EditorError> {
            let (start, end) = table.span(line)?;
            let mut stop = end;
            if stop < table.len && table.text[stop] == b'\n' {
                stop += 1;
            }
            if *len + (stop - start) > owned.len() {
                return Err(EditorError::TooLong);
            }
            owned[*len..*len + (stop - start)].copy_from_slice(&table.text[start..stop]);
            *len += stop - start;
            Ok(())
        };
        for line in 1..number {
            copy_line(&mut owned, &mut len, line, self)?;
        }
        // Copy the new text with its implicit newline.
        if len + text.len() + 1 > owned.len() {
            return Err(EditorError::TooLong);
        }
        owned[len..len + text.len()].copy_from_slice(text.as_bytes());
        len += text.len();
        if !text.is_empty() && !text.ends_with('\n') {
            owned[len] = b'\n';
            len += 1;
        }
        for line in number..=self.count {
            copy_line(&mut owned, &mut len, line, self)?;
        }
        if len > MAX_TEXT {
            return Err(EditorError::TooLong);
        }
        self.text[..len].copy_from_slice(&owned[..len]);
        self.len = len;
        self.reindex()
    }

    fn delete(&mut self, from: usize, to: usize) -> Result<(), EditorError> {
        if from == 0 || to < from || to > self.count {
            return Err(EditorError::InvalidArgument);
        }
        let start = self.offsets[from - 1] as usize;
        let end = if to < self.count {
            self.offsets[to] as usize
        } else {
            self.len
        };
        self.text.copy_within(end..self.len, start);
        self.len -= end - start;
        self.reindex()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contents<S: TextStore>(store: &S) -> Vec<String> {
        let mut out = Vec::new();
        let mut buf = [0u8; 256];
        for number in 1..=store.line_count() {
            let len = store.read_line(number, &mut buf).unwrap();
            out.push(String::from_utf8_lossy(&buf[..len]).into_owned());
        }
        out
    }

    #[test]
    fn test_gap_insert_and_read() {
        let mut store = GapStore::new();
        store.insert(1, "hello\nworld\n").unwrap();
        assert_eq!(store.line_count(), 2);
        let mut buf = [0u8; 16];
        assert_eq!(store.read_line(1, &mut buf).unwrap(), 5);
        assert_eq!(&buf[..5], b"hello");
    }

    #[test]
    fn test_gap_implicit_newline() {
        let mut store = GapStore::new();
        store.insert(1, "single").unwrap();
        assert_eq!(store.line_count(), 1);
    }

    #[test]
    fn test_gap_delete_middle() {
        let mut store = GapStore::new();
        store.insert(1, "a\nb\nc\n").unwrap();
        store.delete(2, 2).unwrap();
        assert_eq!(contents(&store), ["a", "c"]);
    }

    #[test]
    fn test_gap_bad_addresses_rejected() {
        let mut store = GapStore::new();
        assert_eq!(store.insert(0, "x"), Err(EditorError::InvalidArgument));
        assert_eq!(store.insert(2, "x"), Err(EditorError::InvalidArgument));
        assert_eq!(store.delete(1, 1), Err(EditorError::InvalidArgument));
        let mut buf = [0u8; 8];
        assert_eq!(
            store.read_line(1, &mut buf),
            Err(EditorError::InvalidArgument)
        );
    }

    #[test]
    fn test_table_load_and_read() {
        let mut table = LineTable::new();
        table.load("one\ntwo\nthree").unwrap();
        assert_eq!(table.line_count(), 3);
        assert_eq!(contents(&table), ["one", "two", "three"]);
    }

    #[test]
    fn test_table_insert_delete() {
        let mut table = LineTable::new();
        table.load("a\nc\n").unwrap();
        table.insert(2, "b").unwrap();
        assert_eq!(contents(&table), ["a", "b", "c"]);
        table.delete(1, 1).unwrap();
        assert_eq!(contents(&table), ["b", "c"]);
    }

    #[test]
    fn test_stores_agree() {
        let text = "l1\nl2\nl3\nl4\n";
        let mut gap = GapStore::new();
        gap.insert(1, text).unwrap();
        gap.delete(2, 3).unwrap();
        let mut table = LineTable::new();
        table.load(text).unwrap();
        table.delete(2, 3).unwrap();
        assert_eq!(contents(&gap), contents(&table));
        assert_eq!(contents(&gap), ["l1", "l4"]);
    }
}
