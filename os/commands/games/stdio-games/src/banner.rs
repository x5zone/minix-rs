//! Big letter banners.
//!
//! Ground truth: `minix3/games/banner/banner.c`. The full raster width is 132
//! columns (`DWIDTH`, near line 58); the `-w` option scrunches letters down
//! (80 columns is the classic terminal width, larger values rejected near
//! line 1052); usage is `banner [-w width] [message]` (near line 1057).
//! Glyph scaling maps output columns back onto the full raster with rounding
//! (near lines 1064 to 1066). This module owns width parsing and the glyph
//! source trait; the full bitmap font ships with the execution layer.

use crate::GameError;

/// Full raster width in columns.
pub const FULL_WIDTH: u32 = 132;

/// Classic terminal width.
pub const TERMINAL_WIDTH: u32 = 80;

/// Parse a `-w` width (one through the full width).
pub fn parse_banner_width(word: &str) -> Result<u32, GameError> {
    if word.is_empty() {
        return Err(GameError::InvalidArgument);
    }
    let mut value: u32 = 0;
    for byte in word.bytes() {
        if !byte.is_ascii_digit() {
            return Err(GameError::InvalidArgument);
        }
        value = value
            .checked_mul(10)
            .and_then(|scaled| scaled.checked_add((byte - b'0') as u32))
            .ok_or(GameError::OutOfRange)?;
    }
    if value == 0 || value > FULL_WIDTH {
        return Err(GameError::OutOfRange);
    }
    Ok(value)
}

/// Map an output column back onto the full raster with rounding.
///
/// This is the scrunch formula from the banner source: the output column
/// stretches over the full raster proportionally, rounding to the nearest
/// raster cell.
pub fn scrunch_column(output: u32, width: u32) -> u32 {
    ((output as u64 * width as u64 + (FULL_WIDTH as u64 / 2)) / FULL_WIDTH as u64) as u32
}

/// Glyph source behind banner rows.
pub trait GlyphSource {
    /// Five row block for `ch` (each row is a bit mask, lowest five bits
    /// used), or `None` when the character has no glyph.
    fn glyph_rows(&self, ch: char) -> Option<[u8; 5]>;
}

/// Block renderer: every printable ASCII character renders as its own
/// five by five dotted outline derived from the character code.
///
/// This is not the historic bitmap font (which ships with the execution
/// layer); it is a deterministic stand in with visibly different behavior:
/// letters are recognizable as framed blocks, never as the true font.
pub struct BlockGlyphs;

impl GlyphSource for BlockGlyphs {
    fn glyph_rows(&self, ch: char) -> Option<[u8; 5]> {
        if !ch.is_ascii_graphic() && ch != ' ' {
            return None;
        }
        if ch == ' ' {
            return Some([0, 0, 0, 0, 0]);
        }
        let code = ch as u8;
        Some([
            0b11111,
            0b10000 | (code >> 4) & 0x0F,
            0b10000 | (code & 0x0F),
            0b10000 | ((code >> 2) & 0x0F),
            0b11111,
        ])
    }
}

/// Empty source (every glyph misses).
pub struct EmptyGlyphs;

impl GlyphSource for EmptyGlyphs {
    fn glyph_rows(&self, _ch: char) -> Option<[u8; 5]> {
        None
    }
}

/// Render one row of big text (`row` selects the glyph row, zero through
/// four) into `out` as `#` and blank cells.
pub fn render_banner_row<G: GlyphSource>(
    glyphs: &G,
    text: &str,
    row: usize,
    out: &mut [u8],
) -> Result<usize, GameError> {
    if row > 4 {
        return Err(GameError::InvalidArgument);
    }
    let mut written = 0;
    for ch in text.chars() {
        let rows = glyphs.glyph_rows(ch).ok_or(GameError::NotFound)?;
        for bit in (0..5).rev() {
            if written >= out.len() {
                return Err(GameError::OutOfRange);
            }
            out[written] = if rows[row] & (1 << bit) != 0 { b'#' } else { b' ' };
            written += 1;
        }
        if written >= out.len() {
            return Err(GameError::OutOfRange);
        }
        out[written] = b' ';
        written += 1;
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_widths_parse() {
        assert_eq!(parse_banner_width("80"), Ok(80));
        assert_eq!(parse_banner_width("132"), Ok(132));
        assert_eq!(parse_banner_width("0"), Err(GameError::OutOfRange));
        assert_eq!(parse_banner_width("133"), Err(GameError::OutOfRange));
        assert_eq!(parse_banner_width("wide"), Err(GameError::InvalidArgument));
    }

    #[test]
    fn test_scrunch_endpoints() {
        assert_eq!(scrunch_column(0, 80), 0);
        assert_eq!(scrunch_column(132, 80), 80);
    }

    #[test]
    fn test_block_renders_framed() {
        let glyphs = BlockGlyphs;
        let mut out = [0u8; 32];
        let len = render_banner_row(&glyphs, "A", 0, &mut out).unwrap();
        assert_eq!(&out[..len], b"##### ");
        let len = render_banner_row(&glyphs, " ", 2, &mut out).unwrap();
        assert_eq!(&out[..len], b"      ");
    }

    #[test]
    fn test_empty_source_misses() {
        let glyphs = EmptyGlyphs;
        assert_eq!(glyphs.glyph_rows('A'), None);
        let mut out = [0u8; 32];
        assert_eq!(
            render_banner_row(&glyphs, "A", 0, &mut out),
            Err(GameError::NotFound)
        );
    }

    #[test]
    fn test_bad_row_rejected() {
        let glyphs = BlockGlyphs;
        let mut out = [0u8; 32];
        assert_eq!(
            render_banner_row(&glyphs, "A", 5, &mut out),
            Err(GameError::InvalidArgument)
        );
    }
}
