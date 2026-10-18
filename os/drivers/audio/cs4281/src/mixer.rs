//! The mixer: the AC97 codec protocol and the volume table.
//!
//! C correspondence: `minix3/minix/drivers/audio/cs4281/mixer.c` with
//! `MIXER_AC97` defined (`cs4281.h:7`), so the active half is
//! `get_set_volume` (`mixer.c:158-220`) and `dev_set_default_volume`
//! (`mixer.c:222-272`); the `MIXER_AK4531` and `MIXER_SB16` halves are
//! compiled out in C and are not modeled.
//!
//! The codec is reached through the card's register window
//! (`dev_mixer_write`/`dev_mixer_read`, `cs4281.c:47-91`): the register
//! index goes to `REG_CODEC_ADDR`, the value to `REG_CODEC_DATA`, and a
//! control word starts the transfer; the driver then polls the control
//! register for the codec's done bit (fifty thousand tries, ten
//! microseconds apart). A read is a second transfer whose answer arrives
//! in `REG_CODEC_SDA`, gated by the status register's valid bit.

use minix_audiodriver::sound::{Device, VolumeLevel};

use crate::registers::{
    AC97_READY_DELAY_MICROS, CODEC_POLL_DELAY_MICROS, CODEC_POLL_LIMIT, REG_CODEC_ADDR,
    REG_CODEC_CTRL, REG_CODEC_DATA, REG_CODEC_SDA, REG_CODEC_STATUS, STS_CODEC_DONE,
    STS_CODEC_VALID,
};
use crate::window::RegisterWindow;

/// Master volume (`AC97_MASTER_VOLUME`).
pub const AC97_MASTER_VOLUME: u32 = 0x02;
/// PCM output volume (`AC97_PCM_OUT_VOLUME`).
pub const AC97_PCM_OUT_VOLUME: u32 = 0x18;
/// Compact disc volume (`AC97_CD_VOLUME`).
pub const AC97_CD_VOLUME: u32 = 0x12;
/// Line input volume (`AC97_LINE_IN_VOLUME`).
pub const AC97_LINE_IN_VOLUME: u32 = 0x10;
/// Microphone volume (`AC97_MIC_VOLUME`).
pub const AC97_MIC_VOLUME: u32 = 0x0e;
/// Mono output volume (`AC97_MONO_VOLUME`).
pub const AC97_MONO_VOLUME: u32 = 0x06;
/// Phone volume (`AC97_PHONE_VOLUME`).
pub const AC97_PHONE_VOLUME: u32 = 0x0c;
/// Auxiliary input volume (`AC97_AUX_IN_VOLUME`).
pub const AC97_AUX_IN_VOLUME: u32 = 0x16;
/// Record gain volume (`AC97_RECORD_GAIN_VOLUME`).
pub const AC97_RECORD_GAIN_VOLUME: u32 = 0x1c;
/// Record select (`AC97_RECORD_SELECT`).
pub const AC97_RECORD_SELECT: u32 = 0x1a;
/// General purpose (`AC97_GENERAL_PURPOSE`).
pub const AC97_GENERAL_PURPOSE: u32 = 0x20;
/// Power-down (`AC97_POWERDOWN`).
pub const AC97_POWERDOWN: u32 = 0x26;

/// The control word that starts a codec write (`cs4281.c:51`).
pub const CODEC_CTRL_WRITE: u32 = 0x0e;
/// The control word that starts a codec read (`cs4281.c:63`).
pub const CODEC_CTRL_READ: u32 = 0x1e;

/// The AC97 mixer: the card's register window plus the codec protocol.
pub struct Ac97<W: RegisterWindow> {
    window: W,
}

/// What one codec transfer reported (`dev_mixer_write`/`dev_mixer_read`'s
/// polls): C prints a warning when a poll runs out and continues.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodecAnswer {
    /// The transfer completed within the poll limit.
    pub done: bool,
    /// The read's answer was valid within the poll limit (always true for
    /// a write).
    pub valid: bool,
    /// The value the codec returned (a read only).
    pub value: u32,
}

impl<W: RegisterWindow> Ac97<W> {
    /// A codec over the card's register window.
    pub fn new(window: W) -> Self {
        Ac97 { window }
    }

    /// The register window (tests and the binary's diagnostics).
    pub fn window_mut(&mut self) -> &mut W {
        &mut self.window
    }

    /// Write one codec register (`dev_mixer_write`, `cs4281.c:47-61`).
    pub fn write(&mut self, reg: u32, value: u32) -> CodecAnswer {
        self.window.write32(REG_CODEC_ADDR, reg);
        self.window.write32(REG_CODEC_DATA, value);
        self.window.write32(REG_CODEC_CTRL, CODEC_CTRL_WRITE);
        let mut done = false;
        for _ in 0..CODEC_POLL_LIMIT {
            self.window.delay_micros(CODEC_POLL_DELAY_MICROS);
            if self.window.read32(REG_CODEC_CTRL) & STS_CODEC_DONE == 0 {
                done = true;
                break;
            }
        }
        CodecAnswer {
            done,
            valid: true,
            value: 0,
        }
    }

    /// Read one codec register (`dev_mixer_read`, `cs4281.c:64-91`): the
    /// transfer, the done poll, the valid poll, then the serial data
    /// register.
    pub fn read(&mut self, reg: u32) -> CodecAnswer {
        // C reads the serial data register and discards the value
        // (`cs4281.c:66`).
        let _ = self.window.read32(REG_CODEC_SDA);
        self.window.write32(REG_CODEC_ADDR, reg & 0xff);
        self.window.write32(REG_CODEC_DATA, 0);
        self.window.write32(REG_CODEC_CTRL, CODEC_CTRL_READ);
        let mut done = false;
        for _ in 0..CODEC_POLL_LIMIT {
            self.window.delay_micros(CODEC_POLL_DELAY_MICROS);
            if self.window.read32(REG_CODEC_CTRL) & STS_CODEC_DONE == 0 {
                done = true;
                break;
            }
        }
        let mut valid = false;
        for _ in 0..CODEC_POLL_LIMIT {
            self.window.delay_micros(CODEC_POLL_DELAY_MICROS);
            if self.window.read32(REG_CODEC_STATUS) & STS_CODEC_VALID != 0 {
                valid = true;
                break;
            }
        }
        CodecAnswer {
            done,
            valid,
            value: self.window.read32(REG_CODEC_SDA),
        }
    }
}

/// One device's codec register (`get_set_volume`'s switch,
/// `mixer.c:162-194`). The AC97 table refuses `Dac`, `Speaker`, `Treble`,
/// and `Bass` outright.
fn device_register(device: Device) -> Option<u32> {
    let reg = match device {
        Device::Master => AC97_MASTER_VOLUME,
        Device::Dac => return None,
        Device::Fm => AC97_PCM_OUT_VOLUME,
        Device::Cd => AC97_CD_VOLUME,
        Device::Line => AC97_LINE_IN_VOLUME,
        Device::Mic => AC97_MIC_VOLUME,
        Device::Speaker => return None,
        Device::Treble => return None,
        Device::Bass => return None,
    };
    Some(reg)
}

/// The largest level the AC97 mixer accepts (`mixer.c:161`).
pub const AC97_MAX_LEVEL: i32 = 0x1f;

impl<W: RegisterWindow> Ac97<W> {
    /// Get or set one device's volume (`get_set_volume`,
    /// `mixer.c:158-220`).
    ///
    /// The AC97 convention is attenuation: a set stores
    /// `(0x1f - left) << 8 | (0x1f - right)` in one sixteen-bit word, and
    /// a get complements both halves back into levels, clamping each into
    /// the accepted range first.
    pub fn get_set_volume(&mut self, level: &mut VolumeLevel, set: bool) -> i32 {
        let Some(reg) = device_register(level.device) else {
            return -minix_types::EINVAL;
        };
        if set {
            level.right = level.right.clamp(0, AC97_MAX_LEVEL);
            level.left = level.left.clamp(0, AC97_MAX_LEVEL);
            let data =
                (((AC97_MAX_LEVEL - level.left) << 8) | (AC97_MAX_LEVEL - level.right)) as u32;
            self.write(reg, data);
        } else {
            let answer = self.read(reg);
            let mut left = (answer.value >> 8) as i32 & 0xff;
            let mut right = (answer.value & 0xff) as i32;
            left = left.clamp(0, AC97_MAX_LEVEL);
            right = right.clamp(0, AC97_MAX_LEVEL);
            level.left = AC97_MAX_LEVEL - left;
            level.right = AC97_MAX_LEVEL - right;
        }
        0
    }
}

/// The mixer's power-on values (`dev_set_default_volume`'s `MIXER_AC97`
/// half, `mixer.c:253-268`): register and value pairs, in C's order.
pub const DEFAULT_VOLUMES: [(u32, u32); 11] = [
    (AC97_MASTER_VOLUME, 0x0000),
    (AC97_MONO_VOLUME, 0x8000),
    (AC97_PHONE_VOLUME, 0x8008),
    (AC97_MIC_VOLUME, 0x0000),
    (AC97_LINE_IN_VOLUME, 0x0303),
    (AC97_CD_VOLUME, 0x0808),
    (AC97_AUX_IN_VOLUME, 0x0808),
    (AC97_PCM_OUT_VOLUME, 0x0808),
    (AC97_RECORD_GAIN_VOLUME, 0x0000),
    (AC97_RECORD_SELECT, 0x0000),
    (AC97_GENERAL_PURPOSE, 0x0000),
];

/// Write the default levels (`dev_set_default_volume`), including the
/// power-down probe: C writes zero to the power-down register, then polls
/// up to fifty thousand times (a hundred microseconds apart) for the
/// codec to answer, prints a warning if it never does, and carries on
/// with the eleven writes regardless (`mixer.c:235-241`).
///
/// The report says whether the codec answered; the writes happen either
/// way, exactly as in C.
pub fn set_default_volume<W: RegisterWindow>(codec: &mut Ac97<W>) -> CodecAnswer {
    codec.write(AC97_POWERDOWN, 0x0000);
    let mut ready = false;
    for _ in 0..CODEC_POLL_LIMIT {
        if codec.read(AC97_POWERDOWN).value & 0x03 != 0 {
            ready = true;
            break;
        }
        codec.window_mut().delay_micros(AC97_READY_DELAY_MICROS);
    }
    for (reg, value) in DEFAULT_VOLUMES {
        codec.write(reg, value);
    }
    CodecAnswer {
        done: ready,
        valid: true,
        value: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window::RegisterWindow;
    use alloc::collections::BTreeMap;
    use alloc::vec::Vec;

    /// A register window over a sparse file, with the codec answering
    /// immediately unless told otherwise.
    struct FakeWindow {
        registers: BTreeMap<u32, u32>,
        writes: Vec<(u32, u32)>,
        delays: Vec<u32>,
        /// What the codec's done bit reports.
        busy: bool,
        /// Whether a read's valid bit ever comes up.
        valid: bool,
        /// What the serial data register answers.
        sda: u32,
    }

    impl FakeWindow {
        fn new() -> Self {
            FakeWindow {
                registers: BTreeMap::new(),
                writes: Vec::new(),
                delays: Vec::new(),
                busy: false,
                valid: true,
                sda: 0,
            }
        }
    }

    impl RegisterWindow for FakeWindow {
        fn read8(&mut self, offset: u32) -> u8 {
            self.registers.get(&offset).copied().unwrap_or(0) as u8
        }
        fn read16(&mut self, offset: u32) -> u16 {
            self.registers.get(&offset).copied().unwrap_or(0) as u16
        }
        fn read32(&mut self, offset: u32) -> u32 {
            // The control register reads back the codec's state, not the
            // word that was written to it: C writes `0x0e`/`0x1e` to start
            // a transfer and polls the same register's done bit
            // (`cs4281.c:53-59`), which only works if the store does not
            // read back.
            if offset == REG_CODEC_CTRL {
                return if self.busy { STS_CODEC_DONE } else { 0 };
            }
            if offset == REG_CODEC_STATUS {
                return if self.valid { STS_CODEC_VALID } else { 0 };
            }
            if offset == REG_CODEC_SDA {
                return self.sda;
            }
            self.registers.get(&offset).copied().unwrap_or(0)
        }
        fn write8(&mut self, offset: u32, value: u8) {
            self.writes.push((offset, value as u32));
            self.registers.insert(offset, value as u32);
        }
        fn write16(&mut self, offset: u32, value: u16) {
            self.writes.push((offset, value as u32));
            self.registers.insert(offset, value as u32);
        }
        fn write32(&mut self, offset: u32, value: u32) {
            self.writes.push((offset, value));
            self.registers.insert(offset, value);
        }
        fn delay_micros(&mut self, micros: u32) {
            self.delays.push(micros);
        }
    }

    #[test]
    fn test_a_write_uses_the_codec_port_trio() {
        let mut codec = Ac97::new(FakeWindow::new());
        let answer = codec.write(AC97_MASTER_VOLUME, 0x1234);
        assert!(answer.done);
        let writes = &codec.window_mut().writes;
        assert_eq!(writes[0], (REG_CODEC_ADDR, AC97_MASTER_VOLUME));
        assert_eq!(writes[1], (REG_CODEC_DATA, 0x1234));
        assert_eq!(writes[2], (REG_CODEC_CTRL, CODEC_CTRL_WRITE));
    }

    #[test]
    fn test_a_busy_codec_runs_the_poll_out_and_reports_it() {
        let mut window = FakeWindow::new();
        window.busy = true;
        let mut codec = Ac97::new(window);
        let answer = codec.write(AC97_MASTER_VOLUME, 0);
        assert!(!answer.done, "the poll ran out");
        // The waits happened, ten microseconds each.
        assert_eq!(codec.window_mut().delays.len(), CODEC_POLL_LIMIT as usize);
        assert!(
            codec
                .window_mut()
                .delays
                .iter()
                .all(|d| *d == CODEC_POLL_DELAY_MICROS)
        );
    }

    #[test]
    fn test_a_read_returns_the_serial_data() {
        let mut window = FakeWindow::new();
        window.sda = 0x0a0b;
        let mut codec = Ac97::new(window);
        let answer = codec.read(AC97_CD_VOLUME);
        assert!(answer.done && answer.valid);
        assert_eq!(answer.value, 0x0a0b);
        // The read's control word differs from the write's.
        assert!(
            codec
                .window_mut()
                .writes
                .iter()
                .any(|(port, value)| *port == REG_CODEC_CTRL && *value == CODEC_CTRL_READ)
        );
    }

    #[test]
    fn test_a_set_stores_the_attenuation_word() {
        let mut codec = Ac97::new(FakeWindow::new());
        let mut level = VolumeLevel {
            device: Device::Master,
            left: 0x20,
            right: -3,
        };
        assert_eq!(codec.get_set_volume(&mut level, true), 0);
        assert_eq!(level.left, 0x1f);
        assert_eq!(level.right, 0);
        // (0x1f - 0x1f) << 8 | (0x1f - 0) = 0x1f.
        let data = codec
            .window_mut()
            .writes
            .iter()
            .rev()
            .find(|(port, _)| *port == REG_CODEC_DATA)
            .map(|(_, value)| *value)
            .unwrap();
        assert_eq!(data, 0x001f);
    }

    #[test]
    fn test_a_get_complements_the_word_back_into_levels() {
        let mut window = FakeWindow::new();
        window.sda = 0x1005;
        let mut codec = Ac97::new(window);
        let mut level = VolumeLevel {
            device: Device::Master,
            left: 0,
            right: 0,
        };
        assert_eq!(codec.get_set_volume(&mut level, false), 0);
        assert_eq!(level.left, AC97_MAX_LEVEL - 0x10);
        assert_eq!(level.right, AC97_MAX_LEVEL - 0x05);
    }

    #[test]
    fn test_the_refused_devices_never_reach_the_codec() {
        let mut codec = Ac97::new(FakeWindow::new());
        for device in [Device::Dac, Device::Speaker, Device::Treble, Device::Bass] {
            let mut level = VolumeLevel {
                device,
                left: 5,
                right: 5,
            };
            assert_eq!(codec.get_set_volume(&mut level, true), -minix_types::EINVAL);
            assert_eq!(
                codec.get_set_volume(&mut level, false),
                -minix_types::EINVAL
            );
        }
        assert!(codec.window_mut().writes.is_empty());
    }

    #[test]
    fn test_the_default_volume_writes_the_power_down_probe_and_eleven_registers() {
        let mut window = FakeWindow::new();
        // The power-down probe wants the codec's low two bits set.
        window.sda = 0x03;
        let mut codec = Ac97::new(window);
        let answer = set_default_volume(&mut codec);
        assert!(answer.done, "the codec answered the power-down probe");
        // One power-down write, one read transfer (three writes), then
        // the eleven default writes.
        let codec_data: Vec<u32> = codec
            .window_mut()
            .writes
            .iter()
            .filter(|(port, _)| *port == REG_CODEC_DATA)
            .map(|(_, value)| *value)
            .collect();
        assert_eq!(codec_data[0], 0x0000, "the power-down write");
        assert_eq!(codec_data[1], 0x0000, "the read transfer's data word");
        assert_eq!(codec_data.len(), 2 + DEFAULT_VOLUMES.len());
        // The last write is the general-purpose register's zero.
        let last_addr = codec
            .window_mut()
            .writes
            .iter()
            .filter(|(port, _)| *port == REG_CODEC_ADDR)
            .map(|(_, value)| *value)
            .next_back()
            .unwrap();
        assert_eq!(last_addr, AC97_GENERAL_PURPOSE);
    }
}
