//! The CMI8738 card face: the register sequences behind `AudioHooks`.
//!
//! C correspondence: `cmi8738.c`'s developer interface — `dev_reset`
//! (`:69-76`), `dev_configure` (`:79-88`), `dev_init_mixer` (`:91-96`),
//! `dev_set_sample_rate` (`:99-113`), `dev_set_format` (`:116-130`),
//! `dev_start_channel`/`dev_stop_channel` (`:133-166`), `dev_set_dma`
//! (`:169-178`), `dev_read_dma_current` (`:181-188`), `dev_pause_dma`/
//! `dev_resume_dma` (`:191-206`), `dev_read_clear_intr_status`
//! (`:207-213`), `dev_intr_enable` (`:215-221`), and the `drv_*` hooks
//! that compose them (`:432-597`).
//!
//! The face owns the register traffic through [`PortIo`]; the service
//! owns the DMA buffers (registered `E-DMABUF`) and the caller's bytes.

use minix_audiodriver::hooks::AudioHooks;
use minix_audiodriver::sound::{
    DSPIOBITS, DSPIOFREEBUF, DSPIOMAX, DSPIOPAUSE, DSPIORATE, DSPIORESET, DSPIORESUME,
    DSPIOSAMPLESINBUF, DSPIOSIGN, DSPIOSIZE, DSPIOSTEREO, MIXIOGETVOLUME, MIXIOSETVOLUME,
    VolumeLevel,
};

use crate::mixer::{get_set_volume, mixer_write, set_default_volume};
use crate::ports::PortIo;
use crate::registers::{
    ADC, CMD_ADC_C0, CMD_ADC_C1, CMD_ENA_C0, CMD_ENA_C1, CMD_INTR_ENABLE, CMD_N4SPK3D,
    CMD_PAUSE_C0, CMD_PAUSE_C1, CMD_POWER_DOWN, CMD_RESET, CMD_RESET_C0, CMD_RESET_C1,
    CMD_SPDIF_ENA, CMD_SPDIF_LOOP, DAC, FMT_BIT16, FMT_STEREO, INTR_STS_ADC, INTR_STS_DAC,
    REG_ADC_DMA_ADDR, REG_ADC_DMA_LEN, REG_ADC_SAMPLE_COUNT, REG_DAC_DMA_ADDR, REG_DAC_DMA_LEN,
    REG_DAC_SAMPLE_COUNT, REG_EXT_INDEX, REG_EXT_MISC, REG_FORMAT, REG_FUNC_CTRL, REG_FUNC_CTRL1,
    REG_INTR_CTRL, REG_INTR_STS, REG_MISC_CTRL, REG_MIX_INPUT, rate_code,
};

/// One channel's configuration (C's `aud_conf[]`: the fields `drv_init_hw`
/// seeds at `cmi8738.c:412-422` and `drv_io_ctl` lets a user change).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelConf {
    /// Whether the channel is playing (`busy`).
    pub busy: bool,
    /// Stereo (`stereo`).
    pub stereo: u32,
    /// Sample rate in Hz (`sample_rate`).
    pub sample_rate: u32,
    /// Sample width in bits (`nr_of_bits`).
    pub nr_of_bits: u32,
    /// Sample sign (`sign`).
    pub sign: u32,
    /// The current fragment size in bytes (`fragment_size`).
    pub fragment_size: u32,
}

/// One sub-device's geometry (the DMA fields of `sub_dev_t`,
/// `audio_fw.h:18-28`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometry {
    /// The whole DMA buffer (`DmaSize`).
    pub dma_size: u32,
    /// How many fragments it holds (`NrOfDmaFragments`).
    pub fragments: u32,
    /// The smallest fragment a user may ask for (`MinFragmentSize`).
    pub min_fragment_size: u32,
    /// The extra software buffers (`NrOfExtraBuffers`).
    pub extra_buffers: u32,
}

/// The card face over one port seam.
pub struct Cmi8738<P: PortIo> {
    ports: P,
    /// Per-sub-device geometry, indexed by `DAC`/`ADC`/`MIX` (C's global
    /// `sub_dev[]`).
    geometry: [Geometry; 3],
    /// Per-sub-device configuration, indexed by `DAC`/`ADC`/`MIX` (C's
    /// global `aud_conf[]`).
    conf: [ChannelConf; 3],
    /// The last interrupt status `drv_int_sum` read (`dev.intr_status`).
    intr_status: u32,
    /// The card's IRQ line, as the PCI probe resolved it (`dev.irq`).
    irq: u8,
    /// How many extra buffers are filled (`BufLength`), for
    /// `DSPIOFREEBUF`.
    buf_length: u32,
}

impl<P: PortIo> Cmi8738<P> {
    /// A face over the port seam, with the geometry `drv_init` sets
    /// (`cmi8738.c:351-361`) and the configuration `drv_init_hw` seeds
    /// (`cmi8738.c:412-422`).
    pub fn new(ports: P) -> Self {
        let geometry = [
            Geometry {
                dma_size: 64 * 1024,
                fragments: 2,
                min_fragment_size: 1024,
                extra_buffers: 4,
            },
            Geometry {
                dma_size: 64 * 1024,
                fragments: 2,
                min_fragment_size: 1024,
                extra_buffers: 4,
            },
            Geometry {
                // The mixer sub-device is control-only: C leaves its DMA
                // fields zero and `drv_init_hw` skips it.
                dma_size: 0,
                fragments: 0,
                min_fragment_size: 0,
                extra_buffers: 0,
            },
        ];
        let seeded = ChannelConf {
            busy: false,
            stereo: 1,
            sample_rate: 44100,
            nr_of_bits: 16,
            sign: 1,
            fragment_size: 64 * 1024 / 2,
        };
        Cmi8738 {
            ports,
            geometry,
            conf: [seeded, seeded, seeded],
            intr_status: 0,
            irq: 0,
            buf_length: 0,
        }
    }

    /// The port seam (tests and the binary's diagnostics).
    pub fn ports_mut(&mut self) -> &mut P {
        &mut self.ports
    }

    /// Record the IRQ line the PCI probe found (`dev.irq`).
    pub fn set_irq(&mut self, irq: u8) {
        self.irq = irq;
    }

    /// One channel's configuration.
    pub fn conf(&self, chan: i32) -> Option<ChannelConf> {
        self.conf.get(chan as usize).copied()
    }

    /// One sub-device's geometry.
    pub fn geometry(&self, chan: i32) -> Option<Geometry> {
        self.geometry.get(chan as usize).copied()
    }

    /// Set or clear bits in a register (`dev_io_set_clear`,
    /// `cmi8738.c:40-48`).
    fn set_clear(&mut self, reg: u16, value: u32, set: bool) -> Result<(), i32> {
        let data = self.ports.in32(reg)?;
        let updated = if set { data | value } else { data & !value };
        self.ports.out32(reg, updated)
    }

    /// Reset the card (`dev_reset`, `cmi8738.c:69-76`): clear the
    /// power-down bit, pulse the reset bit, wait, release it.
    pub fn reset(&mut self) -> Result<(), i32> {
        self.set_clear(REG_MISC_CTRL, CMD_POWER_DOWN, false)?;
        self.set_clear(REG_MISC_CTRL, CMD_RESET, true)?;
        self.ports.delay_micros(100);
        self.set_clear(REG_MISC_CTRL, CMD_RESET, false)
    }

    /// Configure the card (`dev_configure`, `cmi8738.c:79-88`).
    pub fn configure(&mut self) -> Result<(), i32> {
        self.set_clear(REG_FUNC_CTRL, CMD_ADC_C0, false)?;
        self.set_clear(REG_FUNC_CTRL, CMD_ADC_C1, true)?;
        self.set_clear(REG_MISC_CTRL, CMD_N4SPK3D, true)?;
        self.set_clear(REG_FUNC_CTRL1, CMD_SPDIF_ENA, false)?;
        self.set_clear(REG_FUNC_CTRL1, CMD_SPDIF_LOOP, false)?;
        self.ports.out8(REG_EXT_INDEX, 0x03)?;
        self.ports.out8(REG_MIX_INPUT, 0x0f)
    }

    /// Initialize the mixer (`dev_init_mixer`, `cmi8738.c:91-96`).
    ///
    /// C writes `MIXER_ADCL` and `MIXER_ADCR` (`mixer.h:34-35`: `0x3d`
    /// and `0x3e`); the third write targets `MIXER_OUT_MUTE` (`0x3c`).
    pub fn init_mixer(&mut self) -> Result<(), i32> {
        mixer_write(&mut self.ports, 0, 0)?;
        mixer_write(&mut self.ports, 0x3d, 0x1f)?;
        mixer_write(&mut self.ports, 0x3e, 0x7f)?;
        mixer_write(&mut self.ports, 0x3c, 0x7f)
    }

    /// Set the sample rate (`dev_set_sample_rate`, `cmi8738.c:99-113`).
    ///
    /// C walks the eight-entry rate table and leaves `rate` at zero when
    /// nothing matches, so an unsupported rate programs the first entry
    /// (5512 Hz) instead of failing.
    fn set_sample_rate(&mut self, rate: u32) -> Result<(), i32> {
        let code = rate_code(rate).unwrap_or(0);
        let data = self.ports.in32(REG_FUNC_CTRL1)?;
        let data = data & !(0xe000 | 0x1c00);
        let data = data | ((code << 13) & 0xe000) | ((code << 10) & 0x1c00);
        self.ports.out32(REG_FUNC_CTRL1, data)
    }

    /// Set the format and the sample counts (`dev_set_format`,
    /// `cmi8738.c:116-130`).
    fn set_format(&mut self, conf: ChannelConf, sample_count: u32) -> Result<(), i32> {
        let mut format = 0u32;
        if conf.stereo == 1 {
            format |= FMT_STEREO;
        }
        if conf.nr_of_bits == 16 {
            format |= FMT_BIT16;
        }
        let data = self.ports.in32(REG_FORMAT)?;
        let data = data & !0x0000_0003 | format;
        let data = data & !0x0000_000c | (format << 2);
        self.ports.out32(REG_FORMAT, data)?;
        self.set_clear(REG_EXT_MISC, 0x1000_0000, false)?;
        self.ports
            .out16(REG_DAC_SAMPLE_COUNT, (sample_count - 1) as u16)?;
        self.ports
            .out16(REG_ADC_SAMPLE_COUNT, (sample_count - 1) as u16)
    }

    /// Start one channel (`dev_start_channel`, `cmi8738.c:133-142`).
    fn start_channel(&mut self, chan: i32) -> Result<(), i32> {
        let enable = match chan {
            DAC => CMD_ENA_C0,
            ADC => CMD_ENA_C1,
            _ => return Ok(()),
        };
        self.set_clear(REG_FUNC_CTRL, enable, true)
    }

    /// Stop one channel (`dev_stop_channel`, `cmi8738.c:145-166`): clear
    /// the enable bit, pulse the channel's reset bit, wait, release it.
    fn stop_channel(&mut self, chan: i32) -> Result<(), i32> {
        let (enable, reset_bit) = match chan {
            DAC => (CMD_ENA_C0, CMD_RESET_C0),
            ADC => (CMD_ENA_C1, CMD_RESET_C1),
            _ => return Ok(()),
        };
        self.set_clear(REG_FUNC_CTRL, enable, false)?;
        self.set_clear(REG_FUNC_CTRL, reset_bit, true)?;
        self.ports.delay_micros(100);
        self.set_clear(REG_FUNC_CTRL, reset_bit, false)
    }

    /// Program one channel's DMA (`dev_set_dma`, `cmi8738.c:169-178`):
    /// the address, then the length minus one.
    fn program_dma(&mut self, dma: u32, length: u32, chan: i32) -> Result<(), i32> {
        match chan {
            DAC => {
                self.ports.out32(REG_DAC_DMA_ADDR, dma)?;
                self.ports.out16(REG_DAC_DMA_LEN, (length - 1) as u16)
            }
            ADC => {
                self.ports.out32(REG_ADC_DMA_ADDR, dma)?;
                self.ports.out16(REG_ADC_DMA_LEN, (length - 1) as u16)
            }
            _ => Ok(()),
        }
    }

    /// The channel's running DMA address (`dev_read_dma_current`,
    /// `cmi8738.c:181-188`): the current-address alias of the DMA address
    /// register, read as a word.
    fn read_dma_current(&mut self, chan: i32) -> Result<u32, i32> {
        match chan {
            DAC => Ok(self.ports.in16(REG_DAC_DMA_ADDR)? as u32),
            ADC => Ok(self.ports.in16(REG_ADC_DMA_ADDR)? as u32),
            _ => Ok(0),
        }
    }

    /// Pause or resume one channel (`dev_pause_dma`/`dev_resume_dma`,
    /// `cmi8738.c:191-206`).
    fn set_paused(&mut self, chan: i32, paused: bool) -> Result<(), i32> {
        let bit = match chan {
            DAC => CMD_PAUSE_C0,
            ADC => CMD_PAUSE_C1,
            _ => return Ok(()),
        };
        self.set_clear(REG_FUNC_CTRL, bit, paused)
    }

    /// Enable or disable the interrupt (`dev_intr_enable`,
    /// `cmi8738.c:215-221`): C reads the interrupt status register and
    /// writes the masked word into the control register.
    fn intr_enable(&mut self, enable: bool) -> Result<(), i32> {
        let data = self.ports.in32(REG_INTR_STS)?;
        let updated = if enable {
            data | CMD_INTR_ENABLE
        } else {
            data & !CMD_INTR_ENABLE
        };
        self.ports.out32(REG_INTR_CTRL, updated)
    }

    /// Read and clear the interrupt status (`dev_read_clear_intr_status`,
    /// `cmi8738.c:207-213`): read the status, then toggle the interrupt
    /// off and on. C does not write the status back.
    fn read_clear_intr_status(&mut self) -> Result<u32, i32> {
        let data = self.ports.in32(REG_INTR_STS)?;
        self.intr_enable(false)?;
        self.intr_enable(true)?;
        Ok(data)
    }

    /// The card's ioctl set (`drv_io_ctl`, `cmi8738.c:484-538`).
    fn io_ctl_inner(&mut self, request: u64, val: &mut [u8], len: &mut i32, chan: i32) -> i32 {
        let read_u32 = |val: &[u8]| u32::from_le_bytes(val[..4].try_into().unwrap());
        fn write_u32(val: &mut [u8], len: &mut i32, value: u32) {
            val[..4].copy_from_slice(&value.to_le_bytes());
            *len = 4;
        }
        match request {
            DSPIORATE => {
                if let Some(conf) = self.conf.get_mut(chan as usize) {
                    conf.sample_rate = read_u32(val);
                    0
                } else {
                    -minix_types::EINVAL
                }
            }
            DSPIOSTEREO => {
                if let Some(conf) = self.conf.get_mut(chan as usize) {
                    conf.stereo = read_u32(val);
                    0
                } else {
                    -minix_types::EINVAL
                }
            }
            DSPIOBITS => {
                if let Some(conf) = self.conf.get_mut(chan as usize) {
                    conf.nr_of_bits = read_u32(val);
                    0
                } else {
                    -minix_types::EINVAL
                }
            }
            DSPIOSIGN => {
                if let Some(conf) = self.conf.get_mut(chan as usize) {
                    conf.sign = read_u32(val);
                    0
                } else {
                    -minix_types::EINVAL
                }
            }
            DSPIOSIZE => {
                // `set_frag_size` (`cmi8738.c:317-325`): the request must
                // sit between the minimum and one whole fragment.
                let size = read_u32(val);
                let Some(geometry) = self.geometry.get(chan as usize).copied() else {
                    return -minix_types::EINVAL;
                };
                if geometry.fragments == 0 {
                    return -minix_types::EINVAL;
                }
                let max = geometry.dma_size / geometry.fragments;
                if size > max || size < geometry.min_fragment_size {
                    return -minix_types::EINVAL;
                }
                self.conf[chan as usize].fragment_size = size;
                0
            }
            DSPIOMAX => {
                // `get_max_frag_size` (`cmi8738.c:328-333`).
                let Some(geometry) = self.geometry.get(chan as usize).copied() else {
                    return -minix_types::EINVAL;
                };
                if geometry.fragments == 0 {
                    return -minix_types::EINVAL;
                }
                write_u32(val, len, geometry.dma_size / geometry.fragments);
                0
            }
            DSPIORESET => match self.reset() {
                Ok(()) => 0,
                Err(status) => status,
            },
            DSPIOFREEBUF => {
                // `free_buf` (`cmi8738.c:336-343`): zero once every extra
                // buffer is filled.
                let Some(geometry) = self.geometry.get(chan as usize).copied() else {
                    return -minix_types::EINVAL;
                };
                let value = if self.buf_length == geometry.extra_buffers {
                    0
                } else {
                    1
                };
                write_u32(val, len, value);
                0
            }
            DSPIOSAMPLESINBUF => {
                // `get_samples_in_buf` (`cmi8738.c:346-353`).
                let Ok(current) = self.read_dma_current(chan) else {
                    return -minix_types::EIO;
                };
                write_u32(val, len, self.buf_length * 8192 + current);
                0
            }
            DSPIOPAUSE => match self.set_paused(chan, true) {
                Ok(()) => 0,
                Err(status) => status,
            },
            DSPIORESUME => match self.set_paused(chan, false) {
                Ok(()) => 0,
                Err(status) => status,
            },
            MIXIOGETVOLUME | MIXIOSETVOLUME => {
                // `get_set_volume` (`cmi8738.c:527-534`).
                let Some(mut level) = VolumeLevel::decode(val) else {
                    return -minix_types::EINVAL;
                };
                let set = request == MIXIOSETVOLUME;
                let status = get_set_volume(&mut self.ports, &mut level, set);
                if status == 0 && !set {
                    let encoded = level.encode();
                    val[..encoded.len()].copy_from_slice(&encoded);
                }
                status
            }
            _ => -minix_types::EINVAL,
        }
    }
}

impl<P: PortIo> AudioHooks for Cmi8738<P> {
    fn init(&mut self) -> i32 {
        // C `drv_init` (`cmi8738.c:344-380`): three sub-devices and three
        // special files; the file table itself lives in the service.
        3
    }

    fn init_hw(&mut self) -> i32 {
        // C `drv_init_hw` (`cmi8738.c:384-428`): reset, configure, mixer,
        // default volume. `dev_probe`'s PCI match happens in the binary
        // (the base port is a constructor argument here).
        if self.reset().is_err() {
            return -minix_types::EIO;
        }
        if self.configure().is_err() {
            return -minix_types::EIO;
        }
        if self.init_mixer().is_err() {
            return -minix_types::EIO;
        }
        if set_default_volume(&mut self.ports).is_err() {
            return -minix_types::EIO;
        }
        0
    }

    fn reset(&mut self) -> i32 {
        match self.reset() {
            Ok(()) => 0,
            Err(status) => status,
        }
    }

    fn start(&mut self, sub_dev: i32, _dma_mode: i32) -> i32 {
        // C `drv_start` (`cmi8738.c:434-459`): rate, format (with the
        // fragment size divided down to frames, `DMA_LENGTH_BY_FRAME`),
        // interrupt, then the channel.
        if sub_dev != DAC && sub_dev != ADC {
            return -minix_types::EINVAL;
        }
        let conf = self.conf[sub_dev as usize];
        if self.set_sample_rate(conf.sample_rate).is_err() {
            return -minix_types::EIO;
        }
        let frame = conf.nr_of_bits * (conf.stereo + 1) / 8;
        let sample_count = conf.fragment_size / frame;
        if self.set_format(conf, sample_count).is_err() {
            return -minix_types::EIO;
        }
        if self.intr_enable(true).is_err() {
            return -minix_types::EIO;
        }
        if self.start_channel(sub_dev).is_err() {
            return -minix_types::EIO;
        }
        self.conf[sub_dev as usize].busy = true;
        0
    }

    fn stop(&mut self, sub_dev: i32) -> i32 {
        // C `drv_stop` (`cmi8738.c:462-473`): interrupt off, then the
        // channel's stop sequence.
        if self.intr_enable(false).is_err() {
            return -minix_types::EIO;
        }
        if self.stop_channel(sub_dev).is_err() {
            return -minix_types::EIO;
        }
        if let Some(conf) = self.conf.get_mut(sub_dev as usize) {
            conf.busy = false;
        }
        0
    }

    fn set_dma(&mut self, dma: u32, length: u32, chan: i32) -> i32 {
        // C `drv_set_dma` (`cmi8738.c:546-554`): with
        // `DMA_LENGTH_BY_FRAME` defined (`cmi8738.h:5`) the byte length
        // becomes a frame count before it reaches the register.
        let Some(conf) = self.conf.get(chan as usize).copied() else {
            return -minix_types::EINVAL;
        };
        let frame = conf.nr_of_bits * (conf.stereo + 1) / 8;
        let frames = length / frame;
        match self.program_dma(dma, frames, chan) {
            Ok(()) => 0,
            Err(status) => status,
        }
    }

    fn reenable_int(&mut self, _chan: i32) -> i32 {
        match self.intr_enable(true) {
            Ok(()) => 0,
            Err(status) => status,
        }
    }

    fn int_sum(&mut self) -> i32 {
        // C `drv_int_sum` (`cmi8738.c:557-565`): read-clear, remember the
        // status, report the two channel bits.
        match self.read_clear_intr_status() {
            Ok(status) => {
                self.intr_status = status;
                (status & (INTR_STS_DAC | INTR_STS_ADC)) as i32
            }
            Err(status) => status,
        }
    }

    fn interrupt(&mut self, sub_dev: i32) -> i32 {
        // C `drv_int` (`cmi8738.c:568-580`): the remembered status
        // filtered by the channel's bit.
        let mask = match sub_dev {
            DAC => INTR_STS_DAC,
            ADC => INTR_STS_ADC,
            _ => return 0,
        };
        (self.intr_status & mask) as i32
    }

    fn pause(&mut self, chan: i32) -> i32 {
        match self.set_paused(chan, true) {
            Ok(()) => 0,
            Err(status) => status,
        }
    }

    fn resume(&mut self, chan: i32) -> i32 {
        match self.set_paused(chan, false) {
            Ok(()) => 0,
            Err(status) => status,
        }
    }

    fn io_ctl(&mut self, request: u64, val: &mut [u8], len: &mut i32, sub_dev: i32) -> i32 {
        self.io_ctl_inner(request, val, len, sub_dev)
    }

    fn get_irq(&self) -> i32 {
        self.irq as i32
    }

    fn get_frag_size(&self, sub_dev: i32) -> u32 {
        self.conf
            .get(sub_dev as usize)
            .map(|conf| conf.fragment_size)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registers::{MIX, REG_SB_DATA};
    use alloc::collections::BTreeMap;
    use alloc::vec::Vec;
    use minix_audiodriver::sound::Device;

    /// A port seam over a register file, with the traffic logged.
    struct FakePorts {
        registers: BTreeMap<u16, u32>,
        writes: Vec<(u16, u32, u8)>,
        delays: Vec<u32>,
        fail: bool,
    }

    impl FakePorts {
        fn new() -> Self {
            FakePorts {
                registers: BTreeMap::new(),
                writes: Vec::new(),
                delays: Vec::new(),
                fail: false,
            }
        }
        fn value(&self, port: u16) -> u32 {
            self.registers.get(&port).copied().unwrap_or(0)
        }
    }

    impl PortIo for FakePorts {
        fn out8(&mut self, port: u16, value: u8) -> Result<(), i32> {
            if self.fail {
                return Err(-minix_types::EIO);
            }
            self.writes.push((port, value as u32, 1));
            self.registers.insert(port, value as u32);
            Ok(())
        }
        fn out16(&mut self, port: u16, value: u16) -> Result<(), i32> {
            if self.fail {
                return Err(-minix_types::EIO);
            }
            self.writes.push((port, value as u32, 2));
            self.registers.insert(port, value as u32);
            Ok(())
        }
        fn out32(&mut self, port: u16, value: u32) -> Result<(), i32> {
            if self.fail {
                return Err(-minix_types::EIO);
            }
            self.writes.push((port, value, 4));
            self.registers.insert(port, value);
            Ok(())
        }
        fn in8(&mut self, port: u16) -> Result<u8, i32> {
            if self.fail {
                return Err(-minix_types::EIO);
            }
            Ok(self.value(port) as u8)
        }
        fn in16(&mut self, port: u16) -> Result<u16, i32> {
            if self.fail {
                return Err(-minix_types::EIO);
            }
            Ok(self.value(port) as u16)
        }
        fn in32(&mut self, port: u16) -> Result<u32, i32> {
            if self.fail {
                return Err(-minix_types::EIO);
            }
            Ok(self.value(port))
        }
        fn delay_micros(&mut self, micros: u32) {
            self.delays.push(micros);
        }
    }

    fn card() -> Cmi8738<FakePorts> {
        Cmi8738::new(FakePorts::new())
    }

    #[test]
    fn test_init_reports_three_sub_devices_and_init_hw_resets() {
        let mut card = card();
        assert_eq!(card.init(), 3);
        assert_eq!(card.init_hw(), 0);
        let writes = &card.ports_mut().writes;
        // The reset pulsed the reset bit; the configure wrote the
        // extended index and the mixer input.
        assert!(
            writes
                .iter()
                .any(|(port, value, _)| *port == REG_MISC_CTRL && *value & CMD_RESET != 0)
        );
        assert!(
            writes
                .iter()
                .any(|(port, value, _)| *port == REG_EXT_INDEX && *value == 0x03)
        );
        assert!(
            writes
                .iter()
                .any(|(port, value, _)| *port == REG_MIX_INPUT && *value == 0x0f)
        );
        // The reset waits 100 microseconds.
        assert_eq!(card.ports_mut().delays, alloc::vec![100]);
        // The default volume wrote all sixteen mixer registers, on top of
        // the four the mixer initialization wrote: twenty data-port bytes
        // behind the address port.
        let data_writes = card
            .ports_mut()
            .writes
            .iter()
            .filter(|(port, _, _)| *port == crate::registers::REG_SB_DATA)
            .count();
        assert_eq!(data_writes, 20);
        // The seeded configuration matches C's `drv_init_hw`.
        let dac = card.conf(DAC).unwrap();
        assert_eq!(dac.sample_rate, 44100);
        assert_eq!(dac.nr_of_bits, 16);
        assert_eq!(dac.stereo, 1);
        assert_eq!(dac.fragment_size, 32768);
        assert!(!dac.busy);
    }

    #[test]
    fn test_start_programs_rate_format_and_the_channel() {
        let mut card = card();
        assert_eq!(card.start(DAC, 2), 0);
        // The rate code for 44100 (index 3) lands in both fields.
        let ctrl1 = card
            .ports_mut()
            .writes
            .iter()
            .rev()
            .find(|(port, _, _)| *port == REG_FUNC_CTRL1)
            .map(|(_, value, _)| *value)
            .unwrap();
        assert_eq!(ctrl1 & 0xe000, 3 << 13);
        assert_eq!(ctrl1 & 0x1c00, 3 << 10);
        // The format register carries stereo plus 16 bits.
        let format = card
            .ports_mut()
            .writes
            .iter()
            .rev()
            .find(|(port, _, _)| *port == REG_FORMAT)
            .map(|(_, value, _)| *value)
            .unwrap();
        assert_eq!(format & 0x03, FMT_STEREO | FMT_BIT16);
        // The sample count is frames minus one: 32768 / (16 * 2 / 8) =
        // 8192, so the register sees 8191.
        let count = card
            .ports_mut()
            .writes
            .iter()
            .rev()
            .find(|(port, _, _)| *port == REG_DAC_SAMPLE_COUNT)
            .map(|(_, value, _)| *value)
            .unwrap();
        assert_eq!(count, 8191);
        assert_eq!(
            card.ports_mut().value(REG_FUNC_CTRL) & CMD_ENA_C0,
            CMD_ENA_C0
        );
        assert!(card.conf(DAC).unwrap().busy);
        assert_eq!(card.stop(DAC), 0);
        assert!(!card.conf(DAC).unwrap().busy);
        // The stop cleared the enable bit and released the reset pulse.
        assert_eq!(card.ports_mut().value(REG_FUNC_CTRL) & CMD_ENA_C0, 0);
        assert_eq!(card.ports_mut().value(REG_FUNC_CTRL) & CMD_RESET_C0, 0);
        // `drv_start` waits nowhere; the stop's reset pulse is the one
        // hundred-microsecond wait (`cmi8738.c:155`).
        assert_eq!(card.ports_mut().delays, alloc::vec![100]);
    }

    #[test]
    fn test_an_unknown_rate_programs_the_first_table_entry() {
        // C leaves `rate` at zero when nothing matches.
        let mut card = card();
        let mut len = 4;
        assert_eq!(
            card.io_ctl_inner(DSPIORATE, &mut 12345u32.to_le_bytes(), &mut len, DAC),
            0
        );
        assert_eq!(card.start(DAC, 2), 0);
        let ctrl1 = card
            .ports_mut()
            .writes
            .iter()
            .rev()
            .find(|(port, _, _)| *port == REG_FUNC_CTRL1)
            .map(|(_, value, _)| *value)
            .unwrap();
        assert_eq!(ctrl1 & 0xe000, 0, "the 5512 Hz code");
    }

    #[test]
    fn test_set_dma_divides_the_length_into_frames() {
        let mut card = card();
        // 32768 bytes at 16 bits stereo is 8192 frames.
        assert_eq!(card.set_dma(0x0010_0000, 32768, DAC), 0);
        assert_eq!(card.ports_mut().value(REG_DAC_DMA_ADDR), 0x0010_0000);
        assert_eq!(
            card.ports_mut().value(REG_DAC_DMA_LEN),
            8191,
            "frames minus one"
        );
        assert_eq!(card.set_dma(0, 4096, 9), -minix_types::EINVAL);
    }

    #[test]
    fn test_the_ioctl_set_round_trips_the_dsp_requests() {
        let mut card = card();
        let mut len = 4;
        // A fragment size between the minimum and one whole fragment is
        // accepted; anything outside is refused (`set_frag_size`).
        assert_eq!(
            card.io_ctl_inner(DSPIOSIZE, &mut 4096u32.to_le_bytes(), &mut len, DAC),
            0
        );
        assert_eq!(card.get_frag_size(DAC), 4096);
        assert_eq!(
            card.io_ctl_inner(DSPIOSIZE, &mut 1023u32.to_le_bytes(), &mut len, DAC),
            -minix_types::EINVAL
        );
        assert_eq!(
            card.io_ctl_inner(DSPIOSIZE, &mut 32769u32.to_le_bytes(), &mut len, DAC),
            -minix_types::EINVAL
        );
        // The maximum is one whole fragment.
        let mut answer = [0u8; 4];
        let mut len = 0;
        assert_eq!(card.io_ctl_inner(DSPIOMAX, &mut answer, &mut len, DAC), 0);
        assert_eq!(u32::from_le_bytes(answer), 32768);
        assert_eq!(len, 4);
        // The sample counter combines the extra-buffer count and the
        // running DMA address.
        card.ports_mut().registers.insert(REG_DAC_DMA_ADDR, 0x40);
        let mut answer = [0u8; 4];
        let mut len = 0;
        assert_eq!(
            card.io_ctl_inner(DSPIOSAMPLESINBUF, &mut answer, &mut len, DAC),
            0
        );
        assert_eq!(u32::from_le_bytes(answer), 0x40);
        // An unknown request is refused.
        assert_eq!(
            card.io_ctl_inner(0xdead_beef, &mut answer, &mut len, DAC),
            -minix_types::EINVAL
        );
        // The mixer sub-device has no fragment geometry to report.
        assert_eq!(
            card.io_ctl_inner(DSPIOMAX, &mut answer, &mut len, MIX),
            -minix_types::EINVAL
        );
    }

    #[test]
    fn test_the_mixer_ioctls_reach_the_volume_table() {
        let mut card = card();
        let level = VolumeLevel {
            device: Device::Master,
            left: 0x0c,
            right: 0x0c,
        };
        let mut len = 0;
        assert_eq!(
            card.io_ctl_inner(MIXIOSETVOLUME, &mut level.encode(), &mut len, MIX),
            0
        );
        assert_eq!(
            card.ports_mut().value(REG_SB_DATA),
            0xc0,
            "the last write was the right channel"
        );
        // A get reads the pair back.
        let mut bytes = level.encode();
        let mut len = 0;
        assert_eq!(
            card.io_ctl_inner(MIXIOGETVOLUME, &mut bytes, &mut len, MIX),
            0
        );
        assert_eq!(VolumeLevel::decode(&bytes).unwrap().left, 0x0c);
        // A device outside the enumerator never reaches the card.
        let mut bad = level.encode();
        bad[0] = 9;
        assert_eq!(
            card.io_ctl_inner(MIXIOGETVOLUME, &mut bad, &mut len, MIX),
            -minix_types::EINVAL
        );
    }

    #[test]
    fn test_int_sum_remembers_the_status_for_the_channel_hook() {
        let mut card = card();
        card.ports_mut()
            .registers
            .insert(REG_INTR_STS, INTR_STS_ADC);
        assert_eq!(card.int_sum(), INTR_STS_ADC as i32);
        assert_eq!(card.interrupt(ADC), INTR_STS_ADC as i32);
        assert_eq!(card.interrupt(DAC), 0);
        assert_eq!(card.interrupt(MIX), 0);
    }

    #[test]
    fn test_a_failing_port_seam_is_reported() {
        let mut card = Cmi8738::new(FakePorts {
            fail: true,
            ..FakePorts::new()
        });
        assert_eq!(card.init_hw(), -minix_types::EIO);
        assert_eq!(card.start(DAC, 2), -minix_types::EIO);
        assert_eq!(card.stop(DAC), -minix_types::EIO);
    }
}
