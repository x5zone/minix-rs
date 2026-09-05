//! Temperature conversion: calibration plus raw reading.
//!
//! C correspondence: the calibration structure (`struct
//! calibration` with eleven coefficients, `bmp085.c:74-88`), the
//! measurement modes (temperature trigger plus four pressure
//! commands, `bmp085.c:14-35`), and the temperature half of
//! `measure` (`bmp085.c:382-385`: intermediate values x1, x2, b5
//! and the final tenth-degrees t, per the datasheet formula).
//!
//! Bus traffic stays in the service binary; this module owns the
//! pure math half: calibration in, temperature out.

/// Calibration coefficients read from the chip
/// (`struct calibration`, `bmp085.c:74-88`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Calibration {
    /// First temperature coefficient.
    pub ac5: u16,
    /// Second temperature coefficient.
    pub ac6: u16,
    /// First conversion coefficient.
    pub mc: i16,
    /// Second conversion coefficient.
    pub md: i16,
}

/// Convert a raw temperature reading to tenth-degrees Celsius
/// (`measure`, `bmp085.c:382-385`: x1 from raw and ac5/ac6, x2
/// from mc/md, b5 their sum, t the scaled result).
pub fn temperature_tenth_degrees(cal: Calibration, raw: u16) -> i32 {
    let x1 = ((raw as i32 - cal.ac6 as i32) * cal.ac5 as i32) >> 15;
    let x2 = ((cal.mc as i32) << 11) / (x1 + cal.md as i32);
    let b5 = x1 + x2;
    (b5 + 8) >> 4
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_calibration() -> Calibration {
        Calibration { ac5: 32757, ac6: 23153, mc: -8711, md: 2868 }
    }

    #[test]
    fn test_datasheet_example_converts() {
        let cal = sample_calibration();
        assert_eq!(temperature_tenth_degrees(cal, 27898), 150);
    }

    #[test]
    fn test_cold_reads_lower() {
        let cal = sample_calibration();
        let warm = temperature_tenth_degrees(cal, 27898);
        let cold = temperature_tenth_degrees(cal, 25000);
        assert!(cold < warm);
    }
}
