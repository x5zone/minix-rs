//! The ES1370 card face: the register sequences behind `AudioHooks`.
//!
//! C correspondence: `minix3/minix/drivers/audio/es1370/es1370.c` — the
//! initialization sequence (`drv_init_hw`, `:119-168`), the reset
//! (`reset`/`drv_reset`, `:210-221`), start and stop (`:224-273`), the
//! ioctl set (`drv_io_ctl`, `:276-318`), the fragment size report
//! (`drv_get_frag_size`, `:326-329`), the DMA programming (`drv_set_dma`,
//! `:332-365`), the interrupt summary and per-channel interrupt
//! (`:368-393`), interrupt enable (`drv_reenable_int`, `:396-412`), pause
//! and resume (`:415-447`), the format setters (`:450-511`), the sample
//! rate (`:531-547`), the interrupt count (`set_int_cnt`, `:549-575`),
//! and the small reporting helpers (`:577-654`).
//!
//! The codec's register traffic lives in [`crate::codec`]; the port seam
//! in [`crate::ports`]; the DMA buffers belong to the service (registered
//! `E-DMABUF`).

use minix_audiodriver::hooks::AudioHooks;
use minix_audiodriver::sound::{
    DSPIOBITS, DSPIOFREEBUF, DSPIOMAX, DSPIOPAUSE, DSPIORATE, DSPIORESET, DSPIORESUME,
    DSPIOSAMPLESINBUF, DSPIOSIGN, DSPIOSIZE, DSPIOSTEREO, MIXIOGETVOLUME, MIXIOSETVOLUME,
    VolumeLevel,
};

use crate::codec::Ak4531;
use crate::ports::PortIo;
use crate::registers::{
    ADC, ADC_BUFFER_SIZE, ADC_CURR_SAMP_CT, ADC_MEM_PAGE, ADC_PCI_ADDRESS, ADC_SAMP_CT, ADC1_CHAN,
    ADC1_EN, CDC_EN, CHIP_SEL_CTRL, DAC_MEM_PAGE, DAC1, DAC1_BUFFER_SIZE, DAC1_CHAN,
    DAC1_CURR_SAMP_CT, DAC1_EN, DAC1_PCI_ADDRESS, DAC1_SAMP_CT, DAC2, DAC2_BUFFER_SIZE, DAC2_CHAN,
    DAC2_CURR_SAMP_CT, DAC2_EN, DAC2_PCI_ADDRESS, DAC2_SAMP_CT, DEFAULT_NR_OF_BITS, DEFAULT_RATE,
    DEFAULT_SIGNED, DEFAULT_STEREO, FREQ_44K100, INTERRUPT_STATUS, INTR, LEGACY, MAX_RATE,
    MEM_PAGE, MEMORY, MIN_RATE, MIXER, P1_INTR_EN, P1_PAUSE, P1_S_EB, P1_S_MB, P2_INTR_EN,
    P2_PAUSE, P2_S_EB, P2_S_MB, R1_INT_EN, R1_S_EB, R1_S_MB, SERIAL_INTERFACE_CTRL, XCTL0,
};

/// One channel's configuration (C's `aud_conf[]`: the fields `drv_init_hw`
/// seeds at `es1370.c:154-165` and `drv_io_ctl` lets a user change).
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
pub struct Es1370<P: PortIo> {
    /// The card's registers plus the codec's shadow table.
    codec: Ak4531<P>,
    /// Per-sub-device geometry, indexed by the four channel numbers
    /// (C's global `sub_dev[]`).
    geometry: [Geometry; 4],
    /// Per-sub-device configuration, indexed by the four channel numbers
    /// (C's global `aud_conf[]`).
    conf: [ChannelConf; 4],
    /// The card's IRQ line, as the PCI probe resolved it (`dev.irq`).
    irq: u8,
    /// How many extra buffers are filled (`BufLength`), for
    /// `DSPIOFREEBUF` and `DSPIOSAMPLESINBUF`.
    buf_length: u32,
}

impl<P: PortIo> Es1370<P> {
    /// A face over the port seam, with the geometry `drv_init` sets
    /// (`es1370.c:60-97`) and the configuration `drv_init_hw` seeds
    /// (`es1370.c:154-165`).
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
            stereo: DEFAULT_STEREO,
            sample_rate: DEFAULT_RATE,
            nr_of_bits: DEFAULT_NR_OF_BITS,
            sign: DEFAULT_SIGNED,
            fragment_size: 64 * 1024 / 2,
        };
        Es1370 {
            codec: Ak4531::new(ports),
            geometry: [playback, playback, control_only, playback],
            conf: [seeded, seeded, seeded, seeded],
            irq: 0,
            buf_length: 0,
        }
    }

    /// The codec (tests and the binary's diagnostics).
    pub fn codec(&self) -> &Ak4531<P> {
        &self.codec
    }

    /// The codec, mutably.
    pub fn codec_mut(&mut self) -> &mut Ak4531<P> {
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

    /// The channel's format bits and the sample-count register
    /// (`set_stereo`/`set_bits`/`set_int_cnt`'s switches, `es1370.c:450-575`).
    fn channel_bits(chan: i32) -> Option<ChannelBits> {
        let bits = match chan {
            ADC1_CHAN => ChannelBits {
                stereo_bit: R1_S_MB,
                sixteen_bit: R1_S_EB,
                int_enable: R1_INT_EN,
                sample_count: ADC_SAMP_CT,
                current_count: ADC_CURR_SAMP_CT,
                pause: None,
                enable: ADC1_EN,
                interrupt: ADC,
                mem_page: ADC_MEM_PAGE,
                frame_count: ADC_BUFFER_SIZE,
                dma_address: ADC_PCI_ADDRESS,
            },
            DAC1_CHAN => ChannelBits {
                stereo_bit: P1_S_MB,
                sixteen_bit: P1_S_EB,
                int_enable: P1_INTR_EN,
                sample_count: DAC1_SAMP_CT,
                current_count: DAC1_CURR_SAMP_CT,
                pause: Some(P1_PAUSE),
                enable: DAC1_EN,
                interrupt: DAC1,
                mem_page: DAC_MEM_PAGE,
                frame_count: DAC1_BUFFER_SIZE,
                dma_address: DAC1_PCI_ADDRESS,
            },
            DAC2_CHAN => ChannelBits {
                stereo_bit: P2_S_MB,
                sixteen_bit: P2_S_EB,
                int_enable: P2_INTR_EN,
                sample_count: DAC2_SAMP_CT,
                current_count: DAC2_CURR_SAMP_CT,
                pause: Some(P2_PAUSE),
                enable: DAC2_EN,
                interrupt: DAC2,
                mem_page: DAC_MEM_PAGE,
                frame_count: DAC2_BUFFER_SIZE,
                dma_address: DAC2_PCI_ADDRESS,
            },
            _ => return None,
        };
        Some(bits)
    }

    /// Set the sample width (`set_bits`, `es1370.c:470-492`): sixteen or
    /// eight bits only.
    fn set_bits(&mut self, nr_of_bits: u32, chan: i32) -> i32 {
        let Some(bits) = Self::channel_bits(chan) else {
            return -minix_types::EINVAL;
        };
        if nr_of_bits != 16 && nr_of_bits != 8 {
            return -minix_types::EINVAL;
        }
        let Ok(ser) = self.codec.ports_mut().in16(SERIAL_INTERFACE_CTRL) else {
            return -minix_types::EIO;
        };
        let mut ser = ser & !bits.sixteen_bit;
        if nr_of_bits == 16 {
            ser |= bits.sixteen_bit;
        }
        if self
            .codec
            .ports_mut()
            .out16(SERIAL_INTERFACE_CTRL, ser)
            .is_err()
        {
            return -minix_types::EIO;
        }
        self.conf[chan as usize].nr_of_bits = nr_of_bits;
        0
    }

    /// Set the channel mode (`set_stereo`, `es1370.c:494-511`): any
    /// nonzero value is stereo.
    fn set_stereo(&mut self, stereo: u32, chan: i32) -> i32 {
        let Some(bits) = Self::channel_bits(chan) else {
            return -minix_types::EINVAL;
        };
        let Ok(ser) = self.codec.ports_mut().in16(SERIAL_INTERFACE_CTRL) else {
            return -minix_types::EIO;
        };
        let mut ser = ser & !bits.stereo_bit;
        if stereo != 0 {
            ser |= bits.stereo_bit;
        }
        if self
            .codec
            .ports_mut()
            .out16(SERIAL_INTERFACE_CTRL, ser)
            .is_err()
        {
            return -minix_types::EIO;
        }
        self.conf[chan as usize].stereo = stereo;
        0
    }

    /// Set the sample rate (`set_sample_rate`, `es1370.c:531-547`): the
    /// range gate, then the fixed 44.1 kHz clock select.
    fn set_sample_rate(&mut self, rate: u32, chan: i32) -> i32 {
        if !(MIN_RATE..=MAX_RATE).contains(&rate) {
            return -minix_types::EINVAL;
        }
        let Ok(control) = self.codec.ports_mut().in32(CHIP_SEL_CTRL) else {
            return -minix_types::EIO;
        };
        if self
            .codec
            .ports_mut()
            .out32(CHIP_SEL_CTRL, control | FREQ_44K100 as u32)
            .is_err()
        {
            return -minix_types::EIO;
        }
        self.conf[chan as usize].sample_rate = rate;
        0
    }

    /// Write the channel's interrupt count (`set_int_cnt`,
    /// `es1370.c:549-575`): the fragment size divided down by the channel
    /// mode, minus one.
    fn set_int_cnt(&mut self, chan: i32) -> i32 {
        let Some(bits) = Self::channel_bits(chan) else {
            return -minix_types::EINVAL;
        };
        let Some(geometry) = self.geometry.get(chan as usize).copied() else {
            return -minix_types::EINVAL;
        };
        if geometry.fragments == 0 {
            return -minix_types::EINVAL;
        }
        let conf = self.conf[chan as usize];
        let max = geometry.dma_size / geometry.fragments;
        if conf.fragment_size > max || conf.fragment_size < geometry.min_fragment_size {
            return -minix_types::EINVAL;
        }
        if conf.nr_of_bits != 16 && conf.nr_of_bits != 8 {
            return -minix_types::EINVAL;
        }
        let mut sample_count = conf.fragment_size;
        if conf.stereo != 0 {
            sample_count >>= 1;
        }
        if conf.nr_of_bits == 16 {
            sample_count >>= 1;
        }
        if self
            .codec
            .ports_mut()
            .out16(bits.sample_count, (sample_count - 1) as u16)
            .is_err()
        {
            return -minix_types::EIO;
        }
        0
    }

    /// Clear one channel's interrupt enable (`disable_int`,
    /// `es1370.c:583-598`).
    fn disable_int(&mut self, chan: i32) -> i32 {
        let Some(bits) = Self::channel_bits(chan) else {
            return -minix_types::EINVAL;
        };
        let Ok(ser) = self.codec.ports_mut().in16(SERIAL_INTERFACE_CTRL) else {
            return -minix_types::EIO;
        };
        match self
            .codec
            .ports_mut()
            .out16(SERIAL_INTERFACE_CTRL, ser & !bits.int_enable)
        {
            Ok(()) => 0,
            Err(status) => status,
        }
    }

    /// The card's ioctl set (`drv_io_ctl`, `es1370.c:276-318`).
    fn io_ctl_inner(&mut self, request: u64, val: &mut [u8], len: &mut i32, chan: i32) -> i32 {
        let read_u32 = |val: &[u8]| u32::from_le_bytes(val[..4].try_into().unwrap());
        fn write_u32(val: &mut [u8], len: &mut i32, value: u32) {
            val[..4].copy_from_slice(&value.to_le_bytes());
            *len = 4;
        }
        match request {
            DSPIORATE => self.set_sample_rate(read_u32(val), chan),
            DSPIOSTEREO => self.set_stereo(read_u32(val), chan),
            DSPIOBITS => self.set_bits(read_u32(val), chan),
            DSPIOSIGN => {
                // `set_sign` accepts anything and stores nothing
                // (`es1370.c:513-516`).
                0
            }
            DSPIOSIZE => {
                // `set_frag_size` (`es1370.c:518-529`).
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
                // `get_max_frag_size` (`es1370.c:577-582`).
                let Some(geometry) = self.geometry.get(chan as usize).copied() else {
                    return -minix_types::EINVAL;
                };
                if geometry.fragments == 0 {
                    return -minix_types::EINVAL;
                }
                write_u32(val, len, geometry.dma_size / geometry.fragments);
                0
            }
            DSPIORESET => {
                // `reset` (`es1370.c:210-215`): stop the channel and mark
                // it out of data.
                let status = self.stop(chan);
                if status != 0 {
                    return status;
                }
                self.buf_length = 0;
                0
            }
            DSPIOFREEBUF => {
                // `free_buf` (`es1370.c:637-648`).
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
                // `get_samples_in_buf` (`es1370.c:600-635`).
                let Some(bits) = Self::channel_bits(chan) else {
                    return -minix_types::EINVAL;
                };
                // C reads the sample-count register and discards the
                // value (`es1370.c:626-627`).
                if self.codec.ports_mut().in16(bits.sample_count).is_err() {
                    return -minix_types::EIO;
                }
                let Ok(current) = self.codec.ports_mut().in16(bits.current_count) else {
                    return -minix_types::EIO;
                };
                write_u32(val, len, self.buf_length * 8192 + current as u32);
                0
            }
            DSPIOPAUSE => self.pause(chan),
            DSPIORESUME => self.resume(chan),
            MIXIOGETVOLUME | MIXIOSETVOLUME => {
                // `get_set_volume` (`es1370.c:650-659`): only the mixer
                // sub-device has a codec to talk to, and the payload
                // length is reported before the gate.
                *len = VolumeLevel::SIZE as i32;
                if chan != MIXER {
                    return -minix_types::EINVAL;
                }
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

/// One channel's register set (`es1370.c`'s per-channel switches).
struct ChannelBits {
    /// The stereo bit (`P1_S_MB` and friends).
    stereo_bit: u16,
    /// The sixteen-bit sample bit (`P1_S_EB` and friends).
    sixteen_bit: u16,
    /// The interrupt enable bit (`P1_INTR_EN` and friends).
    int_enable: u16,
    /// The interrupt count register (`DAC1_SAMP_CT` and friends).
    sample_count: u16,
    /// The current-count register (`DAC1_CURR_SAMP_CT` and friends).
    current_count: u16,
    /// The pause bit, for the playback channels.
    pause: Option<u16>,
    /// The channel enable bit in the chip-select control register.
    enable: u16,
    /// The channel's interrupt bit in the status register.
    interrupt: u32,
    /// The memory page the channel's buffer lives on.
    mem_page: u8,
    /// The frame-count register (`DAC1_BUFFER_SIZE` and friends).
    frame_count: u16,
    /// The DMA address register (`DAC1_PCI_ADDRESS` and friends).
    dma_address: u16,
}

impl<P: PortIo> AudioHooks for Es1370<P> {
    fn init(&mut self) -> i32 {
        // C `drv_init` (`es1370.c:56-108`): four sub-devices and four
        // special files; the table itself lives in the service.
        4
    }

    fn init_hw(&mut self) -> i32 {
        // C `drv_init_hw` (`es1370.c:111-168`) minus the PCI probe, which
        // the binary performs (the base port is a constructor argument
        // here): turn everything off, enable the codec, initialize it,
        // clear the card's memory.
        let ports = self.codec.ports_mut();
        if ports.out32(CHIP_SEL_CTRL, 0).is_err() {
            return -minix_types::EIO;
        }
        if ports.out32(LEGACY, 0).is_err() || ports.out32(LEGACY + 4, 0).is_err() {
            return -minix_types::EIO;
        }
        if ports.out32(SERIAL_INTERFACE_CTRL, 0).is_err() {
            return -minix_types::EIO;
        }
        let Ok(control) = ports.in16(CHIP_SEL_CTRL) else {
            return -minix_types::EIO;
        };
        if ports
            .out16(CHIP_SEL_CTRL, control | XCTL0 | CDC_EN)
            .is_err()
        {
            return -minix_types::EIO;
        }
        let status = self.codec.init();
        if status != 0 {
            return -minix_types::EINVAL;
        }
        // The memory clear (`es1370.c:145-150`): sixteen pages, each
        // wiped four longs at a time.
        for page in 0..0x10u8 {
            if self.codec.ports_mut().out8(MEM_PAGE, page).is_err() {
                return -minix_types::EIO;
            }
            for offset in (0..0x10u16).step_by(4) {
                if self.codec.ports_mut().out32(MEMORY + offset, 0).is_err() {
                    return -minix_types::EIO;
                }
            }
        }
        0
    }

    fn reset(&mut self) -> i32 {
        // C `drv_reset` (`es1370.c:217-219`) does nothing and reports
        // success.
        0
    }

    fn start(&mut self, sub_dev: i32, _dma_mode: i32) -> i32 {
        // C `drv_start` (`es1370.c:224-257`): the four format setters and
        // the interrupt count, then resume, then the channel's enable
        // bit. Every setter is called even when the user configured the
        // channel already ("everything is written twice", C's own note).
        let conf = match self.conf.get(sub_dev as usize).copied() {
            Some(conf) => conf,
            None => return -minix_types::EINVAL,
        };
        let mut result = 0;
        result |= self.set_sample_rate(conf.sample_rate, sub_dev);
        result |= self.set_stereo(conf.stereo, sub_dev);
        result |= self.set_bits(conf.nr_of_bits, sub_dev);
        result |= self.set_int_cnt(sub_dev);
        if result != 0 {
            return -minix_types::EIO;
        }
        // A paused channel is resumed before it is enabled.
        let _ = self.resume(sub_dev);
        let Some(bits) = Self::channel_bits(sub_dev) else {
            return -minix_types::EINVAL;
        };
        let _ = self.reenable_int(sub_dev);
        let Ok(control) = self.codec.ports_mut().in16(CHIP_SEL_CTRL) else {
            return -minix_types::EIO;
        };
        if self
            .codec
            .ports_mut()
            .out16(CHIP_SEL_CTRL, control | bits.enable)
            .is_err()
        {
            return -minix_types::EIO;
        }
        self.conf[sub_dev as usize].busy = true;
        0
    }

    fn stop(&mut self, sub_dev: i32) -> i32 {
        // C `drv_stop` (`es1370.c:260-273`): clear the enable bit, clear
        // the busy flag, then disable the channel's interrupt.
        let Some(bits) = Self::channel_bits(sub_dev) else {
            return -minix_types::EINVAL;
        };
        let Ok(control) = self.codec.ports_mut().in16(CHIP_SEL_CTRL) else {
            return -minix_types::EIO;
        };
        if self
            .codec
            .ports_mut()
            .out16(CHIP_SEL_CTRL, control & !bits.enable)
            .is_err()
        {
            return -minix_types::EIO;
        }
        self.conf[sub_dev as usize].busy = false;
        self.disable_int(sub_dev)
    }

    fn set_dma(&mut self, dma: u32, length: u32, chan: i32) -> i32 {
        // C `drv_set_dma` (`es1370.c:332-365`): select the memory page,
        // write the address, then the longword count minus one.
        let Some(bits) = Self::channel_bits(chan) else {
            return -minix_types::EIO;
        };
        let ports = self.codec.ports_mut();
        if ports.out8(MEM_PAGE, bits.mem_page).is_err() {
            return -minix_types::EIO;
        }
        if ports.out32(bits.dma_address, dma).is_err() {
            return -minix_types::EIO;
        }
        let longwords = length / 4;
        match ports.out32(bits.frame_count, longwords - 1) {
            Ok(()) => 0,
            Err(status) => status,
        }
    }

    fn reenable_int(&mut self, chan: i32) -> i32 {
        // C `drv_reenable_int` (`es1370.c:396-412`): clear the enable bit,
        // then set it again.
        let Some(bits) = Self::channel_bits(chan) else {
            return -minix_types::EINVAL;
        };
        let Ok(ser) = self.codec.ports_mut().in16(SERIAL_INTERFACE_CTRL) else {
            return -minix_types::EIO;
        };
        let ports = self.codec.ports_mut();
        if ports
            .out16(SERIAL_INTERFACE_CTRL, ser & !bits.int_enable)
            .is_err()
        {
            return -minix_types::EIO;
        }
        match ports.out16(SERIAL_INTERFACE_CTRL, ser | bits.int_enable) {
            Ok(()) => 0,
            Err(status) => status,
        }
    }

    fn int_sum(&mut self) -> i32 {
        // C `drv_int_sum` (`es1370.c:368-370`): the summary bit alone.
        match self.codec.ports_mut().in32(INTERRUPT_STATUS) {
            Ok(status) => (status & INTR) as i32,
            Err(status) => status,
        }
    }

    fn interrupt(&mut self, sub_dev: i32) -> i32 {
        // C `drv_int` (`es1370.c:372-387`): the channel's bit alone.
        let Some(bits) = Self::channel_bits(sub_dev) else {
            return -minix_types::EINVAL;
        };
        match self.codec.ports_mut().in32(INTERRUPT_STATUS) {
            Ok(status) => (status & bits.interrupt) as i32,
            Err(status) => status,
        }
    }

    fn pause(&mut self, chan: i32) -> i32 {
        // C `drv_pause` (`es1370.c:415-431`): disable the interrupt, then
        // raise the pause bit. The capture channel has no pause bit and
        // is refused after its interrupt was already disabled.
        let status = self.disable_int(chan);
        if status != 0 {
            return status;
        }
        let Some(bits) = Self::channel_bits(chan) else {
            return -minix_types::EINVAL;
        };
        let Some(pause_bit) = bits.pause else {
            return -minix_types::EINVAL;
        };
        let Ok(ser) = self.codec.ports_mut().in32(SERIAL_INTERFACE_CTRL) else {
            return -minix_types::EIO;
        };
        match self
            .codec
            .ports_mut()
            .out32(SERIAL_INTERFACE_CTRL, ser | pause_bit as u32)
        {
            Ok(()) => 0,
            Err(status) => status,
        }
    }

    fn resume(&mut self, chan: i32) -> i32 {
        // C `drv_resume` (`es1370.c:434-447`): reenable the interrupt,
        // then clear the pause bit.
        let _ = self.reenable_int(chan);
        let Some(bits) = Self::channel_bits(chan) else {
            return -minix_types::EINVAL;
        };
        let Some(pause_bit) = bits.pause else {
            return -minix_types::EINVAL;
        };
        let Ok(ser) = self.codec.ports_mut().in32(SERIAL_INTERFACE_CTRL) else {
            return -minix_types::EIO;
        };
        match self
            .codec
            .ports_mut()
            .out32(SERIAL_INTERFACE_CTRL, ser & !(pause_bit as u32))
        {
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
    use crate::registers::CWRIP;
    use alloc::collections::BTreeMap;
    use alloc::vec::Vec;

    /// A port seam over a register file, with the traffic logged.
    struct FakePorts {
        registers: BTreeMap<u16, u32>,
        writes: Vec<(u16, u32, u8)>,
        /// Whether the codec reports a write in progress.
        busy: bool,
        fail: bool,
    }

    impl FakePorts {
        fn new() -> Self {
            FakePorts {
                registers: BTreeMap::new(),
                writes: Vec::new(),
                busy: false,
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
            if port == INTERRUPT_STATUS && self.busy {
                return Ok(CWRIP as u16);
            }
            Ok(self.value(port) as u16)
        }
        fn in32(&mut self, port: u16) -> Result<u32, i32> {
            if self.fail {
                return Err(-minix_types::EIO);
            }
            Ok(self.value(port))
        }
    }

    fn card() -> Es1370<FakePorts> {
        Es1370::new(FakePorts::new())
    }

    #[test]
    fn test_init_reports_four_sub_devices_and_init_hw_wipes_the_card() {
        let mut card = card();
        assert_eq!(card.init(), 4);
        assert_eq!(card.init_hw(), 0);
        // The codec was enabled, then initialized: the reset pair and the
        // input select came through the codec port.
        assert_eq!(
            card.codec_mut().ports_mut().value(CHIP_SEL_CTRL) & u32::from(XCTL0 | CDC_EN),
            u32::from(XCTL0 | CDC_EN)
        );
        let writes = &card.codec_mut().ports_mut().writes;
        assert!(
            writes
                .iter()
                .any(|(port, value, _)| *port == 0x10 && *value == 0x1603)
        );
        assert!(
            writes
                .iter()
                .any(|(port, value, _)| *port == 0x10 && *value == 0x1800)
        );
        // Sixteen pages were selected and each was wiped with four longs.
        let pages: Vec<u32> = writes
            .iter()
            .filter(|(port, _, size)| *port == MEM_PAGE && *size == 1)
            .map(|(_, value, _)| *value)
            .collect();
        assert_eq!(pages, (0..0x10).collect::<Vec<u32>>());
        let wipes = writes
            .iter()
            .filter(|(port, value, size)| {
                (MEMORY..MEMORY + 0x10).contains(port) && *value == 0 && *size == 4
            })
            .count();
        assert_eq!(wipes, 16 * 4);
        // The seeded configuration matches C's `drv_init_hw`.
        let dac1 = card.conf(DAC1_CHAN).unwrap();
        assert_eq!(dac1.sample_rate, DEFAULT_RATE);
        assert_eq!(dac1.nr_of_bits, 16);
        assert_eq!(dac1.stereo, 1);
        assert_eq!(dac1.sign, 0, "the header's DEFAULT_SIGNED is zero");
        assert_eq!(dac1.fragment_size, 32768);
        assert!(!dac1.busy);
    }

    #[test]
    fn test_start_programs_the_format_and_enables_the_channel() {
        let mut card = card();
        assert_eq!(card.start(DAC1_CHAN, 2), 0);
        let ser = card.codec_mut().ports_mut().value(SERIAL_INTERFACE_CTRL) as u16;
        assert_eq!(ser & P1_S_MB, P1_S_MB, "stereo");
        assert_eq!(ser & P1_S_EB, P1_S_EB, "sixteen bits");
        assert_eq!(ser & P1_INTR_EN, P1_INTR_EN, "interrupts on");
        // The sample count is the fragment size over both format factors:
        // 32768 / 2 (stereo) / 2 (sixteen bits) = 8192, minus one.
        assert_eq!(card.codec_mut().ports_mut().value(DAC1_SAMP_CT), 8191);
        // The clock select went up and the channel is enabled.
        assert_eq!(
            card.codec_mut().ports_mut().value(CHIP_SEL_CTRL) & FREQ_44K100 as u32,
            FREQ_44K100 as u32
        );
        assert_eq!(
            card.codec_mut().ports_mut().value(CHIP_SEL_CTRL) & DAC1_EN as u32,
            DAC1_EN as u32
        );
        assert!(card.conf(DAC1_CHAN).unwrap().busy);
        // The capture channel has its own bits.
        assert_eq!(card.start(ADC1_CHAN, 2), 0);
        let ser = card.codec_mut().ports_mut().value(SERIAL_INTERFACE_CTRL) as u16;
        assert_eq!(ser & R1_S_MB, R1_S_MB);
        assert_eq!(ser & R1_S_EB, R1_S_EB);
        assert_eq!(
            card.codec_mut().ports_mut().value(CHIP_SEL_CTRL) & ADC1_EN as u32,
            ADC1_EN as u32
        );
        // A stop clears the enable bit and the channel's interrupt.
        assert_eq!(card.stop(DAC1_CHAN), 0);
        assert!(!card.conf(DAC1_CHAN).unwrap().busy);
        assert_eq!(
            card.codec_mut().ports_mut().value(CHIP_SEL_CTRL) & DAC1_EN as u32,
            0
        );
        assert_eq!(
            card.codec_mut().ports_mut().value(SERIAL_INTERFACE_CTRL) as u16 & P1_INTR_EN,
            0
        );
    }

    #[test]
    fn test_the_rate_gate_and_the_bits_gate_match_c() {
        let mut card = card();
        let mut len = 4;
        assert_eq!(
            card.io_ctl_inner(DSPIORATE, &mut 3000u32.to_le_bytes(), &mut len, DAC1_CHAN),
            -minix_types::EINVAL
        );
        assert_eq!(
            card.io_ctl_inner(DSPIORATE, &mut 48000u32.to_le_bytes(), &mut len, DAC1_CHAN),
            -minix_types::EINVAL
        );
        assert_eq!(
            card.io_ctl_inner(DSPIORATE, &mut 22050u32.to_le_bytes(), &mut len, DAC1_CHAN),
            0
        );
        assert_eq!(card.conf(DAC1_CHAN).unwrap().sample_rate, 22050);
        // Only eight and sixteen bits are accepted.
        assert_eq!(
            card.io_ctl_inner(DSPIOBITS, &mut 12u32.to_le_bytes(), &mut len, DAC1_CHAN),
            -minix_types::EINVAL
        );
        assert_eq!(
            card.io_ctl_inner(DSPIOBITS, &mut 8u32.to_le_bytes(), &mut len, DAC1_CHAN),
            0
        );
        assert_eq!(card.conf(DAC1_CHAN).unwrap().nr_of_bits, 8);
        // A sign request is accepted and stored nowhere.
        assert_eq!(
            card.io_ctl_inner(DSPIOSIGN, &mut 1u32.to_le_bytes(), &mut len, DAC1_CHAN),
            0
        );
        assert_eq!(card.conf(DAC1_CHAN).unwrap().sign, 0);
        // An unknown request is refused, and so is a channel that has no
        // registers.
        assert_eq!(
            card.io_ctl_inner(0xdead_beef, &mut [0u8; 4], &mut len, DAC1_CHAN),
            -minix_types::EINVAL
        );
        assert_eq!(
            card.io_ctl_inner(DSPIOMAX, &mut [0u8; 4], &mut len, MIXER),
            -minix_types::EINVAL
        );
    }

    #[test]
    fn test_set_dma_selects_the_page_and_writes_longwords() {
        let mut card = card();
        assert_eq!(card.set_dma(0x0010_0000, 32768, DAC1_CHAN), 0);
        assert_eq!(
            card.codec_mut().ports_mut().value(MEM_PAGE) as u8,
            DAC_MEM_PAGE
        );
        assert_eq!(
            card.codec_mut().ports_mut().value(DAC1_PCI_ADDRESS),
            0x0010_0000
        );
        assert_eq!(
            card.codec_mut().ports_mut().value(DAC1_BUFFER_SIZE),
            8191,
            "32768 bytes are 8192 longwords, minus one"
        );
        // The capture channel uses its own page and address register.
        assert_eq!(card.set_dma(0x0020_0000, 4096, ADC1_CHAN), 0);
        assert_eq!(
            card.codec_mut().ports_mut().value(MEM_PAGE) as u8,
            ADC_MEM_PAGE
        );
        assert_eq!(
            card.codec_mut().ports_mut().value(ADC_PCI_ADDRESS),
            0x0020_0000
        );
        assert_eq!(card.codec_mut().ports_mut().value(ADC_BUFFER_SIZE), 1023);
        // The second playback channel has its own frame-count register
        // but shares the page and the address register with the first.
        assert_eq!(card.set_dma(0x0030_0000, 8192, DAC2_CHAN), 0);
        assert_eq!(
            card.codec_mut().ports_mut().value(MEM_PAGE) as u8,
            DAC_MEM_PAGE
        );
        assert_eq!(
            card.codec_mut().ports_mut().value(DAC2_PCI_ADDRESS),
            0x0030_0000
        );
        assert_eq!(card.codec_mut().ports_mut().value(DAC2_BUFFER_SIZE), 2047);
        // The mixer has no DMA registers.
        assert_eq!(card.set_dma(0, 0, MIXER), -minix_types::EIO);
    }

    #[test]
    fn test_the_volume_ioctls_only_answer_on_the_mixer() {
        let mut card = card();
        let level = VolumeLevel {
            device: minix_audiodriver::sound::Device::Master,
            left: 0x0c,
            right: 0x0c,
        };
        let mut len = 0;
        assert_eq!(
            card.io_ctl_inner(MIXIOSETVOLUME, &mut level.encode(), &mut len, MIXER),
            0
        );
        assert_eq!(len, VolumeLevel::SIZE as i32);
        assert_eq!(card.codec().value(0x00), Some(0x13), "0x1f - 0x0c");
        // The playback channel has no codec: the payload length is still
        // reported, and the request is refused.
        let mut len = 0;
        assert_eq!(
            card.io_ctl_inner(MIXIOSETVOLUME, &mut level.encode(), &mut len, DAC1_CHAN),
            -minix_types::EINVAL
        );
        assert_eq!(len, VolumeLevel::SIZE as i32);
    }

    #[test]
    fn test_pause_and_resume_are_refused_for_the_capture_channel() {
        let mut card = card();
        // The playback channel pauses and resumes.
        assert_eq!(card.pause(DAC1_CHAN), 0);
        assert_eq!(
            card.codec_mut().ports_mut().value(SERIAL_INTERFACE_CTRL) as u16 & P1_PAUSE,
            P1_PAUSE
        );
        assert_eq!(card.resume(DAC1_CHAN), 0);
        assert_eq!(
            card.codec_mut().ports_mut().value(SERIAL_INTERFACE_CTRL) as u16 & P1_PAUSE,
            0
        );
        // The capture channel has no pause bit (C's `default:` arm).
        assert_eq!(card.pause(ADC1_CHAN), -minix_types::EINVAL);
        assert_eq!(card.resume(ADC1_CHAN), -minix_types::EINVAL);
    }

    #[test]
    fn test_the_interrupt_hooks_report_the_channel_bits() {
        let mut card = card();
        card.codec_mut()
            .ports_mut()
            .registers
            .insert(INTERRUPT_STATUS, INTR | DAC1);
        assert_eq!(card.int_sum(), INTR as i32);
        assert_eq!(card.interrupt(DAC1_CHAN), DAC1 as i32);
        assert_eq!(card.interrupt(DAC2_CHAN), 0);
        assert_eq!(card.interrupt(ADC1_CHAN), 0);
        assert_eq!(card.interrupt(MIXER), -minix_types::EINVAL);
    }

    #[test]
    fn test_the_reset_ioctl_stops_the_channel() {
        let mut card = card();
        assert_eq!(card.start(DAC1_CHAN, 2), 0);
        let mut len = 0;
        assert_eq!(
            card.io_ctl_inner(DSPIORESET, &mut [0u8; 4], &mut len, DAC1_CHAN),
            0
        );
        assert!(!card.conf(DAC1_CHAN).unwrap().busy);
        assert_eq!(
            card.codec_mut().ports_mut().value(CHIP_SEL_CTRL) & DAC1_EN as u32,
            0
        );
        // `drv_reset` itself is a no-op that reports success.
        assert_eq!(card.reset(), 0);
    }

    #[test]
    fn test_a_failing_port_seam_is_reported() {
        let mut card = Es1370::new(FakePorts {
            fail: true,
            ..FakePorts::new()
        });
        assert_eq!(card.init_hw(), -minix_types::EIO);
        assert_eq!(card.start(DAC1_CHAN, 2), -minix_types::EIO);
        assert_eq!(card.stop(DAC1_CHAN), -minix_types::EIO);
        assert_eq!(card.set_dma(0, 4096, DAC1_CHAN), -minix_types::EIO);
    }
}
