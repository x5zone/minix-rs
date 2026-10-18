//! The Trident card face: the register sequences behind `AudioHooks`.
//!
//! C correspondence: `minix3/minix/drivers/audio/trident/trident.c` — the
//! channel register composer (`dev_write_chan_reg`, `:43-56`), the reset
//! (`dev_reset`, `:100-104`), the configuration (`dev_configure`,
//! `:107-110`), the sample-rate deltas (`:113-116`), the format writer
//! (`:119-130`), the channel start and stop (`:133-152`), the DMA
//! programming (`:155-168`), the current-address read (`:171-180`), pause
//! and resume (`:183-203`), the interrupt status and control (`:206-224`),
//! and the `drv_*` hooks that compose them (`:474-603`).
//!
//! The codec's register traffic lives in [`crate::mixer`]; the port seam
//! in [`crate::ports`]; the DMA buffers belong to the service (registered
//! `E-DMABUF`).

use minix_audiodriver::hooks::AudioHooks;
use minix_audiodriver::sound::{
    DSPIOBITS, DSPIOFREEBUF, DSPIOMAX, DSPIOPAUSE, DSPIORATE, DSPIORESET, DSPIORESUME,
    DSPIOSAMPLESINBUF, DSPIOSIGN, DSPIOSIZE, DSPIOSTEREO, MIXIOGETVOLUME, MIXIOSETVOLUME,
    VolumeLevel,
};

use crate::mixer::{Ac97, set_default_volume};
use crate::ports::PortIo;
use crate::registers::{
    ADC, CHAN_FMC, CHAN_VOLUME, CMD_FORMAT_BIT16, CMD_FORMAT_SIGN, CMD_FORMAT_STEREO,
    CODEC_CTRL_RESET, DAC, GCTRL_CONFIG, INTR_STS_ADC, INTR_STS_DAC, RATE_SCALE, REG_ADDR_INT_A,
    REG_CHAN_BASE, REG_CODEC_CTRL, REG_DMA0, REG_DMA4, REG_DMA11, REG_DMA15, REG_GCTRL,
    REG_INTR_CTRL_A, REG_INTR_STS, REG_SB_BASE, REG_SB_CTRL, REG_SB_DELTA, REG_START_A, REG_STOP_A,
    SB_CTRL_FORMAT,
};

/// One channel's configuration (C's `aud_conf[]`: the fields `drv_init_hw`
/// seeds at `trident.c:430-442` and `drv_io_ctl` lets a user change).
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

/// The channel's register image (C's file-static `my_chan`, a
/// `channel_info` at `trident.h:68-72`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChannelInfo {
    /// Current sample offset (`cso`).
    pub cso: u32,
    /// Alpha value (`alpha`).
    pub alpha: u32,
    /// FIFO mode start (`fms`).
    pub fms: u32,
    /// FIFO mode control (`fmc`).
    pub fmc: u32,
    /// End count (`ec`).
    pub ec: u32,
    /// The DMA address (`dma`).
    pub dma: u32,
    /// End sample offset (`eso`).
    pub eso: u32,
    /// The rate delta (`delta`).
    pub delta: u32,
    /// Half-buffer marker (`bufhalf`).
    pub bufhalf: u32,
    /// Index (`index`).
    pub index: u32,
    /// Right volume (`rvol`).
    pub rvol: u32,
    /// Current volume (`cvol`).
    pub cvol: u32,
    /// Global volume select (`gvsel`).
    pub gvsel: u32,
    /// Pan (`pan`).
    pub pan: u32,
    /// Volume (`vol`).
    pub vol: u32,
    /// Control (`ctrl`).
    pub ctrl: u32,
}

/// The card face over one port seam.
pub struct Trident<P: PortIo> {
    codec: Ac97<P>,
    /// Per-sub-device geometry, indexed by `DAC`/`ADC`/`MIX` (C's global
    /// `sub_dev[]`).
    geometry: [Geometry; 3],
    /// Per-sub-device configuration, indexed by `DAC`/`ADC`/`MIX` (C's
    /// global `aud_conf[]`).
    conf: [ChannelConf; 3],
    /// The channel's register image (C's `my_chan`).
    chan: ChannelInfo,
    /// The last interrupt status `drv_int_sum` read (`dev.intr_status`).
    intr_status: u32,
    /// The card's IRQ line, as the PCI probe resolved it (`dev.irq`).
    irq: u8,
    /// How many extra buffers are filled (`BufLength`), for
    /// `DSPIOFREEBUF` and `DSPIOSAMPLESINBUF`.
    buf_length: u32,
}

impl<P: PortIo> Trident<P> {
    /// A face over the port seam, with the geometry `drv_init` sets
    /// (`trident.c:371-404`) and the configuration `drv_init_hw` seeds
    /// (`trident.c:430-442`).
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
        };
        Trident {
            codec: Ac97::new(ports),
            geometry: [playback, playback, control_only],
            conf: [seeded, seeded, seeded],
            chan: ChannelInfo::default(),
            intr_status: 0,
            irq: 0,
            buf_length: 0,
        }
    }

    /// The codec (tests and the binary's diagnostics).
    pub fn codec(&self) -> &Ac97<P> {
        &self.codec
    }

    /// The codec, mutably.
    pub fn codec_mut(&mut self) -> &mut Ac97<P> {
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

    /// The channel's register image (`my_chan`).
    pub fn chan(&self) -> ChannelInfo {
        self.chan
    }

    /// Compose and write the five channel registers (`dev_write_chan_reg`,
    /// `trident.c:43-56`).
    fn write_chan_reg(&mut self) {
        let chan = self.chan;
        let reg = [
            (chan.cso << 16) | (chan.alpha << 4) | chan.fms,
            chan.dma,
            (chan.eso << 16) | chan.delta,
            (chan.fmc << 14) | (chan.rvol << 7) | chan.cvol,
            (chan.gvsel << 31) | (chan.pan << 24) | (chan.vol << 16) | (chan.ctrl << 12) | chan.ec,
        ];
        for (index, value) in reg.iter().enumerate() {
            let _ = self
                .codec
                .ports_mut()
                .out32(REG_CHAN_BASE + (index as u16) * 4, *value);
        }
    }

    /// Reset the card (`dev_reset`, `trident.c:100-104`): one codec
    /// control write.
    pub fn reset(&mut self) -> i32 {
        match self
            .codec
            .ports_mut()
            .out32(REG_CODEC_CTRL, CODEC_CTRL_RESET)
        {
            Ok(()) => 0,
            Err(status) => status,
        }
    }

    /// Configure the card (`dev_configure`, `trident.c:107-110`).
    pub fn configure(&mut self) -> i32 {
        match self.codec.ports_mut().out32(REG_GCTRL, GCTRL_CONFIG) {
            Ok(()) => 0,
            Err(status) => status,
        }
    }

    /// Set the sample rate (`dev_set_sample_rate`, `trident.c:113-116`):
    /// the channel's delta and the Sound Blaster delta register.
    ///
    /// C divides by the rate with no guard, so a zero rate faults the
    /// driver process; the rewrite refuses it instead.
    fn set_sample_rate(&mut self, sample_rate: u32) -> i32 {
        if sample_rate == 0 {
            return -minix_types::EINVAL;
        }
        self.chan.delta = (sample_rate << 12) / RATE_SCALE;
        match self
            .codec
            .ports_mut()
            .out16(REG_SB_DELTA, ((RATE_SCALE << 12) / sample_rate) as u16)
        {
            Ok(()) => 0,
            Err(status) => status,
        }
    }

    /// Write the format (`dev_set_format`, `trident.c:119-130`): the
    /// channel control byte, the Sound Blaster base pair, and the Sound
    /// Blaster control byte.
    fn set_format(&mut self, conf: ChannelConf) -> i32 {
        let mut data = 0u32;
        if conf.nr_of_bits == 16 {
            data |= CMD_FORMAT_BIT16;
        }
        if conf.sign == 1 {
            data |= CMD_FORMAT_SIGN;
        }
        if conf.stereo == 1 {
            data |= CMD_FORMAT_STEREO;
        }
        self.chan.ctrl = data;
        let mut count = self.chan.eso + 1;
        if conf.nr_of_bits == 16 {
            count >>= 1;
        }
        count -= 1;
        let ports = self.codec.ports_mut();
        if ports.out32(REG_SB_BASE, count | (count << 16)).is_err() {
            return -minix_types::EIO;
        }
        match ports.out8(REG_SB_CTRL, ((self.chan.ctrl << 4) | SB_CTRL_FORMAT) as u8) {
            Ok(()) => 0,
            Err(status) => status,
        }
    }

    /// Start one channel (`dev_start_channel`, `trident.c:133-148`).
    fn start_channel(&mut self, sub_dev: i32) {
        match sub_dev {
            DAC => {
                self.chan.fmc = CHAN_FMC;
                self.chan.fms = 0;
                self.chan.ec = 0;
                self.chan.alpha = 0;
                self.chan.cso = 0;
                self.chan.rvol = CHAN_VOLUME;
                self.chan.cvol = CHAN_VOLUME;
                self.chan.gvsel = 0;
                self.chan.pan = 0;
                self.chan.vol = 0;
                self.chan.ctrl |= 0x01;
                self.write_chan_reg();
                let data = self.codec.ports_mut().in32(REG_START_A).unwrap_or(0);
                let _ = self.codec.ports_mut().out32(REG_START_A, data | 0x01);
            }
            ADC => {
                let data = self.codec.ports_mut().in8(REG_SB_CTRL).unwrap_or(0);
                let _ = self.codec.ports_mut().out8(REG_SB_CTRL, data | 0x01);
            }
            _ => {}
        }
    }

    /// Stop one channel (`dev_stop_channel`, `trident.c:151-156`).
    fn stop_channel(&mut self, sub_dev: i32) {
        match sub_dev {
            DAC => {
                let _ = self.codec.ports_mut().out32(REG_STOP_A, 1);
            }
            ADC => {
                let _ = self.codec.ports_mut().out8(REG_SB_CTRL, 0);
            }
            _ => {}
        }
    }

    /// Program one channel's DMA (`dev_set_dma`, `trident.c:159-168`):
    /// the channel image takes the address and the end offset; the
    /// capture side also gets its own DMA registers.
    fn program_dma(&mut self, dma: u32, len: u32, sub_dev: i32) -> i32 {
        self.chan.dma = dma;
        self.chan.eso = len - 1;
        if sub_dev != ADC {
            return 0;
        }
        let ports = self.codec.ports_mut();
        if ports.out8(REG_DMA15, 0).is_err() {
            return -minix_types::EIO;
        }
        if ports.out8(REG_DMA11, 0x54).is_err() {
            return -minix_types::EIO;
        }
        if ports.out32(REG_DMA0, dma).is_err() {
            return -minix_types::EIO;
        }
        let data = ports.in32(REG_DMA4).unwrap_or(0) & 0xff00_0000;
        match ports.out16(REG_DMA4, ((len - 1) | data) as u16) {
            Ok(()) => 0,
            Err(status) => status,
        }
    }

    /// The channel's running DMA address (`dev_read_dma_current`,
    /// `trident.c:171-180`).
    ///
    /// C reads the channel's sample counter (playback) or the Sound
    /// Blaster base (capture), masks the low sixteen bits — and then
    /// returns a hard **zero** (`trident.c:179`). The read has no
    /// observable effect on the answer, so the rewrite keeps the reads
    /// (they touch the hardware, as C's do) and returns zero, and the
    /// defect is recorded in document `21-audio-drivers.md`.
    fn read_dma_current(&mut self, sub_dev: i32) -> u32 {
        let _ = self.codec.ports_mut().out8(REG_GCTRL, 0);
        let _data = match sub_dev {
            DAC => self
                .codec
                .ports_mut()
                .in16(REG_CHAN_BASE + 0x02)
                .unwrap_or(0) as u32,
            ADC => self.codec.ports_mut().in16(REG_SB_BASE).unwrap_or(0) as u32,
            _ => 0,
        };
        0
    }

    /// Pause or resume one channel (`dev_pause_dma`/`dev_resume_dma`,
    /// `trident.c:183-203`).
    fn set_paused(&mut self, sub_dev: i32, paused: bool) {
        match sub_dev {
            DAC => {
                if paused {
                    let _ = self.codec.ports_mut().out32(REG_STOP_A, 1);
                } else {
                    let data = self.codec.ports_mut().in32(REG_START_A).unwrap_or(0);
                    let _ = self.codec.ports_mut().out32(REG_START_A, data | 0x01);
                }
            }
            ADC => {
                let data = self.codec.ports_mut().in8(REG_SB_CTRL).unwrap_or(0);
                let value = if paused { data & 0xf8 } else { data | 0x01 };
                let _ = self.codec.ports_mut().out8(REG_SB_CTRL, value);
            }
            _ => {}
        }
    }

    /// Read and clear the interrupt status (`dev_read_clear_intr_status`,
    /// `trident.c:206-214`): the status, then two acknowledgement reads
    /// when the capture channel raised the interrupt.
    fn read_clear_intr_status(&mut self) -> u32 {
        let status = self.codec.ports_mut().in32(REG_INTR_STS).unwrap_or(0);
        if status & INTR_STS_ADC != 0 {
            let _ = self.codec.ports_mut().in8(0x1e);
            let _ = self.codec.ports_mut().in8(0x1f);
        }
        status
    }

    /// Enable or disable the interrupt (`dev_intr_enable`,
    /// `trident.c:217-224`).
    fn intr_enable(&mut self, enable: bool) {
        let mut data = self.codec.ports_mut().in32(REG_INTR_CTRL_A).unwrap_or(0);
        data &= 0xfffe;
        if enable {
            data |= 0x01;
        }
        let _ = self.codec.ports_mut().out32(REG_ADDR_INT_A, 1);
        let _ = self.codec.ports_mut().out32(REG_INTR_CTRL_A, data);
    }

    /// The card's ioctl set (`drv_io_ctl`, `trident.c:470-537`).
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
                // `set_frag_size` (`trident.c:345-353`).
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
                // `get_max_frag_size` (`trident.c:356-361`).
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
                // `free_buf` (`trident.c:364-371`).
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
                // `get_samples_in_buf` (`trident.c:374-380`): the current
                // address hook answers zero (see `read_dma_current`).
                let current = self.read_dma_current(chan);
                write_u32(val, len, self.buf_length * 8192 + current);
                0
            }
            DSPIOPAUSE => self.pause(chan),
            DSPIORESUME => self.resume(chan),
            MIXIOGETVOLUME | MIXIOSETVOLUME => {
                // `get_set_volume` (`trident.c:529-534`).
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

impl<P: PortIo> AudioHooks for Trident<P> {
    fn init(&mut self) -> i32 {
        // C `drv_init` (`trident.c:371-404`): three sub-devices and three
        // special files; the table itself lives in the service.
        3
    }

    fn init_hw(&mut self) -> i32 {
        // C `drv_init_hw` (`trident.c:407-447`) minus the PCI probe, which
        // the binary performs: reset, configure, initialize the mixer (one
        // codec write of zero), then the default volume.
        if self.reset() != 0 {
            return -minix_types::EIO;
        }
        if self.configure() != 0 {
            return -minix_types::EIO;
        }
        self.codec.write(0, 0);
        let _ = set_default_volume(&mut self.codec);
        0
    }

    fn reset(&mut self) -> i32 {
        Trident::reset(self)
    }

    fn start(&mut self, sub_dev: i32, _dma_mode: i32) -> i32 {
        // C `drv_start` (`trident.c:455-481`): the rate deltas, the format
        // (with the fragment size divided down to frames,
        // `DMA_LENGTH_BY_FRAME`), the interrupt, then the channel.
        let Some(conf) = self.conf.get(sub_dev as usize).copied() else {
            return -minix_types::EINVAL;
        };
        if self.set_sample_rate(conf.sample_rate) != 0 {
            return -minix_types::EIO;
        }
        if self.set_format(conf) != 0 {
            return -minix_types::EIO;
        }
        let _ = self.reenable_int(sub_dev);
        self.start_channel(sub_dev);
        self.conf[sub_dev as usize].busy = true;
        0
    }

    fn stop(&mut self, sub_dev: i32) -> i32 {
        // C `drv_stop` (`trident.c:484-494`): interrupt off, then the
        // channel's stop sequence.
        self.intr_enable(false);
        self.stop_channel(sub_dev);
        if let Some(conf) = self.conf.get_mut(sub_dev as usize) {
            conf.busy = false;
        }
        0
    }

    fn set_dma(&mut self, dma: u32, length: u32, chan: i32) -> i32 {
        // C `drv_set_dma` (`trident.c:551-558`): the byte length becomes a
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
        // C `drv_int_sum` (`trident.c:562-572`): read-clear, remember the
        // status, report the two channel bits.
        let status = self.read_clear_intr_status();
        self.intr_status = status;
        (status & (INTR_STS_DAC | INTR_STS_ADC)) as i32
    }

    fn interrupt(&mut self, sub_dev: i32) -> i32 {
        // C `drv_int` (`trident.c:575-587`): the remembered status
        // filtered by the channel's bit.
        let mask = match sub_dev {
            DAC => INTR_STS_DAC,
            ADC => INTR_STS_ADC,
            _ => return 0,
        };
        (self.intr_status & mask) as i32
    }

    fn pause(&mut self, chan: i32) -> i32 {
        // C `drv_pause` (`trident.c:590-594`).
        self.set_paused(chan, true);
        0
    }

    fn resume(&mut self, chan: i32) -> i32 {
        // C `drv_resume` (`trident.c:597-601`).
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
