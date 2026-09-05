//! Exported files: one pin becomes up to four virtual files.
//!
//! C correspondence: `add_gpio_inode` in
//! `minix3/minix/drivers/system/gpio/gpio.c` (name plus `On`/`Off` for
//! outputs, name plus `Intr` for inputs) and `read_hook` (on/off drive,
//! interrupt versus level read, `"%d\n"` rendering with offset end-of-file,
//! `DATA_SIZE 26`).
//!
//! The virtual filesystem owns the inodes; this module owns the naming
//! and rendering rules.

/// Read buffer size in bytes.
///
/// C: `DATA_SIZE 26` (`gpio.c`).
pub const READ_BUFFER: usize = 26;

/// Which virtual file of a pin a read targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportedFile {
    /// `name`: read the level.
    Read,
    /// `nameOn`: drive high (read drives, answers zero bytes).
    TurnOn,
    /// `nameOff`: drive low.
    TurnOff,
    /// `nameIntr`: read the latched interrupt flag.
    Interrupt,
}

impl ExportedFile {
    /// File-name suffix for this export (empty for the plain read).
    pub const fn suffix(self) -> &'static str {
        match self {
            ExportedFile::Read => "",
            ExportedFile::TurnOn => "On",
            ExportedFile::TurnOff => "Off",
            ExportedFile::Interrupt => "Intr",
        }
    }

    /// Full file name for this pin and export.
    pub fn file_name(pin_name: &str, file: ExportedFile) -> alloc::string::String {
        let mut name = alloc::string::String::from(pin_name);
        name.push_str(file.suffix());
        name
    }
}

/// What reading one exported file does, decided without hardware.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadPlan {
    /// Drive high, answer zero bytes.
    DriveHigh,
    /// Drive low, answer zero bytes.
    DriveLow,
    /// Sample the level and render it.
    SampleLevel,
    /// Sample the interrupt flag and render it.
    SampleInterrupt,
}

/// Decide a read of one exported file.
///
/// C: the branch structure of `read_hook` (`gpio.c`): on/off drive and
/// answer zero, interrupt read versus level read otherwise.
pub const fn plan(file: ExportedFile) -> ReadPlan {
    match file {
        ExportedFile::TurnOn => ReadPlan::DriveHigh,
        ExportedFile::TurnOff => ReadPlan::DriveLow,
        ExportedFile::Interrupt => ReadPlan::SampleInterrupt,
        ExportedFile::Read => ReadPlan::SampleLevel,
    }
}

/// Render a level as `"%d\n"` and apply the offset rule.
///
/// C: `snprintf(ptr, DATA_SIZE, "%d\n", value)` then end-of-file at or
/// past the end, memmove for a positive offset (`read_hook`, `gpio.c`).
/// Returns the bytes the reader takes (at most two).
pub fn render(value: bool, offset: usize) -> (u8, Option<u8>, usize) {
    let digit = if value { b'1' } else { b'0' };
    match offset {
        0 => (digit, Some(b'\n'), 2),
        1 => (b'\n', None, 1),
        _ => (0, None, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_suffixes_match_c_names() {
        assert_eq!(ExportedFile::Read.suffix(), "");
        assert_eq!(ExportedFile::TurnOn.suffix(), "On");
        assert_eq!(ExportedFile::TurnOff.suffix(), "Off");
        assert_eq!(ExportedFile::Interrupt.suffix(), "Intr");
        assert_eq!(
            ExportedFile::file_name("USR0", ExportedFile::TurnOn).as_str(),
            "USR0On"
        );
        assert_eq!(
            ExportedFile::file_name("Button", ExportedFile::Interrupt).as_str(),
            "ButtonIntr"
        );
    }

    #[test]
    fn test_plans_cover_all_four_exports() {
        assert_eq!(plan(ExportedFile::TurnOn), ReadPlan::DriveHigh);
        assert_eq!(plan(ExportedFile::TurnOff), ReadPlan::DriveLow);
        assert_eq!(plan(ExportedFile::Interrupt), ReadPlan::SampleInterrupt);
        assert_eq!(plan(ExportedFile::Read), ReadPlan::SampleLevel);
    }

    #[test]
    fn test_render_applies_offset_rule() {
        assert_eq!(render(true, 0), (b'1', Some(b'\n'), 2));
        assert_eq!(render(false, 0), (b'0', Some(b'\n'), 2));
        assert_eq!(render(true, 1), (b'\n', None, 1));
        assert_eq!(render(true, 2), (0, None, 0));
    }

    #[test]
    fn test_read_buffer_matches_c_header() {
        assert_eq!(READ_BUFFER, 26);
    }
}
