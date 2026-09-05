//! Head/tail line windows behind one trait.
//!
//! Ground truth: `head` keeps the first N lines, `tail` the last N. Both
//! answer "which lines survive?" from opposite ends, so both implement
//! [`LineWindow`]. The caller feeds lines in order and drains the survivors
//! at the end; the window never holds more than N borrowed lines.

use crate::TextError;

/// One line selection strategy: feed lines in order, drain survivors.
pub trait LineWindow<'a> {
    /// Offer one line (1 based number for diagnostics); survivors stay
    /// inside the window.
    fn push(&mut self, line: &'a str);
    /// Survivors in original order; call once after the last push.
    fn drain(&mut self, out: &mut [&'a str]) -> usize;
    /// How many lines were offered in total.
    fn seen(&self) -> u64;
}

/// Keep the first N lines (`head -n`).
pub struct HeadWindow<'a> {
    lines: [&'a str; 32],
    kept: usize,
    limit: usize,
    seen: u64,
}

impl<'a> HeadWindow<'a> {
    /// A window keeping the first `n` lines (at most 32).
    pub fn new(n: usize) -> Result<Self, TextError> {
        if n == 0 || n > 32 {
            return Err(TextError::InvalidArgument);
        }
        Ok(HeadWindow {
            lines: [""; 32],
            kept: 0,
            limit: n,
            seen: 0,
        })
    }
}

impl<'a> LineWindow<'a> for HeadWindow<'a> {
    fn push(&mut self, line: &'a str) {
        self.seen += 1;
        if self.kept < self.limit {
            self.lines[self.kept] = line;
            self.kept += 1;
        }
    }

    fn drain(&mut self, out: &mut [&'a str]) -> usize {
        let count = self.kept.min(out.len());
        out[..count].copy_from_slice(&self.lines[..count]);
        count
    }

    fn seen(&self) -> u64 {
        self.seen
    }
}

/// Keep the last N lines (`tail -n`): a ring holding the most recent N.
pub struct TailWindow<'a> {
    ring: [&'a str; 32],
    limit: usize,
    seen: u64,
}

impl<'a> TailWindow<'a> {
    /// A window keeping the last `n` lines (at most 32).
    pub fn new(n: usize) -> Result<Self, TextError> {
        if n == 0 || n > 32 {
            return Err(TextError::InvalidArgument);
        }
        Ok(TailWindow {
            ring: [""; 32],
            limit: n,
            seen: 0,
        })
    }
}

impl<'a> LineWindow<'a> for TailWindow<'a> {
    fn push(&mut self, line: &'a str) {
        self.ring[(self.seen as usize) % self.limit] = line;
        self.seen += 1;
    }

    fn drain(&mut self, out: &mut [&'a str]) -> usize {
        let kept = (self.seen as usize).min(self.limit).min(out.len());
        if kept == 0 {
            return 0;
        }
        // Oldest survivor first: it sits right past the write cursor when
        // the ring wrapped, else at slot zero.
        let start = if (self.seen as usize) > self.limit {
            (self.seen as usize) % self.limit
        } else {
            0
        };
        for index in 0..kept {
            out[index] = self.ring[(start + index) % self.limit];
        }
        kept
    }

    fn seen(&self) -> u64 {
        self.seen
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed<'a, W: LineWindow<'a>>(window: &mut W, lines: &[&'a str]) -> [&'a str; 8] {
        for line in lines {
            window.push(line);
        }
        let mut out: [&'a str; 8] = [""; 8];
        let count = window.drain(&mut out);
        assert!(count <= 8);
        out
    }

    #[test]
    fn test_head_keeps_first() {
        let mut window = HeadWindow::new(2).unwrap();
        let out = feed(&mut window, &["a", "b", "c"]);
        assert_eq!(&out[..2], &["a", "b"]);
        assert_eq!(window.seen(), 3);
    }

    #[test]
    fn test_tail_keeps_last() {
        let mut window = TailWindow::new(2).unwrap();
        let out = feed(&mut window, &["a", "b", "c", "d"]);
        assert_eq!(&out[..2], &["c", "d"]);
        assert_eq!(window.seen(), 4);
    }

    #[test]
    fn test_tail_short_input() {
        let mut window = TailWindow::new(5).unwrap();
        let out = feed(&mut window, &["a", "b"]);
        assert_eq!(&out[..2], &["a", "b"]);
    }

    #[test]
    fn test_zero_or_huge_rejected() {
        assert_eq!(HeadWindow::new(0).map(|_| ()), Err(TextError::InvalidArgument));
        assert_eq!(TailWindow::new(33).map(|_| ()), Err(TextError::InvalidArgument));
    }

    #[test]
    fn test_windows_share_the_trait() {
        let mut head = HeadWindow::new(1).unwrap();
        let mut tail = TailWindow::new(1).unwrap();
        {
            let windows: [&mut dyn LineWindow<'_>; 2] = [&mut head, &mut tail];
            for window in windows {
                window.push("x");
                window.push("y");
            }
        }
        let mut out = [""; 2];
        assert_eq!(head.drain(&mut out), 1);
        assert_eq!(out[0], "x");
        assert_eq!(tail.drain(&mut out), 1);
        assert_eq!(out[0], "y");
    }
}
