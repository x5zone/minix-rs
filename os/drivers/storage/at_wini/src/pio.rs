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
/// Read sector(s), LBA48-addressed. C: `CMD_READ_EXT` — at_wini.h:59.
pub const CMD_READ_EXT: u8 = 0x24;
/// Write sector(s), LBA48-addressed. C: `CMD_WRITE_EXT` — at_wini.h:62.
pub const CMD_WRITE_EXT: u8 = 0x34;
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

/// Build the drive/head selector byte for a drive on the CHS / pre-IDENTIFY
/// path (no LBA bit yet). C: `ldh_init(drive) = LDH_DEFAULT | ((drive) << 4)`
/// (at_wini.h:33); the drive select rides bit 4 (LDH_DEV == 1<<4).
pub const fn ldh_init(drive: u8) -> u8 {
    LDH_DEFAULT | ((drive & 0x1) << 4)
}

/// Build the drive/head selector byte for a drive on the LBA28 path.
///
/// C: `ldh_init(drive)` OR'd with `LDH_LBA` (at_wini.h:31,33).
pub const fn ldh_lba(drive: u8) -> u8 {
    ldh_init(drive) | LDH_LBA
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

/// An LBA48 task-file command block.
///
/// C: `struct command` LBA48 fields written by `com_out_ext` (at_wini.c:1268-
/// 1316): each of COUNT/SECTOR/CYL is sent twice (high byte first, then low)
/// per the ATA 48-bit "set features" protocol. Fields named `*_prev` carry the
/// high bytes (C `*_prev`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Command48 {
    /// Sector count high byte. C: `cmd.count_prev`.
    pub count_prev: u8,
    /// Sector count low byte. C: `cmd.count`.
    pub count: u8,
    /// LBA bits 32-39. C: `cmd.sector_prev`.
    pub sector_prev: u8,
    /// LBA bits 0-7. C: `cmd.sector`.
    pub sector: u8,
    /// LBA bits 40-47. C: `cmd.cyl_lo_prev`.
    pub cyl_lo_prev: u8,
    /// LBA bits 8-15. C: `cmd.cyl_lo`.
    pub cyl_lo: u8,
    /// LBA bits 48-55 (unused at 48-bit width, always 0 here). C: `cmd.cyl_hi_prev`.
    pub cyl_hi_prev: u8,
    /// LBA bits 16-23. C: `cmd.cyl_hi`.
    pub cyl_hi: u8,
    /// Drive/LBA-enable byte (no address bits ride in it at 48-bit). C: `cmd.ldh`.
    pub ldh: u8,
    /// Opcode written last. C: `cmd.command`.
    pub command: u8,
}

/// Encode an LBA48 read/write command over a 48-bit block address.
///
/// C: `do_transfer` LBA48 branch (at_wini.c:896-914) — `sector/cyl_lo/cyl_hi`
/// carry bits 0-23, `sector_prev` bits 24-31, `cyl_lo_prev` bits 32-39,
/// `cyl_hi_prev` bits 40-47; `ldh` is the controller's LBA `ldhpref` (no
/// address nibble); the 16-bit `count` splits into `count`/`count_prev`.
pub fn lba48_command(block: u64, count: u16, drive: u8, do_write: bool) -> Command48 {
    Command48 {
        count_prev: ((count >> 8) & 0xFF) as u8,
        count: (count & 0xFF) as u8,
        sector_prev: ((block >> 24) & 0xFF) as u8,
        sector: (block & 0xFF) as u8,
        cyl_lo_prev: ((block >> 32) & 0xFF) as u8,
        cyl_lo: ((block >> 8) & 0xFF) as u8,
        cyl_hi_prev: ((block >> 40) & 0xFF) as u8,
        cyl_hi: ((block >> 16) & 0xFF) as u8,
        ldh: ldh_lba(drive),
        command: if do_write {
            CMD_WRITE_EXT
        } else {
            CMD_READ_EXT
        },
    }
}

/// Push an LBA48 command block (the `*_ext` register vector).
///
/// C: `com_out_ext` (at_wini.c:1268-1316): wait BSY clear, select drive
/// (`REG_LDH`), wait BSY clear, then the ten-byte vector `[CTL=0, COUNT_hi,
/// SECTOR_hi, CYL_LO_hi, CYL_HI_hi, COUNT_lo, SECTOR_lo, CYL_LO_lo, CYL_HI_lo,
/// COMMAND]` — high bytes before low. Unlike the 28-bit `com_out`, the control
/// register is written as 0 (no eight-heads geometry bit here).
pub fn com_out_ext(
    bus: &mut impl IoBus,
    base_cmd: u16,
    base_ctl: u16,
    cmd: &Command48,
    max_polls: u32,
) -> Result<(), PioError> {
    wait_for(bus, base_ctl, STATUS_BSY, 0, max_polls)?;
    bus.outb(base_cmd + REG_LDH, cmd.ldh);
    wait_for(bus, base_ctl, STATUS_BSY, 0, max_polls)?;

    bus.outb(base_ctl + REG_CTL_ALTSTAT, 0);
    bus.outb(base_cmd + REG_COUNT, cmd.count_prev);
    bus.outb(base_cmd + REG_SECTOR, cmd.sector_prev);
    bus.outb(base_cmd + REG_CYL_LO, cmd.cyl_lo_prev);
    bus.outb(base_cmd + REG_CYL_HI, cmd.cyl_hi_prev);
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

/// IDENTIFY word holding the LBA capability bit. C: `ID_CAPABILITIES` (49) —
/// at_wini.h:85.
pub const ID_CAPABILITIES_WORD: usize = 49;
/// IDENTIFY word holding the supported-command-set bits (LBA48). C: `ID_CSS`
/// (83 = 0x53) — at_wini.h:97.
pub const ID_CSS_WORD: usize = 83;
/// LBA addressing supported — bit in word 49. C: `ID_CAP_LBA` — at_wini.h:86.
pub const ID_CAP_LBA: u16 = 0x0200;
/// LBA48 addressing supported — bit in word 83. C: `ID_CSS_LBA48` — at_wini.h:98.
pub const ID_CSS_LBA48: u16 = 0x0400;
/// Only trust LBA once the CHS size exceeds this many sectors (~512 MiB).
/// C: `size > 512L*1024*2` — at_wini.c:689.
pub const LBA_TRUST_MIN_SECTORS: u32 = 512 * 1024 * 2;
/// Only probe for LBA48 once the LBA28 sector count reaches this. C:
/// `LBA48_CHECK_SIZE` — at_wini.h:126.
pub const LBA48_CHECK_SIZE: u32 = 0x0f00_0000;

/// Geometry / capacity / addressing decoded from an IDENTIFY block.
///
/// C: the `wini` fields `cylinders/heads/sectors/lba48` plus `size` set by
/// `w_identify` (at_wini.c:682-718).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdentifyInfo {
    /// CHS cylinders (word 1). C: `id_word(1)` — at_wini.c:683.
    pub cylinders: u16,
    /// CHS heads (word 3). C: `id_word(3)` — at_wini.c:684.
    pub heads: u16,
    /// CHS sectors-per-track (word 6). C: `id_word(6)` — at_wini.c:685.
    pub sectors_per_track: u16,
    /// LBA addressing selected. C: `wn->ldhpref |= LDH_LBA` (at_wini.c:693).
    pub lba: bool,
    /// LBA48 addressing selected. C: `wn->lba48 = 1` (at_wini.c:714).
    pub lba48: bool,
    /// Total addressable sectors under the chosen mode. C: `size`.
    pub total_sectors: u32,
}

/// 4-byte little-endian longword at word index `n` (words `n` and `n+1`).
/// C: `id_longword(n)` — at_wini.c:555.
pub fn id_longword(block: &[u16; SECTOR_WORDS], n: usize) -> u32 {
    block[n] as u32 | (block[n + 1] as u32) << 16
}

/// Decode LBA / LBA48 support, geometry and total capacity from a 256-word
/// IDENTIFY block, mirroring C `w_identify` (at_wini.c:682-718).
///
/// The LBA bit is in word 49 (`ID_CAPABILITIES`) and the LBA48 bit is in word
/// 83 (`ID_CSS`) — two different words. LBA is trusted only past
/// `LBA_TRUST_MIN_SECTORS` of the CHS size; LBA48 is probed only at/over
/// `LBA48_CHECK_SIZE` of the LBA28 count (words 60-61), and its 48-bit count
/// (words 100-103) truncates to `u32::MAX` when the high longword is nonzero.
pub fn parse_identify(block: &[u16; SECTOR_WORDS]) -> IdentifyInfo {
    let cylinders = block[1];
    let heads = block[3];
    let sectors_per_track = block[6];
    let chs_size = (cylinders as u32) * (heads as u32) * (sectors_per_track as u32);
    let mut info = IdentifyInfo {
        cylinders,
        heads,
        sectors_per_track,
        lba: false,
        lba48: false,
        total_sectors: chs_size,
    };
    // C: only enable LBA when the drive is LBA-capable AND big enough to trust
    // it not to garble the translation (at_wini.c:689).
    if block[ID_CAPABILITIES_WORD] & ID_CAP_LBA != 0 && chs_size > LBA_TRUST_MIN_SECTORS {
        info.lba = true;
        info.total_sectors = id_longword(block, 60); // LBA28 sector count
        // C: only look for LBA48 once the count reaches the check size
        // (at_wini.c:697-714).
        if info.total_sectors >= LBA48_CHECK_SIZE && block[ID_CSS_WORD] & ID_CSS_LBA48 != 0 {
            info.lba48 = true;
            // A 48-bit count that overflows 32 bits → truncate to LBA32 max.
            info.total_sectors = if id_longword(block, 102) != 0 {
                u32::MAX
            } else {
                id_longword(block, 100)
            };
        }
    }
    info
}

/// The ATA IDENTIFY command for a drive (CHS selector — LBA capability is
/// exactly what this command discovers, so the selector carries no LBA bit).
///
/// C: `w_identify` sets `cmd.command = ATA_IDENTIFY` with the drive's `ldh_init`
/// (at_wini.c:656-662).
pub const fn identify_command(drive: u8) -> Command {
    Command {
        precomp: 0,
        count: 0,
        sector: 0,
        cyl_lo: 0,
        cyl_hi: 0,
        ldh: ldh_init(drive),
        command: ATA_IDENTIFY,
    }
}

/// Issue IDENTIFY and decode the drive's geometry / capacity / addressing.
///
/// C: `w_identify` (at_wini.c:644-718): send the IDENTIFY command, wait for the
/// DRQ, transfer the 256-word identify block, then decode it.
pub fn read_identify(
    bus: &mut impl IoBus,
    base_cmd: u16,
    base_ctl: u16,
    drive: u8,
    max_polls: u32,
) -> Result<IdentifyInfo, PioError> {
    let cmd = identify_command(drive);
    // Geometry is not yet known before identify, so the eight-heads bit is off.
    com_out(bus, base_cmd, base_ctl, &cmd, false, max_polls)?;
    // The identify block is ready to read once DRQ asserts. C: at_wini.c:672.
    wait_for(bus, base_ctl, STATUS_DRQ, STATUS_DRQ, max_polls)?;
    let mut block = [0u16; SECTOR_WORDS];
    pio_read_sector(bus, base_cmd, &mut block);
    Ok(parse_identify(&block))
}

/// The read/write command encoding chosen for a transfer, per the drive's
/// addressing capability. C: `do_transfer` dispatches on `wn->lba48` then the
/// LBA bit, else the CHS geometry path (at_wini.c:896-930).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockCommand {
    /// 28-bit LBA (≤128 GiB). C: the `ldhpref & LDH_LBA` branch (:917-921).
    Lba28(Command),
    /// 48-bit LBA (large drives). C: the `do_lba48` branch (:896-916).
    Lba48(Command48),
}

/// Choose the command encoding for a block transfer given the drive's IDENTIFY
/// result.
///
/// Returns `None` for a CHS-only drive (media too small to trust LBA, or
/// LBA-incapable): the geometry command path is a later increment, and the
/// caller must not silently fall back to LBA (C keeps CHS for such drives).
/// The LBA28 count saturates at one command's worth (a sector count is a byte;
/// larger requests split across commands by the caller).
pub fn block_command(
    caps: &IdentifyInfo,
    block: u64,
    count: u16,
    drive: u8,
    do_write: bool,
) -> Option<BlockCommand> {
    if caps.lba48 {
        Some(BlockCommand::Lba48(lba48_command(
            block, count, drive, do_write,
        )))
    } else if caps.lba {
        Some(BlockCommand::Lba28(lba28_command(
            block as u32,
            count.min(u8::MAX as u16) as u8,
            drive,
            do_write,
        )))
    } else {
        None
    }
}

/// Clip a byte request against the device extent end. C: `w_transfer`
/// (at_wini.c:1038-1040) — at/over the end answers 0; a request crossing the
/// end is clipped to the remaining bytes. `size` is the device's byte size.
pub const fn clip_to_extent(position: u64, want: u64, size: u64) -> u64 {
    if position >= size {
        0
    } else {
        let room = size - position;
        if want > room { room } else { want }
    }
}

/// Convert a byte position within an extent to a starting LBA block. C:
/// `block = (dv_base + position) / SECTOR_SIZE` (at_wini.c:1041). `extent_base`
/// is the device's byte offset into the medium (whole disk 0; a partition its
/// base). `SECTOR_WORDS * 2` = the 512-byte sector.
pub const fn byte_to_block(extent_base: u64, position: u64) -> u64 {
    (extent_base + position) / (SECTOR_WORDS as u64 * 2)
}

/// Read `n_sectors` sectors over PIO after the read command was sent: per
/// sector wait BSY-clear then DRQ-set, then move one sector. C: the read arm of
/// `w_transfer`'s per-sector loop (at_wini.c:1150-1165). `dst` (length
/// `n_sectors * SECTOR_WORDS`) is filled sequentially with the sector words.
pub fn pio_read_sectors(
    bus: &mut impl IoBus,
    base_cmd: u16,
    base_ctl: u16,
    n_sectors: usize,
    dst: &mut [u16],
    max_polls: u32,
) -> Result<(), PioError> {
    let mut sector = [0u16; SECTOR_WORDS];
    for chunk in dst.chunks_mut(SECTOR_WORDS).take(n_sectors) {
        wait_for(bus, base_ctl, STATUS_BSY, 0, max_polls)?;
        wait_for(bus, base_ctl, STATUS_DRQ, STATUS_DRQ, max_polls)?;
        pio_read_sector(bus, base_cmd, &mut sector);
        chunk.copy_from_slice(&sector[..chunk.len()]);
    }
    Ok(())
}

/// Write sectors over PIO after the write command was sent: per sector wait
/// BSY-clear + DRQ-set, transfer a full sector, then wait for BSY to drop
/// again. C: the write arm of `w_transfer` (at_wini.c:1150-1186); the trailing
/// interrupt wait is polled here. `src` length need not be a sector multiple —
/// the final partial sector is zero-padded to 256 words.
pub fn pio_write_sectors(
    bus: &mut impl IoBus,
    base_cmd: u16,
    base_ctl: u16,
    src: &[u16],
    max_polls: u32,
) -> Result<(), PioError> {
    for chunk in src.chunks(SECTOR_WORDS) {
        wait_for(bus, base_ctl, STATUS_BSY, 0, max_polls)?;
        wait_for(bus, base_ctl, STATUS_DRQ, STATUS_DRQ, max_polls)?;
        let mut sector = [0u16; SECTOR_WORDS];
        sector[..chunk.len()].copy_from_slice(chunk);
        pio_write_sector(bus, base_cmd, &sector);
        wait_for(bus, base_ctl, STATUS_BSY, 0, max_polls)?;
    }
    Ok(())
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

    #[test]
    fn test_lba48_field_split() {
        // C: at_wini.c:896-914 — a 48-bit LBA splits across low + *_prev bytes.
        let block: u64 = 0x1234_5678_9ABC;
        let cmd = lba48_command(block, 0x0201, 0, false);
        assert_eq!(cmd.sector, 0xBC); // bits 0-7
        assert_eq!(cmd.cyl_lo, 0x9A); // bits 8-15
        assert_eq!(cmd.cyl_hi, 0x78); // bits 16-23
        assert_eq!(cmd.sector_prev, 0x56); // bits 24-31
        assert_eq!(cmd.cyl_lo_prev, 0x34); // bits 32-39
        assert_eq!(cmd.cyl_hi_prev, 0x12); // bits 40-47
        assert_eq!((cmd.count, cmd.count_prev), (0x01, 0x02)); // 16-bit count split
        assert_eq!(cmd.command, CMD_READ_EXT);
        // ldh is the plain LBA ldhpref, no address nibble folded in (C: cmd.ldh = ldhpref).
        assert_eq!(cmd.ldh, ldh_lba(0));
        assert_eq!(lba48_command(0, 1, 0, true).command, CMD_WRITE_EXT);
    }

    #[test]
    fn test_com_out_ext_writes_high_then_low_vector() {
        let mut bus = MockBus::new(vec![0x00]);
        let cmd = Command48 {
            count_prev: 0x02,
            count: 0x01,
            sector_prev: 0x03,
            sector: 0x04,
            cyl_lo_prev: 0x05,
            cyl_lo: 0x06,
            cyl_hi_prev: 0x07,
            cyl_hi: 0x08,
            ldh: 0xE0,
            command: CMD_READ_EXT,
        };
        com_out_ext(&mut bus, BASE_CMD, BASE_CTL, &cmd, 8).unwrap();
        // LDH select first, then the ten-byte vector (C: at_wini.c:1300-1310).
        assert_eq!(bus.writes[0], (BASE_CMD + REG_LDH, 0xE0));
        assert_eq!(
            &bus.writes[1..],
            &[
                (BASE_CTL + REG_CTL_ALTSTAT, 0x00), // CTL always 0 for ext
                (BASE_CMD + REG_COUNT, 0x02),       // count high
                (BASE_CMD + REG_SECTOR, 0x03),      // sector high
                (BASE_CMD + REG_CYL_LO, 0x05),      // cyl_lo high
                (BASE_CMD + REG_CYL_HI, 0x07),      // cyl_hi high
                (BASE_CMD + REG_COUNT, 0x01),       // count low
                (BASE_CMD + REG_SECTOR, 0x04),      // sector low
                (BASE_CMD + REG_CYL_LO, 0x06),      // cyl_lo low
                (BASE_CMD + REG_CYL_HI, 0x08),      // cyl_hi low
                (BASE_CMD + REG_COMMAND, CMD_READ_EXT),
            ]
        );
    }

    /// A baseline IDENTIFY block with CHS geometry big enough to trust LBA
    /// (2048*255*63 = 32,901,120 sectors > LBA_TRUST_MIN_SECTORS). C: words
    /// 1/3/6 (at_wini.c:683-685).
    fn id_block() -> [u16; SECTOR_WORDS] {
        let mut b = [0u16; SECTOR_WORDS];
        b[1] = 2048;
        b[3] = 255;
        b[6] = 63;
        b
    }

    #[test]
    fn test_parse_identify_no_lba_falls_back_to_chs() {
        // word 49 without the LBA bit → CHS addressing, size = cyl*head*sectors.
        let mut b = id_block();
        b[ID_CAPABILITIES_WORD] = 0;
        let info = parse_identify(&b);
        assert!(!info.lba);
        assert!(!info.lba48);
        assert_eq!(info.total_sectors, 2048 * 255 * 63);
    }

    #[test]
    fn test_parse_identify_lba_bit_is_word49_not_word83() {
        // Regression guard for the R9 conflation: LBA48 bit lives in word 83
        // (ID_CSS), NOT word 49. Setting only word 49's LBA bit gives lba but
        // not lba48; setting the LBA48 bit in word 49 must be ignored.
        let mut b = id_block();
        b[ID_CAPABILITIES_WORD] = ID_CAP_LBA | ID_CSS_LBA48; // wrong-word LBA48 bit ignored
        let info = parse_identify(&b);
        assert!(info.lba);
        assert!(!info.lba48); // requires word 83, which is still 0 here
    }

    #[test]
    fn test_parse_identify_lba28() {
        // LBA enabled, count below the LBA48 check size → stays LBA28.
        let mut b = id_block();
        b[ID_CAPABILITIES_WORD] = ID_CAP_LBA;
        b[60] = 50_000; // id_longword(60) = 50,000 < LBA48_CHECK_SIZE
        b[83] = ID_CSS_LBA48; // even with LBA48 support, size gate suppresses it
        let info = parse_identify(&b);
        assert!(info.lba);
        assert!(!info.lba48);
        assert_eq!(info.total_sectors, 50_000);
    }

    #[test]
    fn test_parse_identify_lba48() {
        // Big enough + word 83 LBA48 bit + 48-bit count fitting in 32 bits.
        let mut b = id_block();
        b[ID_CAPABILITIES_WORD] = ID_CAP_LBA;
        b[60] = 0;
        b[61] = 0x0F00; // LBA28 count = 0x0F000000 == LBA48_CHECK_SIZE
        b[ID_CSS_WORD] = ID_CSS_LBA48;
        b[100] = 0x1234; // id_longword(100) = the real 48-bit count
        let info = parse_identify(&b);
        assert!(info.lba);
        assert!(info.lba48);
        assert_eq!(info.total_sectors, 0x1234);
    }

    #[test]
    fn test_parse_identify_lba48_overflow_truncates() {
        // High longword (words 102-103) nonzero → count doesn't fit 32 bits.
        let mut b = id_block();
        b[ID_CAPABILITIES_WORD] = ID_CAP_LBA;
        b[61] = 0x0F00;
        b[ID_CSS_WORD] = ID_CSS_LBA48;
        b[102] = 0x1; // id_longword(102) != 0
        let info = parse_identify(&b);
        assert!(info.lba48);
        assert_eq!(info.total_sectors, u32::MAX); // LBA32 truncation (C :709)
    }

    #[test]
    fn test_read_identify_issues_identify_and_reads_block() {
        // STATUS stream: DRQ set & BSY clear — both com_out's BSY wait and the
        // post-command DRQ wait succeed on the first poll.
        let mut bus = MockBus::new(vec![STATUS_DRQ]);
        let info = read_identify(&mut bus, BASE_CMD, BASE_CTL, 0, 16).unwrap();
        // The IDENTIFY command must be written (CHS selector, ATA_IDENTIFY).
        // C: at_wini.c:656-662.
        assert!(bus.writes.contains(&(BASE_CMD + REG_LDH, ldh_init(0))));
        assert!(bus.writes.contains(&(BASE_CMD + REG_COMMAND, ATA_IDENTIFY)));
        // read_identify returns exactly the parse of the block the bus returned.
        assert_eq!(info.cylinders, BASE_CMD + REG_DATA + 1);
    }

    #[test]
    fn test_identify_command_encoding() {
        let cmd = identify_command(1);
        assert_eq!(cmd.command, ATA_IDENTIFY);
        // drive 1 sets LDH_DEV, and no LBA bit (capability is what identify finds).
        assert_eq!(cmd.ldh, ldh_init(1));
        assert_eq!(cmd.ldh & LDH_DEV, LDH_DEV);
        assert_eq!(cmd.ldh & LDH_LBA, 0);
    }

    /// A drive caps record with only the two addressing bits set.
    fn caps(lba: bool, lba48: bool) -> IdentifyInfo {
        IdentifyInfo {
            cylinders: 0,
            heads: 0,
            sectors_per_track: 0,
            lba,
            lba48,
            total_sectors: 0,
        }
    }

    #[test]
    fn test_block_command_selects_by_capability() {
        // C: do_transfer dispatches LBA48 first, then the LBA bit (:896/:917).
        match block_command(&caps(false, true), 0x1_2345_6789, 3, 0, false).unwrap() {
            BlockCommand::Lba48(c) => {
                assert_eq!(c.command, CMD_READ_EXT);
                assert_eq!(c.sector, 0x89); // bits 0-7 of the 48-bit block
            }
            other => panic!("expected Lba48, got {other:?}"),
        }
        match block_command(&caps(true, false), 0x1234_5678, 2, 0, true).unwrap() {
            BlockCommand::Lba28(c) => {
                assert_eq!(c.command, CMD_WRITE);
                assert_eq!(c.sector, 0x78);
            }
            other => panic!("expected Lba28, got {other:?}"),
        }
        // CHS-only drive → None; never silently mis-fall back to LBA.
        assert!(block_command(&caps(false, false), 0, 1, 0, false).is_none());
    }

    #[test]
    fn test_block_command_lba28_count_saturates() {
        // A sector count is one byte; LBA28 saturates at u8::MAX (larger
        // requests split across commands by the caller).
        match block_command(&caps(true, false), 0, 5000, 0, false).unwrap() {
            BlockCommand::Lba28(c) => assert_eq!(c.count, u8::MAX),
            other => panic!("expected Lba28, got {other:?}"),
        }
    }

    #[test]
    fn clip_and_block_math() {
        // C: w_transfer EOF clip (:1038-1040).
        assert_eq!(clip_to_extent(0, 100, 1000), 100);
        assert_eq!(clip_to_extent(950, 100, 1000), 50); // crosses the end
        assert_eq!(clip_to_extent(1000, 100, 1000), 0); // at/over the end
        // block = (base + position) / 512.
        assert_eq!(byte_to_block(0, 512), 1);
        assert_eq!(byte_to_block(0, 511), 0);
        assert_eq!(byte_to_block(2048, 512), 5); // partition at byte 2048
    }

    #[test]
    fn pio_read_sectors_fills_each_sector() {
        // DRQ set / BSY clear lets both per-sector waits pass immediately.
        let mut bus = MockBus::new(vec![STATUS_DRQ]);
        let mut dst = [0u16; 2 * SECTOR_WORDS];
        pio_read_sectors(&mut bus, BASE_CMD, BASE_CTL, 2, &mut dst, 4).unwrap();
        // MockBus.in_words writes (data-port + i) per sector, both sectors alike.
        assert_eq!(dst[0], BASE_CMD + REG_DATA);
        assert_eq!(dst[SECTOR_WORDS], BASE_CMD + REG_DATA);
    }

    #[test]
    fn pio_write_sectors_pads_and_transfers() {
        let mut bus = MockBus::new(vec![STATUS_DRQ]);
        // A short (partial-sector) write becomes one padded sector transfer.
        pio_write_sectors(&mut bus, BASE_CMD, BASE_CTL, &[0xAAAA, 0xBBBB], 4).unwrap();
        assert_eq!(bus.word_writes.len(), 1);
        assert_eq!(bus.word_writes[0].0, BASE_CMD + REG_DATA);
        assert_eq!(bus.word_writes[0].1.len(), SECTOR_WORDS);
        assert_eq!(&bus.word_writes[0].1[..2], &[0xAAAA, 0xBBBB]);
    }
}
