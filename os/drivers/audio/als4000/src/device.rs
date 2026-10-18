//! The ALS4000 card face: the register sequences behind `AudioHooks`.
//!
//! C correspondence: `minix3/minix/drivers/audio/als4000/als4000.c` — the
//! global-control accessors (`dev_gcr_read`/`dev_gcr_write`, `:45-52`),
//! the Sound Blaster command engine (`dev_command`, `:55-71`), the mixer
//! accessors (`:75-94`), the reset (`dev_reset`, `:97-112`), the
//! configuration (`dev_configure`, `:115-129`), the sample rate and
//! format writers (`:126-167`), the channel start/stop (`:170-187`), the
//! DMA programming (`:190-208`), the interrupt status and control
//! (`:222-248`), and the `drv_*` hooks that compose them (`:494-639`).
//!
//! The mixer's register traffic lives in [`crate::mixer`]; the port seam
//! in [`crate::ports`]; the DMA buffers belong to the service (registered
//! `E-DMABUF`).

use minix_audiodriver::hooks::AudioHooks;
use minix_audiodriver::sound::{
    DSPIOBITS, DSPIOFREEBUF, DSPIOMAX, DSPIOPAUSE, DSPIORATE, DSPIORESET, DSPIORESUME,
    DSPIOSAMPLESINBUF, DSPIOSIGN, DSPIOSIZE, DSPIOSTEREO, MIXIOGETVOLUME, MIXIOSETVOLUME,
    VolumeLevel,
};

use crate::mixer::{get_set_volume, mixer_read, mixer_write, set_default_volume};
use crate::ports::PortIo;
use crate::registers::{
    ADC, CMD_BIT8_AI, CMD_BIT8_DMA_OFF, CMD_BIT8_DMA_ON, CMD_BIT16_AI, CMD_BIT16_DMA_OFF,
    CMD_BIT16_DMA_ON, CMD_INTR_ENABLE, CMD_MIXER_WRITE_ENABLE, CMD_REC_SIGN, CMD_REC_STEREO,
    CMD_REC_WIDTH8, CMD_SAMPLE_RATE_OUT, CMD_SIGN_STEREO, CMD_SOUND_ON, CMD_UNSIGN_MONO,
    CMD_UNSIGN_STEREO, COMMAND_TRIES, DAC, DAC_DMA_LEN_FLAGS, INTR_STS_ADC, INTR_STS_DAC,
    REG_ADC_CUR_ADDR, REG_ADC_DMA_ADDR, REG_ADC_DMA_LEN, REG_DAC_CUR_ADDR, REG_DAC_DMA_ADDR,
    REG_DAC_DMA_LEN, REG_DMA_EM_CTRL, REG_GCR_DATA, REG_GCR_INDEX, REG_INTR_CTRL, REG_INTR_STS,
    REG_SB_BASE, REG_SB_CONFIG, REG_SB_CTRL, REG_SB_DATA, REG_SB_DMA_SETUP, REG_SB_FIFO_CTRL,
    REG_SB_FIFO_LEN_HI, REG_SB_FIFO_LEN_LO, REG_SB_IRQ_STATUS, REG_SB_READ, REG_SB_RESET,
    RESET_TRIES,
};

/// One channel's configuration (C's `aud_conf[]`: the fields `drv_init_hw`
/// seeds at `als4000.c:452-464` and `drv_io_ctl` lets a user change).
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
    /// The capture format byte (`format`).
    pub format: u32,
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
pub struct Als4000<P: PortIo> {
    ports: P,
    /// Per-sub-device geometry, indexed by `DAC`/`ADC`/`MIX` (C's global
    /// `sub_dev[]`).
    geometry: [Geometry; 3],
    /// Per-sub-device configuration, indexed by `DAC`/`ADC`/`MIX` (C's
    /// global `aud_conf[]`).
    conf: [ChannelConf; 3],
    /// The capture format byte C keeps in a file static (`rec_format`,
    /// `als4000.c:133`).
    rec_format: u32,
    /// The last interrupt status `drv_int_sum` read (`dev.intr_status`).
    intr_status: u32,
    /// The card's IRQ line, as the PCI probe resolved it (`dev.irq`).
    irq: u8,
    /// How many extra buffers are filled (`BufLength`), for
    /// `DSPIOFREEBUF` and `DSPIOSAMPLESINBUF`.
    buf_length: u32,
}

impl<P: PortIo> Als4000<P> {
    /// A face over the port seam, with the geometry `drv_init` sets
    /// (`als4000.c:376-409`) and the configuration `drv_init_hw` seeds
    /// (`als4000.c:452-464`).
    pub fn new(ports: P) -> Self {
        let playback = Geometry {
            dma_size: 64 * 1024,
            fragments: 2,
            min_fragment_size: 1024,
            extra_buffers: 4,
        };
        let control_only = Geometry {
            // The mixer sub-device is control-only: C leaves its DMA
            // fields zero and skips it when seeding the configuration.
            dma_size: 0,
            fragments: 0,
            min_fragment_size: 0,
            extra_buffers: 0,
        };
        let seeded = ChannelConf {
            busy: false,
            stereo: 1,
            sample_rate: 44100,
            nr_of_bits: 16,
            sign: 1,
            fragment_size: 64 * 1024 / 2,
            format: 0,
        };
        Als4000 {
            ports,
            geometry: [playback, playback, control_only],
            conf: [seeded, seeded, seeded],
            rec_format: 0,
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

    /// The capture format byte (`rec_format`).
    pub fn rec_format(&self) -> u32 {
        self.rec_format
    }

    /// Read a global control register (`dev_gcr_read`, `als4000.c:45-49`).
    fn gcr_read(&mut self, reg: u16) -> Result<u32, i32> {
        self.ports.out8(REG_GCR_INDEX, reg as u8)?;
        self.ports.in32(REG_GCR_DATA)
    }

    /// Write a global control register (`dev_gcr_write`,
    /// `als4000.c:51-54`).
    fn gcr_write(&mut self, reg: u16, value: u32) -> Result<(), i32> {
        self.ports.out8(REG_GCR_INDEX, reg as u8)?;
        self.ports.out32(REG_GCR_DATA, value)
    }

    /// Send one Sound Blaster command (`dev_command`, `als4000.c:55-71`):
    /// wait until the command port's busy bit clears, then write the
    /// command. C prints a warning and drops the command when the port
    /// never clears; the rewrite reports the failure.
    fn command(&mut self, cmd: u32) -> Result<(), i32> {
        for _ in 0..COMMAND_TRIES {
            let status = self.ports.in8(REG_SB_BASE + crate::registers::REG_SB_CMD)?;
            if status & 0x80 == 0 {
                return self
                    .ports
                    .out8(REG_SB_BASE + crate::registers::REG_SB_CMD, cmd as u8);
            }
        }
        Err(-minix_types::EIO)
    }

    /// Reset the card (`dev_reset`, `als4000.c:97-112`): pulse the reset
    /// line, then poll for the Sound Blaster echo.
    pub fn reset(&mut self) -> Result<(), i32> {
        self.ports.out8(REG_SB_RESET, 1)?;
        self.ports.delay_micros(10);
        self.ports.out8(REG_SB_RESET, 0)?;
        self.ports.delay_micros(30);
        for _ in 0..RESET_TRIES {
            if self.ports.in8(REG_SB_BASE + REG_SB_DATA)? & 0x80 != 0 {
                if self.ports.in8(REG_SB_BASE + REG_SB_READ)? == 0xaa {
                    break;
                }
                // C reports the wrong echo immediately (`als4000.c:106-108`).
                return Err(-minix_types::EIO);
            }
        }
        Ok(())
    }

    /// Configure the card (`dev_configure`, `als4000.c:115-129`).
    pub fn configure(&mut self) -> Result<(), i32> {
        let data = mixer_read(&mut self.ports, REG_SB_CONFIG as u8)? as u32;
        mixer_write(
            &mut self.ports,
            (REG_SB_CONFIG | REG_SB_CTRL) as u8,
            (data | CMD_MIXER_WRITE_ENABLE) as u8,
        )?;
        mixer_write(&mut self.ports, REG_SB_DMA_SETUP as u8, 0x01)?;
        mixer_write(
            &mut self.ports,
            (REG_SB_CONFIG | REG_SB_CTRL) as u8,
            (data & !CMD_MIXER_WRITE_ENABLE) as u8,
        )?;
        for reg in 0x91..=0x96u16 {
            self.gcr_write(reg, 0)?;
        }
        // C reads the extended-mode control register and writes it back
        // unchanged (`als4000.c:127-129`: the masking line above the write
        // is commented out).
        let data = self.gcr_read(REG_DMA_EM_CTRL)?;
        self.gcr_write(REG_DMA_EM_CTRL, data)
    }

    /// Set the sample rate (`dev_set_sample_rate`, `als4000.c:126-132`):
    /// the rate command then the two rate bytes.
    fn set_sample_rate(&mut self, sample_rate: u16) -> Result<(), i32> {
        self.command(CMD_SAMPLE_RATE_OUT)?;
        self.command((sample_rate >> 8) as u32)?;
        self.command((sample_rate & 0xff) as u32)
    }

    /// Set the format (`dev_set_format`, `als4000.c:135-167`): the
    /// auto-initialize command, the sign and stereo byte, the sample
    /// count, and the FIFO length.
    ///
    /// C's signed-mono arm writes the **stereo** command
    /// (`als4000.c:153-154`: `else format = CMD_SIGN_STEREO;` where the
    /// other three arms pick the matching constant) — a defect the
    /// rewrite preserves, recorded in document `21-audio-drivers.md`.
    fn set_format(&mut self, conf: ChannelConf, mut sample_count: u32) -> Result<(), i32> {
        let mut format;
        if conf.nr_of_bits == 16 {
            format = CMD_BIT16_AI;
            self.rec_format = 0;
        } else if conf.nr_of_bits == 8 {
            format = CMD_BIT8_AI;
            self.rec_format = CMD_REC_WIDTH8;
        } else {
            return Err(-minix_types::EINVAL);
        }
        self.command(format)?;
        if conf.sign == 0 {
            if conf.stereo == 1 {
                format = CMD_UNSIGN_STEREO;
                self.rec_format |= CMD_REC_STEREO;
            } else {
                format = CMD_UNSIGN_MONO;
            }
        } else {
            self.rec_format |= CMD_REC_SIGN;
            if conf.stereo == 1 {
                format = CMD_SIGN_STEREO;
                self.rec_format |= CMD_REC_STEREO;
            } else {
                format = CMD_SIGN_STEREO;
            }
        }
        self.command(format)?;
        if conf.nr_of_bits == 16 {
            sample_count >>= 1;
        }
        sample_count -= 1;
        self.command(sample_count & 0xff)?;
        self.command(sample_count >> 8)?;
        mixer_write(
            &mut self.ports,
            (REG_SB_FIFO_LEN_LO | REG_SB_CTRL) as u8,
            (sample_count & 0xff) as u8,
        )?;
        mixer_write(
            &mut self.ports,
            (REG_SB_FIFO_LEN_HI | REG_SB_CTRL) as u8,
            (sample_count >> 8) as u8,
        )
    }

    /// Start one channel (`dev_start_channel`, `als4000.c:170-180`).
    fn start_channel(&mut self, sub_dev: i32) -> Result<(), i32> {
        self.command(CMD_SOUND_ON)?;
        match sub_dev {
            DAC => {
                self.command(CMD_BIT16_DMA_ON)?;
                self.command(CMD_BIT8_DMA_ON)
            }
            ADC => mixer_write(
                &mut self.ports,
                (REG_SB_FIFO_CTRL | REG_SB_CTRL) as u8,
                (self.rec_format | 0x80) as u8,
            ),
            _ => Ok(()),
        }
    }

    /// Stop one channel (`dev_stop_channel`, `als4000.c:183-193`).
    fn stop_channel(&mut self, sub_dev: i32) -> Result<(), i32> {
        match sub_dev {
            DAC => {
                self.command(CMD_BIT16_DMA_OFF)?;
                self.command(CMD_BIT8_DMA_OFF)
            }
            ADC => mixer_write(
                &mut self.ports,
                (REG_SB_FIFO_CTRL | REG_SB_CTRL) as u8,
                0x00,
            ),
            _ => Ok(()),
        }
    }

    /// Program one channel's DMA (`dev_set_dma`, `als4000.c:196-208`):
    /// the address and the length minus one, with the playback length
    /// carrying flag bits.
    fn program_dma(&mut self, dma: u32, length: u32, sub_dev: i32) -> Result<(), i32> {
        match sub_dev {
            DAC => {
                self.gcr_write(REG_DAC_DMA_ADDR, dma)?;
                self.gcr_write(REG_DAC_DMA_LEN, (length - 1) | DAC_DMA_LEN_FLAGS)
            }
            ADC => {
                self.gcr_write(REG_ADC_DMA_ADDR, dma)?;
                self.gcr_write(REG_ADC_DMA_LEN, length - 1)
            }
            _ => Ok(()),
        }
    }

    /// The channel's running DMA address (`dev_read_dma_current`,
    /// `als4000.c:211-218`): the low sixteen bits of the current-address
    /// register.
    fn read_dma_current(&mut self, sub_dev: i32) -> Result<u32, i32> {
        let value = match sub_dev {
            DAC => self.gcr_read(REG_DAC_CUR_ADDR)?,
            ADC => self.gcr_read(REG_ADC_CUR_ADDR)?,
            _ => return Ok(0),
        };
        Ok(value & 0xffff)
    }

    /// Enable or disable the interrupt (`dev_intr_enable`,
    /// `als4000.c:241-248`).
    fn intr_enable(&mut self, enable: bool) -> Result<(), i32> {
        let data = self.gcr_read(REG_INTR_CTRL)?;
        let updated = if enable {
            data | CMD_INTR_ENABLE
        } else {
            data & !CMD_INTR_ENABLE
        };
        self.gcr_write(REG_INTR_CTRL, updated)
    }

    /// Read and clear the interrupt status (`dev_read_clear_intr_status`,
    /// `als4000.c:222-238`): read the status, write it back, then ack the
    /// Sound Blaster side through one of three reads.
    fn read_clear_intr_status(&mut self) -> Result<u32, i32> {
        let status = self.ports.in8(REG_INTR_STS)? as u32;
        self.ports.out8(REG_INTR_STS, status as u8)?;
        let data = mixer_read(&mut self.ports, REG_SB_IRQ_STATUS as u8)? as u32;
        if data & 0x02 != 0 {
            self.ports.in8(REG_SB_BASE + 0x0f)?;
        } else if data & 0x01 != 0 {
            self.ports.in8(REG_SB_BASE + 0x0e)?;
        } else if data & 0x20 != 0 {
            self.ports.in8(0x16)?;
        }
        Ok(status)
    }

    /// The card's ioctl set (`drv_io_ctl`, `als4000.c:524-576`).
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
                // `set_frag_size` (`als4000.c:347-355`).
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
                // `get_max_frag_size` (`als4000.c:358-363`).
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
                // `free_buf` (`als4000.c:366-373`).
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
                // `get_samples_in_buf` (`als4000.c:376-382`).
                let Ok(current) = self.read_dma_current(chan) else {
                    return -minix_types::EIO;
                };
                write_u32(val, len, self.buf_length * 8192 + current);
                0
            }
            DSPIOPAUSE => self.pause(chan),
            DSPIORESUME => self.resume(chan),
            MIXIOGETVOLUME | MIXIOSETVOLUME => {
                // `get_set_volume` (`als4000.c:567-573`): C passes only the
                // base, so any channel's ioctl reaches the mixer.
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

impl<P: PortIo> AudioHooks for Als4000<P> {
    fn init(&mut self) -> i32 {
        // C `drv_init` (`als4000.c:376-409`): three sub-devices and three
        // special files; the table itself lives in the service.
        3
    }

    fn init_hw(&mut self) -> i32 {
        // C `drv_init_hw` (`als4000.c:412-467`) minus the PCI probe, which
        // the binary performs: reset, configure, initialize the mixer
        // (empty in C, `als4000.c:120-122`), then the default volume.
        if self.reset().is_err() {
            return -minix_types::EIO;
        }
        if self.configure().is_err() {
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
        // C `drv_start` (`als4000.c:472-506`): the rate, the format, the
        // interrupt, then the channel. `DMA_LENGTH_BY_FRAME` is **not**
        // defined for this card (`als4000.h` never defines it), so the
        // sample count is the fragment size as-is.
        let Some(conf) = self.conf.get(sub_dev as usize).copied() else {
            return -minix_types::EINVAL;
        };
        if self.set_sample_rate(conf.sample_rate as u16).is_err() {
            return -minix_types::EIO;
        }
        if self.set_format(conf, conf.fragment_size).is_err() {
            return -minix_types::EIO;
        }
        let _ = self.reenable_int(sub_dev);
        if self.start_channel(sub_dev).is_err() {
            return -minix_types::EIO;
        }
        self.conf[sub_dev as usize].busy = true;
        0
    }

    fn stop(&mut self, sub_dev: i32) -> i32 {
        // C `drv_stop` (`als4000.c:509-519`): interrupt off, then the
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
        // C `drv_set_dma` (`als4000.c:588-596`): the frame conversion is
        // compiled out for this card, so the byte length reaches the
        // register as-is.
        match self.program_dma(dma, length, chan) {
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
        // C `drv_int_sum` (`als4000.c:599-610`): read-clear, remember the
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
        // C `drv_int` (`als4000.c:613-625`): the remembered status
        // filtered by the channel's bit.
        let mask = match sub_dev {
            DAC => INTR_STS_DAC,
            ADC => INTR_STS_ADC,
            _ => return 0,
        };
        (self.intr_status & mask) as i32
    }

    fn pause(&mut self, chan: i32) -> i32 {
        // C `drv_pause` (`als4000.c:628-633`) is the stop sequence for
        // the channel's direction.
        match self.stop_channel(chan) {
            Ok(()) => 0,
            Err(status) => status,
        }
    }

    fn resume(&mut self, chan: i32) -> i32 {
        // C `drv_resume` (`als4000.c:636-641`) is the start sequence for
        // the channel's direction.
        match self.start_channel(chan) {
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
    use crate::registers::MIX;
    use alloc::collections::BTreeMap;
    use alloc::vec::Vec;

    /// A port seam over a register file, with the traffic logged.
    ///
    /// The global-control pair is modeled as a real index-and-data pair
    /// (the data port's value depends on the last index written), and the
    /// Sound Blaster command port's read is a status line: zero when the
    /// card is ready, `0x80` when it is busy.
    struct FakePorts {
        registers: BTreeMap<u16, u32>,
        /// The global-control register file, keyed by index.
        gcr: BTreeMap<u8, u32>,
        /// The index the last `REG_GCR_INDEX` write selected.
        gcr_index: u8,
        writes: Vec<(u16, u32, u8)>,
        delays: Vec<u32>,
        /// Whether the Sound Blaster command port reports busy.
        command_busy: bool,
        /// Whether the data port reports an echo waiting after the reset.
        reset_ready: bool,
        /// What the read port answers after the reset.
        reset_echo: u8,
        fail: bool,
    }

    impl FakePorts {
        fn new() -> Self {
            FakePorts {
                registers: BTreeMap::new(),
                gcr: BTreeMap::new(),
                gcr_index: 0,
                writes: Vec::new(),
                delays: Vec::new(),
                command_busy: false,
                reset_ready: true,
                reset_echo: 0xaa,
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
            if port == REG_GCR_INDEX {
                self.gcr_index = value;
            } else {
                self.registers.insert(port, value as u32);
            }
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
            if port == REG_GCR_DATA {
                self.gcr.insert(self.gcr_index, value);
            } else {
                self.registers.insert(port, value);
            }
            Ok(())
        }
        fn in8(&mut self, port: u16) -> Result<u8, i32> {
            if self.fail {
                return Err(-minix_types::EIO);
            }
            if port == REG_SB_BASE + crate::registers::REG_SB_CMD {
                return Ok(if self.command_busy { 0x80 } else { 0 });
            }
            if port == REG_SB_BASE + REG_SB_DATA {
                return Ok(if self.reset_ready { 0x80 } else { 0 });
            }
            if port == REG_SB_BASE + REG_SB_READ {
                return Ok(self.reset_echo);
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
            if port == REG_GCR_DATA {
                return Ok(self.gcr.get(&self.gcr_index).copied().unwrap_or(0));
            }
            Ok(self.value(port))
        }
        fn delay_micros(&mut self, micros: u32) {
            self.delays.push(micros);
        }
    }

    fn card() -> Als4000<FakePorts> {
        Als4000::new(FakePorts::new())
    }

    #[test]
    fn test_init_reports_three_sub_devices_and_init_hw_configures() {
        let mut card = card();
        assert_eq!(card.init(), 3);
        assert_eq!(card.init_hw(), 0);
        // The reset pulsed the line and waited ten and thirty
        // microseconds.
        assert_eq!(card.ports_mut().delays[0], 10);
        assert_eq!(card.ports_mut().delays[1], 30);
        let writes = &card.ports_mut().writes;
        // The configuration enabled the mixer write, set the DMA setup,
        // and cleared the six extended-mode registers.
        assert!(writes.iter().any(|(port, value, _)| *port
            == REG_SB_BASE + crate::registers::REG_MIXER_ADDR
            && *value == (REG_SB_CONFIG | REG_SB_CTRL) as u32));
        let gcr_indices: Vec<u32> = writes
            .iter()
            .filter(|(port, _, _)| *port == REG_GCR_INDEX)
            .map(|(_, value, _)| *value)
            .collect();
        for reg in 0x91..=0x96u32 {
            assert!(gcr_indices.contains(&reg), "register {reg:#x} was cleared");
        }
        assert!(gcr_indices.contains(&(REG_DMA_EM_CTRL as u32)));
        // The seeded configuration matches C's `drv_init_hw`.
        let dac = card.conf(DAC).unwrap();
        assert_eq!(dac.sample_rate, 44100);
        assert_eq!(dac.nr_of_bits, 16);
        assert_eq!(dac.stereo, 1);
        assert_eq!(dac.sign, 1);
        assert_eq!(dac.fragment_size, 32768);
        assert!(!dac.busy);
    }

    #[test]
    fn test_the_reset_reports_a_wrong_echo() {
        // A correct echo ends the poll.
        assert_eq!(card().reset(), Ok(()));
        // The wrong echo is reported immediately (`als4000.c:106-108`).
        let mut card = Als4000::new(FakePorts {
            reset_echo: 0x55,
            ..FakePorts::new()
        });
        assert_eq!(card.reset(), Err(-minix_types::EIO));
        // A card that never reports an echo runs the poll out and is
        // still accepted (C's loop falls through to `return OK`).
        let mut card = Als4000::new(FakePorts {
            reset_ready: false,
            ..FakePorts::new()
        });
        assert_eq!(card.reset(), Ok(()));
    }

    #[test]
    fn test_a_busy_command_port_is_reported() {
        let mut card = Als4000::new(FakePorts {
            command_busy: true,
            ..FakePorts::new()
        });
        assert_eq!(card.command(CMD_SOUND_ON), Err(-minix_types::EIO));
        assert!(card.ports_mut().writes.is_empty(), "no command was sent");
    }

    #[test]
    fn test_start_programs_the_rate_format_and_the_channel() {
        let mut card = card();
        assert_eq!(card.start(DAC, 2), 0);
        // The command port received the rate command, the two rate bytes
        // (44100 = 0xac44), the sixteen-bit format, the sign and stereo
        // byte, the sample count, and the two DMA-on commands.
        let commands: Vec<u8> = card
            .ports_mut()
            .writes
            .iter()
            .filter(|(port, _, _)| *port == REG_SB_BASE + crate::registers::REG_SB_CMD)
            .map(|(_, value, _)| *value as u8)
            .collect();
        assert_eq!(commands[0], CMD_SAMPLE_RATE_OUT as u8);
        assert_eq!(commands[1], 0xac);
        assert_eq!(commands[2], 0x44);
        assert_eq!(commands[3], CMD_BIT16_AI as u8);
        assert_eq!(commands[4], CMD_SIGN_STEREO as u8, "signed stereo");
        // The sample count is the fragment size over two (sixteen bits)
        // minus one: 32768 / 2 - 1 = 16383 = 0x3fff.
        assert_eq!(commands[5], 0xff);
        assert_eq!(commands[6], 0x3f);
        assert_eq!(commands[7], CMD_SOUND_ON as u8);
        assert_eq!(commands[8], CMD_BIT16_DMA_ON as u8);
        assert_eq!(commands[9], CMD_BIT8_DMA_ON as u8);
        // The FIFO length was written through the mixer pair.
        assert_eq!(
            card.ports_mut()
                .value(REG_SB_BASE + crate::registers::REG_MIXER_ADDR),
            (REG_SB_FIFO_LEN_HI | REG_SB_CTRL) as u32,
            "the last mixer index was the high length byte"
        );
        assert!(card.conf(DAC).unwrap().busy);
        // The capture channel's start ends with the FIFO control write
        // carrying the capture format byte.
        assert_eq!(card.start(ADC, 2), 0);
        assert_eq!(card.rec_format() & CMD_REC_STEREO, CMD_REC_STEREO);
        assert_eq!(card.rec_format() & CMD_REC_SIGN, CMD_REC_SIGN);
        assert_eq!(card.rec_format() & CMD_REC_WIDTH8, 0);
        // A stop clears the busy flag and sends the DMA-off pair.
        assert_eq!(card.stop(DAC), 0);
        assert!(!card.conf(DAC).unwrap().busy);
    }

    #[test]
    fn test_the_signed_mono_arm_keeps_the_c_defect() {
        // C writes the stereo command for signed mono (`als4000.c:153-154`).
        let mut card = card();
        let mut len = 4;
        assert_eq!(
            card.io_ctl_inner(DSPIOSIGN, &mut 1u32.to_le_bytes(), &mut len, DAC),
            0
        );
        assert_eq!(
            card.io_ctl_inner(DSPIOSTEREO, &mut 0u32.to_le_bytes(), &mut len, DAC),
            0
        );
        assert_eq!(card.start(DAC, 2), 0);
        let commands: Vec<u8> = card
            .ports_mut()
            .writes
            .iter()
            .filter(|(port, _, _)| *port == REG_SB_BASE + crate::registers::REG_SB_CMD)
            .map(|(_, value, _)| *value as u8)
            .collect();
        assert!(
            commands.contains(&(CMD_SIGN_STEREO as u8)),
            "signed mono still asks for the stereo byte"
        );
    }

    #[test]
    fn test_set_dma_does_not_divide_the_length() {
        // `DMA_LENGTH_BY_FRAME` is not defined for this card.
        let mut card = card();
        assert_eq!(card.set_dma(0x0010_0000, 32768, DAC), 0);
        let gcr_writes: Vec<u32> = card
            .ports_mut()
            .writes
            .iter()
            .filter(|(port, _, size)| *port == REG_GCR_DATA && *size == 4)
            .map(|(_, value, _)| *value)
            .collect();
        assert_eq!(gcr_writes[0], 0x0010_0000, "the address as given");
        assert_eq!(
            gcr_writes[1],
            (32767) | DAC_DMA_LEN_FLAGS,
            "the byte length minus one, with the flag bits"
        );
        // The capture channel's length carries no flags.
        assert_eq!(card.set_dma(0x0020_0000, 4096, ADC), 0);
        let gcr_writes: Vec<u32> = card
            .ports_mut()
            .writes
            .iter()
            .filter(|(port, _, size)| *port == REG_GCR_DATA && *size == 4)
            .map(|(_, value, _)| *value)
            .collect();
        assert_eq!(gcr_writes[3], 4095);
        // The mixer sub-device has no DMA registers.
        assert_eq!(card.set_dma(0, 0, MIX), 0, "C's default arm is silent");
    }

    #[test]
    fn test_the_ioctl_set_round_trips_the_dsp_requests() {
        let mut card = card();
        let mut len = 4;
        assert_eq!(
            card.io_ctl_inner(DSPIOSIZE, &mut 4096u32.to_le_bytes(), &mut len, DAC),
            0
        );
        assert_eq!(card.get_frag_size(DAC), 4096);
        assert_eq!(
            card.io_ctl_inner(DSPIOSIZE, &mut 1023u32.to_le_bytes(), &mut len, DAC),
            -minix_types::EINVAL
        );
        let mut answer = [0u8; 4];
        let mut len = 0;
        assert_eq!(card.io_ctl_inner(DSPIOMAX, &mut answer, &mut len, DAC), 0);
        assert_eq!(u32::from_le_bytes(answer), 32768);
        // The sample counter combines the extra-buffer count and the
        // running DMA address (the low sixteen bits).
        card.ports_mut()
            .gcr
            .insert(REG_DAC_CUR_ADDR as u8, 0x1_0040);
        let mut answer = [0u8; 4];
        let mut len = 0;
        assert_eq!(
            card.io_ctl_inner(DSPIOSAMPLESINBUF, &mut answer, &mut len, DAC),
            0
        );
        assert_eq!(u32::from_le_bytes(answer), 0x40);
        assert_eq!(
            card.io_ctl_inner(0xdead_beef, &mut answer, &mut len, DAC),
            -minix_types::EINVAL
        );
    }

    #[test]
    fn test_the_mixer_ioctls_reach_the_volume_table_on_any_channel() {
        let mut card = card();
        let level = VolumeLevel {
            device: minix_audiodriver::sound::Device::Master,
            left: 0x0c,
            right: 0x0c,
        };
        let mut len = 0;
        // C passes only the base, so the playback channel reaches the
        // mixer too.
        assert_eq!(
            card.io_ctl_inner(MIXIOSETVOLUME, &mut level.encode(), &mut len, DAC),
            0
        );
        assert_eq!(
            card.ports_mut()
                .value(REG_SB_BASE + crate::registers::REG_MIXER_DATA),
            0xc0
        );
        let mut bytes = level.encode();
        let mut len = 0;
        assert_eq!(
            card.io_ctl_inner(MIXIOGETVOLUME, &mut bytes, &mut len, MIX),
            0
        );
        assert_eq!(VolumeLevel::decode(&bytes).unwrap().left, 0x0c);
        let mut bad = level.encode();
        bad[0] = 9;
        assert_eq!(
            card.io_ctl_inner(MIXIOGETVOLUME, &mut bad, &mut len, MIX),
            -minix_types::EINVAL
        );
    }

    #[test]
    fn test_the_interrupt_hooks_report_the_channel_bits() {
        let mut card = card();
        card.ports_mut()
            .registers
            .insert(REG_INTR_STS, INTR_STS_ADC);
        assert_eq!(card.int_sum(), INTR_STS_ADC as i32);
        assert_eq!(card.interrupt(ADC), INTR_STS_ADC as i32);
        assert_eq!(card.interrupt(DAC), 0);
        assert_eq!(card.interrupt(MIX), 0);
        // The read-clear wrote the status back.
        assert_eq!(card.ports_mut().value(REG_INTR_STS), INTR_STS_ADC);
    }

    #[test]
    fn test_a_failing_port_seam_is_reported() {
        let mut card = Als4000::new(FakePorts {
            fail: true,
            ..FakePorts::new()
        });
        assert_eq!(card.init_hw(), -minix_types::EIO);
        assert_eq!(card.start(DAC, 2), -minix_types::EIO);
        assert_eq!(card.stop(DAC), -minix_types::EIO);
        assert_eq!(card.set_dma(0, 4096, DAC), -minix_types::EIO);
    }
}
