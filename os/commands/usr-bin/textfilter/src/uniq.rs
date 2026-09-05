//! Adjacent duplicate handling behind `uniq`.
//!
//! Ground truth: `minix3/usr.bin/uniq/uniq.c` (257 lines; flag variables
//! `cflag`/`dflag`/`uflag` around line 56, the skip decision at line 188,
//! the count print at line 190). Rules:
//!
//! - Only *adjacent* equal lines collapse (the input is expected sorted;
//!   `uniq` never reorders).
//! - Default prints one copy of each run.
//! - `-c` prefixes each printed line with its run count.
//! - `-d` prints only lines from runs longer than one.
//! - `-u` prints only lines from runs of exactly one.
//!
//! The handler is a small state machine fed one line at a time: it reports
//! completed runs through a callback, so arbitrarily long inputs stream
//! without buffering.

use crate::TextError;

/// Which runs the handler reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UniqMode {
    /// Print one copy of every run.
    All,
    /// Print one copy prefixed with the run count (`-c`).
    Counted,
    /// Print only repeated lines (`-d`).
    Repeated,
    /// Print only unique lines (`-u`).
    Unique,
}

/// One completed run handed to the reporter: the line plus its count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Run<'a> {
    /// The repeated line.
    pub line: &'a str,
    /// How many adjacent copies formed the run.
    pub count: u64,
}

/// Adjacent duplicate handler: feed lines in order, collect runs.
pub struct Uniq<'a> {
    mode: UniqMode,
    previous: Option<&'a str>,
    count: u64,
}

impl<'a> Uniq<'a> {
    /// A new handler in `mode`.
    pub fn new(mode: UniqMode) -> Self {
        Uniq {
            mode,
            previous: None,
            count: 0,
        }
    }

    /// Offer one line; returns a completed run when the run breaks.
    pub fn push(&mut self, line: &'a str) -> Option<Run<'a>> {
        match self.previous {
            Some(previous) if previous == line => {
                self.count += 1;
                None
            }
            _ => {
                let finished = self.previous.map(|previous| Run {
                    line: previous,
                    count: self.count,
                });
                self.previous = Some(line);
                self.count = 1;
                finished
            }
        }
    }

    /// The trailing run after the last push, if any input arrived.
    pub fn finish(&mut self) -> Option<Run<'a>> {
        let finished = self.previous.map(|previous| Run {
            line: previous,
            count: self.count,
        });
        self.previous = None;
        self.count = 0;
        finished
    }

    /// Decide whether `run` is reported under the handler's mode.
    pub fn keep(&self, run: &Run<'_>) -> bool {
        match self.mode {
            UniqMode::All | UniqMode::Counted => true,
            UniqMode::Repeated => run.count > 1,
            UniqMode::Unique => run.count == 1,
        }
    }

    /// Format one kept run into `out` (`-c` prefixes the count and a
    /// blank), returning the used byte count.
    pub fn format(&self, run: &Run<'_>, out: &mut [u8]) -> Result<usize, TextError> {
        let mut written = 0;
        if self.mode == UniqMode::Counted {
            written = write_decimal(run.count, out, written)?;
            written = write_byte(out, written, b' ')?;
        }
        for byte in run.line.as_bytes() {
            written = write_byte(out, written, *byte)?;
        }
        Ok(written)
    }
}

fn write_byte(out: &mut [u8], mut written: usize, byte: u8) -> Result<usize, TextError> {
    if written >= out.len() {
        return Err(TextError::TooLong);
    }
    out[written] = byte;
    written += 1;
    Ok(written)
}

fn write_decimal(mut value: u64, out: &mut [u8], mut written: usize) -> Result<usize, TextError> {
    if value == 0 {
        return write_byte(out, written, b'0');
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
        written = write_byte(out, written, digits[count])?;
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(mode: UniqMode, lines: &[&str]) -> Vec<(String, u64)> {
        let mut handler = Uniq::new(mode);
        let mut runs = Vec::new();
        for line in lines {
            if let Some(run) = handler.push(line) {
                if handler.keep(&run) {
                    runs.push((run.line.to_string(), run.count));
                }
            }
        }
        if let Some(run) = handler.finish() {
            if handler.keep(&run) {
                runs.push((run.line.to_string(), run.count));
            }
        }
        runs
    }

    #[test]
    fn test_collapses_adjacent() {
        assert_eq!(
            collect(UniqMode::All, &["a", "a", "b", "a"]),
            [("a".to_string(), 2), ("b".to_string(), 1), ("a".to_string(), 1)]
        );
    }

    #[test]
    fn test_repeated_and_unique_modes() {
        let lines = ["a", "a", "b"];
        assert_eq!(
            collect(UniqMode::Repeated, &lines),
            [("a".to_string(), 2)]
        );
        assert_eq!(
            collect(UniqMode::Unique, &lines),
            [("b".to_string(), 1)]
        );
    }

    #[test]
    fn test_counted_format() {
        let handler = Uniq::new(UniqMode::Counted);
        let run = Run { line: "a", count: 12 };
        let mut out = [0u8; 16];
        let len = handler.format(&run, &mut out).unwrap();
        assert_eq!(&out[..len], b"12 a");
    }

    #[test]
    fn test_empty_input_no_runs() {
        let mut handler = Uniq::new(UniqMode::All);
        assert_eq!(handler.finish(), None);
    }
}
