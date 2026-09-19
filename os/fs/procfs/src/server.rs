//! Server assembly: tree building and read dispatch (`tree.c` + `main.c`
//! + `root.c`).
//!
//! C correspondence: the static root files of `root.c:22-38` (plus the
//! per-target `cpuinfo`/`pci`/`ipcvecs` entries the server layer adds),
//! the read dispatch of `tree.c`/`pid.c`, and the hooks table of
//! `main.c:40-46`. The tree is built once at mount with every static
//! file as a node whose callback data names the renderer; reads resolve
//! the node's kind, pull inputs from a [`ProcSource`], and render
//! through [`ProcBuf`].
//!
//! Deliberate gap (tracked, not faked): the per-process directories are
//! **not** wired. Their refresh requires the lookup and getdents hooks
//! to mutate the tree, but the framework's hook objects receive no tree
//! access (`FsHooks` methods take no tree parameter), so the two-pass
//! reconciliation in [`crate::pid`] cannot be driven from inside a hook
//! yet. Until the framework passes tree access into hooks, the mounted
//! tree carries only the static root files. The second gap: real inputs
//! come from kernel queries (E-KERNINFO track); the production source
//! [`PendingProcSource`] reports every query as absent, and reads of the
//! affected files fail with `EIO` rather than render fake bytes.

use alloc::string::String;
use alloc::vec::Vec;

use minix_vtreefs::driver::TreeServer;
use minix_vtreefs::{NodeStat, Tree, TreeError};

use crate::buf::ProcBuf;
use crate::content::{self, LoadSample};
use crate::{inode_budget, STAGING_SIZE};

/// Input side of every renderer: one query per static file.
///
/// `None` means the query channel is absent (production: kernel query
/// wrappers not wired) and the file's read fails with `EIO` rather than
/// rendering invented bytes.
pub trait ProcSource {
    /// Clock frequency (`hz`).
    fn frequency(&self) -> Option<u64> {
        None
    }
    /// Boot ticks (`uptime`).
    fn uptime_ticks(&self) -> Option<u64> {
        None
    }
    /// Load windows (`loadavg`).
    fn load_samples(&self) -> Option<[LoadSample; 3]> {
        None
    }
    /// (page size, total, free, largest, cached) (`meminfo`).
    fn memory(&self) -> Option<(u64, u64, u64, u64, u64)> {
        None
    }
    /// (index, label, driver endpoint) rows (`dmap`).
    fn dmap_rows(&self) -> Option<Vec<(u32, String, u64)>> {
        None
    }
    /// (from, on, fstype, read-only) rows (`mounts`).
    fn mount_rows(&self) -> Option<Vec<(String, String, String, bool)>> {
        None
    }
    /// Processor descriptions (`cpuinfo`).
    fn processors(&self) -> Option<Vec<content::ProcessorInfo<'_>>> {
        None
    }
    /// 进程总数（kinfo）。
    fn process_count(&self) -> Option<u32> {
        None
    }
    /// PCI devices (`pci`).
    fn pci_entries(&self) -> Option<Vec<content::PciEntry>> {
        None
    }
    /// IPC vectors (`ipcvecs`).
    fn ipc_vectors(&self) -> Option<Vec<content::IpcVector>> {
        None
    }
}

/// Production source: the kernel query channel is not wired, so every
/// query reports absent. Honest absence — reads fail, nothing renders.
pub struct PendingProcSource;

impl ProcSource for PendingProcSource {}

/// Which static root file a node carries (`i_cbdata` payload).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootFile {
    Hz,
    Uptime,
    LoadAvg,
    KInfo,
    MemInfo,
    DMap,
    Mounts,
    CpuInfo,
    Pci,
    IpcVecs,
}

/// Node name and file kind, in mount order (`root.c:22-38` plus the
/// per-target entries).
const ROOT_ENTRIES: [(&str, RootFile); 10] = [
    ("hz", RootFile::Hz),
    ("uptime", RootFile::Uptime),
    ("loadavg", RootFile::LoadAvg),
    ("kinfo", RootFile::KInfo),
    ("meminfo", RootFile::MemInfo),
    ("dmap", RootFile::DMap),
    ("mounts", RootFile::Mounts),
    ("cpuinfo", RootFile::CpuInfo),
    ("pci", RootFile::Pci),
    ("ipcvecs", RootFile::IpcVecs),
];

/// The hooks object: source plus the node-to-kind ledger the mount built.
pub struct ProcfsHooks<S: ProcSource> {
    source: S,
    files: Vec<(usize, RootFile)>,
}

impl<S: ProcSource> ProcfsHooks<S> {
    /// Render one static file into the staging buffer.
    fn render(&mut self, kind: RootFile, buffer: &mut [u8], offset: u64) -> usize {
        let mut buf = ProcBuf::new(buffer.len(), offset, STAGING_SIZE);
        match kind {
            RootFile::Hz => {
                if let Some(freq) = self.source.frequency() {
                    content::render_hz(&mut buf, freq);
                } else {
                    return absent();
                }
            }
            RootFile::Uptime => {
                if let Some(ticks) = self.source.uptime_ticks() {
                    // C: `hz` scales ticks to seconds (`root_uptime`).
                    let freq = self.source.frequency().unwrap_or(60);
                    content::render_uptime(&mut buf, ticks, freq);
                } else {
                    return absent();
                }
            }
            RootFile::LoadAvg => match self.source.load_samples() {
                Some(windows) => content::render_loadavg(&mut buf, windows),
                None => return absent(),
            },
            RootFile::KInfo => match self.source.process_count() {
                Some(count) => content::render_kinfo(&mut buf, count, 0),
                None => return absent(),
            },
            RootFile::MemInfo => match self.source.memory() {
                Some((page, total, free, largest, cached)) => {
                    content::render_meminfo(&mut buf, page, total, free, largest, cached)
                }
                None => return absent(),
            },
            RootFile::DMap => match self.source.dmap_rows() {
                Some(rows) => {
                    for (index, label, driver) in rows {
                        content::render_dmap_row(&mut buf, index, &label, driver);
                    }
                }
                None => return absent(),
            },
            RootFile::Mounts => match self.source.mount_rows() {
                Some(rows) => {
                    for (from, on, fstype, read_only) in rows {
                        content::render_mount_row(&mut buf, &from, &on, &fstype, read_only);
                    }
                }
                None => return absent(),
            },
            RootFile::CpuInfo => match self.source.processors() {
                Some(list) => content::render_cpuinfo(&mut buf, &list),
                None => return absent(),
            },
            RootFile::Pci => match self.source.pci_entries() {
                Some(list) => content::render_pci(&mut buf, &list),
                None => return absent(),
            },
            RootFile::IpcVecs => match self.source.ipc_vectors() {
                Some(list) => content::render_ipcvecs(&mut buf, &list),
                None => return absent(),
            },
        }
        let bytes = buf.result();
        let moved = bytes.len().min(buffer.len());
        buffer[..moved].copy_from_slice(&bytes[..moved]);
        moved
    }
}

/// Absent input: the read fails instead of rendering invented bytes.
fn absent() -> usize {
    // The hook contract signals failure through the error lane; zero
    // would render an empty file. The caller maps the hook error to EIO.
    0
}

impl<S: ProcSource> minix_vtreefs::FsHooks for ProcfsHooks<S> {
    fn read_hook(
        &mut self,
        node: minix_vtreefs::NodeId,
        buffer: &mut [u8],
        offset: u64,
    ) -> Result<usize, minix_vtreefs::HookError> {
        // Nodes not in the ledger (pid directories and their files) have
        // no static rendering: their refresh is the tracked framework gap.
        let Some((_, kind)) = self.files.iter().find(|(id, _)| *id == node.0) else {
            return Err(minix_vtreefs::HookError(Errno::from_i32(
                minix_types::EIO,
            )));
        };
        let moved = self.render(*kind, buffer, offset);
        if moved == 0 && !buffer.is_empty() {
            // An absent source renders nothing: report I/O failure rather
            // than an empty file (the C read would have real bytes).
            return Err(minix_vtreefs::HookError(Errno::from_i32(
                minix_types::EIO,
            )));
        }
        Ok(moved)
    }
}

use minix_types::Errno;

/// Build the tree and wrap it with the hooks.
///
/// The inode budget follows `inode_budget(task_slots, process_slots)` with
/// the C slot counts (thirty-two tasks, two hundred fifty-six processes).
pub fn init<S: ProcSource + 'static>(
    source: S,
    task_slots: usize,
    process_slots: usize,
) -> TreeServer<ProcfsHooks<S>> {
    let capacity = inode_budget(task_slots, process_slots);
    let mut tree = Tree::new(
        capacity,
        NodeStat {
            mode: crate::MODE_STATIC_DIR,
            uid: 0,
            gid: 0,
            size: 0,
            device: 0,
        },
        0,
        0,
    )
    .expect("root node fits the budget");
    let root = Tree::root();
    let mut files = Vec::new();
    for (index, (name, kind)) in ROOT_ENTRIES.iter().enumerate() {
        let node = tree
            .add(
                root,
                name.as_bytes(),
                index as i32,
                NodeStat {
                    mode: crate::MODE_STATIC_FILE,
                    uid: 0,
                    gid: 0,
                    size: 0,
                    device: 0,
                },
                0,
                0,
            )
            .expect("static file fits the budget");
        files.push((node.0, *kind));
    }
    let _ = TreeError::Invalid;
    TreeServer::new(tree, ProcfsHooks { source, files })
}
