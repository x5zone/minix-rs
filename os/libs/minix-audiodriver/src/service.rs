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
//! size gate, the direction gate, the revive record, and the
//! "no reply yet" answer), and `init_buffers` (`audio_fw.c:753-810`) as
//! [`crate::dma`] models it.
//!
//! Registered seam: the fragment data plane (`data_from_user`,
//! `data_to_user`, `get_started`, and the two interrupt handlers,
//! `audio_fw.c:500-752`) is not modeled yet, so a read or write is
//! refused with `ENOSYS` rather than parking the caller on a reply that
//! would never come (C answers `EDONTREPLY` and revives the caller from
//! the interrupt path). The open/close and DMA bring-up paths are real.

use alloc::vec::Vec;

use minix_types::{EBUSY, EDONTREPLY, EINVAL, EIO, ENOSYS, OK};

use crate::dma::{DmaAlloc, RingPlan, plan_ring};
use crate::hooks::AudioHooks;
use crate::special::{NO_CHANNEL, SpecialFile, get_special_file};
use crate::subdev::{DmaMode, SubDevice};

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
    frag_size: Vec<u32>,
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
            frag_size: geometry.iter().map(|entry| entry.2).collect(),
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
        // (`audio_fw.c:271-277`).
        let dma_size = u64::from(fragments) * u64::from(frag_size);
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

    /// A write request's gates (`msg_write`, `audio_fw.c:394-442`).
    ///
    /// The data plane that would follow the gates (the fragment copy, the
    /// DMA start, and the deferred reply) is the registered seam, so a
    /// request that passes the gates is refused with `ENOSYS` rather than
    /// parked.
    pub fn write(&mut self, minor: i32, size: u64, parked: Parked) -> i32 {
        self.transfer_gates(minor, size, parked, true)
    }

    /// A read request's gates (`msg_read`, `audio_fw.c:444-495`).
    pub fn read(&mut self, minor: i32, size: u64, parked: Parked) -> i32 {
        self.transfer_gates(minor, size, parked, false)
    }

    fn transfer_gates(&mut self, minor: i32, size: u64, parked: Parked, do_write: bool) -> i32 {
        let Some(file) = get_special_file(&self.files, minor).copied() else {
            return EIO;
        };
        let chan = if do_write {
            file.write_chan
        } else {
            file.read_chan
        };
        if chan == NO_CHANNEL {
            return EIO;
        }
        let chan = chan as usize;
        let channel = &mut self.channels[chan];
        let frag_size = self.frag_size[chan] as u64;
        if size != frag_size {
            return EINVAL;
        }
        let wanted = if do_write {
            DmaMode::Write
        } else {
            DmaMode::Read
        };
        if channel.dev.is_dma_busy() && channel.mode != wanted {
            return EBUSY;
        }
        // C records the caller and answers EDONTREPLY: the reply comes
        // from the interrupt path. That path is the registered seam.
        channel.parked = Some(parked);
        -(ENOSYS)
    }

    /// The caller parked on a channel, if any (`RevivePending`).
    pub fn parked(&self, chan: usize) -> Option<Parked> {
        self.channels[chan].parked
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
            // The hardware notify drives the interrupt path, which is the
            // registered data-plane seam; nothing to do yet.
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
        let minor = read_i32(PAYLOAD_MINOR);
        let id = read_i32(PAYLOAD_ID);
        let status = match request {
            CdevRequest::Open => self.open(minor),
            CdevRequest::Close => self.close(minor),
            CdevRequest::Read | CdevRequest::Write => {
                let grant = read_i32(PAYLOAD_GRANT);
                let size = read_i32(PAYLOAD_SIZE) as u64;
                let parked = Parked {
                    id,
                    grant,
                    caller: msg.m_source.0,
                };
                if matches!(request, CdevRequest::Write) {
                    self.write(minor, size, parked)
                } else {
                    self.read(minor, size, parked)
                }
            }
            CdevRequest::Ioctl => minix_types::ENOTTY,
            CdevRequest::Select | CdevRequest::Cancel => return,
        };
        self.reply(transport, msg.m_source, status, id);
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
                base.add(PAYLOAD_STATUS),
                4,
            );
            ::core::ptr::copy_nonoverlapping(id.to_le_bytes().as_ptr(), base.add(PAYLOAD_ID), 4);
        }
        //[SD19-gate] 回复发送尽力而为：C 的 send 失败即 panic；单线程泵按文档丢弃而不是卡死。
        #[allow(clippy::let_underscore_must_use)]
        let _ = transport.asynsend(caller, &mut reply);
    }
}

/// The character request payload offsets (the same lanes every character
/// driver's pump reads).
const PAYLOAD_MINOR: usize = 8;
/// The request identifier lane.
const PAYLOAD_ID: usize = 44;
/// The grant lane.
const PAYLOAD_GRANT: usize = 16;
/// The size lane.
const PAYLOAD_SIZE: usize = 20;
/// The reply's status lane.
const PAYLOAD_STATUS: usize = 8;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dma::{DmaBuffer, dma_bytes_left};
    use alloc::vec;

    /// A card whose DMA programming is recorded.
    struct FakeCard {
        frag: u32,
        set_dma: Vec<(u32, u32, i32)>,
        stops: Vec<i32>,
    }

    impl FakeCard {
        fn new() -> Self {
            FakeCard {
                frag: 1024,
                set_dma: Vec::new(),
                stops: Vec::new(),
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

    /// An allocator over a fixed physical block.
    struct FakeAlloc {
        next: u64,
    }

    impl DmaAlloc for FakeAlloc {
        fn alloc_contig(&mut self, size: u64) -> Result<DmaBuffer, i32> {
            let physical = self.next;
            self.next += 0x10_0000;
            Ok(DmaBuffer {
                address: 0x1000_0000 + physical,
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

    #[test]
    fn test_the_transfer_gates_match_c() {
        let mut svc = service();
        svc.init();
        svc.open(0);
        let parked = Parked {
            id: 7,
            grant: 5,
            caller: 20,
        };
        // The fragment size must match exactly.
        assert_eq!(svc.write(0, 100, parked), EINVAL);
        // A correct-sized write passes the gates and is refused by the
        // data-plane seam (never parked on a reply that cannot come).
        assert_eq!(svc.write(0, 1024, parked), -(ENOSYS));
        assert_eq!(svc.parked(0), Some(parked));
        assert_eq!(svc.read(0, 1024, parked), -(ENOSYS));
        // A write channel serving a read request is refused (no read
        // channel on this file would answer EIO; here channel 1 exists).
        assert_eq!(svc.write(9, 1024, parked), EIO);
        assert_eq!(AudioService::<FakeCard, FakeAlloc>::DONT_REPLY, EDONTREPLY);
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
}
