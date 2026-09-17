//! Sub-device runtime: the DMA fragment ring and the extra buffer.
//!
//! C correspondence: `sub_dev_t` (`audio_fw.h:18-55`) and the interrupt
//! handlers that drive it — `handle_int_write` (playback: the card ate
//! one fragment) and `handle_int_read` (capture: the card filled one),
//! both in `audio_fw.c:520-610`. Byte-level copying and card registers
//! stay in the service binary; this module owns the cursor arithmetic
//! and the out-of-data rule.

/// Transfer direction of one sub-device (`DEV_WRITE`/`DEV_READ`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmaMode {
    /// Playback: the card consumes fragments.
    Write,
    /// Capture: the card produces fragments.
    Read,
}

/// What happened to the DMA ring after one fragment interrupt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FragmentOutcome {
    /// The ring still holds data; interrupts continue.
    Continue,
    /// A fragment moved in from the extra buffer.
    RefilledFromExtra,
    /// The DMA ring ran dry: pause the card (`handle_int_write`,
    /// `audio_fw.c:593-601`).
    OutOfData,
}

/// One sub-device's DMA fragment ring plus extra buffer cursors.
///
/// C: `sub_dev_t` fields `DmaReadNext`/`DmaFillNext`/`DmaLength` (the
/// hardware ring), `BufReadNext`/`BufFillNext`/`BufLength` (the extra
/// software buffer), `OutOfData`, and the open/busy flags.
#[derive(Debug, Clone)]
pub struct SubDevice {
    fragments: u32,
    frag_size: u32,
    extra_buffers: u32,
    mode: DmaMode,
    opened: bool,
    dma_busy: bool,
    out_of_data: bool,
    dma_read_next: u32,
    dma_fill_next: u32,
    dma_length: u32,
    buf_read_next: u32,
    buf_fill_next: u32,
    buf_length: u32,
}

impl SubDevice {
    /// A closed sub-device with an empty ring.
    ///
    /// C: `sub_dev_t` starts zeroed (`sef_cb_init_fresh`), fragments and
    /// fragment size arriving from the card driver's `drv_init` path.
    pub fn new(fragments: u32, frag_size: u32, extra_buffers: u32, mode: DmaMode) -> Self {
        SubDevice {
            fragments,
            frag_size,
            extra_buffers,
            mode,
            opened: false,
            dma_busy: false,
            out_of_data: false,
            dma_read_next: 0,
            dma_fill_next: 0,
            dma_length: 0,
            buf_read_next: 0,
            buf_fill_next: 0,
            buf_length: 0,
        }
    }

    /// DMA fragment count (`NrOfDmaFragments`).
    pub const fn fragments(&self) -> u32 {
        self.fragments
    }

    /// Fragment size in bytes (`FragSize`).
    pub const fn frag_size(&self) -> u32 {
        self.frag_size
    }

    /// Transfer direction (`DmaMode`).
    pub const fn mode(&self) -> DmaMode {
        self.mode
    }

    /// Whether the device was opened (`Opened`).
    pub const fn is_opened(&self) -> bool {
        self.opened
    }

    /// Mark the device open (the service's open path).
    pub fn set_opened(&mut self, opened: bool) {
        self.opened = opened;
    }

    /// Whether the DMA engine is running (`DmaBusy`).
    pub const fn is_dma_busy(&self) -> bool {
        self.dma_busy
    }

    /// Mark the DMA engine started (after `drv_start` succeeded).
    pub fn set_dma_busy(&mut self, busy: bool) {
        self.dma_busy = busy;
    }

    /// Whether every buffer ran dry (`OutOfData`).
    pub const fn is_out_of_data(&self) -> bool {
        self.out_of_data
    }

    /// How many fragments are in flight (`DmaLength`).
    pub const fn dma_length(&self) -> u32 {
        self.dma_length
    }

    /// How many fragments wait in the extra buffer (`BufLength`).
    pub const fn buf_length(&self) -> u32 {
        self.buf_length
    }

    /// Fill one fragment from the application side (playback path).
    ///
    /// Advances the fill cursor modulo the fragment count and grows the
    /// in-flight length; refuses when the ring is already full.
    pub fn fill_fragment(&mut self) -> bool {
        if self.dma_length == self.fragments {
            return false;
        }
        self.dma_fill_next = (self.dma_fill_next + 1) % self.fragments;
        self.dma_length += 1;
        true
    }

    /// Playback interrupt: the card consumed one fragment
    /// (`handle_int_write`, `audio_fw.c:520-560`).
    ///
    /// The read cursor advances and the in-flight length drops; a queued
    /// extra-buffer fragment is then pulled in (cursors advance, length
    /// restored). When nothing remains the device reports out-of-data —
    /// the service pauses the card, or closes it if nobody holds the
    /// device open.
    pub fn consume_fragment(&mut self) -> FragmentOutcome {
        self.dma_read_next = (self.dma_read_next + 1) % self.fragments;
        self.dma_length -= 1;
        let mut outcome = FragmentOutcome::Continue;
        if self.buf_length != 0 {
            // Extra-buffer fragment moves into the freed DMA slot.
            self.buf_read_next = (self.buf_read_next + 1) % self.extra_buffers;
            self.dma_fill_next = (self.dma_fill_next + 1) % self.fragments;
            self.buf_length -= 1;
            self.dma_length += 1;
            outcome = FragmentOutcome::RefilledFromExtra;
        }
        if self.dma_length == 0 {
            self.out_of_data = true;
            return FragmentOutcome::OutOfData;
        }
        outcome
    }

    /// Capture interrupt: the card produced one fragment
    /// (`handle_int_read`, `audio_fw.c:561-610`).
    ///
    /// The fill cursor advances and the in-flight length grows; refused
    /// when the ring is already full.
    pub fn produce_fragment(&mut self) -> bool {
        if self.dma_length == self.fragments {
            return false;
        }
        self.dma_fill_next = (self.dma_fill_next + 1) % self.fragments;
        self.dma_length += 1;
        true
    }

    /// Queue one fragment into the extra buffer (application side).
    pub fn queue_extra(&mut self) -> bool {
        if self.buf_length == self.extra_buffers {
            return false;
        }
        self.buf_fill_next = (self.buf_fill_next + 1) % self.extra_buffers;
        self.buf_length += 1;
        true
    }
}

/// Bytes of DMA left before crossing a 64 KiB boundary
/// (`dma_bytes_left`, `audio_fw.h:56-58`): ISA-style DMA engines cannot
/// cross the boundary inside one transfer, so callers split there.
pub const fn dma_bytes_left(phys: u64) -> u32 {
    0x1_0000 - (phys & 0xFFFF) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playback() -> SubDevice {
        SubDevice::new(4, 4096, 2, DmaMode::Write)
    }

    #[test]
    fn test_fill_and_consume_advance_modulo() {
        let mut device = playback();
        device.set_opened(true);
        assert!(device.fill_fragment());
        assert!(device.fill_fragment());
        assert_eq!(device.dma_length(), 2);
        assert_eq!(device.consume_fragment(), FragmentOutcome::Continue);
        // Cursors wrap modulo the fragment count (4): consuming past 4
        // returns to zero, never out of range.
        for _ in 0..3 {
            assert!(device.fill_fragment());
            device.consume_fragment();
        }
        assert!(device.dma_read_next < device.fragments());
    }

    #[test]
    fn test_playback_runs_out_of_data_when_ring_empties() {
        let mut device = playback();
        device.set_opened(true);
        device.fill_fragment();
        assert_eq!(device.consume_fragment(), FragmentOutcome::OutOfData);
        assert!(device.is_out_of_data());
    }

    #[test]
    fn test_extra_buffer_refills_the_freed_slot() {
        let mut device = playback();
        device.set_opened(true);
        device.fill_fragment();
        assert!(device.queue_extra());
        assert_eq!(device.buf_length(), 1);
        // The consume pulls the extra fragment back in the same tick.
        assert_eq!(
            device.consume_fragment(),
            FragmentOutcome::RefilledFromExtra
        );
        assert_eq!(device.dma_length(), 1);
        assert_eq!(device.buf_length(), 0);
        assert!(!device.is_out_of_data());
    }

    #[test]
    fn test_dma_bytes_left_respects_64k_boundary() {
        assert_eq!(dma_bytes_left(0), 0x1_0000);
        assert_eq!(dma_bytes_left(0xFFFF), 1);
        assert_eq!(dma_bytes_left(0x1_2345), 0x1_0000 - 0x2345);
    }
}
