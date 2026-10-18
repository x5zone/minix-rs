//! The CS4281 card face: the register sequences behind `AudioHooks`.
//!
//! C correspondence: `minix3/minix/drivers/audio/cs4281/cs4281.c` — the
//! reset (`dev_reset`, `:96-116`), the configuration (`dev_configure`,
//! `:119-135`), the sample-rate writer (`:137-147`), the format builder
//! (`dev_set_format`, `:150-164`), the channel start and stop
//! (`:167-217`), the DMA programming (`:220-232`), the current-address
//! read (`:235-245`), pause and resume (`:248-286`), the interrupt status
//! and control (`:289-314`), and the `drv_*` hooks that compose them
//! (`:530-670`).
//!
//! The codec's register traffic lives in [`crate::mixer`]; the register
//! window in [`crate::window`]; the DMA buffers belong to the service
//! (registered `E-DMABUF`).

use minix_audiodriver::hooks::AudioHooks;
use minix_audiodriver::sound::{
    DSPIOBITS, DSPIOFREEBUF, DSPIOMAX, DSPIOPAUSE, DSPIORATE, DSPIORESET, DSPIORESUME,
    DSPIOSAMPLESINBUF, DSPIOSIGN, DSPIOSIZE, DSPIOSTEREO, MIXIOGETVOLUME, MIXIOSETVOLUME,
    VolumeLevel,
};

use crate::mixer::{Ac97, set_default_volume};
use crate::registers::{
    ADC, CMD_AC97_MODE, CMD_ADC_FCR_INIT, CMD_DAC_FCR_INIT, CMD_DCR_INIT, CMD_DCR_MASK,
    CMD_DMR_BIT8, CMD_DMR_BIT32, CMD_DMR_DMA, CMD_DMR_INIT, CMD_DMR_MONO, CMD_DMR_POLL,
    CMD_DMR_READ, CMD_DMR_SWAP, CMD_DMR_UNSIGN, CMD_DMR_WRITE, CMD_FCR_FEN, CMD_INTR_DMA,
    CMD_INTR_DMA0, CMD_INTR_DMA1, CMD_INTR_ENABLE, CMD_MASTER_SERIAL, CMD_PORT_TIMING,
    CMD_POWER_DOWN, DAC, INTR_STS_ADC, INTR_STS_DAC, REG_ADC_DCC, REG_ADC_DCR, REG_ADC_DMA_ADDR,
    REG_ADC_DMA_LEN, REG_ADC_DMR, REG_ADC_FCR, REG_ADC_FSIC, REG_ADC_HDSR, REG_ADC_SAMPLE_RATE,
    REG_CLK_CTRL, REG_CODEC_CTRL, REG_CODEC_OSV, REG_CONF_LOAD, REG_CONF_WRITE, REG_DAC_DCC,
    REG_DAC_DCR, REG_DAC_DMA_ADDR, REG_DAC_DMA_LEN, REG_DAC_DMR, REG_DAC_FCR, REG_DAC_FSIC,
    REG_DAC_HDSR, REG_DAC_SAMPLE_RATE, REG_INTR_CTRL, REG_INTR_MASK, REG_INTR_STS, REG_MASTER_CTRL,
    REG_PCM_LVOL, REG_PCM_RVOL, REG_POWER_EXT, REG_SOUND_POWER, REG_SPOWER_CTRL, rate_code,
};
use crate::window::RegisterWindow;

/// One channel's configuration (C's `aud_conf[]`: the fields `drv_init_hw`
/// seeds at `cs4281.c:486-498` and `drv_io_ctl` lets a user change).
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

/// The card face over one register window.
pub struct Cs4281<W: RegisterWindow> {
    codec: Ac97<W>,
    /// Per-sub-device geometry, indexed by `DAC`/`ADC`/`MIX` (C's global
    /// `sub_dev[]`).
    geometry: [Geometry; 3],
    /// Per-sub-device configuration, indexed by `DAC`/`ADC`/`MIX` (C's
    /// global `aud_conf[]`).
    conf: [ChannelConf; 3],
    /// The DMA mode word C keeps in a file static (`dmr_data`,
    /// `cs4281.c:126`), built by the format setter and used by the
    /// channel start, stop, pause, and resume.
    dmr_data: u32,
    /// The DMA control word (`dcr_data`).
    dcr_data: u32,
    /// The FIFO control word (`fcr_data`).
    fcr_data: u32,
    /// The last interrupt status `drv_int_sum` read (`dev.intr_status`).
    intr_status: u32,
    /// The card's IRQ line, as the PCI probe resolved it (`dev.irq`).
    irq: u8,
    /// How many extra buffers are filled (`BufLength`), for
    /// `DSPIOFREEBUF` and `DSPIOSAMPLESINBUF`.
    buf_length: u32,
}

impl<W: RegisterWindow> Cs4281<W> {
    /// A face over the register window, with the geometry `drv_init` sets
    /// (`cs4281.c:427-460`) and the configuration `drv_init_hw` seeds
    /// (`cs4281.c:486-498`).
    pub fn new(window: W) -> Self {
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
        };
        Cs4281 {
            codec: Ac97::new(window),
            geometry: [playback, playback, control_only],
            conf: [seeded, seeded, seeded],
            dmr_data: 0,
            dcr_data: 0,
            fcr_data: 0,
            intr_status: 0,
            irq: 0,
            buf_length: 0,
        }
    }

    /// The codec (tests and the binary's diagnostics).
    pub fn codec(&self) -> &Ac97<W> {
        &self.codec
    }

    /// The codec, mutably.
    pub fn codec_mut(&mut self) -> &mut Ac97<W> {
        &mut self.codec
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

    /// The DMA mode word (`dmr_data`).
    pub fn dmr_data(&self) -> u32 {
        self.dmr_data
    }

    /// Reset the card (`dev_reset`, `cs4281.c:96-116`): clear the
    /// power-down bit, load the configuration, pulse the sound power, and
    /// set the master control's initial value.
    pub fn reset(&mut self) -> i32 {
        let window = self.codec.window_mut();
        let data = window.read32(REG_POWER_EXT);
        if data & CMD_POWER_DOWN != 0 {
            window.write32(REG_POWER_EXT, data & !CMD_POWER_DOWN);
        }
        let data = window.read32(REG_CONF_LOAD);
        if data != 0x01 {
            window.write32(REG_CONF_LOAD, 0x01);
        }
        window.write32(REG_CONF_WRITE, 0x4281);
        window.write32(REG_SOUND_POWER, 0x3f);
        window.write32(REG_CLK_CTRL, 0);
        window.write32(REG_MASTER_CTRL, 0);
        window.write32(REG_CODEC_CTRL, 0);
        window.delay_micros(100);
        window.write32(REG_SPOWER_CTRL, 0);
        window.delay_micros(100);
        window.write32(REG_SPOWER_CTRL, 0x01);
        window.delay_micros(100);
        window.write32(REG_MASTER_CTRL, 0x0001_0003);
        0
    }

    /// Configure the card (`dev_configure`, `cs4281.c:119-135`).
    pub fn configure(&mut self) {
        let window = self.codec.window_mut();
        window.write32(
            REG_MASTER_CTRL,
            CMD_PORT_TIMING | CMD_AC97_MODE | CMD_MASTER_SERIAL,
        );
        window.write32(REG_CLK_CTRL, 0x10);
        window.delay_micros(50);
        window.write32(REG_CLK_CTRL, 0x30);
        window.delay_micros(500);
        window.write32(REG_CODEC_CTRL, 0x02);
        window.delay_micros(500);
        window.write32(REG_CODEC_CTRL, 0x06);
        window.delay_micros(500);
        window.write32(REG_CODEC_OSV, 0x03);
        window.write32(REG_PCM_LVOL, 0x07);
        window.write32(REG_PCM_RVOL, 0x07);
    }

    /// Set the sample rate (`dev_set_sample_rate`, `cs4281.c:137-147`):
    /// the rate's table index goes to both channels' rate registers.
    fn set_sample_rate(&mut self, sample_rate: u32) {
        let data = rate_code(sample_rate);
        let window = self.codec.window_mut();
        window.write32(REG_DAC_SAMPLE_RATE, data);
        window.write32(REG_ADC_SAMPLE_RATE, data);
    }

    /// Build the DMA mode word (`dev_set_format`, `cs4281.c:150-164`).
    ///
    /// C writes no registers here: it only composes the word the channel
    /// start will use. Eight-bit mono adds the byte-swap bit; thirty-two
    /// bits sets the width bit.
    fn set_format(&mut self, conf: ChannelConf) -> i32 {
        let mut dmr = CMD_DMR_INIT;
        if conf.stereo == 0 {
            dmr |= CMD_DMR_MONO;
        }
        if conf.sign == 0 {
            dmr |= CMD_DMR_UNSIGN;
        }
        if conf.nr_of_bits == 8 {
            dmr |= CMD_DMR_BIT8;
            if conf.stereo == 0 {
                dmr |= CMD_DMR_SWAP;
            }
        } else if conf.nr_of_bits == 32 {
            dmr |= CMD_DMR_BIT32;
        } else if conf.nr_of_bits != 16 {
            return -minix_types::EINVAL;
        }
        self.dmr_data = dmr;
        0
    }

    /// Start one channel (`dev_start_channel`, `cs4281.c:167-196`).
    fn start_channel(&mut self, sub_dev: i32) {
        self.dcr_data = CMD_DCR_INIT;
        if sub_dev == DAC {
            self.fcr_data = CMD_DAC_FCR_INIT;
        }
        if sub_dev == ADC {
            self.fcr_data = CMD_ADC_FCR_INIT;
        }
        self.dmr_data |= CMD_DMR_DMA;
        self.dcr_data &= !CMD_DCR_MASK;
        self.fcr_data |= CMD_FCR_FEN;
        let (fsic, dmr, fcr, dcr) = match sub_dev {
            DAC => (REG_DAC_FSIC, REG_DAC_DMR, REG_DAC_FCR, REG_DAC_DCR),
            ADC => (REG_ADC_FSIC, REG_ADC_DMR, REG_ADC_FCR, REG_ADC_DCR),
            _ => return,
        };
        if sub_dev == DAC {
            self.dmr_data |= CMD_DMR_WRITE;
        } else {
            self.dmr_data |= CMD_DMR_READ;
        }
        let window = self.codec.window_mut();
        window.write32(fsic, 0);
        window.write32(dmr, self.dmr_data & !CMD_DMR_DMA);
        window.write32(dmr, self.dmr_data);
        window.write32(fcr, self.fcr_data);
        window.write32(dcr, self.dcr_data);
    }

    /// Stop one channel (`dev_stop_channel`, `cs4281.c:199-217`).
    fn stop_channel(&mut self, sub_dev: i32) {
        self.dmr_data &= !(CMD_DMR_DMA | CMD_DMR_POLL);
        self.dcr_data |= !CMD_DCR_MASK;
        self.fcr_data &= !CMD_FCR_FEN;
        let (dmr, fcr, dcr) = match sub_dev {
            DAC => (REG_DAC_DMR, REG_DAC_FCR, REG_DAC_DCR),
            ADC => (REG_ADC_DMR, REG_ADC_FCR, REG_ADC_DCR),
            _ => return,
        };
        let window = self.codec.window_mut();
        window.write32(dmr, self.dmr_data);
        window.write32(fcr, self.fcr_data);
        window.write32(dcr, self.dcr_data);
    }

    /// Program one channel's DMA (`dev_set_dma`, `cs4281.c:220-232`):
    /// the address and the length minus one.
    fn program_dma(&mut self, dma: u32, len: u32, sub_dev: i32) -> i32 {
        let (address_reg, length_reg) = match sub_dev {
            DAC => (REG_DAC_DMA_ADDR, REG_DAC_DMA_LEN),
            ADC => (REG_ADC_DMA_ADDR, REG_ADC_DMA_LEN),
            _ => return -minix_types::EIO,
        };
        let window = self.codec.window_mut();
        window.write32(address_reg, dma);
        window.write32(length_reg, len - 1);
        0
    }

    /// The channel's running DMA address (`dev_read_dma_current`,
    /// `cs4281.c:235-245`): the playback side reads a long word, the
    /// capture side a word, and both keep the low sixteen bits.
    fn read_dma_current(&mut self, sub_dev: i32) -> u32 {
        let value = match sub_dev {
            DAC => self.codec.window_mut().read32(REG_DAC_DCC),
            ADC => self.codec.window_mut().read16(REG_ADC_DCC) as u32,
            _ => 0,
        };
        value & 0xffff
    }

    /// Pause or resume one channel (`dev_pause_dma`/`dev_resume_dma`,
    /// `cs4281.c:248-286`).
    fn set_paused(&mut self, sub_dev: i32, paused: bool) {
        if paused {
            self.dcr_data |= CMD_DCR_MASK;
            self.fcr_data |= CMD_FCR_FEN;
        } else {
            self.dcr_data &= !CMD_DCR_MASK;
            self.fcr_data &= !CMD_FCR_FEN;
        }
        let (dmr, fcr, dcr) = match sub_dev {
            DAC => (REG_DAC_DMR, REG_DAC_FCR, REG_DAC_DCR),
            ADC => (REG_ADC_DMR, REG_ADC_FCR, REG_ADC_DCR),
            _ => return,
        };
        let window = self.codec.window_mut();
        window.write32(dmr, self.dmr_data);
        window.write32(fcr, self.fcr_data);
        window.write32(dcr, self.dcr_data);
    }

    /// Read and clear the interrupt status (`dev_read_clear_intr_status`,
    /// `cs4281.c:289-297`): read the status, read both head-status
    /// registers, then re-arm the interrupt control register.
    fn read_clear_intr_status(&mut self) -> u32 {
        let window = self.codec.window_mut();
        let status = window.read32(REG_INTR_STS);
        window.read32(REG_DAC_HDSR);
        window.read32(REG_ADC_HDSR);
        window.write32(REG_INTR_CTRL, CMD_INTR_ENABLE);
        status
    }

    /// Enable or disable the interrupt (`dev_intr_enable`,
    /// `cs4281.c:300-314`).
    fn intr_enable(&mut self, enable: bool) {
        let window = self.codec.window_mut();
        if enable {
            window.write32(REG_INTR_CTRL, CMD_INTR_ENABLE);
            window.write32(
                REG_INTR_MASK,
                !(CMD_INTR_DMA | CMD_INTR_DMA0 | CMD_INTR_DMA1),
            );
        } else {
            window.write32(REG_INTR_CTRL, !CMD_INTR_ENABLE);
            window.write32(REG_INTR_MASK, 0x7fff_ffff);
        }
    }

    /// The card's ioctl set (`drv_io_ctl`, `cs4281.c:576-628`).
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
                // `set_frag_size` (`cs4281.c:401-409`).
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
                // `get_max_frag_size` (`cs4281.c:412-417`).
                let Some(geometry) = self.geometry.get(chan as usize).copied() else {
                    return -minix_types::EINVAL;
                };
                if geometry.fragments == 0 {
                    return -minix_types::EINVAL;
                }
                write_u32(val, len, geometry.dma_size / geometry.fragments);
                0
            }
            DSPIORESET => self.reset(),
            DSPIOFREEBUF => {
                // `free_buf` (`cs4281.c:420-427`).
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
                // `get_samples_in_buf` (`cs4281.c:430-436`).
                let current = self.read_dma_current(chan);
                write_u32(val, len, self.buf_length * 8192 + current);
                0
            }
            DSPIOPAUSE => self.pause(chan),
            DSPIORESUME => self.resume(chan),
            MIXIOGETVOLUME | MIXIOSETVOLUME => {
                // `get_set_volume` (`cs4281.c:619-625`).
                let Some(mut level) = VolumeLevel::decode(val) else {
                    return -minix_types::EINVAL;
                };
                let set = request == MIXIOSETVOLUME;
                let status = self.codec.get_set_volume(&mut level, set);
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

impl<W: RegisterWindow> AudioHooks for Cs4281<W> {
    fn init(&mut self) -> i32 {
        // C `drv_init` (`cs4281.c:427-460`): three sub-devices and three
        // special files; the table itself lives in the service.
        3
    }

    fn init_hw(&mut self) -> i32 {
        // C `drv_init_hw` (`cs4281.c:463-503`) minus the PCI probe and
        // the BAR mapping, which the binary performs: reset, configure,
        // initialize the mixer (one codec write of zero,
        // `cs4281.c:131-133`), then the default volume.
        self.reset();
        self.configure();
        self.codec.write(0, 0);
        let _ = set_default_volume(&mut self.codec);
        0
    }

    fn reset(&mut self) -> i32 {
        Cs4281::reset(self)
    }

    fn start(&mut self, sub_dev: i32, _dma_mode: i32) -> i32 {
        // C `drv_start` (`cs4281.c:511-540`): the rate, the format word
        // (with the fragment size divided down to frames,
        // `DMA_LENGTH_BY_FRAME`), the interrupt, then the channel.
        let Some(conf) = self.conf.get(sub_dev as usize).copied() else {
            return -minix_types::EINVAL;
        };
        self.set_sample_rate(conf.sample_rate);
        let status = self.set_format(conf);
        if status != 0 {
            return status;
        }
        let _ = self.reenable_int(sub_dev);
        self.start_channel(sub_dev);
        self.conf[sub_dev as usize].busy = true;
        0
    }

    fn stop(&mut self, sub_dev: i32) -> i32 {
        // C `drv_stop` (`cs4281.c:543-553`): interrupt off, then the
        // channel's stop sequence.
        self.intr_enable(false);
        self.stop_channel(sub_dev);
        if let Some(conf) = self.conf.get_mut(sub_dev as usize) {
            conf.busy = false;
        }
        0
    }

    fn set_dma(&mut self, dma: u32, length: u32, chan: i32) -> i32 {
        // C `drv_set_dma` (`cs4281.c:620-630`): the byte length becomes a
        // frame count before it reaches the register.
        let Some(conf) = self.conf.get(chan as usize).copied() else {
            return -minix_types::EINVAL;
        };
        let frame = conf.nr_of_bits * (conf.stereo + 1) / 8;
        let frames = length / frame;
        self.program_dma(dma, frames, chan)
    }

    fn reenable_int(&mut self, _chan: i32) -> i32 {
        self.intr_enable(true);
        0
    }

    fn int_sum(&mut self) -> i32 {
        // C `drv_int_sum` (`cs4281.c:634-644`): read-clear, remember the
        // status, report the two channel bits.
        let status = self.read_clear_intr_status();
        self.intr_status = status;
        (status & (INTR_STS_DAC | INTR_STS_ADC)) as i32
    }

    fn interrupt(&mut self, sub_dev: i32) -> i32 {
        // C `drv_int` (`cs4281.c:647-659`): the remembered status
        // filtered by the channel's bit.
        let mask = match sub_dev {
            DAC => INTR_STS_DAC,
            ADC => INTR_STS_ADC,
            _ => return 0,
        };
        (self.intr_status & mask) as i32
    }

    fn pause(&mut self, chan: i32) -> i32 {
        // C `drv_pause` (`cs4281.c:662-666`).
        self.set_paused(chan, true);
        0
    }

    fn resume(&mut self, chan: i32) -> i32 {
        // C `drv_resume` (`cs4281.c:669-673`).
        self.set_paused(chan, false);
        0
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

    /// A register window over a sparse file, with the codec answering
    /// immediately.
    struct FakeWindow {
        registers: BTreeMap<u32, u32>,
        writes: Vec<(u32, u32)>,
        delays: Vec<u32>,
        /// What the codec's serial data register answers (the power-down
        /// probe wants its low two bits set).
        sda: u32,
    }

    impl FakeWindow {
        fn new() -> Self {
            FakeWindow {
                registers: BTreeMap::new(),
                writes: Vec::new(),
                delays: Vec::new(),
                sda: 0x03,
            }
        }
        fn value(&self, offset: u32) -> u32 {
            self.registers.get(&offset).copied().unwrap_or(0)
        }
    }

    impl RegisterWindow for FakeWindow {
        fn read8(&mut self, offset: u32) -> u8 {
            self.value(offset) as u8
        }
        fn read16(&mut self, offset: u32) -> u16 {
            self.value(offset) as u16
        }
        fn read32(&mut self, offset: u32) -> u32 {
            if offset == crate::registers::REG_CODEC_STATUS {
                // The codec's answer is valid, so a read's second poll
                // ends at once.
                return crate::registers::STS_CODEC_VALID;
            }
            if offset == crate::registers::REG_CODEC_SDA {
                return self.sda;
            }
            if offset == crate::registers::REG_CODEC_CTRL {
                // The codec is idle: the done bit is clear.
                return 0;
            }
            self.value(offset)
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

    fn card() -> Cs4281<FakeWindow> {
        Cs4281::new(FakeWindow::new())
    }

    #[test]
    fn test_init_reports_three_sub_devices_and_init_hw_resets() {
        let mut card = card();
        assert_eq!(card.init(), 3);
        assert_eq!(card.init_hw(), 0);
        // The reset's three hundred-microsecond waits happened.
        assert_eq!(
            card.codec_mut()
                .window_mut()
                .delays
                .iter()
                .filter(|d| **d == 100)
                .count(),
            3
        );
        assert_eq!(card.codec_mut().window_mut().value(REG_CONF_WRITE), 0x4281);
        assert_eq!(card.codec_mut().window_mut().value(REG_SOUND_POWER), 0x3f);
        // The configure ended with the PCM volume pair.
        assert_eq!(card.codec_mut().window_mut().value(REG_PCM_LVOL), 0x07);
        assert_eq!(card.codec_mut().window_mut().value(REG_PCM_RVOL), 0x07);
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
    fn test_start_programs_the_rate_the_format_word_and_the_channel() {
        let mut card = card();
        assert_eq!(card.start(DAC, 2), 0);
        // The rate register carries the table index: 44100 is the second
        // entry.
        assert_eq!(card.codec_mut().window_mut().value(REG_DAC_SAMPLE_RATE), 1);
        assert_eq!(card.codec_mut().window_mut().value(REG_ADC_SAMPLE_RATE), 1);
        // The DMA mode word: init, stereo (no mono bit), signed (no
        // unsigned bit), sixteen bits, the write direction, and the run
        // bit.
        let dmr = card.dmr_data();
        assert_eq!(dmr & CMD_DMR_INIT, CMD_DMR_INIT);
        assert_eq!(dmr & CMD_DMR_MONO, 0);
        assert_eq!(dmr & CMD_DMR_UNSIGN, 0);
        assert_eq!(dmr & CMD_DMR_BIT8, 0);
        assert_eq!(dmr & CMD_DMR_BIT32, 0);
        assert_eq!(dmr & CMD_DMR_WRITE, CMD_DMR_WRITE);
        assert_eq!(dmr & CMD_DMR_DMA, CMD_DMR_DMA);
        // The channel registers were written: the FIFO slot count, the
        // mode word twice (with and without the run bit), the FIFO
        // control, and the DMA control.
        let writes = &card.codec_mut().window_mut().writes;
        let dmr_writes: Vec<u32> = writes
            .iter()
            .filter(|(offset, _)| *offset == REG_DAC_DMR)
            .map(|(_, value)| *value)
            .collect();
        assert_eq!(dmr_writes.len(), 2);
        assert_eq!(dmr_writes[0], dmr & !CMD_DMR_DMA);
        assert_eq!(dmr_writes[1], dmr);
        assert_eq!(
            card.codec_mut().window_mut().value(REG_DAC_FCR),
            CMD_DAC_FCR_INIT | CMD_FCR_FEN
        );
        assert_eq!(
            card.codec_mut().window_mut().value(REG_DAC_DCR),
            CMD_DCR_INIT & !CMD_DCR_MASK
        );
        assert!(card.conf(DAC).unwrap().busy);
        // The capture channel uses its own FIFO constant and the read
        // direction.
        assert_eq!(card.start(ADC, 2), 0);
        assert_eq!(card.dmr_data() & CMD_DMR_READ, CMD_DMR_READ);
        assert_eq!(
            card.codec_mut().window_mut().value(REG_ADC_FCR),
            CMD_ADC_FCR_INIT | CMD_FCR_FEN
        );
        // A stop clears the run bit and the busy flag.
        assert_eq!(card.stop(DAC), 0);
        assert!(!card.conf(DAC).unwrap().busy);
        assert_eq!(card.dmr_data() & CMD_DMR_DMA, 0);
        assert_eq!(card.dmr_data() & CMD_DMR_POLL, 0);
    }

    #[test]
    fn test_the_format_word_reflects_the_channel_mode() {
        let mut card = card();
        let mut len = 4;
        // Eight-bit mono adds both the width and the swap bits.
        assert_eq!(
            card.io_ctl_inner(DSPIOBITS, &mut 8u32.to_le_bytes(), &mut len, DAC),
            0
        );
        assert_eq!(
            card.io_ctl_inner(DSPIOSTEREO, &mut 0u32.to_le_bytes(), &mut len, DAC),
            0
        );
        assert_eq!(
            card.io_ctl_inner(DSPIOSIGN, &mut 0u32.to_le_bytes(), &mut len, DAC),
            0
        );
        assert_eq!(card.start(DAC, 2), 0);
        let dmr = card.dmr_data();
        assert_eq!(dmr & CMD_DMR_BIT8, CMD_DMR_BIT8);
        assert_eq!(dmr & CMD_DMR_SWAP, CMD_DMR_SWAP);
        assert_eq!(dmr & CMD_DMR_MONO, CMD_DMR_MONO);
        assert_eq!(dmr & CMD_DMR_UNSIGN, CMD_DMR_UNSIGN);
        // Thirty-two bits sets its own bit and no swap.
        assert_eq!(
            card.io_ctl_inner(DSPIOBITS, &mut 32u32.to_le_bytes(), &mut len, DAC),
            0
        );
        assert_eq!(
            card.io_ctl_inner(DSPIOSTEREO, &mut 1u32.to_le_bytes(), &mut len, DAC),
            0
        );
        assert_eq!(card.start(DAC, 2), 0);
        assert_eq!(card.dmr_data() & CMD_DMR_BIT32, CMD_DMR_BIT32);
        assert_eq!(card.dmr_data() & CMD_DMR_SWAP, 0);
        // A width no branch covers is refused.
        assert_eq!(
            card.io_ctl_inner(DSPIOBITS, &mut 12u32.to_le_bytes(), &mut len, DAC),
            0,
            "the ioctl stores anything"
        );
        assert_eq!(card.start(DAC, 2), -minix_types::EINVAL);
    }

    #[test]
    fn test_an_unknown_rate_programs_the_first_table_entry() {
        let mut card = card();
        let mut len = 4;
        assert_eq!(
            card.io_ctl_inner(DSPIORATE, &mut 12345u32.to_le_bytes(), &mut len, DAC),
            0
        );
        assert_eq!(card.start(DAC, 2), 0);
        assert_eq!(card.codec_mut().window_mut().value(REG_DAC_SAMPLE_RATE), 0);
    }

    #[test]
    fn test_set_dma_divides_the_length_into_frames() {
        let mut card = card();
        // 32768 bytes at sixteen bits stereo is 8192 frames.
        assert_eq!(card.set_dma(0x0010_0000, 32768, DAC), 0);
        assert_eq!(
            card.codec_mut().window_mut().value(REG_DAC_DMA_ADDR),
            0x0010_0000
        );
        assert_eq!(card.codec_mut().window_mut().value(REG_DAC_DMA_LEN), 8191);
        assert_eq!(card.set_dma(0, 4096, MIX), -minix_types::EIO);
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
        assert_eq!(
            card.io_ctl_inner(DSPIOSIZE, &mut 32769u32.to_le_bytes(), &mut len, DAC),
            -minix_types::EINVAL
        );
        let mut answer = [0u8; 4];
        let mut len = 0;
        assert_eq!(card.io_ctl_inner(DSPIOMAX, &mut answer, &mut len, DAC), 0);
        assert_eq!(u32::from_le_bytes(answer), 32768);
        // The sample counter reads the playback current-address register
        // as a long word and keeps the low sixteen bits.
        card.codec_mut()
            .window_mut()
            .registers
            .insert(REG_DAC_DCC, 0x1_0040);
        let mut answer = [0u8; 4];
        let mut len = 0;
        assert_eq!(
            card.io_ctl_inner(DSPIOSAMPLESINBUF, &mut answer, &mut len, DAC),
            0
        );
        assert_eq!(u32::from_le_bytes(answer), 0x40);
        // The capture side reads a word.
        card.codec_mut()
            .window_mut()
            .registers
            .insert(REG_ADC_DCC, 0x40);
        let mut answer = [0u8; 4];
        let mut len = 0;
        assert_eq!(
            card.io_ctl_inner(DSPIOSAMPLESINBUF, &mut answer, &mut len, ADC),
            0
        );
        assert_eq!(u32::from_le_bytes(answer), 0x40);
        assert_eq!(
            card.io_ctl_inner(0xdead_beef, &mut answer, &mut len, DAC),
            -minix_types::EINVAL
        );
    }

    #[test]
    fn test_the_mixer_ioctls_reach_the_ac97_codec() {
        let mut card = card();
        let level = VolumeLevel {
            device: minix_audiodriver::sound::Device::Cd,
            left: 0x0c,
            right: 0x0c,
        };
        let mut len = 0;
        assert_eq!(
            card.io_ctl_inner(MIXIOSETVOLUME, &mut level.encode(), &mut len, MIX),
            0
        );
        // The attenuation word for equal levels is (0x1f-0x0c)<<8 | 0x13.
        let data = card
            .codec_mut()
            .window_mut()
            .writes
            .iter()
            .rev()
            .find(|(offset, _)| *offset == crate::registers::REG_CODEC_DATA)
            .map(|(_, value)| *value)
            .unwrap();
        assert_eq!(data, (0x13 << 8) | 0x13);
        // A refused device never reaches the codec.
        let refused = VolumeLevel {
            device: minix_audiodriver::sound::Device::Speaker,
            left: 1,
            right: 1,
        };
        let mut len = 0;
        let mut refused_bytes = refused.encode();
        assert_eq!(
            card.io_ctl_inner(MIXIOSETVOLUME, &mut refused_bytes, &mut len, MIX),
            -minix_types::EINVAL
        );
    }

    #[test]
    fn test_the_interrupt_hooks_report_the_channel_bits() {
        let mut card = card();
        card.codec_mut()
            .window_mut()
            .registers
            .insert(REG_INTR_STS, INTR_STS_ADC);
        assert_eq!(card.int_sum(), INTR_STS_ADC as i32);
        assert_eq!(card.interrupt(ADC), INTR_STS_ADC as i32);
        assert_eq!(card.interrupt(DAC), 0);
        assert_eq!(card.interrupt(MIX), 0);
        // The read-clear re-armed the interrupt control register.
        assert_eq!(
            card.codec_mut().window_mut().value(REG_INTR_CTRL),
            CMD_INTR_ENABLE
        );
        // The disable path masks the three DMA interrupts.
        card.stop(DAC);
        assert_eq!(
            card.codec_mut().window_mut().value(REG_INTR_MASK),
            0x7fff_ffff
        );
    }

    #[test]
    fn test_pause_and_resume_flip_the_control_words() {
        let mut card = card();
        assert_eq!(card.pause(DAC), 0);
        assert_eq!(
            card.codec_mut().window_mut().value(REG_DAC_DCR) & CMD_DCR_MASK,
            CMD_DCR_MASK
        );
        assert_eq!(card.resume(DAC), 0);
        assert_eq!(
            card.codec_mut().window_mut().value(REG_DAC_DCR) & CMD_DCR_MASK,
            0
        );
    }
}
