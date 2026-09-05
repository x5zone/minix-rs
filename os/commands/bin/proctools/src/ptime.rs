//! Duration formatting for `time` and the `ps` elapsed columns.
//!
//! Elapsed times print compactly: seconds alone below a minute
//! (`45s`), minutes and seconds below an hour (`12:34`), hours and minutes
//! below a day (`3:12:34`), days above (`2-04:00:00`). Negative durations
//! are rejected (clocks in this layer never run backward).

use crate::ProcError;

/// Format `seconds` into `out`, returning the used byte count:
///
/// - below 60: `45s`.
/// - below 3600: `mm:ss` (`12:34`).
/// - below 86400: `h:mm:ss` (`3:12:34`, hours unpadded).
/// - at or above: `d-hh:mm:ss` (`2-04:00:00`, days unpadded).
pub fn format_duration(seconds: u64, out: &mut [u8]) -> Result<usize, ProcError> {
    let mut written = 0;
    let mut put = |out: &mut [u8], written: &mut usize, byte: u8| -> Result<(), ProcError> {
        if *written >= out.len() {
            return Err(ProcError::InvalidArgument);
        }
        out[*written] = byte;
        *written += 1;
        Ok(())
    };
    let mut number = |out: &mut [u8], written: &mut usize, mut value: u64| -> Result<(), ProcError> {
        if value == 0 {
            put(out, written, b'0')?;
            return Ok(());
        }
        let mut digits = [0u8; 20];
        let mut count = 0;
        while value > 0 {
            digits[count] = b'0' + (value % 10) as u8;
            value /= 10;
            count += 1;
        }
        while count > 0 {
            count -= 1;
            put(out, written, digits[count])?;
        }
        Ok(())
    };
    let mut pair = |out: &mut [u8], written: &mut usize, value: u64| -> Result<(), ProcError> {
        put(out, written, b'0' + (value / 10) as u8)?;
        put(out, written, b'0' + (value % 10) as u8)?;
        Ok(())
    };
    if seconds < 60 {
        number(out, &mut written, seconds)?;
        put(out, &mut written, b's')?;
    } else if seconds < 3600 {
        pair(out, &mut written, seconds / 60)?;
        put(out, &mut written, b':')?;
        pair(out, &mut written, seconds % 60)?;
    } else if seconds < 86400 {
        number(out, &mut written, seconds / 3600)?;
        put(out, &mut written, b':')?;
        pair(out, &mut written, (seconds / 60) % 60)?;
        put(out, &mut written, b':')?;
        pair(out, &mut written, seconds % 60)?;
    } else {
        number(out, &mut written, seconds / 86400)?;
        put(out, &mut written, b'-')?;
        pair(out, &mut written, (seconds / 3600) % 24)?;
        put(out, &mut written, b':')?;
        pair(out, &mut written, (seconds / 60) % 60)?;
        put(out, &mut written, b':')?;
        pair(out, &mut written, seconds % 60)?;
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn show(seconds: u64) -> String {
        let mut out = [0u8; 32];
        let len = format_duration(seconds, &mut out).unwrap();
        String::from_utf8_lossy(&out[..len]).into_owned()
    }

    #[test]
    fn test_four_shapes() {
        assert_eq!(show(45), "45s");
        assert_eq!(show(754), "12:34");
        assert_eq!(show(11554), "3:12:34");
        assert_eq!(show(187200), "2-04:00:00");
    }

    #[test]
    fn test_boundaries() {
        assert_eq!(show(0), "0s");
        assert_eq!(show(59), "59s");
        assert_eq!(show(60), "01:00");
        assert_eq!(show(3599), "59:59");
        assert_eq!(show(3600), "1:00:00");
        assert_eq!(show(86399), "23:59:59");
        assert_eq!(show(86400), "1-00:00:00");
    }
}
