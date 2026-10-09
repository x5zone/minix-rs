//! ATA PIO data-movement + task-file register layer (U-6 阶段二 increment 1).
//!
//! C correspondence: `minix3/minix/drivers/storage/at_wini/at_wini.h`
//! (register offsets `:23-36`, status bits `:37-45`, LDH constants `:30-33`,
//! command opcodes `:56-72`, `MAX_SECS 256` `:186`) and `at_wini.c`
//! (`com_out` 7-byte register vector `:1220-1262`, LBA28 field split
//! `:917-921`, PIO transfer loop `:1128-1198`, `w_waitfor` alt-status poll).
//!
//! [`crate::controller`] owns the *order* (reset/probe/identify/ready) and the
//! *wait policy*; this module owns the *hardware mechanics* that the service
//! binary drives: reading/writing the task-file registers and moving a sector
//! in and out of the data register. Port access is abstracted behind [`IoBus`]
//! so the sequencing is testable on the host without a controller — the real
//! service binary maps [`IoBus`] onto `minix_sys::sys_inb`/`sys_outb`/
//! `sys_insw`/`sys_outsw`.
//!
//! This is the "ports and data movement stay in the service crate" side of the
//! [`crate::controller`] split. The live event loop, device probe, interrupt
//! handling, and on-hardware end-to-end read/write are later increments of the
//! same U-6 batch (see the ledger); nothing here is wired into `main.rs` yet.

/// Data register (offset from the command base). C: `REG_DATA` — at_wini.h:23.
pub const REG_DATA: u16 = 0;
/// Write-precompensation / LBA high byte. C: `REG_PRECOMP` — at_wini.h:24.
pub const REG_PRECOMP: u16 = 1;
/// Sectors to transfer. C: `REG_COUNT` — at_wini.h:25.
pub const REG_COUNT: u16 = 2;
/// Sector number / LBA bits 0-7. C: `REG_SECTOR` — at_wini.h:26.
pub const REG_SECTOR: u16 = 3;
/// Cylinder low / LBA bits 8-15. C: `REG_CYL_LO` — at_wini.h:27.
pub const REG_CYL_LO: u16 = 4;
/// Cylinder high / LBA bits 16-23. C: `REG_CYL_HI` — at_wini.h:28.
pub const REG_CYL_HI: u16 = 5;
/// Drive/LBA/head select. C: `REG_LDH` — at_wini.h:29.
pub const REG_LDH: u16 = 6;
/// Status (read) / command (write) register. C: `REG_STATUS`/`REG_COMMAND` — at_wini.h:36,55.
pub const REG_STATUS: u16 = 7;
/// Command register (write) — same offset as the status register. C: `REG_COMMAND` — at_wini.h:55.
pub const REG_COMMAND: u16 = 7;

/// `LDH_DEFAULT`: ECC on, 512 bytes/sector. C: at_wini.h:30.
pub const LDH_DEFAULT: u8 = 0xA0;
/// `LDH_LBA`: use LBA addressing. C: at_wini.h:31.
pub const LDH_LBA: u8 = 0x40;
/// `LDH_DEV`: drive 1 when set. C: at_wini.h:32.
pub const LDH_DEV: u8 = 0x10;

/// Alternate status offset from the control base (also the reset/nINT-disable
/// control register). C: `REG_CTL_ALTSTAT`/`REG_CTL` — at_wini.h:74,80.
pub const REG_CTL_ALTSTAT: u16 = 0;
/// `CTL_EIGHTHEADS`: >8-heads geometry bit. C: at_wini.h:77.
pub const CTL_EIGHTHEADS: u8 = 0x08;

/// Controller busy. C: `STATUS_BSY` — at_wini.h:37.
pub const STATUS_BSY: u8 = 0x80;
/// Drive ready. C: `STATUS_RDY` — at_wini.h:38.
pub const STATUS_RDY: u8 = 0x40;
/// Data request — the data register holds a sector. C: `STATUS_DRQ` — at_wini.h:41.
pub const STATUS_DRQ: u8 = 0x08;
/// Error — read the error register. C: `STATUS_ERR` — at_wini.h:44.
pub const STATUS_ERR: u8 = 0x01;

/// Read sector(s) (LBA28). C: `CMD_READ` — at_wini.h:58.
pub const CMD_READ: u8 = 0x20;
/// Write sector(s) (LBA28). C: `CMD_WRITE` — at_wini.h:61.
pub const CMD_WRITE: u8 = 0x30;
/// Identify drive. C: `ATA_IDENTIFY` — at_wini.h:72.
pub const ATA_IDENTIFY: u8 = 0xEC;

/// Words (16 bits) per 512-byte sector. C: `SECTOR_SIZE` 512 bytes = 256 words
/// moved per `sys_insw`/`sys_outsw` (at_wini.c:1156-1178).
pub const SECTOR_WORDS: usize = 256;

/// A task-file command block: the register values to push to the controller.
///
/// C: `struct command` subset used by `com_out` (at_wini.c:1253-1259). Only the
/// LBA28 / CHS fields the 28-bit `com_out` path writes; the LBA48 (`com_out_ext`)
/// path and its extra `*_prev` registers are a later increment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Command {
    /// Write-precomp / LBA24-31 low half. C: `cmd.precomp` (REG_PRECOMP).
    pub precomp: u8,
    /// Sector count (low byte). C: `cmd.count` (REG_COUNT).
    pub count: u8,
    /// LBA bits 0-7. C: `cmd.sector` (REG_SECTOR).
    pub sector: u8,
    /// LBA bits 8-15. C: `cmd.cyl_lo` (REG_CYL_LO).
    pub cyl_lo: u8,
    /// LBA bits 16-23. C: `cmd.cyl_hi` (REG_CYL_HI).
    pub cyl_hi: u8,
    /// Drive / LBA-enable byte. C: `cmd.ldh` (REG_LDH).
    pub ldh: u8,
    /// Opcode written last. C: `cmd.command` (REG_COMMAND).
    pub command: u8,
}

/// Port access the layer drives. The service binary implements this over the
/// kernel port-I/O syscalls; host tests inject a recording mock.
pub trait IoBus {
    /// Read one byte from `port`. C: `sys_inb` (at_wini.c:1074).
    fn inb(&mut self, port: u16) -> u8;
    /// Write one byte. C: `sys_outb` (at_wini.c:1236).
    fn outb(&mut self, port: u16, value: u8);
    /// Read `dst.len()` 16-bit words from `port`. C: `sys_insw` (at_wini.c:1162).
    fn in_words(&mut self, port: u16, dst: &mut [u16]);
    /// Write `src.len()` 16-bit words to `port`. C: `sys_outsw` (at_wini.c:1175).
    fn out_words(&mut self, port: u16, src: &[u16]);
}

/// Failure to reach a required register state within the poll budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PioError {
    /// `(status & mask) != value` after `max_polls` reads. C: `w_waitfor`
    /// returns 0 on the timeout alarm path (at_wini.c:1230-1242, 1150-1153).
    Timeout,
}

/// Build the drive/head selector byte for a drive on the LBA28 path.
///
/// C: `ldh_init(drive) = LDH_DEFAULT | ((drive) << 4)` (at_wini.h:33) OR'd with
/// `LDH_LBA`; the low nibble later carries LBA bits 24-27.
pub const fn ldh_lba(drive: u8) -> u8 {
    // Drive select rides bit 4 (LDH_DEV == 1<<4), so drive 1 sets it.
    LDH_DEFAULT | ((drive & 0x1) << 4) | LDH_LBA
}

/// Encode an LBA28 read/write command.
///
/// C: `do_transfer` LBA branch (at_wini.c:917-921) — `sector/cyl_lo/cyl_hi`
/// carry bits 0-23 and `ldh` ORs in bits 24-27 over the controller's
/// `ldhpref`. `count` is the sector count for this command.
pub fn lba28_command(block: u32, count: u8, drive: u8, do_write: bool) -> Command {
    Command {
        precomp: 0,
        count,
        sector: (block & 0xFF) as u8,
        cyl_lo: ((block >> 8) & 0xFF) as u8,
        cyl_hi: ((block >> 16) & 0xFF) as u8,
        ldh: ldh_lba(drive) | ((block >> 24) & 0x0F) as u8,
        command: if do_write { CMD_WRITE } else { CMD_READ },
    }
}

/// Poll the alternate-status port until `(status & mask) == value`, bounded by
/// `max_polls`. C: `w_waitfor` (at_wini.c:1150, 1230) — reads
/// `base_ctl + REG_CTL_ALTSTAT` each round. Returns the matching status byte on
/// success, [`PioError::Timeout`] if the state is never reached.
pub fn wait_for(
    bus: &mut impl IoBus,
    base_ctl: u16,
    mask: u8,
    value: u8,
    max_polls: u32,
) -> Result<u8, PioError> {
    let port = base_ctl + REG_CTL_ALTSTAT;
    for _ in 0..max_polls {
        let status = bus.inb(port);
        if status & mask == value {
            return Ok(status);
        }
    }
    Err(PioError::Timeout)
}

/// Push a command block to the controller in the task-file order the hardware
/// expects.
///
/// C: `com_out` (at_wini.c:1220-1262): wait for BSY to clear, select the drive
/// (`REG_LDH`), wait for BSY to clear again, then write the seven-register
/// vector `[CTL, PRECOMP, COUNT, SECTOR, CYL_LO, CYL_HI, COMMAND]`. `eightheads`
/// is the controller's `heads >= 8` geometry flag (at_wini.c:1253).
pub fn com_out(
    bus: &mut impl IoBus,
    base_cmd: u16,
    base_ctl: u16,
    cmd: &Command,
    eightheads: bool,
    max_polls: u32,
) -> Result<(), PioError> {
    // BSY must be clear before touching the drive selector. C: at_wini.c:1230.
    wait_for(bus, base_ctl, STATUS_BSY, 0, max_polls)?;

    // Select the drive first; BSY must clear again before the vector. C: :1236-1242.
    bus.outb(base_cmd + REG_LDH, cmd.ldh);
    wait_for(bus, base_ctl, STATUS_BSY, 0, max_polls)?;

    // The seven-byte register vector, in C's exact order. C: at_wini.c:1253-1259.
    bus.outb(
        base_ctl + REG_CTL_ALTSTAT,
        if eightheads { CTL_EIGHTHEADS } else { 0 },
    );
    bus.outb(base_cmd + REG_PRECOMP, cmd.precomp);
    bus.outb(base_cmd + REG_COUNT, cmd.count);
    bus.outb(base_cmd + REG_SECTOR, cmd.sector);
    bus.outb(base_cmd + REG_CYL_LO, cmd.cyl_lo);
    bus.outb(base_cmd + REG_CYL_HI, cmd.cyl_hi);
    bus.outb(base_cmd + REG_COMMAND, cmd.command);
    Ok(())
}

/// Read one sector from the data register into `buf` (256 words).
/// C: `sys_insw(base_cmd + REG_DATA, ..., SECTOR_SIZE)` (at_wini.c:1156-1165).
pub fn pio_read_sector(bus: &mut impl IoBus, base_cmd: u16, buf: &mut [u16; SECTOR_WORDS]) {
    bus.in_words(base_cmd + REG_DATA, buf);
}

/// Write one sector to the data register from `buf` (256 words).
/// C: `sys_outsw(base_cmd + REG_DATA, ..., SECTOR_SIZE)` (at_wini.c:1169-1178).
pub fn pio_write_sector(bus: &mut impl IoBus, base_cmd: u16, buf: &[u16; SECTOR_WORDS]) {
    bus.out_words(base_cmd + REG_DATA, buf);
}

/// Whether a status byte reports the drive ready (DRDY set, BSY clear).
/// C: the transfer loop's per-sector gate (at_wini.c:1150-1153).
pub const fn status_ready(status: u8) -> bool {
    status & (STATUS_BSY | STATUS_RDY) == STATUS_RDY
}

/// Whether a status byte signals a latched error. C: `STATUS_ERR` (at_wini.h:44).
pub const fn status_error(status: u8) -> bool {
    status & STATUS_ERR != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    /// Records byte writes and serves scripted byte reads, so the sequencing is
    /// asserted without a controller.
    struct MockBus {
        writes: Vec<(u16, u8)>,
        /// Status bytes handed out per `inb` call, in order (last repeats).
        status_stream: Vec<u8>,
        reads: usize,
        word_writes: Vec<(u16, Vec<u16>)>,
    }

    impl MockBus {
        fn new(status_stream: Vec<u8>) -> Self {
            MockBus {
                writes: Vec::new(),
                status_stream,
                reads: 0,
                word_writes: Vec::new(),
            }
        }

        fn next_status(&mut self) -> u8 {
            let i = self.reads.min(self.status_stream.len().saturating_sub(1));
            self.reads += 1;
            self.status_stream[i]
        }
    }

    impl IoBus for MockBus {
        fn inb(&mut self, _port: u16) -> u8 {
            self.next_status()
        }
        fn outb(&mut self, port: u16, value: u8) {
            self.writes.push((port, value));
        }
        fn in_words(&mut self, port: u16, dst: &mut [u16]) {
            for (i, slot) in dst.iter_mut().enumerate() {
                *slot = (port as usize + i) as u16;
            }
        }
        fn out_words(&mut self, port: u16, src: &[u16]) {
            self.word_writes.push((port, src.to_vec()));
        }
    }

    const BASE_CMD: u16 = 0x1F0;
    const BASE_CTL: u16 = 0x3F6;

    #[test]
    fn test_lba28_field_split() {
        // C: at_wini.c:917-921 — sector=LBA[0..7], cyl_lo=LBA[8..15],
        // cyl_hi=LBA[16..23], ldh low nibble = LBA[24..27].
        let cmd = lba28_command(0x1234_5678, 1, 0, false);
        assert_eq!(cmd.sector, 0x78);
        assert_eq!(cmd.cyl_lo, 0x56);
        assert_eq!(cmd.cyl_hi, 0x34);
        assert_eq!(cmd.ldh & 0x0F, 0x02); // 0x12345678 >> 24 = 0x12, low nibble 2
        assert_eq!(cmd.command, CMD_READ);
        assert_eq!(cmd.count, 1);
        // LBA enable + default bits ride in ldh; drive 0 keeps LDH_DEV clear.
        assert_eq!(cmd.ldh & LDH_LBA, LDH_LBA);
        assert_eq!(cmd.ldh & LDH_DEFAULT, LDH_DEFAULT);
        assert_eq!(cmd.ldh & LDH_DEV, 0);
        // Write opcode flips to CMD_WRITE; drive 1 sets LDH_DEV.
        let w = lba28_command(0, 2, 1, true);
        assert_eq!(w.command, CMD_WRITE);
        assert_eq!(w.ldh & LDH_DEV, LDH_DEV);
    }

    #[test]
    fn test_com_out_writes_registers_in_c_order() {
        // Status stream: every poll returns not-busy, so com_out runs to the end.
        let mut bus = MockBus::new(vec![0x00]);
        let cmd = Command {
            precomp: 0x11,
            count: 0x22,
            sector: 0x33,
            cyl_lo: 0x44,
            cyl_hi: 0x55,
            ldh: 0xE0,
            command: CMD_READ,
        };
        com_out(&mut bus, BASE_CMD, BASE_CTL, &cmd, false, 8).unwrap();
        // Expect: LDH select, then the 7-byte vector [CTL, PRECOMP, COUNT,
        // SECTOR, CYL_LO, CYL_HI, COMMAND] (C: at_wini.c:1236, 1253-1259).
        assert_eq!(bus.writes[0], (BASE_CMD + REG_LDH, 0xE0));
        assert_eq!(
            &bus.writes[1..],
            &[
                (BASE_CTL + REG_CTL_ALTSTAT, 0x00), // CTL_EIGHTHEADS off
                (BASE_CMD + REG_PRECOMP, 0x11),
                (BASE_CMD + REG_COUNT, 0x22),
                (BASE_CMD + REG_SECTOR, 0x33),
                (BASE_CMD + REG_CYL_LO, 0x44),
                (BASE_CMD + REG_CYL_HI, 0x55),
                (BASE_CMD + REG_COMMAND, CMD_READ),
            ]
        );
    }

    #[test]
    fn test_com_out_eightheads_sets_ctl_bit() {
        let mut bus = MockBus::new(vec![0x00]);
        let cmd = lba28_command(0, 1, 0, false);
        com_out(&mut bus, BASE_CMD, BASE_CTL, &cmd, true, 8).unwrap();
        assert_eq!(bus.writes[1], (BASE_CTL + REG_CTL_ALTSTAT, CTL_EIGHTHEADS));
    }

    #[test]
    fn test_wait_for_success_and_timeout() {
        // BSY clears on the second poll.
        let mut bus = MockBus::new(vec![STATUS_BSY, 0x00]);
        assert_eq!(wait_for(&mut bus, BASE_CTL, STATUS_BSY, 0, 4), Ok(0x00));
        // Never clears BSY within budget → timeout.
        let mut busy = MockBus::new(vec![STATUS_BSY]);
        assert_eq!(
            wait_for(&mut busy, BASE_CTL, STATUS_BSY, 0, 3),
            Err(PioError::Timeout)
        );
        // com_out must surface the timeout rather than write a half command.
        let mut stuck = MockBus::new(vec![STATUS_BSY]);
        let cmd = lba28_command(1, 1, 0, false);
        assert_eq!(
            com_out(&mut stuck, BASE_CMD, BASE_CTL, &cmd, false, 2),
            Err(PioError::Timeout)
        );
        assert!(stuck.writes.is_empty());
    }

    #[test]
    fn test_pio_transfer_uses_data_register() {
        let mut bus = MockBus::new(vec![0x00]);
        let mut buf = [0u16; SECTOR_WORDS];
        pio_read_sector(&mut bus, BASE_CMD, &mut buf);
        assert_eq!(buf[0], BASE_CMD + REG_DATA);
        let out = [0xABCDu16; SECTOR_WORDS];
        pio_write_sector(&mut bus, BASE_CMD, &out);
        assert_eq!(bus.word_writes[0].0, BASE_CMD + REG_DATA);
        assert_eq!(bus.word_writes[0].1.len(), SECTOR_WORDS);
    }

    #[test]
    fn test_status_predicates() {
        assert!(status_ready(STATUS_RDY));
        assert!(!status_ready(STATUS_RDY | STATUS_BSY));
        assert!(!status_ready(0x00));
        assert!(status_error(STATUS_ERR));
        assert!(!status_error(STATUS_RDY));
    }
}
