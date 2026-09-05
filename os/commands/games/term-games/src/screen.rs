//! Cell screens and escape sequences.
//!
//! Ground truth: the terminal games draw through the terminal library
//! (`minix3/games/worm/worm.c` sets up the screen near line 120 and draws
//! cells near lines 203 to 204; `minix3/games/rain/rain.c` draws drops near
//! lines 117 to 118). Here the screen is a plain cell buffer and control
//! codes are plain strings; no terminal library is linked.

use crate::TermGameError;

/// Largest screen width accepted.
pub const MAX_WIDTH: usize = 132;

/// Largest screen height accepted.
pub const MAX_HEIGHT: usize = 50;

/// One screen cell (a display character plus a highlight flag).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cell {
    /// Display character.
    pub ch: u8,
    /// True when drawn highlighted.
    pub bright: bool,
}

/// Screen behind the games.
pub trait Screen {
    /// Screen width in cells.
    fn width(&self) -> usize;
    /// Screen height in cells.
    fn height(&self) -> usize;
    /// Write `cell` at (`x`, `y`).
    fn put(&mut self, x: usize, y: usize, cell: Cell) -> Result<(), TermGameError>;
    /// Read the cell at (`x`, `y`).
    fn get(&self, x: usize, y: usize) -> Result<Cell, TermGameError>;
    /// Clear every cell to blanks.
    fn clear(&mut self);
}

/// Memory screen with room for the largest accepted size.
pub struct MemoryScreen {
    width: usize,
    height: usize,
    cells: [Cell; MAX_WIDTH * MAX_HEIGHT],
}

impl MemoryScreen {
    /// Build a blank screen of `width` by `height` cells.
    pub fn new(width: usize, height: usize) -> Result<Self, TermGameError> {
        if width == 0 || height == 0 || width > MAX_WIDTH || height > MAX_HEIGHT {
            return Err(TermGameError::InvalidArgument);
        }
        Ok(MemoryScreen {
            width,
            height,
            cells: [Cell { ch: b' ', bright: false }; MAX_WIDTH * MAX_HEIGHT],
        })
    }

    fn index(&self, x: usize, y: usize) -> Result<usize, TermGameError> {
        if x >= self.width || y >= self.height {
            return Err(TermGameError::InvalidArgument);
        }
        Ok(y * MAX_WIDTH + x)
    }
}

impl Screen for MemoryScreen {
    fn width(&self) -> usize {
        self.width
    }

    fn height(&self) -> usize {
        self.height
    }

    fn put(&mut self, x: usize, y: usize, cell: Cell) -> Result<(), TermGameError> {
        let index = self.index(x, y)?;
        self.cells[index] = cell;
        Ok(())
    }

    fn get(&self, x: usize, y: usize) -> Result<Cell, TermGameError> {
        let index = self.index(x, y)?;
        Ok(self.cells[index])
    }

    fn clear(&mut self) {
        for cell in self.cells.iter_mut() {
            *cell = Cell { ch: b' ', bright: false };
        }
    }
}

/// Null screen (writes vanish, reads report blanks, sizes read zero).
pub struct NullScreen;

impl Screen for NullScreen {
    fn width(&self) -> usize {
        0
    }

    fn height(&self) -> usize {
        0
    }

    fn put(&mut self, _x: usize, _y: usize, _cell: Cell) -> Result<(), TermGameError> {
        Ok(())
    }

    fn get(&self, _x: usize, _y: usize) -> Result<Cell, TermGameError> {
        Ok(Cell { ch: b' ', bright: false })
    }

    fn clear(&mut self) {}
}

/// Clear screen escape sequence.
pub const CLEAR_SCREEN: &str = "\u{1b}[2J";

/// Move cursor home escape sequence.
pub const CURSOR_HOME: &str = "\u{1b}[H";

/// Build a cursor move (`row`, `col`, both one based) into `out`.
pub fn move_cursor(row: u32, col: u32, out: &mut [u8]) -> Result<usize, TermGameError> {
    if row == 0 || col == 0 {
        return Err(TermGameError::InvalidArgument);
    }
    let mut written = 0;
    emit_byte(out, &mut written, 0x1B)?;
    emit_byte(out, &mut written, b'[')?;
    let mut cursor = written;
    emit_number(out, &mut cursor, row)?;
    emit_byte(out, &mut cursor, b';')?;
    emit_number(out, &mut cursor, col)?;
    emit_byte(out, &mut cursor, b'H')?;
    Ok(cursor)
}

fn emit_byte(out: &mut [u8], cursor: &mut usize, byte: u8) -> Result<(), TermGameError> {
    if *cursor >= out.len() {
        return Err(TermGameError::NoSpace);
    }
    out[*cursor] = byte;
    *cursor += 1;
    Ok(())
}

fn emit_number(out: &mut [u8], cursor: &mut usize, mut value: u32) -> Result<(), TermGameError> {
    if value == 0 {
        emit_byte(out, cursor, b'0')?;
        return Ok(());
    }
    let mut digits = [0u8; 10];
    let mut len = 0;
    while value > 0 {
        digits[len] = b'0' + (value % 10) as u8;
        value /= 10;
        len += 1;
    }
    while len > 0 {
        len -= 1;
        emit_byte(out, cursor, digits[len])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_screen_round_trip() {
        let mut screen = MemoryScreen::new(10, 5).unwrap();
        screen.put(3, 2, Cell { ch: b'@', bright: true }).unwrap();
        assert_eq!(
            screen.get(3, 2).unwrap(),
            Cell { ch: b'@', bright: true }
        );
        assert_eq!(screen.get(0, 0).unwrap().ch, b' ');
    }

    #[test]
    fn test_bad_sizes_rejected() {
        assert_eq!(MemoryScreen::new(0, 5).map(|_| ()), Err(TermGameError::InvalidArgument));
        assert_eq!(
            MemoryScreen::new(200, 5).map(|_| ()),
            Err(TermGameError::InvalidArgument)
        );
    }

    #[test]
    fn test_out_of_bounds_rejected() {
        let mut screen = MemoryScreen::new(10, 5).unwrap();
        assert_eq!(
            screen.put(10, 0, Cell { ch: b'x', bright: false }),
            Err(TermGameError::InvalidArgument)
        );
    }

    #[test]
    fn test_clear_blanks() {
        let mut screen = MemoryScreen::new(4, 4).unwrap();
        screen.put(1, 1, Cell { ch: b'x', bright: true }).unwrap();
        screen.clear();
        assert_eq!(screen.get(1, 1).unwrap().ch, b' ');
    }

    #[test]
    fn test_null_screen_quiet() {
        let mut screen = NullScreen;
        screen.put(99, 99, Cell { ch: b'x', bright: false }).unwrap();
        assert_eq!(screen.get(99, 99).unwrap().ch, b' ');
    }

    #[test]
    fn test_cursor_moves() {
        let mut out = [0u8; 16];
        let len = move_cursor(12, 40, &mut out).unwrap();
        assert_eq!(&out[..len], b"\x1b[12;40H");
        assert_eq!(move_cursor(0, 1, &mut out), Err(TermGameError::InvalidArgument));
    }
}
