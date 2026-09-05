//! Falling blocks.
//!
//! Ground truth: `minix3/games/tetris/shapes.c` (neighbor offsets near lines
//! 46 to 53, the fit test near line 82, placement near line 97) and
//! `minix3/games/tetris/tetris.h` (board width 12 and height 23 near lines 54
//! to 56). The board array and row clearing live in `tetris.c` (near lines 62
//! and 109 to 118). Scoring and input stay with the execution layer; this
//! module owns pieces, fitting, placement, clearing, and points.

use crate::TermGameError;

/// Board width in cells.
pub const BOARD_WIDTH: usize = 10;

/// Board height in cells (playable rows; walls excluded).
pub const BOARD_HEIGHT: usize = 20;

/// One piece: four cell offsets from the pivot plus a display color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece {
    /// Four (dx, dy) offsets; the pivot sits at index zero.
    pub cells: [(i8, i8); 4],
    /// Display color number.
    pub color: u8,
}

/// The seven classic pieces (spawn orientation, pivot first).
pub const PIECES: &[Piece] = &[
    // Straight line (horizontal).
    Piece { cells: [(0, 0), (-1, 0), (1, 0), (2, 0)], color: 1 },
    // Square.
    Piece { cells: [(0, 0), (1, 0), (0, 1), (1, 1)], color: 2 },
    // Tee.
    Piece { cells: [(0, 0), (-1, 0), (1, 0), (0, 1)], color: 3 },
    // Left gun.
    Piece { cells: [(0, 0), (-1, 0), (1, 0), (-1, 1)], color: 4 },
    // Right gun.
    Piece { cells: [(0, 0), (-1, 0), (1, 0), (1, 1)], color: 5 },
    // Left snake.
    Piece { cells: [(0, 0), (1, 0), (0, 1), (-1, 1)], color: 6 },
    // Right snake.
    Piece { cells: [(0, 0), (-1, 0), (0, 1), (1, 1)], color: 7 },
];

/// Rotate `piece` a quarter turn clockwise around its pivot.
pub fn rotate_piece(piece: Piece) -> Piece {
    let mut cells = [(0i8, 0i8); 4];
    for (index, (dx, dy)) in piece.cells.iter().enumerate() {
        cells[index] = (*dy, -*dx);
    }
    Piece { cells, color: piece.color }
}

/// Board occupancy (true means filled).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board {
    cells: [bool; BOARD_WIDTH * BOARD_HEIGHT],
}

impl Board {
    /// An empty board.
    pub fn new() -> Self {
        Board {
            cells: [false; BOARD_WIDTH * BOARD_HEIGHT],
        }
    }

    fn at(&self, x: i32, y: i32) -> Result<bool, TermGameError> {
        if x < 0 || y < 0 || x >= BOARD_WIDTH as i32 || y >= BOARD_HEIGHT as i32 {
            return Err(TermGameError::InvalidArgument);
        }
        Ok(self.cells[y as usize * BOARD_WIDTH + x as usize])
    }

    /// True when `piece` placed with its pivot at (`x`, `y`) touches only
    /// empty cells inside the board.
    pub fn fits(&self, piece: Piece, x: i32, y: i32) -> bool {
        piece.cells.iter().all(|(dx, dy)| {
            let (cx, cy) = (x + *dx as i32, y + *dy as i32);
            cx >= 0
                && cy >= 0
                && cx < BOARD_WIDTH as i32
                && cy < BOARD_HEIGHT as i32
                && !self.cells[cy as usize * BOARD_WIDTH + cx as usize]
        })
    }

    /// Fill the cells of `piece` at (`x`, `y`); fails when it does not fit.
    pub fn place(&mut self, piece: Piece, x: i32, y: i32) -> Result<(), TermGameError> {
        if !self.fits(piece, x, y) {
            return Err(TermGameError::InvalidArgument);
        }
        for (dx, dy) in piece.cells {
            let (cx, cy) = (x + dx as i32, y + dy as i32);
            self.cells[cy as usize * BOARD_WIDTH + cx as usize] = true;
        }
        Ok(())
    }

    /// Clear full rows, drop the rows above down, return cleared row count.
    pub fn clear_rows(&mut self) -> usize {
        let mut cleared = 0;
        let mut row = BOARD_HEIGHT as i32 - 1;
        while row >= 0 {
            let full = (0..BOARD_WIDTH).all(|col| {
                self.cells[row as usize * BOARD_WIDTH + col]
            });
            if full {
                cleared += 1;
                // Move every row above down by one.
                let mut above = row;
                while above > 0 {
                    let (dst, src) = (
                        above as usize * BOARD_WIDTH,
                        (above as usize - 1) * BOARD_WIDTH,
                    );
                    self.cells.copy_within(src..src + BOARD_WIDTH, dst);
                    above -= 1;
                }
                for col in 0..BOARD_WIDTH {
                    self.cells[col] = false;
                }
            } else {
                row -= 1;
            }
        }
        cleared
    }

    /// Read one cell (for tests and display).
    pub fn cell(&self, x: usize, y: usize) -> Result<bool, TermGameError> {
        self.at(x as i32, y as i32)
    }
}

impl Default for Board {
    fn default() -> Self {
        Self::new()
    }
}

/// Points for clearing `rows` at once (classic table: 40/100/300/1200).
pub fn line_points(rows: usize) -> u32 {
    match rows {
        1 => 40,
        2 => 100,
        3 => 300,
        4 => 1200,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seven_pieces() {
        assert_eq!(PIECES.len(), 7);
        for piece in PIECES {
            assert_eq!(piece.cells[0], (0, 0));
        }
    }

    #[test]
    fn test_rotation_cycles() {
        let piece = PIECES[0];
        let turned = rotate_piece(rotate_piece(rotate_piece(rotate_piece(piece))));
        assert_eq!(turned.cells, piece.cells);
    }

    #[test]
    fn test_fits_rejects_walls() {
        let board = Board::new();
        assert!(board.fits(PIECES[1], 4, 4));
        assert!(!board.fits(PIECES[1], -1, 4));
        assert!(!board.fits(PIECES[0], 0, 4));
    }

    #[test]
    fn test_place_marks_cells() {
        let mut board = Board::new();
        board.place(PIECES[1], 4, 4).unwrap();
        assert!(board.cell(4, 4).unwrap());
        assert!(board.cell(5, 5).unwrap());
        assert_eq!(board.place(PIECES[1], 4, 4), Err(TermGameError::InvalidArgument));
    }

    #[test]
    fn test_row_clearing_drops() {
        let mut board = Board::new();
        for col in 0..BOARD_WIDTH {
            board.cells[(BOARD_HEIGHT - 1) * BOARD_WIDTH + col] = true;
        }
        board.cells[(BOARD_HEIGHT - 2) * BOARD_WIDTH] = true;
        assert_eq!(board.clear_rows(), 1);
        assert!(board.cell(0, BOARD_HEIGHT - 1).unwrap());
        assert!(!board.cell(1, BOARD_HEIGHT - 1).unwrap());
    }

    #[test]
    fn test_points_table() {
        assert_eq!(line_points(1), 40);
        assert_eq!(line_points(4), 1200);
        assert_eq!(line_points(0), 0);
        assert_eq!(line_points(9), 0);
    }
}
