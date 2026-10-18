//! The AK4531 codec: the write protocol and the volume table.
//!
//! C correspondence: `minix3/minix/drivers/audio/es1370/ak4531.c` — the
//! register numbers and the power-on table (`:9-35`), the write
//! completion poll (`ak4531_finished`, `:46-56`), the write itself
//! (`ak4531_write`, `:58-69`), the initialization (`ak4531_init`,
//! `:72-101`), and the volume get/set (`ak4531_get_set_volume`, `:104-176`).
//!
//! The codec's registers cannot be read back, so C keeps a shadow array
//! (`mixer_values`) and answers a volume *get* from it. Only the volume
//! setter writes into the shadow (`ak4531.c:169-172`); the register
//! loader (`ak4531_init`) sends the table without touching it. The
//! address byte travels in the high half of a 16-bit word written to the
//! codec's port, and a write must wait until the codec clears its
//! in-progress bit in the card's interrupt status register.

use crate::ports::PortIo;
use crate::registers::{CODEC_WRITE_ADDRESS, CWRIP, INTERRUPT_STATUS};

/// Master volume, left channel (`MASTER_VOLUME_LCH`).
pub const MASTER_VOLUME_LCH: u8 = 0x00;
/// Master volume, right channel (`MASTER_VOLUME_RCH`).
pub const MASTER_VOLUME_RCH: u8 = 0x01;
/// FM volume, left channel (`FM_VOLUME_LCH`).
pub const FM_VOLUME_LCH: u8 = 0x04;
/// FM volume, right channel (`FM_VOLUME_RCH`).
pub const FM_VOLUME_RCH: u8 = 0x05;
/// Compact disc volume, left channel (`CD_AUDIO_VOLUME_LCH`).
pub const CD_AUDIO_VOLUME_LCH: u8 = 0x06;
/// Compact disc volume, right channel (`CD_AUDIO_VOLUME_RCH`).
pub const CD_AUDIO_VOLUME_RCH: u8 = 0x07;
/// Line volume, left channel (`LINE_VOLUME_LCH`).
pub const LINE_VOLUME_LCH: u8 = 0x08;
/// Line volume, right channel (`LINE_VOLUME_RCH`).
pub const LINE_VOLUME_RCH: u8 = 0x09;
/// Microphone volume (`MIC_VOLUME`).
pub const MIC_VOLUME: u8 = 0x0e;
/// Mono output volume (`MONO_OUT_VOLUME`).
pub const MONO_OUT_VOLUME: u8 = 0x0f;
/// Reset and power-down register (`RESET_AND_POWER_DOWN`).
pub const RESET_AND_POWER_DOWN: u8 = 0x16;
/// Power-down bit (`PD`).
pub const PD: u8 = 0x02;
/// Reset bit (`RST`).
pub const RST: u8 = 0x01;
/// Input select register (`AD_INPUT_SELECT`).
pub const AD_INPUT_SELECT: u8 = 0x18;
/// Microphone amplifier gain register (`MIC_AMP_GAIN`).
pub const MIC_AMP_GAIN: u8 = 0x19;
/// The mute bit inside a volume byte (`MUTE`).
pub const MUTE: u8 = 0x80;

/// The power-on register values (`mixer_values`, `ak4531.c:32-35`), for
/// registers `0x00` through `0x19`.
pub const MIXER_VALUES: [u8; 0x1a] = [
    0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, // 0x00 - 0x07
    0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x00, 0x08, // 0x08 - 0x0f
    0x7e, 0x3d, 0x01, 0x01, 0x00, 0x00, 0x03, 0x00, // 0x10 - 0x17
    0x00, 0x01, // 0x18 - 0x19
];

/// How many polls the write completion wait allows (`ak4531.c:49`:
/// `for (i = 0; i < 0x40000; i++)`).
pub const WRITE_POLL_LIMIT: u32 = 0x40000;

/// How many byte reads the initialization polls between steps
/// (`ak4531.c:77-78`, `:83-84`: two hundred reads of the poll address).
pub const INIT_POLL_READS: u32 = 100;

/// The codec's write leg: the card's base port, the shadow table, and the
/// poll address C reads while waiting (`ak4531.c`'s four file statics).
pub struct Ak4531<P: PortIo> {
    ports: P,
    /// The shadow table (`mixer_values`): the codec cannot be read back.
    values: [u8; 0x1a],
}

impl<P: PortIo> Ak4531<P> {
    /// A codec over the card's port seam, with the power-on table in the
    /// shadow (`ak4531.c:32-35`).
    pub fn new(ports: P) -> Self {
        Ak4531 {
            ports,
            values: MIXER_VALUES,
        }
    }

    /// The codec's port seam.
    pub fn ports_mut(&mut self) -> &mut P {
        &mut self.ports
    }

    /// One register's shadow value.
    pub fn value(&self, reg: u8) -> Option<u8> {
        self.values.get(reg as usize).copied()
    }

    /// Waits until the codec clears its write-in-progress bit
    /// (`ak4531_finished`, `ak4531.c:46-56`).
    fn finished(&mut self) -> bool {
        for _ in 0..WRITE_POLL_LIMIT {
            match self.ports.in16(INTERRUPT_STATUS) {
                Ok(status) if status as u32 & CWRIP == 0 => return true,
                Ok(_) => {}
                // C's helper returns an uninitialized value on a failed
                // read; the rewrite treats the failure as "not finished"
                // and lets the poll limit decide.
                Err(_) => {}
            }
        }
        false
    }

    /// Writes one codec register (`ak4531_write`, `ak4531.c:58-69`): the
    /// address must be a known register, the codec must be ready, and the
    /// word carries the address in its high byte.
    pub fn write(&mut self, address: u8, data: u8) -> i32 {
        if address > MIC_AMP_GAIN {
            return -minix_types::EINVAL;
        }
        if !self.finished() {
            return -minix_types::EIO;
        }
        let to_be_written = ((address as u16) << 8) | data as u16;
        match self.ports.out16(CODEC_WRITE_ADDRESS, to_be_written) {
            Ok(()) => 0,
            Err(status) => status,
        }
    }

    /// Initializes the codec (`ak4531_init`, `ak4531.c:72-101`): reset and
    /// power down, select the input, then load the power-on table.
    pub fn init(&mut self) -> i32 {
        for _ in 0..INIT_POLL_READS {
            if self.ports.in8(0).is_err() {
                return -minix_types::EIO;
            }
        }
        let status = self.write(RESET_AND_POWER_DOWN, PD | RST);
        if status != 0 {
            return status;
        }
        for _ in 0..INIT_POLL_READS {
            if self.ports.in8(0).is_err() {
                return -minix_types::EIO;
            }
        }
        let status = self.write(AD_INPUT_SELECT, 0x00);
        if status != 0 {
            return status;
        }
        for reg in MASTER_VOLUME_LCH..=MIC_AMP_GAIN {
            let status = self.write(reg, MIXER_VALUES[reg as usize]);
            if status != 0 {
                return status;
            }
        }
        0
    }
}

/// One mixer device's register pair and ceiling (`ak4531_get_set_volume`'s
/// switch, `ak4531.c:107-149`).
struct VolumeDevice {
    /// Left channel register.
    left: u8,
    /// Right channel register.
    right: u8,
    /// The largest level the device accepts.
    max_level: i32,
}

/// The device table (`ak4531_get_set_volume`'s switch). The codec's
/// table refuses `Dac`, `Treble`, and `Bass` outright, exactly as C does.
fn device_table(device: minix_audiodriver::sound::Device) -> Option<VolumeDevice> {
    use minix_audiodriver::sound::Device;
    let entry = match device {
        Device::Master => VolumeDevice {
            left: MASTER_VOLUME_LCH,
            right: MASTER_VOLUME_RCH,
            max_level: 0x1f,
        },
        Device::Dac => return None,
        Device::Fm => VolumeDevice {
            left: FM_VOLUME_LCH,
            right: FM_VOLUME_RCH,
            max_level: 0x1f,
        },
        Device::Cd => VolumeDevice {
            left: CD_AUDIO_VOLUME_LCH,
            right: CD_AUDIO_VOLUME_RCH,
            max_level: 0x1f,
        },
        Device::Line => VolumeDevice {
            left: LINE_VOLUME_LCH,
            right: LINE_VOLUME_RCH,
            max_level: 0x1f,
        },
        Device::Mic => VolumeDevice {
            left: MIC_VOLUME,
            right: MIC_VOLUME,
            max_level: 0x1f,
        },
        Device::Speaker => VolumeDevice {
            left: MONO_OUT_VOLUME,
            right: MONO_OUT_VOLUME,
            max_level: 0x03,
        },
        Device::Treble => return None,
        Device::Bass => return None,
    };
    Some(entry)
}

impl<P: PortIo> Ak4531<P> {
    /// Gets or sets one device's volume (`ak4531_get_set_volume`,
    /// `ak4531.c:104-176`).
    ///
    /// A get answers from the shadow table — the codec cannot be read
    /// back — with the stored byte complemented against the ceiling
    /// (`0x1f - (value & ~MUTE)`). A set clamps both levels and stores
    /// `0x1f - level` (the codec's attenuation convention).
    pub fn get_set_volume(
        &mut self,
        level: &mut minix_audiodriver::sound::VolumeLevel,
        set: bool,
    ) -> i32 {
        let Some(entry) = device_table(level.device) else {
            return -minix_types::EINVAL;
        };
        if set {
            level.right = level.right.clamp(0, entry.max_level);
            level.left = level.left.clamp(0, entry.max_level);
            let left = (-level.left + 0x1f) as u8;
            let right = (-level.right + 0x1f) as u8;
            self.values[entry.left as usize] = left;
            let status = self.write(entry.left, left);
            if status != 0 {
                return status;
            }
            self.values[entry.right as usize] = right;
            self.write(entry.right, right)
        } else {
            level.left = -(i32::from(self.values[entry.left as usize] & !MUTE)) + 0x1f;
            level.right = -(i32::from(self.values[entry.right as usize] & !MUTE)) + 0x1f;
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeMap;
    use alloc::vec::Vec;
    use minix_audiodriver::sound::{Device, VolumeLevel};

    /// A port seam over a register file, with the codec's ready bit.
    struct FakePorts {
        registers: BTreeMap<u16, u32>,
        writes: Vec<(u16, u32)>,
        /// Whether the codec reports a write in progress.
        busy: bool,
        /// Reads of the poll address.
        polls: u32,
        fail: bool,
    }

    impl FakePorts {
        fn new() -> Self {
            FakePorts {
                registers: BTreeMap::new(),
                writes: Vec::new(),
                busy: false,
                polls: 0,
                fail: false,
            }
        }
    }

    impl PortIo for FakePorts {
        fn out8(&mut self, port: u16, value: u8) -> Result<(), i32> {
            if self.fail {
                return Err(-minix_types::EIO);
            }
            self.writes.push((port, value as u32));
            self.registers.insert(port, value as u32);
            Ok(())
        }
        fn out16(&mut self, port: u16, value: u16) -> Result<(), i32> {
            if self.fail {
                return Err(-minix_types::EIO);
            }
            self.writes.push((port, value as u32));
            self.registers.insert(port, value as u32);
            Ok(())
        }
        fn out32(&mut self, port: u16, value: u32) -> Result<(), i32> {
            if self.fail {
                return Err(-minix_types::EIO);
            }
            self.writes.push((port, value));
            self.registers.insert(port, value);
            Ok(())
        }
        fn in8(&mut self, port: u16) -> Result<u8, i32> {
            if self.fail {
                return Err(-minix_types::EIO);
            }
            if port == 0 {
                self.polls += 1;
            }
            Ok(self.registers.get(&port).copied().unwrap_or(0) as u8)
        }
        fn in16(&mut self, port: u16) -> Result<u16, i32> {
            if self.fail {
                return Err(-minix_types::EIO);
            }
            if self.busy {
                return Ok(CWRIP as u16);
            }
            Ok(self.registers.get(&port).copied().unwrap_or(0) as u16)
        }
        fn in32(&mut self, port: u16) -> Result<u32, i32> {
            if self.fail {
                return Err(-minix_types::EIO);
            }
            Ok(self.registers.get(&port).copied().unwrap_or(0))
        }
    }

    #[test]
    fn test_a_write_carries_the_address_in_the_high_byte() {
        let mut codec = Ak4531::new(FakePorts::new());
        assert_eq!(codec.write(MASTER_VOLUME_LCH, 0x08), 0);
        assert_eq!(
            codec.ports_mut().writes,
            alloc::vec![(CODEC_WRITE_ADDRESS, 0x0008)],
            "address zero, value eight"
        );
        assert_eq!(codec.write(FM_VOLUME_LCH, 0x1f), 0);
        assert_eq!(codec.ports_mut().writes[1], (CODEC_WRITE_ADDRESS, 0x041f));
        // A plain write leaves the shadow table alone: only the volume
        // setter updates it (`ak4531.c:169-172`).
        assert_eq!(codec.value(FM_VOLUME_LCH), Some(0x08));
    }

    #[test]
    fn test_a_write_beyond_the_register_range_is_refused() {
        let mut codec = Ak4531::new(FakePorts::new());
        assert_eq!(codec.write(MIC_AMP_GAIN + 1, 0), -minix_types::EINVAL);
        assert!(codec.ports_mut().writes.is_empty());
    }

    #[test]
    fn test_a_busy_codec_blocks_until_the_poll_limit() {
        let mut codec = Ak4531::new(FakePorts {
            busy: true,
            ..FakePorts::new()
        });
        assert_eq!(codec.write(MASTER_VOLUME_LCH, 0x08), -minix_types::EIO);
        assert!(codec.ports_mut().writes.is_empty(), "nothing was sent");
    }

    #[test]
    fn test_the_initialization_loads_the_power_on_table() {
        let mut codec = Ak4531::new(FakePorts::new());
        assert_eq!(codec.init(), 0);
        // The two poll runs read the poll address a hundred times each.
        assert_eq!(codec.ports_mut().polls, 2 * INIT_POLL_READS);
        // The reset pair came first, then the input select, then the
        // whole table from the master volume up to the microphone gain.
        let writes = &codec.ports_mut().writes;
        assert_eq!(writes[0], (CODEC_WRITE_ADDRESS, 0x1603));
        assert_eq!(writes[1], (CODEC_WRITE_ADDRESS, 0x1800));
        assert_eq!(writes[2], (CODEC_WRITE_ADDRESS, 0x0008));
        assert_eq!(
            writes.len(),
            2 + (MIC_AMP_GAIN - MASTER_VOLUME_LCH + 1) as usize
        );
        // The shadow still holds the table, not what was sent: the
        // initialization is a plain loader. (The reset register's table
        // entry happens to equal the value the initialization writes.)
        assert_eq!(codec.value(RESET_AND_POWER_DOWN), Some(PD | RST));
        assert_eq!(codec.value(MIC_AMP_GAIN), Some(0x01));
        assert_eq!(codec.value(0x10), Some(0x7e), "the first of the 0x10 row");
    }

    #[test]
    fn test_a_set_clamps_and_stores_the_attenuation() {
        let mut codec = Ak4531::new(FakePorts::new());
        let mut level = VolumeLevel {
            device: Device::Cd,
            left: 0x20,
            right: -4,
        };
        assert_eq!(codec.get_set_volume(&mut level, true), 0);
        assert_eq!(level.left, 0x1f);
        assert_eq!(level.right, 0);
        assert_eq!(codec.value(CD_AUDIO_VOLUME_LCH), Some(0x00));
        assert_eq!(codec.value(CD_AUDIO_VOLUME_RCH), Some(0x1f));
        // The speaker's ceiling is three, not thirty-one.
        let mut speaker = VolumeLevel {
            device: Device::Speaker,
            left: 9,
            right: 9,
        };
        assert_eq!(codec.get_set_volume(&mut speaker, true), 0);
        assert_eq!(speaker.left, 3);
        assert_eq!(codec.value(MONO_OUT_VOLUME), Some(0x1c));
    }

    #[test]
    fn test_a_get_answers_from_the_shadow() {
        let mut codec = Ak4531::new(FakePorts::new());
        let mut level = VolumeLevel {
            device: Device::Master,
            left: 0,
            right: 0,
        };
        // The power-on table holds 0x08 for both master registers.
        assert_eq!(codec.get_set_volume(&mut level, false), 0);
        assert_eq!(level.left, 0x1f - 0x08);
        assert_eq!(level.right, 0x1f - 0x08);
        // A set followed by a get round-trips.
        let mut set = VolumeLevel {
            device: Device::Master,
            left: 0x10,
            right: 0x03,
        };
        assert_eq!(codec.get_set_volume(&mut set, true), 0);
        let mut get = VolumeLevel {
            device: Device::Master,
            left: 0,
            right: 0,
        };
        assert_eq!(codec.get_set_volume(&mut get, false), 0);
        assert_eq!(get, set);
    }

    #[test]
    fn test_the_refused_devices_never_reach_the_codec() {
        let mut codec = Ak4531::new(FakePorts::new());
        for device in [Device::Dac, Device::Treble, Device::Bass] {
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
    fn test_a_failing_port_seam_is_reported() {
        let mut codec = Ak4531::new(FakePorts {
            fail: true,
            ..FakePorts::new()
        });
        assert_eq!(codec.init(), -minix_types::EIO);
    }
}
