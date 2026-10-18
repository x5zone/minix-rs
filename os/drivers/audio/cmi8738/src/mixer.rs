//! The mixer: the Sound Blaster register pair and the volume table.
//!
//! C correspondence: `minix3/minix/drivers/audio/cmi8738/mixer.c` with
//! `MIXER_SB16` defined (`cmi8738.h:6`), so the active half is
//! `get_set_volume` (`mixer.c:81-160`) and `dev_set_default_volume`
//! (`mixer.c:240-272`); the `MIXER_AK4531` and `MIXER_AC97` halves are
//! compiled out in C and are not modeled.
//!
//! The mixer registers live behind the Sound Blaster address/data pair:
//! `dev_mixer_write` sends the register index to `REG_SB_ADDR` and the
//! value to `REG_SB_DATA`, and `dev_mixer_read` sends the index and reads
//! the data port back (`cmi8738.c:53-66`).

use minix_audiodriver::sound::{Device, VolumeLevel};

use crate::ports::PortIo;
use crate::registers::{REG_SB_ADDR, REG_SB_DATA};

/// Master volume, left (`SB16_MASTER_LEFT`).
pub const SB16_MASTER_LEFT: u8 = 0x30;
/// Master volume, right (`SB16_MASTER_RIGHT`).
pub const SB16_MASTER_RIGHT: u8 = 0x31;
/// Digitized sound, left (`SB16_DAC_LEFT`).
pub const SB16_DAC_LEFT: u8 = 0x32;
/// Digitized sound, right (`SB16_DAC_RIGHT`).
pub const SB16_DAC_RIGHT: u8 = 0x33;
/// FM sound, left (`SB16_FM_LEFT`).
pub const SB16_FM_LEFT: u8 = 0x34;
/// FM sound, right (`SB16_FM_RIGHT`).
pub const SB16_FM_RIGHT: u8 = 0x35;
/// Compact disc input, left (`SB16_CD_LEFT`).
pub const SB16_CD_LEFT: u8 = 0x36;
/// Compact disc input, right (`SB16_CD_RIGHT`).
pub const SB16_CD_RIGHT: u8 = 0x37;
/// Line input, left (`SB16_LINE_LEFT`).
pub const SB16_LINE_LEFT: u8 = 0x38;
/// Line input, right (`SB16_LINE_RIGHT`).
pub const SB16_LINE_RIGHT: u8 = 0x39;
/// Microphone level (`SB16_MIC_LEVEL`).
pub const SB16_MIC_LEVEL: u8 = 0x3a;
/// PC speaker level (`SB16_PC_LEVEL`).
pub const SB16_PC_LEVEL: u8 = 0x3b;
/// Treble, left (`SB16_TREBLE_LEFT`).
pub const SB16_TREBLE_LEFT: u8 = 0x44;
/// Treble, right (`SB16_TREBLE_RIGHT`).
pub const SB16_TREBLE_RIGHT: u8 = 0x45;
/// Bass, left (`SB16_BASS_LEFT`).
pub const SB16_BASS_LEFT: u8 = 0x46;
/// Bass, right (`SB16_BASS_RIGHT`).
pub const SB16_BASS_RIGHT: u8 = 0x47;

/// Write one mixer register (`dev_mixer_write`, `cmi8738.c:53-57`).
pub fn mixer_write<P: PortIo>(ports: &mut P, reg: u8, value: u8) -> Result<(), i32> {
    ports.out8(REG_SB_ADDR, reg)?;
    ports.out8(REG_SB_DATA, value)
}

/// Read one mixer register (`dev_mixer_read`, `cmi8738.c:60-66`).
pub fn mixer_read<P: PortIo>(ports: &mut P, reg: u8) -> Result<u8, i32> {
    ports.out8(REG_SB_ADDR, reg)?;
    ports.in8(REG_SB_DATA)
}

/// The register pair, shift, and ceiling one device's levels use
/// (`get_set_volume`'s switch, `mixer.c:86-131`).
struct MixerDevice {
    /// Left channel register.
    left: u8,
    /// Right channel register.
    right: u8,
    /// How far the level shifts into the register byte.
    shift: u32,
    /// The largest level the device accepts.
    max_level: i32,
}

/// The device table (`get_set_volume`'s switch).
///
/// `Dac` uses the digitized-sound pair here (the `MIXER_AK4531` half is
/// the one that refuses `Dac`), and a device outside the enumerator has
/// no entry at all — C's `default:` arm.
fn device_table(device: Device) -> Option<MixerDevice> {
    let entry = match device {
        Device::Master => MixerDevice {
            left: SB16_MASTER_LEFT,
            right: SB16_MASTER_RIGHT,
            shift: 4,
            max_level: 0x0f,
        },
        Device::Dac => MixerDevice {
            left: SB16_DAC_LEFT,
            right: SB16_DAC_RIGHT,
            shift: 4,
            max_level: 0x0f,
        },
        Device::Fm => MixerDevice {
            left: SB16_FM_LEFT,
            right: SB16_FM_RIGHT,
            shift: 4,
            max_level: 0x0f,
        },
        Device::Cd => MixerDevice {
            left: SB16_CD_LEFT,
            right: SB16_CD_RIGHT,
            shift: 4,
            max_level: 0x0f,
        },
        Device::Line => MixerDevice {
            // C's `Line` arm assigns the left register twice
            // (`mixer.c:113-114`: `cmd_left = SB16_LINE_LEFT; cmd_left =
            // SB16_LINE_RIGHT;`), so both channels of a line-volume
            // request hit `SB16_LINE_RIGHT` and `SB16_LINE_LEFT` is never
            // written. This is a defect in the C driver (the obvious
            // reading is a second `cmd_right` assignment); the rewrite
            // preserves the observed behavior, and the defect is recorded
            // in document `21-audio-drivers.md` so it is not "fixed" by
            // accident later.
            left: SB16_LINE_RIGHT,
            right: SB16_LINE_RIGHT,
            shift: 4,
            max_level: 0x0f,
        },
        Device::Mic => MixerDevice {
            left: SB16_MIC_LEVEL,
            right: SB16_MIC_LEVEL,
            shift: 4,
            max_level: 0x0f,
        },
        Device::Speaker => MixerDevice {
            left: SB16_PC_LEVEL,
            right: SB16_PC_LEVEL,
            shift: 6,
            max_level: 0x03,
        },
        Device::Treble => MixerDevice {
            left: SB16_TREBLE_LEFT,
            right: SB16_TREBLE_RIGHT,
            shift: 4,
            max_level: 0x0f,
        },
        Device::Bass => MixerDevice {
            left: SB16_BASS_LEFT,
            right: SB16_BASS_RIGHT,
            shift: 4,
            max_level: 0x0f,
        },
    };
    Some(entry)
}

/// Get or set one device's volume (`get_set_volume`, `mixer.c:81-160`).
///
/// `set` chooses the direction, exactly as C's `flag`: a set clamps both
/// levels into the device's range and writes them shifted into the
/// register pair; a get reads the pair back and shifts the levels down.
pub fn get_set_volume<P: PortIo>(ports: &mut P, level: &mut VolumeLevel, set: bool) -> i32 {
    let Some(entry) = device_table(level.device) else {
        return -minix_types::EINVAL;
    };
    if set {
        // C clamps right first, then left (`mixer.c:134-146`).
        level.right = level.right.clamp(0, entry.max_level);
        level.left = level.left.clamp(0, entry.max_level);
        if mixer_write(ports, entry.left, (level.left << entry.shift) as u8).is_err() {
            return -minix_types::EIO;
        }
        if mixer_write(ports, entry.right, (level.right << entry.shift) as u8).is_err() {
            return -minix_types::EIO;
        }
    } else {
        let Ok(left) = mixer_read(ports, entry.left) else {
            return -minix_types::EIO;
        };
        let Ok(right) = mixer_read(ports, entry.right) else {
            return -minix_types::EIO;
        };
        level.left = (left >> entry.shift) as i32;
        level.right = (right >> entry.shift) as i32;
    }
    0
}

/// The mixer's power-on levels (`dev_set_default_volume`'s `MIXER_SB16`
/// half, `mixer.c:249-266`): register and value pairs, in C's order.
pub const DEFAULT_VOLUMES: [(u8, u8); 16] = [
    (SB16_MASTER_LEFT, 0x18 << 3),
    (SB16_MASTER_RIGHT, 0x18 << 3),
    (SB16_DAC_LEFT, 0x0f << 4),
    (SB16_DAC_RIGHT, 0x0f << 4),
    (SB16_FM_LEFT, 0x08 << 4),
    (SB16_FM_RIGHT, 0x08 << 4),
    (SB16_CD_LEFT, 0x08 << 4),
    (SB16_CD_RIGHT, 0x08 << 4),
    (SB16_LINE_LEFT, 0x08 << 4),
    (SB16_LINE_RIGHT, 0x08 << 4),
    (SB16_MIC_LEVEL, 0x0f << 4),
    (SB16_PC_LEVEL, 0x02 << 6),
    (SB16_TREBLE_LEFT, 0x08 << 4),
    (SB16_TREBLE_RIGHT, 0x08 << 4),
    (SB16_BASS_LEFT, 0x08 << 4),
    (SB16_BASS_RIGHT, 0x08 << 4),
];

/// Write the default levels (`dev_set_default_volume`).
pub fn set_default_volume<P: PortIo>(ports: &mut P) -> Result<(), i32> {
    for (reg, value) in DEFAULT_VOLUMES {
        mixer_write(ports, reg, value)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeMap;
    use alloc::vec::Vec;

    /// A port seam that models the address/data pair.
    struct FakePorts {
        /// The register the last address write selected.
        selected: u8,
        /// The mixer register file.
        registers: BTreeMap<u8, u8>,
        /// The write log, so the traffic order is visible.
        writes: Vec<(u16, u8)>,
    }

    impl FakePorts {
        fn new() -> Self {
            FakePorts {
                selected: 0,
                registers: BTreeMap::new(),
                writes: Vec::new(),
            }
        }
    }

    impl PortIo for FakePorts {
        fn out8(&mut self, port: u16, value: u8) -> Result<(), i32> {
            self.writes.push((port, value));
            if port == REG_SB_ADDR {
                self.selected = value;
            } else if port == REG_SB_DATA {
                self.registers.insert(self.selected, value);
            }
            Ok(())
        }
        fn out16(&mut self, _port: u16, _value: u16) -> Result<(), i32> {
            Ok(())
        }
        fn out32(&mut self, _port: u16, _value: u32) -> Result<(), i32> {
            Ok(())
        }
        fn in8(&mut self, port: u16) -> Result<u8, i32> {
            if port != REG_SB_DATA {
                return Err(-minix_types::EIO);
            }
            Ok(self.registers.get(&self.selected).copied().unwrap_or(0))
        }
        fn in16(&mut self, _port: u16) -> Result<u16, i32> {
            Ok(0)
        }
        fn in32(&mut self, _port: u16) -> Result<u32, i32> {
            Ok(0)
        }
        fn delay_micros(&mut self, _micros: u32) {}
    }

    #[test]
    fn test_a_set_clamps_and_writes_the_pair() {
        let mut ports = FakePorts::new();
        let mut level = VolumeLevel {
            device: Device::Cd,
            left: 0x20,
            right: -3,
        };
        assert_eq!(get_set_volume(&mut ports, &mut level, true), 0);
        // The levels were clamped into 0..=0x0f and shifted into the byte.
        assert_eq!(level.left, 0x0f);
        assert_eq!(level.right, 0);
        assert_eq!(ports.registers[&SB16_CD_LEFT], 0xf0);
        assert_eq!(ports.registers[&SB16_CD_RIGHT], 0x00);
        // The address port is written before every data port.
        assert_eq!(ports.writes[0], (REG_SB_ADDR, SB16_CD_LEFT));
        assert_eq!(ports.writes[1], (REG_SB_DATA, 0xf0));
        assert_eq!(ports.writes[2], (REG_SB_ADDR, SB16_CD_RIGHT));
        assert_eq!(ports.writes[3], (REG_SB_DATA, 0x00));
    }

    #[test]
    fn test_a_get_reads_the_pair_back() {
        let mut ports = FakePorts::new();
        ports.registers.insert(SB16_MASTER_LEFT, 0xc0);
        ports.registers.insert(SB16_MASTER_RIGHT, 0x30);
        let mut level = VolumeLevel {
            device: Device::Master,
            left: 0,
            right: 0,
        };
        assert_eq!(get_set_volume(&mut ports, &mut level, false), 0);
        assert_eq!(level.left, 0x0c);
        assert_eq!(level.right, 0x03);
    }

    #[test]
    fn test_the_speaker_uses_its_own_shift_and_ceiling() {
        let mut ports = FakePorts::new();
        let mut level = VolumeLevel {
            device: Device::Speaker,
            left: 9,
            right: 1,
        };
        assert_eq!(get_set_volume(&mut ports, &mut level, true), 0);
        assert_eq!(level.left, 3, "the speaker tops out at three");
        assert_eq!(ports.registers[&SB16_PC_LEVEL], 0x40, "shifted by six");
    }

    #[test]
    fn test_the_default_volume_writes_all_sixteen_registers() {
        let mut ports = FakePorts::new();
        assert_eq!(set_default_volume(&mut ports), Ok(()));
        assert_eq!(ports.registers.len(), 16);
        assert_eq!(ports.registers[&SB16_MASTER_LEFT], 0xc0);
        assert_eq!(ports.registers[&SB16_DAC_LEFT], 0xf0);
        assert_eq!(ports.registers[&SB16_PC_LEVEL], 0x80);
        assert_eq!(ports.registers[&SB16_BASS_RIGHT], 0x80);
    }
}
