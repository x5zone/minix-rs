//! The audio framework's service layer: sub-device lifetime, DMA bring-up,
//! and the transfer gates.
//!
//! C correspondence: `audio_fw.c`'s `open_sub_dev` (`audio_fw.c:244-278`:
//! the single-open and busy gates, the cursor reset, and the DMA buffer
//! bring-up), `close_sub_dev` (`audio_fw.c:316-341`: a playback channel
//! with data still queued keeps its DMA busy and only clears the open
//! flag; otherwise the card is stopped and the buffers are freed),
//! `msg_open`/`msg_close` (`audio_fw.c:199-315`: the special file's three
//! channels), `msg_write`/`msg_read` (`audio_fw.c:394-495`: the fragment
//! size gate — the card's current fragment size is pulled per request
//! through `drv_get_frag_size`, `audio_fw.c:412`/`:462` — the direction
//! gate, the revive record, and the "no reply yet" answer), `msg_ioctl`
//! (`audio_fw.c:342-391`: the ioctl channel resolution, the encoded
//! payload length, and the in/out copies around the card's `drv_io_ctl`),
//! and `init_buffers` (`audio_fw.c:753-810`) as [`crate::dma`] models it.
//!
//! The fragment data plane is modeled (`data_from_user`/`data_to_user`/
//! `get_started` and the two interrupt handlers, `audio_fw.c:500-752`):
//! a read or write parks its caller the way C does (`EDONTREPLY`), the
//! copy happens into the ring or the extra buffer, and the caller is
//! revived with the fragment size — from the request's own path when
//! there is room, or from the interrupt path when the card frees a slot.
//! What is still missing is the allocator the ring comes from
//! (registered `E-DMABUF`): with [`crate::dma::UnboundDma`] a channel's
//! bring-up refuses, so no transfer is ever attempted.

use alloc::vec;
use alloc::vec::Vec;

use minix_types::{EBUSY, EDONTREPLY, EINVAL, EIO, OK};

use crate::dma::{DmaAlloc, RingPlan, plan_ring};
use crate::hooks::AudioHooks;
use crate::sound::{IOC_IN, IOC_OUT, io_ctl_length};
use crate::special::{NO_CHANNEL, SpecialFile, get_special_file};
use crate::subdev::{DmaMode, SubDevice};
use minix_driver_rt::plane::TransferPlane;

/// One channel's runtime state: the fragment model, its DMA plan, and the
/// caller parked on a transfer (`RevivePending` and friends).
#[derive(Debug)]
struct Channel {
    /// The fragment ring and buffer cursors (`sub_dev_t`).
    dev: SubDevice,
    /// The direction this channel serves (`DmaMode`).
    mode: DmaMode,
    /// The ring the card was programmed with, once brought up.
    plan: Option<RingPlan>,
    /// The caller waiting for a transfer to finish (`RevivePending`).
    parked: Option<Parked>,
}

/// A parked caller whose transfer finished: the deferred reply C sends
/// with `chardriver_reply_task` (the byte count in `status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Revive {
    /// Who to answer.
    pub parked: Parked,
    /// The status to carry (the fragment size, or zero when the capture
    /// side gave up).
    pub bytes: i32,
}

/// The caller a deferred reply belongs to (`ReviveId`/`ReviveGrant`/
/// `SourceProcNr`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parked {
    /// The request identifier (`ReviveId`).
    pub id: i32,
    /// The caller's grant (`ReviveGrant`).
    pub grant: i32,
    /// The caller (`SourceProcNr`).
    pub caller: i32,
}

impl Channel {
    fn new(mode: DmaMode, fragments: u32, frag_size: u32, extra: u32) -> Self {
        Channel {
            dev: SubDevice::new(fragments, frag_size, extra, mode),
            mode,
            plan: None,
            parked: None,
        }
    }
}

/// The audio service: one card's hooks, the DMA allocator, and the
/// channels.
pub struct AudioService<H: AudioHooks, A: DmaAlloc> {
    hooks: H,
    alloc: A,
    files: Vec<SpecialFile>,
    channels: Vec<Channel>,
    geometry: Vec<(DmaMode, u32, u32, u32)>,
}

impl<H: AudioHooks, A: DmaAlloc> AudioService<H, A> {
    /// Build the service: `init` is the card's hook (the sub-device
    /// count), and each channel gets its fragment geometry from the card
    /// (`drv_init`).
    pub fn new(
        hooks: H,
        alloc: A,
        files: Vec<SpecialFile>,
        geometry: &[(DmaMode, u32, u32, u32)],
    ) -> Self {
        AudioService {
            hooks,
            alloc,
            files,
            channels: geometry
                .iter()
                .map(|(mode, fragments, frag_size, extra)| {
                    Channel::new(*mode, *fragments, *frag_size, *extra)
                })
                .collect(),
            geometry: geometry.to_vec(),
        }
    }

    /// The card's initialization (`drv_init` then `drv_init_hw`,
    /// `audio_fw.c:110-170`).
    pub fn init(&mut self) -> i32 {
        let count = self.hooks.init();
        if count < 0 {
            return count;
        }
        self.hooks.init_hw()
    }

    /// The card's hooks (tests and the binary's diagnostics).
    pub fn hooks(&self) -> &H {
        &self.hooks
    }

    /// The card's hooks, mutably.
    pub fn hooks_mut(&mut self) -> &mut H {
        &mut self.hooks
    }

    /// One channel's ring plan, once brought up.
    pub fn ring(&self, chan: usize) -> Option<RingPlan> {
        self.channels[chan].plan
    }

    /// Open one sub-device (`open_sub_dev`, `audio_fw.c:244-278`).
    pub fn open_sub_dev(&mut self, chan: usize, mode: DmaMode) -> i32 {
        // The gates first (`audio_fw.c:248-256`).
        if self.channels[chan].dev.is_opened() {
            return EBUSY;
        }
        if self.channels[chan].dev.is_dma_busy() {
            return EBUSY;
        }
        // The cursor reset (`audio_fw.c:258-266`): a fresh model has zeroed
        // cursors and `OutOfData` set, which is what C's reset produces.
        let (_, fragments, frag_size, extra) = self.geometry[chan];
        self.channels[chan].dev = SubDevice::new(fragments, frag_size, extra, mode);
        self.channels[chan].mode = mode;
        self.channels[chan].dev.set_opened(true);
        // C arms the dry flag on every open (`audio_fw.c:266`).
        self.channels[chan].dev.set_out_of_data(true);
        // Arrange DMA: allocate the buffer and program the card
        // (`audio_fw.c:271-277`). A channel with no fragment geometry is
        // a control-only sub-device (C's `NO_DMA` mode) and skips the
        // bring-up entirely (`audio_fw.c:271`: `if (dma_mode != NO_DMA)`).
        let dma_size = u64::from(fragments) * u64::from(frag_size);
        if dma_size == 0 {
            self.channels[chan].plan = None;
            return OK;
        }
        let buffer = match self.alloc.alloc_contig(dma_size + 64 * 1024) {
            Ok(buffer) => buffer,
            Err(status) => return status,
        };
        let plan = plan_ring(buffer, dma_size);
        let status = self
            .hooks
            .set_dma(plan.physical as u32, plan.size as u32, chan as i32);
        if status != OK {
            return status;
        }
        self.channels[chan].plan = Some(plan);
        OK
    }

    /// Close one sub-device (`close_sub_dev`, `audio_fw.c:316-341`).
    pub fn close_sub_dev(&mut self, chan: usize) -> i32 {
        let channel = &mut self.channels[chan];
        if channel.mode == DmaMode::Write && !channel.dev.is_out_of_data() {
            // Data is still queued for the card: keep the DMA busy and
            // only clear the open flag.
            channel.dev.set_opened(false);
            return OK;
        }
        channel.dev.set_opened(false);
        channel.dev.set_dma_busy(false);
        channel.plan = None;
        self.hooks.stop(chan as i32)
    }

    /// Open a special file (`msg_open`, `audio_fw.c:199-242`): the write
    /// channel, the read channel, and the ioctl channel when it differs.
    pub fn open(&mut self, minor: i32) -> i32 {
        let Some(file) = get_special_file(&self.files, minor).copied() else {
            return EIO;
        };
        if file.write_chan != NO_CHANNEL {
            let chan = file.write_chan as usize;
            let status = self.open_sub_dev(chan, DmaMode::Write);
            if status != OK {
                return status;
            }
        }
        if file.read_chan != NO_CHANNEL && file.read_chan != file.write_chan {
            let chan = file.read_chan as usize;
            let status = self.open_sub_dev(chan, DmaMode::Read);
            if status != OK {
                return status;
            }
        }
        if file.ioctl_chan != NO_CHANNEL
            && file.ioctl_chan != file.write_chan
            && file.ioctl_chan != file.read_chan
        {
            let chan = file.ioctl_chan as usize;
            return self.open_sub_dev(chan, DmaMode::Write);
        }
        OK
    }

    /// Close a special file (`msg_close`, `audio_fw.c:280-314`).
    pub fn close(&mut self, minor: i32) -> i32 {
        let Some(file) = get_special_file(&self.files, minor).copied() else {
            return EIO;
        };
        let mut result = OK;
        if file.write_chan != NO_CHANNEL && self.close_sub_dev(file.write_chan as usize) != OK {
            result = EIO;
        }
        if file.read_chan != NO_CHANNEL
            && file.read_chan != file.write_chan
            && self.close_sub_dev(file.read_chan as usize) != OK
        {
            result = EIO;
        }
        if file.ioctl_chan != NO_CHANNEL
            && file.ioctl_chan != file.write_chan
            && file.ioctl_chan != file.read_chan
            && self.close_sub_dev(file.ioctl_chan as usize) != OK
        {
            result = EIO;
        }
        result
    }

    /// A write request (`msg_write`, `audio_fw.c:394-442`): the gates,
    /// then the fragment copy and the card's start when it is idle.
    ///
    /// Returns the status to reply to *this* request with (`EDONTREPLY`
    /// in the normal case — the answer rides the revive below, as C's
    /// `chardriver_reply_task` does) and the parked caller to wake, if
    /// one could be served.
    pub fn write(
        &mut self,
        minor: i32,
        size: u64,
        parked: Parked,
        plane: &mut dyn TransferPlane,
    ) -> (i32, Option<Revive>) {
        self.transfer(minor, size, parked, true, plane)
    }

    /// A read request (`msg_read`, `audio_fw.c:444-495`).
    pub fn read(
        &mut self,
        minor: i32,
        size: u64,
        parked: Parked,
        plane: &mut dyn TransferPlane,
    ) -> (i32, Option<Revive>) {
        self.transfer(minor, size, parked, false, plane)
    }

    fn transfer(
        &mut self,
        minor: i32,
        size: u64,
        parked: Parked,
        do_write: bool,
        plane: &mut dyn TransferPlane,
    ) -> (i32, Option<Revive>) {
        let Some(file) = get_special_file(&self.files, minor).copied() else {
            return (EIO, None);
        };
        let chan = if do_write {
            file.write_chan
        } else {
            file.read_chan
        };
        if chan == NO_CHANNEL {
            return (EIO, None);
        }
        let chan = chan as usize;
        // C pulls the card's current fragment size per request
        // (`drv_get_frag_size`, `audio_fw.c:412-417`).
        let frag_size = self.hooks.get_frag_size(chan as i32) as u64;
        if size != frag_size {
            return (EINVAL, None);
        }
        let wanted = if do_write {
            DmaMode::Write
        } else {
            DmaMode::Read
        };
        if self.channels[chan].dev.is_dma_busy() && self.channels[chan].mode != wanted {
            return (EBUSY, None);
        }
        // The revive record (`audio_fw.c:428-432`).
        self.channels[chan].parked = Some(parked);
        let revive = if do_write {
            // The write path copies first, then starts the card if it is
            // idle (`audio_fw.c:434-441`).
            let revive = self.data_from_user(chan, plane);
            if !self.channels[chan].dev.is_dma_busy() {
                self.get_started(chan);
                self.channels[chan].mode = DmaMode::Write;
            }
            revive
        } else {
            if !self.channels[chan].dev.is_dma_busy() {
                self.get_started(chan);
                self.channels[chan].mode = DmaMode::Read;
                // C returns without looking at the ring: a capture
                // channel has nothing to hand over yet
                // (`audio_fw.c:487-489`).
                return (Self::DONT_REPLY, None);
            }
            self.data_to_user(chan, plane)
        };
        (Self::DONT_REPLY, revive)
    }

    /// Serve the parked caller from the application side
    /// (`data_from_user`, `audio_fw.c:649-704`).
    ///
    /// A fragment goes into the ring when there is room there, otherwise
    /// into the extra buffer; when the device had run dry the card is
    /// resumed. The caller is answered with the fragment size.
    fn data_from_user(&mut self, chan: usize, plane: &mut dyn TransferPlane) -> Option<Revive> {
        // The channel's record first, then its ring: the two borrows
        // cannot overlap.
        let parked = self.channels[chan].parked?;
        let plan = self.channels[chan].plan?;
        let dev = &mut self.channels[chan].dev;
        // No space at all: nothing to do (`audio_fw.c:654-656`).
        if dev.dma_length() == dev.fragments() && dev.buf_length() == dev.extra_buffers() {
            return None;
        }
        let frag = dev.frag_size() as usize;
        let copied = if dev.dma_length() < dev.fragments() {
            let slot = dev.dma_fill_next() as u64;
            let copied = Self::copy_in(plane, plan.address + slot * frag as u64, frag);
            dev.fill_fragment();
            copied
        } else {
            let slot = dev.buf_fill_next() as u64;
            // The extra buffer lives after the ring inside the same
            // allocation (`audio_fw.c:673-678`).
            let base = plan.address + plan.size;
            let copied = Self::copy_in(plane, base + slot * frag as u64, frag);
            dev.queue_extra();
            copied
        };
        if !copied {
            // C prints and carries on; the caller is still answered so it
            // does not hang on a transfer that never started.
        }
        let was_dry = dev.is_out_of_data();
        if was_dry {
            dev.set_out_of_data(false);
        }
        if was_dry {
            let _ = self.hooks.resume(chan as i32);
        }
        self.channels[chan].parked = None;
        Some(Revive {
            parked,
            bytes: frag as i32,
        })
    }

    /// Hand one fragment to the parked caller (`data_to_user`,
    /// `audio_fw.c:706-751`).
    ///
    /// The extra buffer is drained first; a fragment from the ring
    /// follows.
    fn data_to_user(&mut self, chan: usize, plane: &mut dyn TransferPlane) -> Option<Revive> {
        let parked = self.channels[chan].parked?;
        let plan = self.channels[chan].plan?;
        let dev = &mut self.channels[chan].dev;
        if dev.buf_length() == 0 && dev.dma_length() == 0 {
            return None;
        }
        let frag = dev.frag_size() as usize;
        if dev.buf_length() != 0 {
            let slot = dev.buf_read_next() as u64;
            let base = plan.address + plan.size;
            let copied = Self::copy_out(plane, base + slot * frag as u64, frag);
            dev.consume_extra();
            if !copied {
                // C prints and carries on.
            }
        } else {
            let slot = dev.dma_read_next() as u64;
            let copied = Self::copy_out(plane, plan.address + slot * frag as u64, frag);
            let _ = dev.consume_fragment();
            if !copied {
                // C prints and carries on.
            }
        }
        self.channels[chan].parked = None;
        Some(Revive {
            parked,
            bytes: frag as i32,
        })
    }

    /// The card's start sequence (`get_started`, `audio_fw.c:630-646`):
    /// the card's own start hook, then the busy flag and a rewound read
    /// cursor.
    fn get_started(&mut self, chan: usize) {
        let _ = self.hooks.start(chan as i32, 0);
        let dev = &mut self.channels[chan].dev;
        dev.set_dma_busy(true);
        dev.rewind_read_cursor();
    }

    /// Copy one fragment from the caller into the ring (`sys_safecopyfrom`
    /// at `DmaPtr`/`ExtraBuf`, `audio_fw.c:663-681`).
    ///
    /// # Safety
    ///
    /// `address` must name a `frag`-byte span inside the channel's ring
    /// allocation, which is the plan the channel was brought up with.
    fn copy_in(plane: &mut dyn TransferPlane, address: u64, frag: usize) -> bool {
        // SAFETY: the address comes from the channel's own ring plan.
        let target = unsafe { core::slice::from_raw_parts_mut(address as *mut u8, frag) };
        plane.copy_from_caller(0, target).is_ok()
    }

    /// Copy one fragment out of the ring to the caller (`sys_safecopyto`,
    /// `audio_fw.c:715-737`).
    ///
    /// # Safety
    ///
    /// As [`AudioService::copy_in`], for the read direction.
    fn copy_out(plane: &mut dyn TransferPlane, address: u64, frag: usize) -> bool {
        // SAFETY: the address comes from the channel's own ring plan.
        let source = unsafe { core::slice::from_raw_parts(address as *const u8, frag) };
        plane.copy_to_caller(0, source).is_ok()
    }

    /// The hardware notify (`msg_hardware`, `audio_fw.c:498-517`): one
    /// interrupt summary read, then each busy channel's handler.
    ///
    /// Returns the callers that could be revived; the caller of this
    /// method sends their replies.
    pub fn handle_hardware(&mut self, plane: &mut dyn TransferPlane) -> alloc::vec::Vec<Revive> {
        let mut revived = alloc::vec::Vec::new();
        if self.hooks.int_sum() == 0 {
            return revived;
        }
        for chan in 0..self.channels.len() {
            let dev = &self.channels[chan].dev;
            if self.hooks.interrupt(chan as i32) == 0 || !dev.is_dma_busy() {
                continue;
            }
            match self.channels[chan].mode {
                DmaMode::Write => {
                    if let Some(revive) = self.handle_int_write(chan, plane) {
                        revived.push(revive);
                    }
                }
                DmaMode::Read => {
                    if let Some(revive) = self.handle_int_read(chan, plane) {
                        revived.push(revive);
                    }
                }
            }
        }
        revived
    }

    /// The playback interrupt (`handle_int_write`, `audio_fw.c:519-560`).
    fn handle_int_write(&mut self, chan: usize, plane: &mut dyn TransferPlane) -> Option<Revive> {
        let outcome = self.channels[chan].dev.consume_fragment();
        let revive = self.data_from_user(chan, plane);
        if matches!(outcome, crate::subdev::FragmentOutcome::OutOfData) {
            // The ring ran dry: C pauses the card, or closes the channel
            // when nobody holds it open (`audio_fw.c:546-554`).
            if !self.channels[chan].dev.is_opened() {
                let _ = self.close_sub_dev(chan);
            } else {
                let _ = self.hooks.pause(chan as i32);
            }
            return revive;
        }
        let _ = self.hooks.reenable_int(chan as i32);
        revive
    }

    /// The capture interrupt (`handle_int_read`, `audio_fw.c:561-610`).
    fn handle_int_read(&mut self, chan: usize, plane: &mut dyn TransferPlane) -> Option<Revive> {
        let plan = self.channels[chan].plan;
        let dev = &mut self.channels[chan].dev;
        let produced = dev.produce_fragment();
        let full = dev.dma_length() == dev.fragments();
        if full {
            // The ring filled up while the caller was not draining it
            // (`audio_fw.c:580-607`).
            if dev.buf_length() == dev.extra_buffers() {
                // Every buffer is full: stop the card and answer the
                // waiting caller with zero bytes (`audio_fw.c:583-591`).
                let parked = self.channels[chan].parked.take();
                let _ = self.hooks.stop(chan as i32);
                self.channels[chan].dev.set_dma_busy(false);
                return parked.map(|parked| Revive { parked, bytes: 0 });
            }
            // The bytes move inside the allocation first
            // (`audio_fw.c:594-600`), then the cursors follow.
            if let Some(plan) = plan {
                let frag = dev.frag_size() as usize;
                let source = plan.address + dev.dma_read_next() as u64 * frag as u64;
                let target = plan.address + plan.size + dev.buf_fill_next() as u64 * frag as u64;
                // SAFETY: both spans live inside the channel's ring
                // allocation; the cursors keep them in range and the two
                // regions never overlap (the extra buffer follows the
                // ring).
                unsafe {
                    core::ptr::copy(source as *const u8, target as *mut u8, frag);
                }
            }
            dev.move_ring_to_extra();
        }
        let revive = self.data_to_user(chan, plane);
        let _ = self.hooks.reenable_int(chan as i32);
        if !produced {
            // A full ring cannot take another fragment; C's handler
            // still confirms the interrupt.
        }
        revive
    }

    /// The caller parked on a channel, if any (`RevivePending`).
    pub fn parked(&self, chan: usize) -> Option<Parked> {
        self.channels[chan].parked
    }

    /// One ioctl request (`msg_ioctl`, `audio_fw.c:342-391`): resolve the
    /// special file's ioctl channel, gate on it being open, move the
    /// payload across the caller's grant, and hand the encoded request to
    /// the card.
    ///
    /// The payload buffer is C's `io_ctl_buf[IOCPARM_MASK]`
    /// (`audio_fw.c:55`), with one deliberate difference: C's buffer is a
    /// static reused across requests, so a failed copy-in leaves the
    /// previous request's bytes in it and the card is called with them
    /// (C logs the failure and proceeds, `audio_fw.c:371-373`). A fresh
    /// buffer is used here, so a failed copy-in reaches the card as
    /// zeroes instead of another caller's payload. The copy failures
    /// themselves are still non-fatal, exactly as in C: the hook runs and
    /// its answer is what the caller sees.
    pub fn ioctl<T: minix_driver_rt::transport::DriverTransport + ?Sized>(
        &mut self,
        transport: &mut T,
        minor: i32,
        request: u64,
        grant: i32,
        caller: minix_types::Endpoint,
    ) -> i32 {
        let Some(file) = get_special_file(&self.files, minor).copied() else {
            return EIO;
        };
        if file.ioctl_chan == NO_CHANNEL {
            // C prints "No io control channel specified!" and refuses.
            return EIO;
        }
        let chan = file.ioctl_chan as usize;
        if !self.channels[chan].dev.is_opened() {
            // C prints "io control impossible - not opened!" and refuses.
            return EIO;
        }
        let mut buf = vec![0u8; crate::sound::IOCPARM_MASK as usize];
        // C leaves `len` uninitialized when the request carries no input
        // (`audio_fw.c:346`, `:368-370`); every output-only hook sets it
        // before the copy-out, so starting at zero is what C's stack
        // garbage is standing in for.
        let mut len: i32 = 0;
        if request & IOC_IN != 0 {
            len = io_ctl_length(request) as i32;
            let mut plane = minix_driver_rt::plane::Plane::new(transport, caller, grant);
            // C logs a failed copy-in and calls the hook anyway
            // (`audio_fw.c:371-376`); the refusal is not turned into an
            // error the caller never saw in C.
            let _copy_failed = plane.copy_from_caller(0, &mut buf[..len as usize]).is_err();
        }
        let status = self.hooks.io_ctl(request, &mut buf, &mut len, chan as i32);
        if status == OK && request & IOC_OUT != 0 {
            let mut plane = minix_driver_rt::plane::Plane::new(transport, caller, grant);
            // Same rule for the copy-out (`audio_fw.c:382-387`).
            let _copy_failed = plane.copy_to_caller(0, &buf[..len as usize]).is_err();
        }
        status
    }

    /// The sentinel C answers from the transfer paths (`EDONTREPLY`),
    /// exported so a caller can recognize it.
    pub const DONT_REPLY: i32 = EDONTREPLY;
}

impl<H: AudioHooks, A: DmaAlloc, T: minix_driver_rt::transport::DriverTransport>
    minix_driver_rt::runtime::DriverHandler<T> for AudioService<H, A>
{
    fn handle(&mut self, transport: &mut T, msg: &minix_types::Message) {
        self.dispatch(transport, msg);
    }
}

impl<H: AudioHooks, A: DmaAlloc> AudioService<H, A> {
    /// Handle one delivered message (`audio_fw.c`'s chardriver table:
    /// open, close, read, write, ioctl, and the hardware notify).
    pub fn dispatch<T: minix_driver_rt::transport::DriverTransport>(
        &mut self,
        transport: &mut T,
        msg: &minix_types::Message,
    ) {
        use minix_chardriver::protocol::CdevRequest;
        let is_notify = (msg.m_type & !0xff) == 0x1000;
        if is_notify {
            // The hardware notify drives the interrupt path
            // (`msg_hardware`): each busy channel's handler runs and the
            // callers it served get their deferred replies.
            let mut plane = minix_driver_rt::plane::NullPlane;
            for revive in self.handle_hardware(&mut plane) {
                self.reply(
                    transport,
                    minix_types::Endpoint(revive.parked.caller),
                    revive.bytes,
                    revive.parked.id,
                );
            }
            return;
        }
        let Some(request) = CdevRequest::decode(msg.m_type) else {
            return;
        };
        let base = ::core::ptr::addr_of!(*msg) as *const u8;
        let read_i32 = |offset: usize| -> i32 {
            let mut bytes = [0u8; 4];
            // SAFETY: the message union's documented slots.
            unsafe { ::core::ptr::copy_nonoverlapping(base.add(offset), bytes.as_mut_ptr(), 4) };
            i32::from_le_bytes(bytes)
        };
        let read_u64 = |offset: usize| -> u64 {
            let mut bytes = [0u8; 8];
            // SAFETY: the message union's documented slots.
            unsafe { ::core::ptr::copy_nonoverlapping(base.add(offset), bytes.as_mut_ptr(), 8) };
            u64::from_le_bytes(bytes)
        };
        match request {
            // Open and close carry `mess_vfs_lchardriver_openclose`; the
            // minor sits behind `id` and `user` (`ipc.h:2228-2235`).
            CdevRequest::Open => {
                let minor = read_i32(cwire::openclose::MINOR);
                let id = read_i32(cwire::openclose::ID);
                let status = self.open(minor);
                self.reply(transport, msg.m_source, status, id);
            }
            CdevRequest::Close => {
                let minor = read_i32(cwire::openclose::MINOR);
                let id = read_i32(cwire::openclose::ID);
                let status = self.close(minor);
                self.reply(transport, msg.m_source, status, id);
            }
            // Read, write, and ioctl share
            // `mess_vfs_lchardriver_readwrite` (`ipc.h:2238-2249`).
            CdevRequest::Read | CdevRequest::Write => {
                let minor = read_i32(cwire::readwrite::MINOR);
                let id = read_i32(cwire::readwrite::ID);
                let grant = read_i32(cwire::readwrite::GRANT);
                let size = read_u64(cwire::readwrite::COUNT);
                let parked = Parked {
                    id,
                    grant,
                    caller: msg.m_source.0,
                };
                let (status, revive) = {
                    let mut plane =
                        minix_driver_rt::plane::Plane::new(transport, msg.m_source, grant);
                    if matches!(request, CdevRequest::Write) {
                        self.write(minor, size, parked, &mut plane)
                    } else {
                        self.read(minor, size, parked, &mut plane)
                    }
                };
                // A finished transfer answers its own caller with the
                // byte count (`chardriver_reply_task`).
                if let Some(revive) = revive {
                    self.reply(
                        transport,
                        minix_types::Endpoint(revive.parked.caller),
                        revive.bytes,
                        revive.parked.id,
                    );
                }
                // The request's own answer is deferred (`EDONTREPLY`):
                // C sends nothing here.
                if status != Self::DONT_REPLY {
                    self.reply(transport, msg.m_source, status, id);
                }
            }
            CdevRequest::Ioctl => {
                let minor = read_i32(cwire::readwrite::MINOR);
                let id = read_i32(cwire::readwrite::ID);
                let grant = read_i32(cwire::readwrite::GRANT);
                let request = read_u64(cwire::readwrite::REQUEST);
                let status = self.ioctl(transport, minor, request, grant, msg.m_source);
                self.reply(transport, msg.m_source, status, id);
            }
            CdevRequest::Select | CdevRequest::Cancel => {}
        }
    }

    /// Answer one request with the character general reply.
    fn reply<T: minix_driver_rt::transport::DriverTransport>(
        &mut self,
        transport: &mut T,
        caller: minix_types::Endpoint,
        status: i32,
        id: i32,
    ) {
        let mut reply = minix_types::Message {
            m_type: minix_types::CDEV_REPLY_BASE,
            ..minix_types::Message::default()
        };
        let base = ::core::ptr::addr_of_mut!(reply) as *mut u8;
        // SAFETY: the reply's documented status and identifier slots.
        unsafe {
            ::core::ptr::copy_nonoverlapping(
                status.to_le_bytes().as_ptr(),
                base.add(cwire::reply::STATUS),
                4,
            );
            ::core::ptr::copy_nonoverlapping(
                id.to_le_bytes().as_ptr(),
                base.add(cwire::reply::ID),
                4,
            );
        }
        //[SD19-gate] 回复发送尽力而为：C 的 send 失败即 panic；单线程泵按文档丢弃而不是卡死。
        #[allow(clippy::let_underscore_must_use)]
        let _ = transport.asynsend(caller, &mut reply);
    }
}

/// The character-request wire offsets, keyed per payload struct.
///
/// C: `mess_vfs_lchardriver_{openclose,readwrite}` and the reply
/// (`ipc.h:2228-2249`, `:943-948`). The union payload begins at byte 8 of
/// [`minix_types::Message`], so every offset is `8 +` the field's position
/// inside its C struct — the same pinned convention the log, memory, and
/// tty pumps use (no typed character arm exists in `MessageUnion` yet).
///
/// Only the lanes this framework reads are listed. C's own paths ignore
/// the rest of the two structs: `msg_open` takes the minor alone (the
/// `access`/`user` fields go to the generic chardriver adapter, not the
/// audio framework), and `msg_write`/`msg_read` mark `position` and
/// `flags` as unused (`audio_fw.c:394`, `:444`).
mod cwire {
    /// The message union payload (after `m_source` and `m_type`).
    const PAYLOAD: usize = 8;

    /// `mess_vfs_lchardriver_openclose` (`ipc.h:2228-2235`): `id` and
    /// `user` first, then the minor.
    pub mod openclose {
        use super::PAYLOAD;
        /// `endpoint_t id` — the request identifier.
        pub const ID: usize = PAYLOAD;
        /// `devminor_t minor`.
        pub const MINOR: usize = PAYLOAD + 8;
    }

    /// `mess_vfs_lchardriver_readwrite` (`ipc.h:2238-2249`): read, write,
    /// and ioctl share this one struct.
    pub mod readwrite {
        use super::PAYLOAD;
        /// `cp_grant_id_t grant`.
        pub const GRANT: usize = PAYLOAD + 8;
        /// `size_t count`.
        pub const COUNT: usize = PAYLOAD + 16;
        /// `unsigned long request` (the ioctl word).
        pub const REQUEST: usize = PAYLOAD + 24;
        /// `endpoint_t id`.
        pub const ID: usize = PAYLOAD + 36;
        /// `devminor_t minor`.
        pub const MINOR: usize = PAYLOAD + 44;
    }

    /// `mess_lchardriver_vfs_reply { int status; uint32_t id; }`
    /// (`ipc.h:943-948`).
    pub mod reply {
        use super::PAYLOAD;
        /// `int status` — the payload's first word.
        pub const STATUS: usize = PAYLOAD;
        /// `uint32_t id`.
        pub const ID: usize = PAYLOAD + 4;
    }
}

/// The playback interrupt bit the test card reports (`INTR_STS_DAC`).
#[cfg(test)]
const INTR_STS_DAC: i32 = 0x80;
/// The capture interrupt bit (`INTR_STS_ADC`).
#[cfg(test)]
const INTR_STS_ADC: i32 = 0x40;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dma::{DmaBuffer, dma_bytes_left};
    use alloc::boxed::Box;
    use alloc::vec;
    use minix_types::ENOSYS;
    use alloc::vec::Vec;
    use minix_driver_rt::plane::TransferPlane;
    use minix_types::Endpoint;

    /// A card whose DMA programming and interrupt state are recorded.
    struct FakeCard {
        frag: u32,
        set_dma: Vec<(u32, u32, i32)>,
        stops: Vec<i32>,
        pauses: Vec<i32>,
        resumes: Vec<i32>,
        /// The interrupt summary the next hardware notify reports.
        intr_status: i32,
    }

    impl FakeCard {
        fn new() -> Self {
            FakeCard {
                frag: 1024,
                set_dma: Vec::new(),
                stops: Vec::new(),
                pauses: Vec::new(),
                resumes: Vec::new(),
                intr_status: 0,
            }
        }
    }

    impl AudioHooks for FakeCard {
        fn init(&mut self) -> i32 {
            2
        }
        fn start(&mut self, _sub_dev: i32, _dma_mode: i32) -> i32 {
            0
        }
        fn pause(&mut self, chan: i32) -> i32 {
            self.pauses.push(chan);
            0
        }
        fn resume(&mut self, chan: i32) -> i32 {
            self.resumes.push(chan);
            0
        }
        fn int_sum(&mut self) -> i32 {
            self.intr_status
        }
        fn interrupt(&mut self, chan: i32) -> i32 {
            match chan {
                0 => self.intr_status & INTR_STS_DAC,
                1 => self.intr_status & INTR_STS_ADC,
                _ => 0,
            }
        }
        fn stop(&mut self, sub_dev: i32) -> i32 {
            self.stops.push(sub_dev);
            0
        }
        fn set_dma(&mut self, dma: u32, length: u32, chan: i32) -> i32 {
            self.set_dma.push((dma, length, chan));
            0
        }
        fn get_frag_size(&self, _sub_dev: i32) -> u32 {
            self.frag
        }
    }

    /// An allocator that hands out a **real** buffer, the way the
    /// production allocator hands out a mapped one: the data plane copies
    /// through the returned address, so a synthetic one would fault. The
    /// buffer is leaked to keep it alive for the test's lifetime, and the
    /// physical address stays synthetic.
    struct FakeAlloc {
        next: u64,
    }

    impl DmaAlloc for FakeAlloc {
        fn alloc_contig(&mut self, size: u64) -> Result<DmaBuffer, i32> {
            let physical = self.next;
            self.next += 0x10_0000;
            let buffer = vec![0u8; size as usize].into_boxed_slice();
            let address = Box::leak(buffer).as_mut_ptr() as u64;
            Ok(DmaBuffer {
                address,
                physical,
                size,
            })
        }
    }

    fn service() -> AudioService<FakeCard, FakeAlloc> {
        let files = vec![SpecialFile {
            minor: 0,
            read_chan: 1,
            write_chan: 0,
            ioctl_chan: 0,
        }];
        let geometry = [(DmaMode::Write, 4, 1024, 2), (DmaMode::Read, 4, 1024, 2)];
        AudioService::new(
            FakeCard::new(),
            FakeAlloc { next: 0x20_1000 },
            files,
            &geometry,
        )
    }

    #[test]
    fn test_open_brings_up_the_dma_and_programs_the_card() {
        let mut svc = service();
        assert_eq!(svc.init(), 0);
        assert_eq!(svc.open(0), OK);
        // Two channels were programmed, each with its ring.
        assert_eq!(svc.hooks().set_dma.len(), 2);
        let (dma, length, chan) = svc.hooks().set_dma[0];
        assert_eq!(chan, 0);
        assert_eq!(length, 4096, "the ring is the whole DMA size");
        assert!(
            dma_bytes_left(dma as u64) >= 4096,
            "the ring fits before the boundary"
        );
        assert_eq!(svc.ring(0).unwrap().size, 4096);
        // A second open while the first is live is refused.
        assert_eq!(svc.open(0), EBUSY);
        // An unknown minor is refused.
        assert_eq!(svc.open(9), EIO);
    }

    #[test]
    fn test_close_stops_the_card_and_frees_the_ring() {
        let mut svc = service();
        svc.init();
        svc.open(0);
        assert_eq!(svc.close(0), OK);
        assert_eq!(svc.hooks().stops, vec![0, 1], "both channels stopped");
        assert!(svc.ring(0).is_none(), "the ring was released");
        assert_eq!(svc.close(9), EIO);
    }

    /// A plane over two scratch buffers: what the caller lent, and what
    /// the driver hands back.
    struct FakePlane {
        from: Vec<u8>,
        to: Vec<u8>,
        copies_in: usize,
        copies_out: usize,
    }

    impl FakePlane {
        fn new() -> Self {
            FakePlane {
                from: alloc::vec![0x5a; 4096],
                to: Vec::new(),
                copies_in: 0,
                copies_out: 0,
            }
        }
    }

    impl minix_driver_rt::plane::GrantIpc for FakePlane {
        fn grant_direct(
            &mut self,
            _dest: Endpoint,
            _address: u64,
            _len: usize,
            _write: bool,
        ) -> Result<i32, i32> {
            Ok(1)
        }
        fn grant_revoke(&mut self, _grant: i32) -> Result<(), i32> {
            Ok(())
        }
        fn send_rec(&mut self, _dst: Endpoint, _msg: &mut minix_types::Message) -> Result<(), i32> {
            Ok(())
        }
    }

    impl TransferPlane for FakePlane {
        fn caller(&self) -> Endpoint {
            Endpoint(20)
        }
        fn copy_from_caller(&mut self, _offset: u64, buf: &mut [u8]) -> Result<(), i32> {
            self.copies_in += 1;
            let n = buf.len().min(self.from.len());
            buf[..n].copy_from_slice(&self.from[..n]);
            Ok(())
        }
        fn copy_to_caller(&mut self, _offset: u64, buf: &[u8]) -> Result<(), i32> {
            self.copies_out += 1;
            self.to.extend_from_slice(buf);
            Ok(())
        }
    }

    fn parked() -> Parked {
        Parked {
            id: 7,
            grant: 5,
            caller: 20,
        }
    }

    #[test]
    fn test_the_transfer_gates_match_c() {
        let mut svc = service();
        let mut plane = FakePlane::new();
        svc.init();
        svc.open(0);
        // The fragment size must match exactly.
        assert_eq!(svc.write(0, 100, parked(), &mut plane).0, EINVAL);
        // A correct-sized write is copied and answered with the fragment
        // size; the request itself is not replied to (`EDONTREPLY`).
        let (status, revive) = svc.write(0, 1024, parked(), &mut plane);
        assert_eq!(status, EDONTREPLY);
        assert_eq!(
            revive,
            Some(Revive {
                parked: parked(),
                bytes: 1024
            })
        );
        assert_eq!(plane.copies_in, 1, "one fragment was pulled");
        assert_eq!(svc.parked(0), None, "the revive record was cleared");
        // The write started the card (`get_started`).
        assert!(svc.channels[0].dev.is_dma_busy());
        assert_eq!(svc.channels[0].mode, DmaMode::Write);
        // An unknown minor is refused.
        assert_eq!(svc.write(9, 1024, parked(), &mut plane).0, EIO);
        assert_eq!(AudioService::<FakeCard, FakeAlloc>::DONT_REPLY, EDONTREPLY);
    }

    #[test]
    fn test_a_second_write_waits_in_the_extra_buffer() {
        let mut svc = service();
        let mut plane = FakePlane::new();
        svc.init();
        svc.open(0);
        // The ring holds four fragments; the two after that land in the
        // extra buffer.
        for _ in 0..4 {
            let (_, revive) = svc.write(0, 1024, parked(), &mut plane);
            assert!(revive.is_some());
        }
        assert_eq!(svc.channels[0].dev.dma_length(), 4, "the ring is full");
        assert_eq!(svc.channels[0].dev.buf_length(), 0);
        let (_, revive) = svc.write(0, 1024, parked(), &mut plane);
        assert!(revive.is_some(), "the extra buffer took it");
        assert_eq!(svc.channels[0].dev.buf_length(), 1);
        assert_eq!(svc.channels[0].dev.dma_length(), 4);
        // One more fills the last extra buffer; a third finds no room and
        // stays parked with no revive.
        assert!(svc.write(0, 1024, parked(), &mut plane).1.is_some());
        assert_eq!(svc.channels[0].dev.buf_length(), 2);
        let (status, revive) = svc.write(0, 1024, parked(), &mut plane);
        assert_eq!(status, EDONTREPLY);
        assert!(
            revive.is_none(),
            "no room: the caller waits for an interrupt"
        );
        assert_eq!(svc.parked(0), Some(parked()));
    }

    #[test]
    fn test_a_playback_interrupt_drains_the_extra_buffer_and_revives() {
        let mut svc = service();
        let mut plane = FakePlane::new();
        svc.init();
        svc.open(0);
        // Six fragments fill the ring and the extra buffer; each write
        // serves its own caller, so the seventh has to wait.
        for _ in 0..6 {
            let (_, revive) = svc.write(0, 1024, parked(), &mut plane);
            assert!(revive.is_some(), "there was room for each of the six");
        }
        assert_eq!(svc.channels[0].dev.buf_length(), 2);
        let (status, revive) = svc.write(0, 1024, parked(), &mut plane);
        assert_eq!(status, EDONTREPLY);
        assert!(revive.is_none(), "no room: the caller waits");
        assert_eq!(svc.parked(0), Some(parked()));
        // The card consumed a fragment and the summary says playback.
        svc.hooks.intr_status = INTR_STS_DAC;
        let revived = svc.handle_hardware(&mut plane);
        assert_eq!(revived.len(), 1, "the waiting caller was served");
        assert_eq!(revived[0].bytes, 1024);
        // The extra buffer moved into the ring and the waiting fragment
        // filled the freed extra slot.
        assert_eq!(svc.channels[0].dev.dma_length(), 4);
        assert_eq!(svc.channels[0].dev.buf_length(), 2);
        assert_eq!(svc.parked(0), None);
    }

    #[test]
    fn test_a_dry_ring_pauses_the_card_and_a_late_write_resumes_it() {
        let mut svc = service();
        let mut plane = FakePlane::new();
        svc.init();
        svc.open(0);
        // One fragment in the ring; the card consumes it.
        let _ = svc.write(0, 1024, parked(), &mut plane);
        svc.hooks.intr_status = INTR_STS_DAC;
        let revived = svc.handle_hardware(&mut plane);
        assert!(revived.is_empty(), "nobody was waiting");
        assert_eq!(svc.channels[0].dev.dma_length(), 0);
        assert!(svc.channels[0].dev.is_out_of_data());
        assert_eq!(svc.hooks.pauses, vec![0], "the card was paused");
        // A later write finds the device dry and resumes it.
        let (_, revive) = svc.write(0, 1024, parked(), &mut plane);
        assert!(revive.is_some());
        assert!(!svc.channels[0].dev.is_out_of_data());
        // Two resumes: the open arms the dry flag, so the very first
        // write already resumed the card.
        assert_eq!(svc.hooks.resumes, vec![0, 0]);
    }

    #[test]
    fn test_a_capture_interrupt_hands_a_fragment_to_the_waiting_reader() {
        let mut svc = service();
        let mut plane = FakePlane::new();
        svc.init();
        svc.open(0);
        // The first read starts the card and parks without data (minor
        // zero's read channel is the capture channel).
        let (status, revive) = svc.read(0, 1024, parked(), &mut plane);
        assert_eq!(status, EDONTREPLY);
        assert!(revive.is_none(), "nothing to hand over yet");
        assert!(svc.channels[1].dev.is_dma_busy());
        // The card fills a fragment and the summary says capture.
        svc.hooks.intr_status = INTR_STS_ADC;
        let revived = svc.handle_hardware(&mut plane);
        assert_eq!(revived.len(), 1);
        assert_eq!(revived[0].bytes, 1024);
        assert_eq!(plane.copies_out, 1, "one fragment went to the caller");
        // The fragment left the ring again with the caller's copy.
        assert_eq!(svc.channels[1].dev.dma_length(), 0);
    }

    #[test]
    fn test_a_full_capture_ring_parks_the_caller_until_it_drains() {
        let mut svc = service();
        let mut plane = FakePlane::new();
        svc.init();
        svc.open(0);
        // Start the card and fill the ring: the first fragment revives the
        // waiting caller, the following ones stay in the ring.
        let _ = svc.read(0, 1024, parked(), &mut plane);
        svc.hooks.intr_status = INTR_STS_ADC;
        for _ in 0..4 {
            let _ = svc.handle_hardware(&mut plane);
        }
        // The first interrupt served the waiting reader (its fragment
        // left the ring again); the other three stayed.
        assert_eq!(svc.channels[1].dev.dma_length(), 3);
        // A new reader is parked; the next interrupt finds the ring full
        // and the extra buffer empty, so the fragment moves there and the
        // reader is served from it.
        let (_, revive) = svc.read(0, 1024, parked(), &mut plane);
        assert!(revive.is_some(), "the ring had data to hand over");
        assert_eq!(plane.copies_out, 2);
    }

    #[test]
    fn test_a_control_only_channel_skips_the_dma_bring_up() {
        let files = vec![SpecialFile {
            minor: 2,
            read_chan: NO_CHANNEL,
            write_chan: NO_CHANNEL,
            ioctl_chan: 0,
        }];
        // A channel with no fragment geometry (C's `NO_DMA` sub-device).
        let geometry = [(DmaMode::Write, 0, 0, 0)];
        let mut svc = AudioService::new(
            FakeCard::new(),
            FakeAlloc { next: 0x1000 },
            files,
            &geometry,
        );
        svc.init();
        assert_eq!(svc.open(2), OK);
        assert!(svc.hooks().set_dma.is_empty(), "nothing programmed");
        assert!(svc.ring(0).is_none());
        assert_eq!(svc.close(2), OK);
    }

    #[test]
    fn test_a_failing_allocator_refuses_the_open() {
        struct Broken;
        impl DmaAlloc for Broken {
            fn alloc_contig(&mut self, _size: u64) -> Result<DmaBuffer, i32> {
                Err(-minix_types::ENOSYS)
            }
        }
        let files = vec![SpecialFile {
            minor: 0,
            read_chan: NO_CHANNEL,
            write_chan: 0,
            ioctl_chan: 0,
        }];
        let geometry = [(DmaMode::Write, 4, 1024, 2)];
        let mut svc = AudioService::new(FakeCard::new(), Broken, files, &geometry);
        svc.init();
        assert_eq!(svc.open(0), -(ENOSYS), "no buffer, no open");
        assert!(svc.hooks().set_dma.is_empty(), "nothing programmed");
    }

    /// A card that answers ioctls: records each call and reports a fixed
    /// value on the output requests.
    struct IoctlCard {
        calls: Vec<(u64, i32, i32)>,
        answer: u32,
        frag: u32,
    }

    impl AudioHooks for IoctlCard {
        fn init(&mut self) -> i32 {
            1
        }
        fn get_frag_size(&self, _sub_dev: i32) -> u32 {
            self.frag
        }
        fn io_ctl(&mut self, request: u64, val: &mut [u8], len: &mut i32, sub_dev: i32) -> i32 {
            self.calls.push((request, *len, sub_dev));
            if request == crate::sound::DSPIOMAX {
                val[..4].copy_from_slice(&self.answer.to_le_bytes());
                *len = 4;
            }
            if request == crate::sound::DSPIOSIZE {
                self.frag = u32::from_le_bytes(val[..4].try_into().unwrap());
            }
            OK
        }
    }

    /// A transport that serves one grant's bytes and records the copies
    /// and the replies.
    struct FakeIpc {
        from: Vec<u8>,
        to: Vec<u8>,
        copies: Vec<(Endpoint, i32, u64, usize)>,
        replies: Vec<(Endpoint, i32, i32)>,
    }

    impl FakeIpc {
        fn new(from: Vec<u8>) -> Self {
            FakeIpc {
                from,
                to: Vec::new(),
                copies: Vec::new(),
                replies: Vec::new(),
            }
        }
    }

    impl minix_driver_rt::transport::DriverTransport for FakeIpc {
        fn receive(&mut self, _msg: &mut minix_types::Message) -> Result<(), i32> {
            Err(-minix_types::EIO)
        }
        fn send(&mut self, _dst: Endpoint, _msg: &mut minix_types::Message) -> Result<(), i32> {
            Ok(())
        }
        fn send_rec(&mut self, _dst: Endpoint, _msg: &mut minix_types::Message) -> Result<(), i32> {
            Ok(())
        }
        fn asynsend(&mut self, dst: Endpoint, msg: &mut minix_types::Message) -> Result<(), i32> {
            let base = ::core::ptr::addr_of!(*msg) as *const u8;
            let mut lanes = [0i32; 2];
            // SAFETY: the reply's documented status and identifier slots.
            unsafe {
                for (index, offset) in [cwire::reply::STATUS, cwire::reply::ID].iter().enumerate() {
                    let mut bytes = [0u8; 4];
                    ::core::ptr::copy_nonoverlapping(base.add(*offset), bytes.as_mut_ptr(), 4);
                    lanes[index] = i32::from_le_bytes(bytes);
                }
            }
            self.replies.push((dst, lanes[0], lanes[1]));
            Ok(())
        }
        fn copy_from_grant(
            &mut self,
            granter: Endpoint,
            grant: i32,
            offset: u64,
            buf: &mut [u8],
        ) -> Result<(), i32> {
            self.copies.push((granter, grant, offset, buf.len()));
            if self.from.len() < buf.len() {
                return Err(-minix_types::EFAULT);
            }
            buf.copy_from_slice(&self.from[..buf.len()]);
            Ok(())
        }
        fn copy_to_grant(
            &mut self,
            granter: Endpoint,
            grant: i32,
            offset: u64,
            buf: &[u8],
        ) -> Result<(), i32> {
            self.copies.push((granter, grant, offset, buf.len()));
            self.to.extend_from_slice(buf);
            Ok(())
        }
        fn publish_label(&mut self, _label: &str) -> Result<(), i32> {
            Ok(())
        }
        fn grant_direct(
            &mut self,
            _dest: Endpoint,
            _address: u64,
            _len: usize,
            _write: bool,
        ) -> Result<i32, i32> {
            Ok(1)
        }
        fn grant_revoke(&mut self, _grant: i32) -> Result<(), i32> {
            Ok(())
        }
    }

    fn ioctl_service(frag: u32) -> AudioService<IoctlCard, FakeAlloc> {
        let files = vec![SpecialFile {
            minor: 0,
            read_chan: NO_CHANNEL,
            write_chan: 0,
            ioctl_chan: 0,
        }];
        let geometry = [(DmaMode::Write, 2, 32 * 1024, 4)];
        let card = IoctlCard {
            calls: Vec::new(),
            answer: 32 * 1024,
            frag,
        };
        AudioService::new(card, FakeAlloc { next: 0x1000 }, files, &geometry)
    }

    #[test]
    fn test_ioctl_resolves_the_channel_and_gates_on_open() {
        let mut svc = ioctl_service(1024);
        let mut ipc = FakeIpc::new(Vec::new());
        svc.init();
        // Nobody opened the node yet (C: "io control impossible - not
        // opened!").
        assert_eq!(
            svc.ioctl(&mut ipc, 0, crate::sound::DSPIORESET, 3, Endpoint(20)),
            EIO
        );
        // An unknown minor has no special file.
        svc.open(0);
        assert_eq!(
            svc.ioctl(&mut ipc, 9, crate::sound::DSPIORESET, 3, Endpoint(20)),
            EIO
        );
        assert!(svc.hooks().calls.is_empty(), "the card was never called");
    }

    #[test]
    fn test_ioctl_carries_the_payload_both_ways() {
        let mut svc = ioctl_service(1024);
        svc.init();
        svc.open(0);
        // A set request copies four bytes in and nothing out.
        let mut ipc = FakeIpc::new(vec![0x40, 0x1f, 0x00, 0x00]);
        assert_eq!(
            svc.ioctl(&mut ipc, 0, crate::sound::DSPIOSIZE, 3, Endpoint(20)),
            OK
        );
        assert_eq!(ipc.copies, vec![(Endpoint(20), 3, 0, 4)]);
        assert!(
            ipc.to.is_empty(),
            "an input-only request copies nothing back"
        );
        assert_eq!(svc.hooks().calls[0], (crate::sound::DSPIOSIZE, 4, 0));
        // The card stored the fragment size, so the transfer gate now
        // compares against it (C re-reads `drv_get_frag_size` per request).
        let parked = Parked {
            id: 1,
            grant: 2,
            caller: 20,
        };
        let mut plane = FakePlane::new();
        assert_eq!(svc.write(0, 8192, parked, &mut plane).0, EINVAL);
        // The size the card stored is accepted, and the transfer runs.
        assert_eq!(svc.write(0, 0x1f40, parked, &mut plane).0, EDONTREPLY);
        // A get request copies the hook's answer back to the caller.
        let mut ipc = FakeIpc::new(Vec::new());
        assert_eq!(
            svc.ioctl(&mut ipc, 0, crate::sound::DSPIOMAX, 4, Endpoint(20)),
            OK
        );
        assert_eq!(ipc.to, vec![0x00, 0x80, 0x00, 0x00], "32 KiB little-endian");
        assert_eq!(ipc.copies, vec![(Endpoint(20), 4, 0, 4)]);
    }

    #[test]
    fn test_dispatch_reads_the_c_lanes() {
        let mut svc = ioctl_service(1024);
        let mut ipc = FakeIpc::new(Vec::new());
        svc.init();
        // An open for minor 0: `mess_vfs_lchardriver_openclose` has `id`
        // first, `minor` at union plus eight (`ipc.h:2228-2235`).
        let mut msg = minix_types::Message {
            m_type: minix_chardriver::protocol::CdevRequest::Open.message_type(),
            m_source: Endpoint(20),
            ..minix_types::Message::default()
        };
        write_lane(&mut msg, cwire::openclose::ID, 0x77);
        write_lane(&mut msg, cwire::openclose::MINOR, 0);
        svc.dispatch(&mut ipc, &msg);
        assert_eq!(ipc.replies, vec![(Endpoint(20), OK, 0x77)]);
        assert!(svc.ring(0).is_some(), "the open reached the DMA bring-up");
        // An ioctl for minor 0: `mess_vfs_lchardriver_readwrite` puts the
        // grant at union plus eight and the request word at plus 24.
        let mut msg = minix_types::Message {
            m_type: minix_chardriver::protocol::CdevRequest::Ioctl.message_type(),
            m_source: Endpoint(20),
            ..minix_types::Message::default()
        };
        write_lane(&mut msg, cwire::readwrite::MINOR, 0);
        write_lane(&mut msg, cwire::readwrite::ID, 0x88);
        write_lane(&mut msg, cwire::readwrite::GRANT, 9);
        write_lane_u64(
            &mut msg,
            cwire::readwrite::REQUEST,
            crate::sound::DSPIORESET,
        );
        svc.dispatch(&mut ipc, &msg);
        assert_eq!(ipc.replies[1], (Endpoint(20), OK, 0x88));
        assert_eq!(
            svc.hooks().calls.last().copied(),
            Some((crate::sound::DSPIORESET, 0, 0))
        );
        // A read for an unknown minor answers EIO through the same lanes.
        let mut msg = minix_types::Message {
            m_type: minix_chardriver::protocol::CdevRequest::Read.message_type(),
            m_source: Endpoint(20),
            ..minix_types::Message::default()
        };
        write_lane(&mut msg, cwire::readwrite::MINOR, 5);
        write_lane(&mut msg, cwire::readwrite::ID, 0x99);
        svc.dispatch(&mut ipc, &msg);
        assert_eq!(ipc.replies[2], (Endpoint(20), EIO, 0x99));
    }

    /// The lane table pinned against `ipc.h`'s struct declarations. The
    /// two payload structs do not share a shape (`openclose` starts with
    /// `id`, `readwrite` with `off_t pos`), so a lane read from the wrong
    /// struct lands on the wrong field — this test is what keeps the
    /// dispatch above on the right one.
    #[test]
    fn test_the_lane_table_matches_ipc_h() {
        // The union payload starts after `m_source` and `m_type`.
        assert_eq!(core::mem::offset_of!(minix_types::Message, m_u), 8);
        // `mess_vfs_lchardriver_openclose`: id@0, user@4, minor@8, access@12.
        assert_eq!(cwire::openclose::ID, 8);
        assert_eq!(cwire::openclose::MINOR, 16);
        // `mess_vfs_lchardriver_readwrite`: pos@0, grant@8, count@16,
        // request@24, flags@32, id@36, user@40, minor@44.
        assert_eq!(cwire::readwrite::GRANT, 16);
        assert_eq!(cwire::readwrite::COUNT, 24);
        assert_eq!(cwire::readwrite::REQUEST, 32);
        assert_eq!(cwire::readwrite::ID, 44);
        assert_eq!(cwire::readwrite::MINOR, 52);
        // `mess_lchardriver_vfs_reply { status, id }`.
        assert_eq!(cwire::reply::STATUS, 8);
        assert_eq!(cwire::reply::ID, 12);
        // The same lanes `minix-types` publishes for the file-system side
        // (union-relative there, absolute here).
        use minix_types::{lchardriver_openclose_off as oc, lchardriver_readwrite_off as rw};
        assert_eq!(cwire::openclose::MINOR, 8 + oc::MINOR);
        assert_eq!(cwire::openclose::ID, 8 + oc::ID);
        assert_eq!(cwire::readwrite::GRANT, 8 + rw::GRANT);
        assert_eq!(cwire::readwrite::COUNT, 8 + rw::COUNT);
        assert_eq!(cwire::readwrite::REQUEST, 8 + rw::REQUEST);
        assert_eq!(cwire::readwrite::ID, 8 + rw::ID);
        assert_eq!(cwire::readwrite::MINOR, 8 + rw::MINOR);
    }

    fn write_lane(msg: &mut minix_types::Message, offset: usize, value: i32) {
        let base = ::core::ptr::addr_of_mut!(*msg) as *mut u8;
        // SAFETY: the request's documented lanes.
        unsafe {
            ::core::ptr::copy_nonoverlapping(value.to_le_bytes().as_ptr(), base.add(offset), 4)
        };
    }

    fn write_lane_u64(msg: &mut minix_types::Message, offset: usize, value: u64) {
        let base = ::core::ptr::addr_of_mut!(*msg) as *mut u8;
        // SAFETY: the request's documented lanes.
        unsafe {
            ::core::ptr::copy_nonoverlapping(value.to_le_bytes().as_ptr(), base.add(offset), 8)
        };
    }
}
