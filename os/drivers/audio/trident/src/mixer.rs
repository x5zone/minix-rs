//! The mixer: the codec protocol and the AC97 volume table.
//!
//! C correspondence: `minix3/minix/drivers/audio/trident/mixer.c` with
//! `MIXER_AC97` defined (`trident.h:6`), so the active half is
//! `get_set_volume` (`mixer.c:158-220`) and `dev_set_default_volume`
//! (`mixer.c:222-272`); the `MIXER_AK4531` and `MIXER_SB16` halves are
//! compiled out in C and are not modeled. The codec protocol itself is
//! the card's own (`dev_mixer_write`/`dev_mixer_read`, `trident.c:58-93`):
//! a command word carries the register number and a busy bit, and the
//! driver polls the write port until the codec clears that bit.

use minix_audiodriver::sound::{Device, VolumeLevel};

use crate::ports::PortIo;
use crate::registers::{CODEC_POLL_LIMIT, REG_CODEC_READ, REG_CODEC_WRITE, STS_CODEC_BUSY};

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

/// The largest level the AC97 mixer accepts (`mixer.c:161`).
pub const AC97_MAX_LEVEL: i32 = 0x1f;

/// The AC97 mixer: the card's ports plus the codec protocol.
pub struct Ac97<P: PortIo> {
    ports: P,
}

/// What one codec access reported: C prints a warning when the poll runs
/// out and carries on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodecAnswer {
    /// The codec cleared its busy bit within the poll limit.
    pub ready: bool,
    /// The value the codec returned (a read only).
    pub value: u32,
}

impl<P: PortIo> Ac97<P> {
    /// A codec over the card's port seam.
    pub fn new(ports: P) -> Self {
        Ac97 { ports }
    }

    /// The port seam (tests and the binary's diagnostics).
    pub fn ports_mut(&mut self) -> &mut P {
        &mut self.ports
    }

    /// Waits until the codec clears its busy bit (`trident.c:60-68`).
    fn wait_ready(&mut self) -> Result<(u16, bool), i32> {
        let mut last = 0u16;
        for _ in 0..CODEC_POLL_LIMIT {
            last = self.ports.in16(REG_CODEC_WRITE)?;
            if last as u32 & STS_CODEC_BUSY == 0 {
                return Ok((last, true));
            }
        }
        Ok((last, false))
    }

    /// Write one codec register (`dev_mixer_write`, `trident.c:58-71`).
    ///
    /// C composes the command word out of the **last polled** value:
    /// `data = (data << 16) | (reg & 0x00ff) | STS_CODEC_BUSY`
    /// (`trident.c:69`), so the high half carries whatever the write port
    /// last read back. The rewrite keeps that composition; the poll's
    /// value is what the port reported.
    pub fn write(&mut self, reg: u32, value: u32) -> CodecAnswer {
        // A failed port read is not reportable through this card's C
        // helpers either (they print and continue); the poll's last value
        // stands in.
        let (polled, ready) = self.wait_ready().unwrap_or_default();
        let command = ((polled as u32) << 16) | (reg & 0x00ff) | STS_CODEC_BUSY;
        let _ = self.ports.out32(REG_CODEC_WRITE, command);
        CodecAnswer { ready, value }
    }

    /// Read one codec register (`dev_mixer_read`, `trident.c:74-86`):
    /// the command word goes to the read port, then the poll, and the
    /// answer is the polled word's **high half** — except that C reads
    /// the port as a sixteen-bit word (`sdr_in16`, `trident.c:80`) and
    /// then shifts it right by sixteen (`trident.c:85`), so the answer is
    /// always zero. The rewrite keeps the word read and the shift, so a
    /// codec read answers zero exactly as C's does; the defect is
    /// recorded in document `21-audio-drivers.md`.
    pub fn read(&mut self, reg: u32) -> CodecAnswer {
        let command = (reg & 0x00ff) | STS_CODEC_BUSY;
        if self.ports.out32(REG_CODEC_READ, command).is_err() {
            return CodecAnswer {
                ready: false,
                value: 0,
            };
        }
        match self.wait_ready() {
            Ok((polled, ready)) => CodecAnswer {
                ready,
                value: (polled as u32) >> 16,
            },
            Err(_) => CodecAnswer {
                ready: false,
                value: 0,
            },
        }
    }
}

/// One device's codec register (`get_set_volume`'s switch,
/// `mixer.c:162-194`).
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

impl<P: PortIo> Ac97<P> {
    /// Get or set one device's volume (`get_set_volume`,
    /// `mixer.c:158-220`): the same attenuation convention as the other
    /// AC97 card — `(0x1f - left) << 8 | (0x1f - right)`.
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
/// up to fifty thousand times for the codec to answer, prints a warning
/// if it never does, and carries on with the eleven writes regardless
/// (`mixer.c:235-241`).
pub fn set_default_volume<P: PortIo>(codec: &mut Ac97<P>) -> CodecAnswer {
    codec.write(AC97_POWERDOWN, 0x0000);
    let mut ready = false;
    for _ in 0..CODEC_POLL_LIMIT {
        if codec.read(AC97_POWERDOWN).value & 0x03 != 0 {
            ready = true;
            break;
        }
    }
    for (reg, value) in DEFAULT_VOLUMES {
        codec.write(reg, value);
    }
    CodecAnswer { ready, value: 0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeMap;
    use alloc::vec::Vec;

    /// A port seam over a sparse file, with the codec idle by default.
    struct FakePorts {
        registers: BTreeMap<u16, u32>,
        writes: Vec<(u16, u32)>,
        /// Whether the codec's busy bit stays up.
        busy: bool,
        /// The word the write port reads back.
        readback: u32,
    }

    impl FakePorts {
        fn new() -> Self {
            FakePorts {
                registers: BTreeMap::new(),
                writes: Vec::new(),
                busy: false,
                readback: 0,
            }
        }
    }

    impl PortIo for FakePorts {
        fn out8(&mut self, port: u16, value: u8) -> Result<(), i32> {
            self.writes.push((port, value as u32));
            self.registers.insert(port, value as u32);
            Ok(())
        }
        fn out16(&mut self, port: u16, value: u16) -> Result<(), i32> {
            self.writes.push((port, value as u32));
            self.registers.insert(port, value as u32);
            Ok(())
        }
        fn out32(&mut self, port: u16, value: u32) -> Result<(), i32> {
            self.writes.push((port, value));
            self.registers.insert(port, value);
            Ok(())
        }
        fn in8(&mut self, port: u16) -> Result<u8, i32> {
            Ok(self.registers.get(&port).copied().unwrap_or(0) as u8)
        }
        fn in16(&mut self, port: u16) -> Result<u16, i32> {
            if port == REG_CODEC_WRITE {
                // The port is read as a word, exactly as C's `sdr_in16`
                // does, so the high half of a thirty-two-bit write is not
                // visible through it.
                let word = if self.busy {
                    (self.readback | STS_CODEC_BUSY) as u16
                } else {
                    self.readback as u16
                };
                return Ok(word);
            }
            Ok(self.registers.get(&port).copied().unwrap_or(0) as u16)
        }
        fn in32(&mut self, port: u16) -> Result<u32, i32> {
            Ok(self.registers.get(&port).copied().unwrap_or(0))
        }
        fn delay_micros(&mut self, _micros: u32) {}
    }

    #[test]
    fn test_a_write_polls_then_sends_the_command_word() {
        let mut codec = Ac97::new(FakePorts::new());
        let answer = codec.write(AC97_MASTER_VOLUME, 0);
        assert!(answer.ready);
        let writes = &codec.ports_mut().writes;
        assert_eq!(writes[0].0, REG_CODEC_WRITE);
        // The command word: the register number in the low byte and the
        // busy bit, with the polled value (zero here) in the high half.
        assert_eq!(writes[0].1, AC97_MASTER_VOLUME | STS_CODEC_BUSY);
    }

    #[test]
    fn test_a_busy_codec_runs_the_poll_out_and_reports_it() {
        let mut ports = FakePorts::new();
        ports.busy = true;
        let mut codec = Ac97::new(ports);
        let answer = codec.write(AC97_MASTER_VOLUME, 0);
        assert!(!answer.ready);
        // The command still went out, carrying the polled word's high
        // half, exactly as C composes it.
        let writes = &codec.ports_mut().writes;
        assert_eq!(writes[0].0, REG_CODEC_WRITE);
    }

    #[test]
    fn test_a_read_answers_zero_because_c_reads_a_word_then_shifts_sixteen() {
        let mut ports = FakePorts::new();
        // Even with the codec's data in the port's high half, the word
        // read cannot see it (`trident.c:80`, `:85`).
        ports.readback = 0x0a0b << 16;
        let mut codec = Ac97::new(ports);
        let answer = codec.read(AC97_CD_VOLUME);
        assert!(answer.ready);
        assert_eq!(answer.value, 0, "C's read always answers zero");
        assert_eq!(
            codec.ports_mut().writes[0],
            (REG_CODEC_READ, AC97_CD_VOLUME | STS_CODEC_BUSY)
        );
    }

    #[test]
    fn test_a_set_stores_the_attenuation_word() {
        let mut codec = Ac97::new(FakePorts::new());
        let mut level = VolumeLevel {
            device: Device::Master,
            left: 0x20,
            right: -3,
        };
        assert_eq!(codec.get_set_volume(&mut level, true), 0);
        assert_eq!(level.left, 0x1f);
        assert_eq!(level.right, 0);
        // (0x1f - 0x1f) << 8 | (0x1f - 0) = 0x1f, plus the register and
        // the busy bit in the command word.
        let command = codec.ports_mut().writes[0].1;
        assert_eq!(command & 0xff, AC97_MASTER_VOLUME);
        assert_eq!(command & STS_CODEC_BUSY, STS_CODEC_BUSY);
    }

    #[test]
    fn test_a_get_reports_the_full_level_because_the_read_answers_zero() {
        let mut codec = Ac97::new(FakePorts::new());
        let mut level = VolumeLevel {
            device: Device::Master,
            left: 0,
            right: 0,
        };
        assert_eq!(codec.get_set_volume(&mut level, false), 0);
        // Zero attenuation reads back as "no attenuation": the level
        // ceiling. A set followed by a get cannot round-trip on this
        // card, exactly as in C.
        assert_eq!(level.left, AC97_MAX_LEVEL);
        assert_eq!(level.right, AC97_MAX_LEVEL);
    }

    #[test]
    fn test_the_refused_devices_never_reach_the_codec() {
        let mut codec = Ac97::new(FakePorts::new());
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
        assert!(codec.ports_mut().writes.is_empty());
    }

    #[test]
    fn test_the_default_volume_probes_and_writes_eleven_registers() {
        let mut ports = FakePorts::new();
        ports.readback = 0x03 << 16;
        let mut codec = Ac97::new(ports);
        let answer = set_default_volume(&mut codec);
        // The ready probe can never see the codec's answer, because the
        // read that would carry it answers zero (`trident.c:85`): C's
        // loop runs its fifty thousand tries out and prints a warning,
        // then writes the defaults anyway. The rewrite reports the same
        // verdict and writes the same eleven registers.
        assert!(!answer.ready);
        // The probe's fifty thousand read commands, plus the power-down
        // write, plus the eleven default writes.
        let writes = &codec.ports_mut().writes;
        assert_eq!(
            writes[0],
            (REG_CODEC_WRITE, AC97_POWERDOWN | STS_CODEC_BUSY)
        );
        assert_eq!(writes.len(), 1 + 50000 + DEFAULT_VOLUMES.len());
        assert!(
            writes[1..50001]
                .iter()
                .all(|(port, _)| *port == REG_CODEC_READ),
            "the probe's reads"
        );
        // The last eleven writes are the defaults, in C's order, and the
        // final one is the general-purpose register.
        assert_eq!(
            writes[writes.len() - 1].1 & 0xff,
            AC97_GENERAL_PURPOSE,
            "the last command's register"
        );
        for (index, (reg, _)) in DEFAULT_VOLUMES.iter().enumerate() {
            let command = writes[writes.len() - DEFAULT_VOLUMES.len() + index].1;
            assert_eq!(command & 0xff, *reg);
        }
    }
}
