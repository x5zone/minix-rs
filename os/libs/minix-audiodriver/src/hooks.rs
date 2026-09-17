//! The fourteen card-specific hooks (`audio_fw.h:9-20`).
//!
//! C: `audio_fw.h` declares fourteen `drv_*` prototypes and every sound
//! card (es1371, sb16, ...) fills the table. The library calls them at
//! fixed points: `drv_init`/`drv_init_hw` during startup, `drv_start`/
//! `drv_stop` around transfers, `drv_int` per fragment interrupt, and
//! `drv_io_ctl` for the sound ioctls. In C every pointer may be null and
//! the library checks; here every method has a default body reproducing
//! the C fallback, so a card overrides only what it implements.
//!
//! Signatures are re-typed for Rust (results instead of pointer out-
//! parameters where the value is the whole point), but each hook keeps
//! its C anchor.

/// Card-specific half of an audio driver.
pub trait AudioHooks {
    /// Global driver initialization (`drv_init`): returns the number of
    /// sub-devices, or a negative errno.
    fn init(&mut self) -> i32 {
        0
    }

    /// Initialize the sound hardware (`drv_init_hw`).
    fn init_hw(&mut self) -> i32 {
        0
    }

    /// Reset the card (`drv_reset`).
    fn reset(&mut self) -> i32 {
        0
    }

    /// Start the device for one sub-device in a DMA direction
    /// (`drv_start`).
    fn start(&mut self, sub_dev: i32, dma_mode: i32) -> i32 {
        let _ = (sub_dev, dma_mode);
        0
    }

    /// Stop the device (`drv_stop`).
    fn stop(&mut self, sub_dev: i32) -> i32 {
        let _ = sub_dev;
        0
    }

    /// Program one DMA transfer (`drv_set_dma`).
    fn set_dma(&mut self, dma: u32, length: u32, chan: i32) -> i32 {
        let _ = (dma, length, chan);
        0
    }

    /// Re-enable the interrupt for one channel (`drv_reenable_int`).
    fn reenable_int(&mut self, chan: i32) -> i32 {
        let _ = chan;
        0
    }

    /// Sum of interrupt reasons on this card (`drv_int_sum`).
    fn int_sum(&mut self) -> i32 {
        0
    }

    /// Interrupt for one sub-device (`drv_int`).
    fn interrupt(&mut self, sub_dev: i32) -> i32 {
        let _ = sub_dev;
        0
    }

    /// Pause one channel (`drv_pause`).
    fn pause(&mut self, chan: i32) -> i32 {
        let _ = chan;
        0
    }

    /// Resume one channel (`drv_resume`).
    fn resume(&mut self, chan: i32) -> i32 {
        let _ = chan;
        0
    }

    /// Sound ioctl (`drv_io_ctl`): request, payload, and its length.
    fn io_ctl(&mut self, request: u64, val: &mut [u8], len: &mut i32, sub_dev: i32) -> i32 {
        let _ = (request, val, len, sub_dev);
        0
    }

    /// The card's IRQ line (`drv_get_irq`), or a negative errno.
    fn get_irq(&self) -> i32 {
        0
    }

    /// The fragment size for one sub-device (`drv_get_frag_size`).
    fn get_frag_size(&self, sub_dev: i32) -> u32 {
        let _ = sub_dev;
        0
    }
}

/// Every hook has a body: a card that implements nothing still answers.
pub struct NoCard;

impl AudioHooks for NoCard {}
