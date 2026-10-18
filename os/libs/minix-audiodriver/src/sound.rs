//! The sound ioctl numbers and the payloads they carry.
//!
//! C correspondence: `minix3/minix/include/sys/ioc_sound.h:11-32` (the
//! DSP and mixer request numbers, built from the `_IO`/`_IOW`/`_IOR`/
//! `_IOWR` macros) and `minix3/minix/include/minix/sound.h:11-44` (the
//! `enum Device`, `struct volume_level`, and `struct inout_ctrl` the
//! mixer requests exchange).
//!
//! The numbers are not free-form: `audio_fw.c`'s `msg_ioctl` reads the
//! direction and the payload length back out of the request word itself
//! (`IOC_IN`/`IOC_OUT` and `io_ctl_length`), so a card's hook sees the
//! encoded word, exactly as in C. The encoding lives in
//! `sys/ioccom.h:83-86` (Minix variant: the length field is twelve bits,
//! `IOCPARM_MASK = 0xfff`).

/// Copy parameters in (`IOC_IN`, `ioccom.h:75`).
pub const IOC_IN: u64 = 0x8000_0000;
/// Copy parameters out (`IOC_OUT`, `ioccom.h:73`).
pub const IOC_OUT: u64 = 0x4000_0000;
/// No parameters (`IOC_VOID`, `ioccom.h:70`).
pub const IOC_VOID: u64 = 0x2000_0000;
/// Parameter length mask (`IOCPARM_MASK`, `ioccom.h:59`, Minix variant).
pub const IOCPARM_MASK: u64 = 0xfff;
/// Where the parameter length sits in the request word
/// (`IOCPARM_SHIFT`, `ioccom.h:63`).
pub const IOCPARM_SHIFT: u32 = 16;
/// Where the command group sits (`IOCGROUP_SHIFT`, `ioccom.h:64`).
pub const IOCGROUP_SHIFT: u32 = 8;

/// The `_IOC` macro (`ioccom.h:83-86`): direction, payload length, group,
/// and number.
pub const fn ioc(inout: u64, group: u8, num: u8, len: u64) -> u64 {
    inout
        | ((len & IOCPARM_MASK) << IOCPARM_SHIFT)
        | ((group as u64) << IOCGROUP_SHIFT)
        | num as u64
}

/// The payload length encoded in a request (`io_ctl_length`,
/// `audio_fw.c:810-813`).
pub const fn io_ctl_length(request: u64) -> u64 {
    (request >> IOCPARM_SHIFT) & IOCPARM_MASK
}

/// Set the sample rate (`DSPIORATE`).
pub const DSPIORATE: u64 = ioc(IOC_IN, b's', 1, 4);
/// Set stereo or mono (`DSPIOSTEREO`).
pub const DSPIOSTEREO: u64 = ioc(IOC_IN, b's', 2, 4);
/// Set the fragment size (`DSPIOSIZE`).
pub const DSPIOSIZE: u64 = ioc(IOC_IN, b's', 3, 4);
/// Set the sample width in bits (`DSPIOBITS`).
pub const DSPIOBITS: u64 = ioc(IOC_IN, b's', 4, 4);
/// Set the sample sign (`DSPIOSIGN`).
pub const DSPIOSIGN: u64 = ioc(IOC_IN, b's', 5, 4);
/// Get the largest fragment size (`DSPIOMAX`).
pub const DSPIOMAX: u64 = ioc(IOC_OUT, b's', 6, 4);
/// Reset the card (`DSPIORESET`).
pub const DSPIORESET: u64 = ioc(IOC_VOID, b's', 7, 0);
/// Whether a free buffer is available (`DSPIOFREEBUF`).
pub const DSPIOFREEBUF: u64 = ioc(IOC_OUT, b's', 30, 4);
/// The sample counter (`DSPIOSAMPLESINBUF`).
pub const DSPIOSAMPLESINBUF: u64 = ioc(IOC_OUT, b's', 31, 4);
/// Pause the DMA (`DSPIOPAUSE`).
pub const DSPIOPAUSE: u64 = ioc(IOC_VOID, b's', 32, 0);
/// Resume the DMA (`DSPIORESUME`).
pub const DSPIORESUME: u64 = ioc(IOC_VOID, b's', 33, 0);

/// The mixer device a volume request names (`enum Device`,
/// `sound.h:11-24`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Device {
    /// Master volume.
    Master,
    /// Digitized sound.
    Dac,
    /// FM synthesized sound.
    Fm,
    /// Compact disc input.
    Cd,
    /// Line input.
    Line,
    /// Microphone.
    Mic,
    /// PC speaker.
    Speaker,
    /// Treble.
    Treble,
    /// Bass.
    Bass,
}

impl Device {
    /// The wire value of the C enumerator.
    pub const fn to_i32(self) -> i32 {
        match self {
            Device::Master => 0,
            Device::Dac => 1,
            Device::Fm => 2,
            Device::Cd => 3,
            Device::Line => 4,
            Device::Mic => 5,
            Device::Speaker => 6,
            Device::Treble => 7,
            Device::Bass => 8,
        }
    }

    /// Decode a wire value; `None` for a value no enumerator has.
    pub const fn from_i32(value: i32) -> Option<Device> {
        match value {
            0 => Some(Device::Master),
            1 => Some(Device::Dac),
            2 => Some(Device::Fm),
            3 => Some(Device::Cd),
            4 => Some(Device::Line),
            5 => Some(Device::Mic),
            6 => Some(Device::Speaker),
            7 => Some(Device::Treble),
            8 => Some(Device::Bass),
            _ => None,
        }
    }
}

/// A volume request's payload (`struct volume_level`, `sound.h:31-36`):
/// the device plus its two channel levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VolumeLevel {
    /// Which mixer device the levels belong to.
    pub device: Device,
    /// Left channel level.
    pub left: i32,
    /// Right channel level.
    pub right: i32,
}

impl VolumeLevel {
    /// The payload's wire size (`sizeof(struct volume_level)`: three
    /// four-byte fields, `sound.h:31-36`).
    pub const SIZE: usize = 12;

    /// Encode the payload.
    pub fn encode(&self) -> [u8; Self::SIZE] {
        let mut bytes = [0u8; Self::SIZE];
        bytes[0..4].copy_from_slice(&self.device.to_i32().to_le_bytes());
        bytes[4..8].copy_from_slice(&self.left.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.right.to_le_bytes());
        bytes
    }

    /// Decode the payload; `None` when the bytes do not hold a device
    /// enumerator.
    pub fn decode(bytes: &[u8]) -> Option<VolumeLevel> {
        if bytes.len() < Self::SIZE {
            return None;
        }
        let read_i32 = |offset: usize| -> i32 {
            i32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
        };
        Some(VolumeLevel {
            device: Device::from_i32(read_i32(0))?,
            left: read_i32(4),
            right: read_i32(8),
        })
    }
}

/// Get one mixer device's volume (`MIXIOGETVOLUME`).
pub const MIXIOGETVOLUME: u64 = ioc(IOC_IN | IOC_OUT, b's', 10, VolumeLevel::SIZE as u64);
/// Set one mixer device's volume (`MIXIOSETVOLUME`).
pub const MIXIOSETVOLUME: u64 = ioc(IOC_IN | IOC_OUT, b's', 20, VolumeLevel::SIZE as u64);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_request_numbers_match_the_c_macros() {
        // _IOW('s', 1, unsigned int): IOC_IN | (4 << 16) | ('s' << 8) | 1.
        assert_eq!(DSPIORATE, 0x8000_0000 | (4 << 16) | (0x73 << 8) | 1);
        assert_eq!(DSPIOMAX, 0x4000_0000 | (4 << 16) | (0x73 << 8) | 6);
        assert_eq!(DSPIORESET, 0x2000_0000 | (0x73 << 8) | 7);
        assert_eq!(DSPIORESUME, 0x2000_0000 | (0x73 << 8) | 33);
        // _IOWR('s', 10, struct volume_level): both directions, 12 bytes.
        assert_eq!(MIXIOGETVOLUME, 0xc000_0000 | (12 << 16) | (0x73 << 8) | 10);
        assert_eq!(MIXIOSETVOLUME, 0xc000_0000 | (12 << 16) | (0x73 << 8) | 20);
    }

    #[test]
    fn test_io_ctl_length_decodes_the_payload_size() {
        assert_eq!(io_ctl_length(DSPIORATE), 4);
        assert_eq!(io_ctl_length(MIXIOSETVOLUME), 12);
        assert_eq!(io_ctl_length(DSPIORESET), 0);
    }

    #[test]
    fn test_a_volume_level_round_trips() {
        let level = VolumeLevel {
            device: Device::Cd,
            left: 0x0f,
            right: 0x08,
        };
        let bytes = level.encode();
        assert_eq!(bytes.len(), VolumeLevel::SIZE);
        assert_eq!(VolumeLevel::decode(&bytes), Some(level));
        // A device value outside the enumerator is refused.
        let mut bad = bytes;
        bad[0] = 9;
        assert_eq!(VolumeLevel::decode(&bad), None);
        assert_eq!(VolumeLevel::decode(&bytes[..8]), None);
    }
}
