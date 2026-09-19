//! The read-only ISO9660 server: mount scan, directory walk, and the
//! [`FsDriver`] implementation over an in-memory disc image.
//!
//! C correspondence: the semantic halves of `super.c` (mount scan),
//! `lookup.c`/`search_dir` (child lookup), `read.c` (extent-clamped
//! reads), `util.c` (listings), and `inode.c` (status) — the parts the
//! V1 audit marks as the unimplemented semantic layer. The format-parsing
//! modules ([`crate::volume`], [`crate::record`], [`crate::rockridge`])
//! are the already-delivered half; this module is the F3a semantic wave.
//!
//! **Disc seam**: the server holds the whole disc image in memory. ISO
//! media is read-only, so the image is the entire device state; the
//! production block-driver channel is the separately tracked E-FSBDEV
//! seam (the hosted tests feed images directly, which is what makes the
//! semantics testable without a driver).
//!
//! **Inode numbering**: a node's number is its extent location (the
//! sector LBA where its directory record or data lives, `record.rs`
//! `location`). A small cache remembers the full record per visited node
//! — lookups and listings populate it, reads and status consume it
//! (the "inode 缓存" the V1 audit lists as missing).

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use minix_fs::data::DataChannel;
use minix_fs::dentry::{DentryEncoder, DirentType};
use minix_fs::driver::FsDriver;
use minix_fs::protocol::{CapabilityFlags, FileNode, MountFlags};
use minix_types::{EIO, EINVAL, ENOENT, Errno, Stat};

use crate::record::{self, DirectoryRecord};
use crate::rockridge;
use crate::volume;
use crate::{Options, clamp_read};

/// Sector size used for directory padding skips (records never cross it).
const DIRECTORY_SECTOR: usize = 2048;

/// The read-only ISO server over an in-memory disc.
#[derive(Debug)]
pub struct IsoServer {
    /// The whole disc image (read-only media: this is the device state).
    image: Vec<u8>,
    /// Logical block size from the primary descriptor.
    block_size: u32,
    /// Root directory record (from the primary's offset 156).
    root: DirectoryRecord,
    /// Mount options (`norock`).
    options: Options,
    /// Visited nodes: extent location → full record.
    cache: BTreeMap<u64, DirectoryRecord>,
}

impl IsoServer {
    /// Validate the image and build the server (C `mount` scan, `super.c`).
    ///
    /// Walks at most twenty descriptor sectors from byte 32768: primaries
    /// replace each other, the set terminator stops the walk, and success
    /// needs both a terminator and a valid primary. Extracts the logical
    /// block size and the root record.
    pub fn from_image(image: Vec<u8>, options: Options) -> Result<Self, Errno> {
        let mut block_size: Option<u32> = None;
        let mut root: Option<DirectoryRecord> = None;
        let mut terminator = false;
        for index in 0..volume::MAX_SECTORS {
            let start = volume::DESCRIPTOR_START as usize + index * volume::SECTOR_BYTES;
            let Some(sector) = image.get(start..start + volume::SECTOR_BYTES) else {
                break;
            };
            let tag = sector[0];
            if tag == volume::TAG_TERMINATOR {
                terminator = true;
                break;
            }
            if tag == volume::TAG_PRIMARY {
                volume::check_consistency(
                    &sector[1..1 + volume::SIGNATURE_BYTES]
                        .try_into()
                        .unwrap(),
                    sector[6],
                    u16::from_le_bytes([sector[128], sector[129]]) as u32,
                )
                .map_err(|_| Errno::from_i32(EINVAL))?;
                block_size =
                    Some(u16::from_le_bytes([sector[128], sector[129]]) as u32);
                let (record, _) = record::decode(&sector[156..])
                    .map_err(|_| Errno::from_i32(EINVAL))?;
                root = Some(record);
            }
        }
        let (block_size, root) = match (block_size, root) {
            (Some(size), Some(root)) if terminator => (size, root),
            _ => return Err(Errno::from_i32(EINVAL)),
        };
        let mut server = Self {
            image,
            block_size,
            root,
            options,
            cache: BTreeMap::new(),
        };
        let root_location = server.root.location as u64;
        server.cache.insert(root_location, server.root.clone());
        Ok(server)
    }

    /// The record of a known node, from the cache.
    fn record_of(&self, inode: u64) -> Result<&DirectoryRecord, Errno> {
        self.cache.get(&inode).ok_or(Errno::from_i32(ENOENT))
    }

    /// Walks one directory extent, handing every decoded record to
    /// `visit` (the visitor returns `false` to stop early). Zero-length
    /// padding skips to the next directory-sector boundary (records never
    /// cross it, ISO9660 §6.8).
    fn walk_directory<F>(&self, record: &DirectoryRecord, mut visit: F) -> Result<(), Errno>
    where
        F: FnMut(usize, &DirectoryRecord) -> bool,
    {
        let start = record.location as usize * self.block_size as usize;
        let length = record.data_length as usize;
        let extent = self
            .image
            .get(start..start + length)
            .ok_or(Errno::from_i32(EIO))?;
        let mut offset = 0usize;
        while offset < extent.len() {
            let window = &extent[offset..];
            if window[0] == 0 {
                // Zero fill to the boundary: records never cross sectors.
                offset += DIRECTORY_SECTOR - (offset % DIRECTORY_SECTOR);
                continue;
            }
            let (entry, advance) =
                record::decode(window).map_err(|_| Errno::from_i32(EIO))?;
            if !visit(offset, &entry) {
                return Ok(());
            }
            offset += advance;
        }
        Ok(())
    }

    /// The display name of a record: Rock Ridge `NM` when present (and
    /// not `norock`), otherwise the interchange name with the version
    /// suffix and any trailing dot stripped (`FILE.TXT;1` → `FILE.TXT`).
    /// Dot records map to `.` and `..`.
    fn display_name(&self, record: &DirectoryRecord) -> String {
        if record.name.len() == 1 && record.name[0] == 0 {
            return String::from(".");
        }
        if record.name.len() == 1 && record.name[0] == 1 {
            return String::from("..");
        }
        if !self.options.no_rock_ridge
            && let Ok(entries) = rockridge::split_entries(&record.rock_tail)
        {
            for entry in &entries {
                // NM 载荷首字节是标志位，名字从第二字节起
                // （susp_rock_ridge.c 的 NM 布局）。
                if entry.tag == *rockridge::TAG_NAME && entry.length > 1 {
                    let payload =
                        &record.rock_tail[entry.payload..entry.payload + entry.length];
                    return String::from_utf8_lossy(&payload[1..]).into_owned();
                }
            }
        }
        let mut name = String::from_utf8_lossy(&record.name).into_owned();
        if let Some(dot) = name.find(';') {
            name.truncate(dot);
        }
        if name.ends_with('.') {
            name.pop();
        }
        name
    }

    fn file_node(record: &DirectoryRecord) -> FileNode {
        let directory = record::is_directory(record.flags);
        FileNode {
            inode_number: record.location as u64,
            mode: if directory {
                0o040555
            } else {
                0o100444
            },
            size: record.data_length as i64,
            owner: 0,
            group: 0,
            device: 0,
        }
    }

    fn is_directory(record: &DirectoryRecord) -> bool {
        record::is_directory(record.flags)
    }
}

impl FsDriver for IsoServer {
    /// Mount: the image scan happened at construction; report the root.
    fn mount(
        &mut self,
        _device: u64,
        _flags: MountFlags,
        capabilities: &mut CapabilityFlags,
    ) -> Result<FileNode, Errno> {
        let _ = capabilities;
        Ok(Self::file_node(&self.root))
    }

    /// Resolve one child by display name (interchange or Rock Ridge `NM`).
    /// Hits populate the inode cache so reads and status find the record.
    fn lookup_child(
        &mut self,
        directory: u64,
        name: &str,
    ) -> Result<(FileNode, bool), Errno> {
        let record = self.record_of(directory)?.clone();
        let mut found: Option<DirectoryRecord> = None;
        self.walk_directory(&record, |_, entry| {
            let display = self.display_name(entry);
            if display == name {
                found = Some(entry.clone());
                false
            } else {
                true
            }
        })?;
        let entry = found.ok_or(Errno::from_i32(ENOENT))?;
        let node = Self::file_node(&entry);
        let directory = Self::is_directory(&entry);
        self.cache.insert(entry.location as u64, entry);
        Ok((node, directory))
    }

    /// Read file bytes, clamped to the extent (`crate::clamp_read`) and
    /// streamed through the callback.
    fn read(
        &mut self,
        inode: u64,
        position: i64,
        length: usize,
        out: &mut dyn FnMut(&[u8]),
    ) -> Result<usize, Errno> {
        if position < 0 {
            return Err(Errno::from_i32(EINVAL));
        }
        let record = self.record_of(inode)?.clone();
        let want = clamp_read(position as u64, record.data_length as u64, length as u64)
            as usize;
        if want == 0 {
            return Ok(0);
        }
        let start = record.location as usize * self.block_size as usize
            + position as usize;
        let bytes = self
            .image
            .get(start..start + want)
            .ok_or(Errno::from_i32(EIO))?;
        // Stream in sector-sized chunks, like the C extent walk.
        for chunk in bytes.chunks(DIRECTORY_SECTOR) {
            out(chunk);
        }
        Ok(want)
    }

    /// List a directory extent through the shared dentry encoder; the
    /// position is the byte offset inside the extent.
    fn get_dents(
        &mut self,
        inode: u64,
        position: &mut i64,
        capacity: usize,
        out: &mut dyn FnMut(&[u8]),
    ) -> Result<usize, Errno> {
        if *position < 0 {
            return Err(Errno::from_i32(EINVAL));
        }
        let record = self.record_of(inode)?.clone();
        let mut offset = *position as usize;
        let mut staging = alloc::vec![0u8; capacity.max(1)];
        let mut backend = OutBackend { out };
        let mut encoder = DentryEncoder::new(
            DataChannel::Present {
                backend: &mut backend,
                size: capacity,
            },
            capacity,
            &mut staging[..],
        );
        let mut stopped = false;
        self.walk_directory(&record, |at, entry| {
            if at < offset {
                return true;
            }
            let display = self.display_name(entry);
            let kind = if record::is_directory(entry.flags) {
                DirentType::Directory
            } else {
                DirentType::Regular
            };
            match encoder.add(entry.location as u64, display.as_bytes(), kind) {
                Ok(_) => {
                    offset = at + entry.record_length;
                    true
                }
                Err(_) => {
                    stopped = true;
                    false
                }
            }
        })?;
        let delivered = encoder.finish()?;
        if !stopped {
            *position = record.data_length as i64;
        } else {
            *position = offset as i64;
        }
        Ok(delivered)
    }

    /// Status from the cached record: read-only modes, extent size.
    fn stat(&mut self, inode: u64, stat: &mut Stat) -> Result<(), Errno> {
        let record = self.record_of(inode)?;
        stat.mode = if Self::is_directory(record) {
            0o040555
        } else {
            0o100444
        };
        stat.inode = record.location as u64;
        stat.size = record.data_length as i64;
        stat.nlinks = 1;
        Ok(())
    }

    /// Link target from the Rock Ridge `SL` components (`assemble_link`).
    fn read_link(
        &mut self,
        inode: u64,
        _capacity: usize,
        out: &mut dyn FnMut(&[u8]),
    ) -> Result<usize, Errno> {
        let record = self.record_of(inode)?.clone();
        if let Ok(entries) = rockridge::split_entries(&record.rock_tail) {
            let mut components: Vec<(u8, &[u8])> = Vec::new();
            for entry in &entries {
                if entry.tag == *rockridge::TAG_LINK {
                    let payload =
                        &record.rock_tail[entry.payload..entry.payload + entry.length];
                    if !payload.is_empty() {
                        components.push((payload[0], &payload[1..]));
                    }
                }
            }
            if !components.is_empty() {
                let target = rockridge::assemble_link(&components);
                out(&target);
                return Ok(target.len());
            }
        }
        Err(Errno::from_i32(EINVAL))
    }
}

/// Bridges the trait's copy callback into the dentry encoder (ptyfs F1
/// precedent: the listing only writes out).
struct OutBackend<'a> {
    out: &'a mut dyn FnMut(&[u8]),
}

impl minix_fs::data::DataBackend for OutBackend<'_> {
    fn copy_from(&self, _offset: usize, _out: &mut [u8]) -> Result<(), Errno> {
        Ok(())
    }
    fn copy_to(&mut self, _offset: usize, data: &[u8]) -> Result<(), Errno> {
        (self.out)(data);
        Ok(())
    }
    fn fill_zero(&mut self, _offset: usize, _length: usize) -> Result<(), Errno> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    const SECTOR: usize = 2048;
    /// 事件域之外的测试镜像布局：16=PVD，17=终止符，18..19=根目录，
    /// 20=文件数据。
    const PVD_SECTOR: usize = 16;
    const TERM_SECTOR: usize = 17;
    const ROOT_SECTOR: usize = 18;
    const FILE_SECTOR: usize = 20;
    const FILE_DATA: &[u8] = b"HELLO ISO";

    /// 一条 ISO 目录记录（record::decode 可解）。
    fn dir_record(location: u32, data_length: u32, flags: u8, name: &[u8], tail: &[u8]) -> Vec<u8> {
        let name_start = 33usize;
        let total = (name_start + name.len() + tail.len() + 1) & !1; // 偶对齐
        let mut r = vec![0u8; total];
        r[0] = total as u8;
        r[1] = 0; // ext_attributes
        r[2..6].copy_from_slice(&location.to_le_bytes());
        r[10..14].copy_from_slice(&data_length.to_le_bytes());
        r[25] = flags;
        r[32] = name.len() as u8;
        r[name_start..name_start + name.len()].copy_from_slice(name);
        if !tail.is_empty() {
            let tail_start = (name_start + name.len() + 1) & !1;
            r[tail_start..tail_start + tail.len()].copy_from_slice(tail);
        }
        r
    }

    /// 构建最小 ISO 镜像：PVD + 终止符 + 根目录（./../FILE.TXT;1 +
    /// 带 NM 尾的长名条目）+ 文件数据。
    fn build_image() -> Vec<u8> {
        let mut image = vec![0u8; 22 * SECTOR];
        // PVD
        let pvd = PVD_SECTOR * SECTOR;
        image[pvd] = 1;
        image[pvd + 1..pvd + 6].copy_from_slice(b"CD001");
        image[pvd + 6] = 1;
        image[pvd + 128..pvd + 130].copy_from_slice(&2048u16.to_le_bytes());
        // 根目录记录（PVD 偏移 156）：location=ROOT_SECTOR
        let root_rec = dir_record(ROOT_SECTOR as u32, SECTOR as u32, 0x02, b"\x00", &[]);
        image[pvd + 156..pvd + 156 + root_rec.len()].copy_from_slice(&root_rec);
        // 终止符
        let term = TERM_SECTOR * SECTOR;
        image[term] = 255;
        image[term + 1..term + 6].copy_from_slice(b"CD001");
        image[term + 6] = 1;
        // 根目录内容：./../FILE.TXT;1 + NM 长名条目
        let root_start = ROOT_SECTOR * SECTOR;
        let mut at = root_start;
        let dot = dir_record(ROOT_SECTOR as u32, SECTOR as u32, 0x02, b"\x00", &[]);
        image[at..at + dot.len()].copy_from_slice(&dot);
        at += dot.len();
        let dotdot = dir_record(ROOT_SECTOR as u32, SECTOR as u32, 0x02, b"\x01", &[]);
        image[at..at + dotdot.len()].copy_from_slice(&dotdot);
        at += dotdot.len();
        let file = dir_record(FILE_SECTOR as u32, FILE_DATA.len() as u32, 0, b"FILE.TXT;1", &[]);
        image[at..at + file.len()].copy_from_slice(&file);
        at += file.len();
        // NM 长名条目（Rock Ridge）：系统 use 尾 = NM 条目
        // SUSP 布局：签名(2) + 长度(1，含 4 字节头) + 版本(1) + 载荷
        //（NM：标志(1) + 名字）。
        let mut nm_tail = alloc::vec::Vec::new();
        nm_tail.extend_from_slice(b"NM");
        nm_tail.push((4 + 1 + b"reallongname.txt".len()) as u8);
        nm_tail.push(1); // 版本
        nm_tail.push(0); // NM 标志
        nm_tail.extend_from_slice(b"reallongname.txt");
        let long = dir_record(
            21,
            1,
            0,
            b"SHORT.TXT;1",
            &nm_tail,
        );
        image[at..at + long.len()].copy_from_slice(&long);
        // 文件数据
        let data = FILE_SECTOR * SECTOR;
        image[data..data + FILE_DATA.len()].copy_from_slice(FILE_DATA);
        image
    }

    #[test]
    fn test_mount_scans_primary_and_reports_root() {
        let mut server = IsoServer::from_image(build_image(), Options::default_options()).unwrap();
        let mut caps = CapabilityFlags::EMPTY;
        let root = server.mount(0, MountFlags::EMPTY, &mut caps).unwrap();
        assert_eq!(root.inode_number, ROOT_SECTOR as u64);
        assert_eq!(root.mode & 0o170000, 0o040000, "根是目录");
    }

    #[test]
    fn test_mount_refuses_garbage() {
        assert!(IsoServer::from_image(vec![0u8; 22 * SECTOR], Options::default_options()).is_err());
    }

    #[test]
    fn test_lookup_by_interchange_and_rock_ridge_names() {
        let mut server = IsoServer::from_image(build_image(), Options::default_options()).unwrap();
        let mut caps = CapabilityFlags::EMPTY;
        let root = server.mount(0, MountFlags::EMPTY, &mut caps).unwrap();
        let (file, is_dir) = server
            .lookup_child(root.inode_number, "FILE.TXT")
            .unwrap();
        assert!(!is_dir);
        assert_eq!(file.inode_number, FILE_SECTOR as u64);
        assert_eq!(file.size, FILE_DATA.len() as i64);
        // Rock Ridge NM 长名。
        let (long, _) = server
            .lookup_child(root.inode_number, "reallongname.txt")
            .unwrap();
        assert_eq!(long.size, 1);
        // 未命中 → ENOENT。
        assert_eq!(
            server.lookup_child(root.inode_number, "NOPE").unwrap_err().to_i32(),
            ENOENT
        );
    }

    #[test]
    fn test_read_clamps_to_extent() {
        let mut server = IsoServer::from_image(build_image(), Options::default_options()).unwrap();
        let mut caps = CapabilityFlags::EMPTY;
        let root = server.mount(0, MountFlags::EMPTY, &mut caps).unwrap();
        let (file, _) = server.lookup_child(root.inode_number, "FILE.TXT").unwrap();
        let mut got = alloc::vec::Vec::new();
        let n = server
            .read(file.inode_number, 0, 1024, &mut |bytes| got.extend_from_slice(bytes))
            .unwrap();
        assert_eq!(n, FILE_DATA.len());
        assert_eq!(got, FILE_DATA);
        // 越界位置读到零（EOF 而非错误）。
        let n = server.read(file.inode_number, 100, 16, &mut |_| {}).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn test_getdents_lists_all_entries() {
        let mut server = IsoServer::from_image(build_image(), Options::default_options()).unwrap();
        let mut caps = CapabilityFlags::EMPTY;
        let root = server.mount(0, MountFlags::EMPTY, &mut caps).unwrap();
        let mut collected = alloc::vec::Vec::new();
        let mut position = 0i64;
        let n = server
            .get_dents(root.inode_number, &mut position, 4096, &mut |bytes| {
                collected.extend_from_slice(bytes)
            })
            .unwrap();
        assert!(n > 0);
        let blob = String::from_utf8_lossy(&collected).into_owned();
        assert!(blob.contains("FILE.TXT"), "交换名进位图: {blob}");
        assert!(blob.contains("reallongname.txt"), "NM 长名进位图: {blob}");
        assert!(position > 0, "位置推进");
    }

    #[test]
    fn test_stat_reports_readonly_modes() {
        let mut server = IsoServer::from_image(build_image(), Options::default_options()).unwrap();
        let mut caps = CapabilityFlags::EMPTY;
        let root = server.mount(0, MountFlags::EMPTY, &mut caps).unwrap();
        let mut st = Stat::zeroed();
        server.stat(root.inode_number, &mut st).unwrap();
        assert_eq!(st.mode & 0o170000, 0o040000);
        assert_eq!(st.size, SECTOR as i64);
    }

    #[test]
    fn test_norock_option_shows_interchange_name() {
        let mut server = IsoServer::from_image(
            build_image(),
            Options { no_rock_ridge: true },
        )
        .unwrap();
        let mut caps = CapabilityFlags::EMPTY;
        let root = server.mount(0, MountFlags::EMPTY, &mut caps).unwrap();
        // norock 下长名条目只认交换名。
        let (long, _) = server
            .lookup_child(root.inode_number, "SHORT.TXT")
            .unwrap();
        assert_eq!(long.size, 1);
    }

}
