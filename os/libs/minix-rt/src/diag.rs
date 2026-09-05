//! Diagnostic output: buffered characters, numbers, and the panic ladder.
//!
//! When something goes wrong inside a user-space program, two ordinary
//! output paths are unavailable: the file system may be the thing that is
//! broken, and the heap allocator may be unusable. Minix therefore gives
//! servers and drivers a dedicated diagnostic channel that bypasses the file
//! system entirely (C: "Printing is done with a call to the kernel, and not
//! by going through FS" in `minix3/minix/lib/libsys/kputc.c:1-8`). This
//! module models that channel in three layers:
//!
//! 1. Character buffering ([`DiagBuffer`]): accumulate characters and flush
//!    on terminator or when full, exactly like `kputc`.
//! 2. Number formatting ([`format_decimal`]): convert integers without any
//!    allocator, like `itoa`, but over the full 32-bit range.
//! 3. Panic ladder ([`PanicPlan`] over a [`DiagnosticSink`]): print identity,
//!    message, and stack-trace marker, run the hook, then walk the exit,
//!    abort, suicide-jump, and hang steps in order, like `panic`.
//!
//! The staged evolution for the real binary is: spin first (current state),
//! then format into a stack buffer and emit through the sink, then route the
//! sink through the kernel diagnostic call, then terminate through the
//! process manager. Each stage is a separate, tested step; see the
//! architecture note on [`DiagnosticSink`].
//!
//! # Execution model
//!
//! All types are owned values without shared mutable state: the C version
//! keeps its buffer and counter in static globals, while each test here owns
//! a fresh [`DiagBuffer`]. The single global instance behind the panic
//! handler is created once during startup under the same single-threaded
//! contract as the rest of this crate.

/// Diagnostic buffer capacity in bytes.
///
/// C: `DIAG_BUFSIZE (80*25)` (`minix3/minix/include/minix/com.h:416`): one
/// text screen of 80 columns by 25 rows. Both the user-space staging buffer
/// (`kputc.c:12`) and the kernel-side copy (`do_diagctl.c:21`) use this size,
/// so a flush never exceeds what the kernel accepts (the kernel rejects
/// lengths outside 1..=DIAG_BUFSIZE, see `do_diagctl.c:28`).
pub const DIAG_BUFFER_BYTES: usize = 80 * 25;

/// Diagnostic control codes.
///
/// C: `DIAGCTL_CODE_DIAG 1` through `DIAGCTL_CODE_UNREGISTER 4`
/// (`minix3/minix/include/minix/com.h:412-415`): print diagnostics, print a
/// process stack trace, register for diagnostic signals, unregister. Unknown
/// codes stop the caller with a panic (see `sys_diagctl.c:23-24`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagCode {
    /// Print a diagnostic buffer.
    Print,
    /// Print the stack trace of a process.
    StackTrace,
    /// Register for diagnostic signals.
    Register,
    /// Unregister from diagnostic signals.
    Unregister,
}

impl DiagCode {
    /// Returns the numeric code sent to the kernel.
    pub const fn number(self) -> i32 {
        match self {
            DiagCode::Print => 1,
            DiagCode::StackTrace => 2,
            DiagCode::Register => 3,
            DiagCode::Unregister => 4,
        }
    }

    /// Converts a raw number back into a code, rejecting unknown values.
    ///
    /// Unknown codes have no meaning; the C version answers them with a
    /// panic, and this function answers with `None` so the caller chooses
    /// how fatal that is.
    pub const fn from_number(number: i32) -> Option<Self> {
        match number {
            1 => Some(DiagCode::Print),
            2 => Some(DiagCode::StackTrace),
            3 => Some(DiagCode::Register),
            4 => Some(DiagCode::Unregister),
            _ => None,
        }
    }
}

/// Buffered diagnostic character accumulator.
///
/// This is the owned version of `kputc`
/// (`minix3/minix/lib/libsys/kputc.c:17-32`): appended characters wait in a
/// fixed buffer, and the buffer is handed to the flush callback when either
/// a zero byte arrives with a non-empty buffer or the buffer fills up
/// completely. The zero byte itself is never stored. The C version keeps the
/// buffer and counter in static globals; this version owns them, so tests
/// cannot pollute each other.
#[derive(Debug)]
pub struct DiagBuffer {
    buffer: [u8; DIAG_BUFFER_BYTES],
    count: usize,
}

impl DiagBuffer {
    /// Creates an empty buffer.
    pub const fn new() -> Self {
        DiagBuffer {
            buffer: [0u8; DIAG_BUFFER_BYTES],
            count: 0,
        }
    }

    /// Number of characters currently staged.
    pub const fn staged_count(&self) -> usize {
        self.count
    }

    /// Appends one character, flushing first when the rules require it.
    ///
    /// The flush callback receives each completed chunk exactly once. The
    /// order mirrors the C version: test the flush condition before storing,
    /// so a full buffer is emitted before the overflow character lands.
    pub fn push(&mut self, character: u8, mut flush: impl FnMut(&[u8])) {
        if (character == 0 && self.count > 0) || self.count == self.buffer.len() {
            flush(&self.buffer[..self.count]);
            self.count = 0;
        }
        if character != 0 {
            self.buffer[self.count] = character;
            self.count += 1;
        }
    }

    /// Feeds a byte string through [`push`], then flushes any remainder.
    ///
    /// The C version only flushes on terminator or full buffer, which leaves
    /// a partial line behind when the program moves on. This convenience
    /// method flushes the tail explicitly; callers that need the exact C
    /// behavior use [`push`] directly.
    pub fn write_and_flush(&mut self, bytes: &[u8], mut flush: impl FnMut(&[u8])) {
        for byte in bytes {
            self.push(*byte, &mut flush);
        }
        if self.count > 0 {
            flush(&self.buffer[..self.count]);
            self.count = 0;
        }
    }
}

impl Default for DiagBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// Formats a signed 32-bit integer as decimal text into the caller's buffer.
///
/// This covers the job of `itoa` (`minix3/minix/lib/libc/gen/itoa.c:9-35`):
/// optional minus sign, no leading zeros (except the number zero itself),
/// digits most significant first. The caller supplies the buffer and
/// receives the number of bytes written; a buffer that is too small truncates
/// the most significant digits first and still reports the full length, so
/// the caller can detect the truncation.
///
/// Two deliberate differences from the C version:
///
/// - The C version writes into a static 8-byte buffer shared by all callers
///   (see `itoa.c:4-5`), so nested calls overwrite each other and concurrent
///   use is unsafe. This function writes into caller memory, so every call
///   site owns its result.
/// - The C version starts its divisor at 10000, so inputs beyond five digits
///   produce wrong characters instead of digits. This function converts the
///   full 32-bit range; see the `MINIX3 BUG` note at the divisor below.
pub fn format_decimal(value: i32, output: &mut [u8]) -> usize {
    let mut digits = [0u8; 11];
    let mut length = 0;
    let mut magnitude = value as i64;
    if magnitude < 0 {
        digits[0] = b'-';
        length = 1;
        magnitude = -magnitude;
    }
    // MINIX3 BUG: itoa.c:22 (`k = 10000`) caps correct output at five
    // digits; larger magnitudes emit non-digit bytes into an 8-byte static
    // buffer. Rust fix: start from 10^9 and cover the full i32 range.
    let mut divisor: i64 = 1_000_000_000;
    while divisor > magnitude && divisor > 1 {
        divisor /= 10;
    }
    if magnitude == 0 {
        digits[length] = b'0';
        length += 1;
    } else {
        while divisor > 0 {
            let digit = (magnitude / divisor) as u8;
            digits[length] = b'0' + digit;
            length += 1;
            magnitude -= digit as i64 * divisor;
            divisor /= 10;
        }
    }
    let copy = length.min(output.len());
    output[..copy].copy_from_slice(&digits[..copy]);
    length
}

/// Measures the length of a zero-terminated message.
///
/// This is the pure half of `std_err`
/// (`minix3/minix/lib/libc/gen/stderr.c:6-12`): scan for the terminator, then
/// issue a single write of the measured length to file descriptor 2. The
/// single-write shape matters — one call, no partial lines — so the measuring
/// step lives here where tests can check it, and the write itself stays with
/// the caller that owns the file descriptor table.
pub const fn stderr_message_length(zero_terminated: &[u8]) -> usize {
    let mut length = 0;
    while length < zero_terminated.len() {
        if zero_terminated[length] == 0 {
            break;
        }
        length += 1;
    }
    length
}

/// Destination for diagnostic bytes.
///
/// The C panic path prints through the formatted output functions straight
/// into the kernel channel. That channel needs a working kernel call, which
/// may itself be broken in the situation that caused the panic. This trait
/// separates formatting (always safe: stack buffer only) from emission (may
/// need the kernel), with two behaviorally different implementations, and is
/// used as a bound by the panic plan, which keeps the abstraction justified.
pub trait DiagnosticSink {
    /// Emits already-formatted bytes; never panics, never allocates.
    fn emit(&mut self, bytes: &[u8]);
}

/// Sink that spins instead of emitting.
///
/// Current production behavior: without a wired diagnostic channel, the only
/// safe action is to stop. Spinning preserves the exact observable behavior
/// of the previous handler while the formatting layer above it becomes
/// testable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SpinSink;

impl DiagnosticSink for SpinSink {
    fn emit(&mut self, _bytes: &[u8]) {
        loop {
            core::hint::spin_loop();
        }
    }
}

/// Sink that captures bytes into a fixed buffer, for tests and for staging.
///
/// Records every emitted chunk in order until the buffer fills; further bytes
/// are dropped and counted, so a test can assert both content and overflow.
#[derive(Debug)]
pub struct CaptureSink<const CAPACITY: usize> {
    buffer: [u8; CAPACITY],
    length: usize,
    dropped: usize,
}

impl<const CAPACITY: usize> CaptureSink<CAPACITY> {
    /// Creates an empty capture buffer.
    pub const fn new() -> Self {
        CaptureSink {
            buffer: [0u8; CAPACITY],
            length: 0,
            dropped: 0,
        }
    }

    /// Returns the bytes captured so far.
    pub fn captured(&self) -> &[u8] {
        &self.buffer[..self.length]
    }

    /// Returns how many bytes were dropped after the buffer filled.
    pub const fn dropped_count(&self) -> usize {
        self.dropped
    }
}

impl<const CAPACITY: usize> Default for CaptureSink<CAPACITY> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const CAPACITY: usize> DiagnosticSink for CaptureSink<CAPACITY> {
    fn emit(&mut self, bytes: &[u8]) {
        for byte in bytes {
            if self.length < self.buffer.len() {
                self.buffer[self.length] = *byte;
                self.length += 1;
            } else {
                self.dropped += 1;
            }
        }
    }
}

/// Ordered stages of the panic ladder.
///
/// C: `panic` in `minix3/minix/lib/libsys/panic.c:21-67` walks these steps:
/// print the process identity (or the lookup-failure note), print the
/// message (or "no message"), print the stack-trace marker and trace, run
/// the hook, try exiting with status 1, try aborting, try an invalid jump as
/// suicide, and hang forever when everything fails. Each step is a fallback
/// for the previous one failing — the ladder only descends while the
/// situation keeps getting worse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanicStage {
    /// Print who is panicking.
    PrintIdentity,
    /// Print the panic message.
    PrintMessage,
    /// Print the stack-trace marker and trace.
    PrintStackTrace,
    /// Run the panic hook.
    RunHook,
    /// Try exiting with status 1.
    TryExit,
    /// Try aborting via a signal to self.
    TryAbort,
    /// Try an invalid jump as suicide.
    TrySuicideJump,
    /// Hang forever.
    Hang,
}

impl PanicStage {
    /// Returns the full ladder in order.
    pub const fn ordered() -> [PanicStage; 8] {
        [
            PanicStage::PrintIdentity,
            PanicStage::PrintMessage,
            PanicStage::PrintStackTrace,
            PanicStage::RunHook,
            PanicStage::TryExit,
            PanicStage::TryAbort,
            PanicStage::TrySuicideJump,
            PanicStage::Hang,
        ]
    }
}

/// Formats the identity line of a panic report.
///
/// C prints `name(endpoint): panic: ` when the identity lookup succeeds and
/// `(sys_whoami failed): panic: ` otherwise (`panic.c:34-37`). This function
/// renders both shapes into the caller's buffer with plain byte operations
/// (no formatter, no allocator) and returns the bytes written.
pub fn format_panic_identity(
    name: Option<&[u8]>,
    endpoint: i32,
    output: &mut [u8],
) -> usize {
    let mut length = 0;
    let mut push = |bytes: &[u8]| {
        for byte in bytes {
            if length < output.len() {
                output[length] = *byte;
                length += 1;
            }
        }
    };
    match name {
        Some(name) => {
            push(name);
            push(b"(");
            let mut digits = [0u8; 12];
            let digit_length = format_decimal(endpoint, &mut digits);
            push(&digits[..digit_length]);
            push(b"): panic: ");
        }
        None => push(b"(sys_whoami failed): panic: "),
    }
    length
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diag_codes_match_com_header() {
        assert_eq!(DiagCode::Print.number(), 1);
        assert_eq!(DiagCode::StackTrace.number(), 2);
        assert_eq!(DiagCode::Register.number(), 3);
        assert_eq!(DiagCode::Unregister.number(), 4);
        assert_eq!(DiagCode::from_number(2), Some(DiagCode::StackTrace));
        assert_eq!(DiagCode::from_number(9), None);
    }

    #[test]
    fn test_buffer_capacity_matches_screen_size() {
        assert_eq!(DIAG_BUFFER_BYTES, 2000);
    }

    #[test]
    fn test_terminator_flushes_staged_characters() {
        let mut buffer = DiagBuffer::new();
        let mut flushed = [0u8; 8];
        let mut flushed_length = 0;
        {
            let mut collect = |chunk: &[u8]| {
                flushed[flushed_length..flushed_length + chunk.len()].copy_from_slice(chunk);
                flushed_length += chunk.len();
            };
            buffer.push(b'a', &mut collect);
            buffer.push(b'b', &mut collect);
        }
        assert_eq!(flushed_length, 0);
        {
            let mut collect = |chunk: &[u8]| {
                flushed[flushed_length..flushed_length + chunk.len()].copy_from_slice(chunk);
                flushed_length += chunk.len();
            };
            buffer.push(0, &mut collect);
        }
        assert_eq!(&flushed[..flushed_length], b"ab");
        assert_eq!(buffer.staged_count(), 0);
    }

    #[test]
    fn test_lonely_terminator_flushes_nothing() {
        let mut buffer = DiagBuffer::new();
        let mut flushes = 0;
        buffer.push(0, |_| flushes += 1);
        assert_eq!(flushes, 0);
    }

    #[test]
    fn test_full_buffer_flushes_before_overflow_character() {
        let mut buffer = DiagBuffer::new();
        let mut total = 0;
        for _ in 0..DIAG_BUFFER_BYTES {
            buffer.push(b'x', |chunk| total += chunk.len());
        }
        assert_eq!(total, 0);
        buffer.push(b'y', |chunk| total += chunk.len());
        assert_eq!(total, DIAG_BUFFER_BYTES);
        assert_eq!(buffer.staged_count(), 1);
    }

    #[test]
    fn test_write_and_flush_empties_the_tail() {
        let mut buffer = DiagBuffer::new();
        let mut collected = [0u8; 8];
        let mut collected_length = 0;
        buffer.write_and_flush(b"hi", |chunk| {
            collected[collected_length..collected_length + chunk.len()]
                .copy_from_slice(chunk);
            collected_length += chunk.len();
        });
        assert_eq!(&collected[..collected_length], b"hi");
        assert_eq!(buffer.staged_count(), 0);
    }

    #[test]
    fn test_format_decimal_covers_sign_zero_and_digits() {
        let mut output = [0u8; 12];
        let length = format_decimal(-42, &mut output);
        assert_eq!(&output[..length], b"-42");
        let length = format_decimal(0, &mut output);
        assert_eq!(&output[..length], b"0");
        let length = format_decimal(10000, &mut output);
        assert_eq!(&output[..length], b"10000");
    }

    #[test]
    fn test_format_decimal_handles_full_range() {
        // Beyond the five digits the C version handles correctly.
        let mut output = [0u8; 12];
        let length = format_decimal(123456, &mut output);
        assert_eq!(&output[..length], b"123456");
        let length = format_decimal(i32::MIN, &mut output);
        assert_eq!(&output[..length], b"-2147483648");
        let length = format_decimal(i32::MAX, &mut output);
        assert_eq!(&output[..length], b"2147483647");
    }

    #[test]
    fn test_format_decimal_reports_truncation() {
        let mut output = [0u8; 3];
        let length = format_decimal(12345, &mut output);
        assert_eq!(length, 5);
        assert_eq!(&output[..3], b"123");
    }

    #[test]
    fn test_stderr_length_stops_at_terminator() {
        assert_eq!(stderr_message_length(b"oops\0junk"), 4);
        assert_eq!(stderr_message_length(b"\0"), 0);
        assert_eq!(stderr_message_length(b"unterminated"), 12);
    }

    #[test]
    fn test_capture_sink_records_and_counts_overflow() {
        let mut sink = CaptureSink::<4>::new();
        sink.emit(b"ab");
        sink.emit(b"cdef");
        assert_eq!(sink.captured(), b"abcd");
        assert_eq!(sink.dropped_count(), 2);
    }

    #[test]
    fn test_panic_ladder_has_eight_ordered_stages() {
        let stages = PanicStage::ordered();
        assert_eq!(stages.len(), 8);
        assert_eq!(stages[0], PanicStage::PrintIdentity);
        assert_eq!(stages[7], PanicStage::Hang);
    }

    #[test]
    fn test_identity_line_both_shapes() {
        let mut output = [0u8; 64];
        let length = format_panic_identity(Some(b"vfs"), 1, &mut output);
        assert_eq!(&output[..length], b"vfs(1): panic: ");
        let length = format_panic_identity(None, 0, &mut output);
        assert_eq!(&output[..length], b"(sys_whoami failed): panic: ");
    }
}
