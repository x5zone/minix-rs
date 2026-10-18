//! Service assembly: the GPIO driver as a virtual-tree filesystem.
//!
//! C correspondence: `init_hook` (`gpio.c:136-190`) claims pins and
//! publishes the per-pin files, `read_hook` (`gpio.c:190-262`) turns a
//! read of an action file into a drive and a read of a level or interrupt
//! file into a rendered value, and `main` (`gpio.c:272-289`) runs the
//! virtual tree. The vtreefs runtime and the VFS wire live in
//! `minix-vtreefs` and `minix-fs-rt`; this module owns the node table and
//! the read hook.
//!
//! Board tables: C exports pins only on BBXM and BB boards (`init_hook`'s
//! `sys_getmachine` branches, including the pad-configuration calls). The
//! generic build passes an empty export list — exactly the tree C builds
//! when neither board matches: an empty root. Board identification and pad
//! configuration ride the board-support seam.
//!
//! The files are read-only, as in C: there is no write hook, and reading
//! `nameOn`/`nameOff` is what drives the pin. A claimed pin keeps its mode;
//! a claim that fails (already claimed) skips that pin, matching C's
//! `add_gpio_inode` refusal.

use minix_types::{Errno, EIO};
use minix_vtreefs::{
    driver::TreeServer, FsHooks, HookError, NodeId, NodeStat, Tree, NO_INDEX,
};

use crate::files::{plan, render, ExportedFile, ReadPlan};
use crate::pins::{PinDb, PinHardware, PinMode, FILESYSTEM_OWNER};

/// Inode budget (`run_vtreefs(&hooks, 30, ...)`, `gpio.c:287`).
pub const INODE_BUDGET: usize = 30;

/// Root directory mode (`S_IFDIR | S_IRUSR | S_IRGRP | S_IROTH`).
pub const MODE_ROOT: u16 = 0o040444;

/// Exported file mode (`default_file_stat.mode`: `S_IFREG | 04`).
pub const MODE_FILE: u16 = 0o100004;

/// One pin the service exports (C's `add_gpio_inode` call).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Export {
    /// File base name (C passes it as the inode name).
    pub name: &'static str,
    /// Pin number.
    pub pin: u32,
    /// Mode programmed at claim time.
    pub mode: PinMode,
}

/// The callback tag C stores per inode (`GPIO_CB_*`, `gpio.c:36-39`).
///
/// The hook itself resolves nodes through [`GpioHooks::files`]; the tag
/// keeps the same encoding in the tree so a future extra-storage port has
/// one vocabulary.
const fn cbdata(pin: u32, file: ExportedFile) -> u64 {
    let kind = match file {
        ExportedFile::Read => 0,
        ExportedFile::Interrupt => 1,
        ExportedFile::TurnOn => 2,
        ExportedFile::TurnOff => 3,
    };
    ((pin as u64) << 2) | kind
}

fn io_error() -> HookError {
    HookError(Errno::from_i32(EIO))
}

fn file_stat() -> NodeStat {
    NodeStat {
        mode: MODE_FILE,
        uid: 0,
        gid: 0,
        size: 0,
        device: 0,
    }
}

/// The GPIO service hooks: the pin database plus the node table.
pub struct GpioHooks<H: PinHardware> {
    pins: PinDb<H>,
    /// Live file nodes: tree slot, pin, which export.
    files: alloc::vec::Vec<(usize, u32, ExportedFile)>,
}

impl<H: PinHardware> GpioHooks<H> {
    /// The pin database (tests inspect claims and hardware).
    pub fn pins(&self) -> &PinDb<H> {
        &self.pins
    }

    /// The pin database, mutably.
    pub fn pins_mut(&mut self) -> &mut PinDb<H> {
        &mut self.pins
    }
}

impl<H: PinHardware> FsHooks for GpioHooks<H> {
    fn read_hook(
        &mut self,
        node: NodeId,
        buffer: &mut [u8],
        offset: u64,
    ) -> Result<usize, HookError> {
        let Some((_, pin, file)) = self.files.iter().find(|(slot, _, _)| *slot == node.0) else {
            return Err(io_error());
        };
        let (pin, file) = (*pin, *file);
        let value = match plan(file) {
            ReadPlan::DriveHigh => {
                self.pins.drive(pin, true).map_err(|_| io_error())?;
                return Ok(0);
            }
            ReadPlan::DriveLow => {
                self.pins.drive(pin, false).map_err(|_| io_error())?;
                return Ok(0);
            }
            ReadPlan::SampleLevel => self.pins.read(pin).map_err(|_| io_error())?,
            ReadPlan::SampleInterrupt => self.pins.read_interrupt(pin).map_err(|_| io_error())?,
        };
        let (first, second, total) = render(value, offset as usize);
        if total == 0 {
            return Ok(0);
        }
        let mut staged = [0u8; 2];
        staged[0] = first;
        if let Some(byte) = second {
            staged[1] = byte;
        }
        let take = total.min(buffer.len());
        buffer[..take].copy_from_slice(&staged[..take]);
        Ok(take)
    }
}

/// Add one exported file node under the root.
fn add_export(
    tree: &mut Tree,
    files: &mut alloc::vec::Vec<(usize, u32, ExportedFile)>,
    root: NodeId,
    export: &Export,
    file: ExportedFile,
) {
    let name = ExportedFile::file_name(export.name, file);
    if let Ok(node) = tree.add(
        root,
        name.as_bytes(),
        NO_INDEX,
        file_stat(),
        0,
        cbdata(export.pin, file),
    ) {
        files.push((node.0, export.pin, file));
    }
    // A node that does not fit the inode budget is dropped; C's
    // `add_inode` has the same ceiling and no recovery path.
}

/// Build the tree from a board and its export table.
///
/// Claims and mode programming happen here, once, as in C's `init_hook`.
/// A claim refusal skips the pin (C returns `EIO` from `add_gpio_inode`
/// and its caller ignores the result).
pub fn init_with<H: PinHardware>(board: H, exports: &[Export]) -> TreeServer<GpioHooks<H>> {
    let mut tree = Tree::new(
        INODE_BUDGET,
        NodeStat {
            mode: MODE_ROOT,
            uid: 0,
            gid: 0,
            size: 0,
            device: 0,
        },
        0,
        0,
    )
    .expect("the root node fits the inode budget");
    let root = Tree::root();
    let mut pins = PinDb::new(board);
    let mut files: alloc::vec::Vec<(usize, u32, ExportedFile)> = alloc::vec::Vec::new();
    for export in exports {
        if pins
            .claim(FILESYSTEM_OWNER, export.pin, export.mode)
            .is_err()
        {
            continue;
        }
        add_export(&mut tree, &mut files, root, export, ExportedFile::Read);
        match export.mode {
            PinMode::Output => {
                add_export(&mut tree, &mut files, root, export, ExportedFile::TurnOn);
                add_export(&mut tree, &mut files, root, export, ExportedFile::TurnOff);
            }
            PinMode::Input => {
                add_export(&mut tree, &mut files, root, export, ExportedFile::Interrupt);
            }
        }
    }
    TreeServer::new(tree, GpioHooks { pins, files })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    use minix_fs::driver::FsDriver;
    use minix_fs::protocol::{CapabilityFlags, MountFlags};

    use crate::pins::MemBoard;

    fn mount<H: PinHardware>(server: &mut TreeServer<GpioHooks<H>>) -> u64 {
        let mut capabilities = CapabilityFlags::EMPTY;
        server
            .mount(0, MountFlags::EMPTY, &mut capabilities)
            .expect("the root mounts")
            .inode_number
    }

    fn read_file<H: PinHardware>(
        server: &mut TreeServer<GpioHooks<H>>,
        root: u64,
        name: &str,
        offset: i64,
    ) -> Result<Vec<u8>, Errno> {
        let (node, _) = server.lookup_child(root, name)?;
        let mut bytes = Vec::new();
        server.read(node.inode_number, offset, 16, &mut |chunk| {
            bytes.extend_from_slice(chunk)
        })?;
        Ok(bytes)
    }

    #[test]
    fn test_output_pin_exports_read_on_and_off() {
        let exports = [Export {
            name: "LED",
            pin: 2,
            mode: PinMode::Output,
        }];
        let mut server = init_with(MemBoard::default(), &exports);
        let root = mount(&mut server);

        assert!(read_file(&mut server, root, "LEDOn", 0).unwrap().is_empty());
        assert_eq!(read_file(&mut server, root, "LED", 0).unwrap(), b"1\n");
        assert!(read_file(&mut server, root, "LEDOff", 0).unwrap().is_empty());
        assert_eq!(read_file(&mut server, root, "LED", 0).unwrap(), b"0\n");
        assert!(
            read_file(&mut server, root, "LEDIntr", 0).is_err(),
            "outputs export no interrupt file"
        );
    }

    #[test]
    fn test_input_pin_exports_the_interrupt_file_and_clears_on_read() {
        let exports = [Export {
            name: "BTN",
            pin: 1,
            mode: PinMode::Input,
        }];
        let mut server = init_with(MemBoard::default(), &exports);
        let root = mount(&mut server);
        assert_eq!(read_file(&mut server, root, "BTNIntr", 0).unwrap(), b"0\n");

        server
            .hooks_mut()
            .pins_mut()
            .hardware_mut()
            .raise_interrupt(1);
        assert_eq!(read_file(&mut server, root, "BTNIntr", 0).unwrap(), b"1\n");
        assert_eq!(read_file(&mut server, root, "BTNIntr", 0).unwrap(), b"0\n");
        assert!(
            read_file(&mut server, root, "BTNOn", 0).is_err(),
            "inputs export no action files"
        );
    }

    #[test]
    fn test_read_offset_rule_matches_the_c_rendering() {
        let exports = [Export {
            name: "PIN",
            pin: 3,
            mode: PinMode::Input,
        }];
        let mut server = init_with(MemBoard::default(), &exports);
        let root = mount(&mut server);
        assert_eq!(read_file(&mut server, root, "PIN", 0).unwrap(), b"0\n");
        assert_eq!(read_file(&mut server, root, "PIN", 1).unwrap(), b"\n");
        assert!(read_file(&mut server, root, "PIN", 2).unwrap().is_empty());
    }

    #[test]
    fn test_duplicate_claim_skips_the_pin() {
        let exports = [
            Export {
                name: "A",
                pin: 5,
                mode: PinMode::Output,
            },
            Export {
                name: "B",
                pin: 5,
                mode: PinMode::Output,
            },
        ];
        let mut server = init_with(MemBoard::default(), &exports);
        let root = mount(&mut server);
        assert!(read_file(&mut server, root, "A", 0).is_ok());
        assert!(
            read_file(&mut server, root, "B", 0).is_err(),
            "the second claim on one pin is refused"
        );
    }

    #[test]
    fn test_empty_board_table_serves_an_empty_root() {
        let mut server = crate::init();
        let root = mount(&mut server);
        assert!(read_file(&mut server, root, "USR0", 0).is_err());
    }
}
