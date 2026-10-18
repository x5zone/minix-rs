//! The PCI bridge tree and the interrupt routing table.
//!
//! C correspondence: `minix3/minix/drivers/power/acpi/pci.c` — the bridge
//! record (`struct pci_bridge`, `:16-24`), the two-mode search
//! (`find_bridge`, `:31-69`), the bridge-mapping handler (`do_map_bridge`,
//! `:71-87`), the IRQ lookup (`do_get_irq`, `:116-134`), the routing table
//! walk (`get_pci_irq_routing`, `:156-230`), and the table's
//! initialization (`bridge_init_irqtable`, `:232-240`).
//!
//! The tree is the MINIX side's own data structure: ACPICA hands over
//! handles, routing tables, and `_CRS` resources through the seam in
//! [`crate::interpreter`], and everything below is this driver's logic.

use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::interpreter::AcpiInterpreter;
use crate::protocol::{IRQ_TABLE_ENTRIES, PCI_MAX_DEVICES, PCI_MAX_PINS};

/// One PCI bridge (`struct pci_bridge`, `pci.c:16-24`).
///
/// The C keeps a parent pointer and an array of child pointers; the
/// rewrite keeps the children (the parent link is only used while
/// attaching, which [`PciBridge::attach`] does by search).
pub struct PciBridge {
    /// The interpreter's handle for the bridge's ACPI device.
    pub handle: i32,
    /// The interrupt routing table: device times four plus pin
    /// (`irqtable`, indexed `dev * PCI_MAX_PINS + pin`).
    pub irqtable: [i32; IRQ_TABLE_ENTRIES],
    /// The bridge's primary bus.
    pub primary_bus: i32,
    /// The bridge's secondary bus.
    pub secondary_bus: i32,
    /// The device number the bridge answers on (`device`).
    pub device: u32,
    /// The bridges behind this one, indexed by device number.
    pub children: [Option<Box<PciBridge>>; PCI_MAX_DEVICES],
}

impl PciBridge {
    /// A bridge with an empty routing table (`bridge_init_irqtable`,
    /// `pci.c:232-240`: every entry starts at minus one, "no route").
    pub fn new(handle: i32) -> Self {
        PciBridge {
            handle,
            irqtable: [-1; IRQ_TABLE_ENTRIES],
            primary_bus: -1,
            secondary_bus: -1,
            device: 0,
            children: core::array::from_fn(|_| None),
        }
    }

    /// Record one route (`add_irq`, `pci.c:136-142`).
    pub fn add_irq(&mut self, dev: u32, pin: u32, irq: i32) {
        let index = dev as usize * PCI_MAX_PINS + pin as usize;
        if index < IRQ_TABLE_ENTRIES {
            self.irqtable[index] = irq;
        }
    }

    /// One route (`irqtable[dev * PCI_MAX_PINS + pin]`).
    pub fn irq(&self, dev: u32, pin: u32) -> i32 {
        let index = dev as usize * PCI_MAX_PINS + pin as usize;
        if index < IRQ_TABLE_ENTRIES {
            self.irqtable[index]
        } else {
            -1
        }
    }

    /// The two-mode search (`find_bridge`, `pci.c:31-69`).
    ///
    /// With `sbnr` at minus one the search walks by primary bus and takes
    /// the child at `dev`; otherwise it looks for the bridge whose
    /// secondary bus is `sbnr`. Either way the walk is depth-first over
    /// the children.
    pub fn find(&self, pbnr: i32, dev: i32, sbnr: i32) -> Option<&PciBridge> {
        if sbnr == -1 {
            if self.secondary_bus == pbnr {
                return self.children.get(dev as usize).and_then(|c| c.as_deref());
            }
            for child in self.children.iter().flatten() {
                if let Some(found) = child.find(pbnr, dev, sbnr) {
                    return Some(found);
                }
            }
        } else {
            if self.secondary_bus == sbnr {
                return Some(self);
            }
            for child in self.children.iter().flatten() {
                if let Some(found) = child.find(pbnr, dev, sbnr) {
                    return Some(found);
                }
            }
        }
        None
    }

    /// The same search, mutably.
    pub fn find_mut(&mut self, pbnr: i32, dev: i32, sbnr: i32) -> Option<&mut PciBridge> {
        if sbnr == -1 {
            if self.secondary_bus == pbnr {
                return self
                    .children
                    .get_mut(dev as usize)
                    .and_then(|c| c.as_deref_mut());
            }
            for child in self.children.iter_mut().flatten() {
                if let Some(found) = child.find_mut(pbnr, dev, sbnr) {
                    return Some(found);
                }
            }
        } else {
            if self.secondary_bus == sbnr {
                return Some(self);
            }
            for child in self.children.iter_mut().flatten() {
                if let Some(found) = child.find_mut(pbnr, dev, sbnr) {
                    return Some(found);
                }
            }
        }
        None
    }
}

/// What one scan found: the root bridge and whether more than one root
/// existed (`add_pci_root_dev`'s `called` counter, `pci.c:283-287`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanResult {
    /// A second PCI root bridge was found and ignored.
    pub multi_root: bool,
}

/// The bridge tree's root plus the scan verdict.
pub struct Routing {
    /// The root bridge (`pci_root_bridge`, `pci.c:26`).
    pub root: PciBridge,
    /// The scan's verdict.
    pub scan: ScanResult,
}

impl Routing {
    /// Scans the namespace for the PCI root bridge and its children
    /// (`pci_scan_devices` with `add_pci_root_dev` and `add_pci_dev`,
    /// `pci.c:242-325`).
    ///
    /// The C walks the namespace with callbacks and recurses into every
    /// bridge; the rewrite asks the seam for the devices whose parent is
    /// the bridge it is filling, which is what the callbacks select
    /// (`add_pci_dev` skips anything with a different parent).
    pub fn scan<I: AcpiInterpreter>(interpreter: &mut I) -> Result<Routing, i32> {
        let roots = interpreter.devices("PNP0A03")?;
        let multi_root = roots.len() > 1;
        let Some(root_handle) = roots.first().copied() else {
            // C asserts on a failed scan (`pci.c:322`); a machine with no
            // PCI root is reported instead of aborting.
            return Err(-minix_types::ENODEV);
        };
        let mut root = PciBridge::new(root_handle);
        root.primary_bus = -1;
        root.secondary_bus = 0;
        fill_routing(interpreter, &mut root)?;
        fill_children(interpreter, &mut root)?;
        Ok(Routing {
            root,
            scan: ScanResult { multi_root },
        })
    }
}

/// Fills one bridge's routing table (`get_pci_irq_routing`, `pci.c:156-230`).
fn fill_routing<I: AcpiInterpreter>(
    interpreter: &mut I,
    bridge: &mut PciBridge,
) -> Result<(), i32> {
    let table = match interpreter.irq_routing_table(bridge.handle) {
        Ok(table) => table,
        // C treats a failed routing-table fetch as "nothing to do"
        // (`pci.c:164-166`).
        Err(_) => return Ok(()),
    };
    let info = interpreter.object_info(bridge.handle)?;
    bridge.device = info.address >> 16;
    for entry in table {
        if entry.source.is_empty() {
            // An empty source names the entry's own interrupt
            // (`pci.c:210-214`).
            bridge.add_irq(entry.address >> 16, entry.pin, entry.source_index as i32);
            continue;
        }
        let Ok(source_handle) = interpreter.get_handle(bridge.handle, &entry.source) else {
            // C prints and skips the entry (`pci.c:217-221`).
            continue;
        };
        let Ok(interrupts) = interpreter.walk_resources(source_handle, "_CRS") else {
            // C prints and skips the entry (`pci.c:224-228`).
            continue;
        };
        if let Some(irq) = interrupts.first() {
            bridge.add_irq(entry.address >> 16, entry.pin, *irq as i32);
        }
    }
    Ok(())
}

/// Fills one bridge's children (`add_pci_dev`'s recursion,
/// `pci.c:242-279`).
fn fill_children<I: AcpiInterpreter>(
    interpreter: &mut I,
    bridge: &mut PciBridge,
) -> Result<(), i32> {
    let handles = interpreter.devices_with_parent(bridge.handle)?;
    for handle in handles {
        let mut child = PciBridge::new(handle);
        fill_routing(interpreter, &mut child)?;
        child.primary_bus = -1;
        child.secondary_bus = -1;
        let device = child.device as usize;
        if device < PCI_MAX_DEVICES {
            let _ = fill_children(interpreter, &mut child);
            bridge.children[device] = Some(Box::new(child));
        }
    }
    Ok(())
}

/// The routing entries one bridge reported, for tests and diagnostics.
pub fn routes(bridge: &PciBridge) -> Vec<(u32, u32, i32)> {
    let mut found = Vec::new();
    for dev in 0..PCI_MAX_DEVICES as u32 {
        for pin in 0..PCI_MAX_PINS as u32 {
            let irq = bridge.irq(dev, pin);
            if irq != -1 {
                found.push((dev, pin, irq));
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::{DeviceInfo, RoutingEntry};
    use alloc::string::String;
    use alloc::vec;

    /// A scripted interpreter: a namespace of devices with routing tables.
    struct FakeInterpreter {
        /// The root bridge's handle.
        root: i32,
        /// Every device: handle, parent, address, routing table.
        devices: Vec<(i32, i32, u32, Vec<RoutingEntry>)>,
        /// The `_CRS` interrupts keyed by source handle.
        resources: Vec<(i32, Vec<u32>)>,
    }

    impl FakeInterpreter {
        fn new() -> Self {
            FakeInterpreter {
                root: 1,
                devices: Vec::new(),
                resources: Vec::new(),
            }
        }
    }

    impl AcpiInterpreter for FakeInterpreter {
        fn initialize_subsystem(&mut self) -> Result<(), i32> {
            Ok(())
        }
        fn initialize_tables(&mut self, _initial_slots: u32) -> Result<(), i32> {
            Ok(())
        }
        fn load_tables(&mut self) -> Result<(), i32> {
            Ok(())
        }
        fn enable_subsystem(&mut self, _flags: u32) -> Result<(), i32> {
            Ok(())
        }
        fn initialize_objects(&mut self, _flags: u32) -> Result<(), i32> {
            Ok(())
        }
        fn evaluate_pic(&mut self, _apic_enabled: bool) -> Result<(), i32> {
            Ok(())
        }
        fn devices(&mut self, id: &str) -> Result<Vec<i32>, i32> {
            if id == "PNP0A03" {
                Ok(vec![self.root])
            } else {
                Ok(Vec::new())
            }
        }
        fn devices_with_parent(&mut self, parent: i32) -> Result<Vec<i32>, i32> {
            Ok(self
                .devices
                .iter()
                .filter(|(_, p, _, _)| *p == parent)
                .map(|(handle, _, _, _)| *handle)
                .collect())
        }
        fn parent(&mut self, handle: i32) -> Result<i32, i32> {
            self.devices
                .iter()
                .find(|(h, _, _, _)| *h == handle)
                .map(|(_, p, _, _)| *p)
                .ok_or(-minix_types::ENODEV)
        }
        fn irq_routing_table(&mut self, handle: i32) -> Result<Vec<RoutingEntry>, i32> {
            self.devices
                .iter()
                .find(|(h, _, _, _)| *h == handle)
                .map(|(_, _, _, table)| table.clone())
                .ok_or(-minix_types::ENODEV)
        }
        fn object_info(&mut self, handle: i32) -> Result<DeviceInfo, i32> {
            self.devices
                .iter()
                .find(|(h, _, _, _)| *h == handle)
                .map(|(_, _, address, _)| DeviceInfo { address: *address })
                .ok_or(-minix_types::ENODEV)
        }
        fn get_handle(&mut self, _parent: i32, name: &str) -> Result<i32, i32> {
            // The sources are named "LNK0" and answer on handle 100.
            if name.starts_with("LNK") {
                Ok(100)
            } else {
                Err(-minix_types::ENODEV)
            }
        }
        fn walk_resources(&mut self, handle: i32, _method: &str) -> Result<Vec<u32>, i32> {
            self.resources
                .iter()
                .find(|(h, _)| *h == handle)
                .map(|(_, interrupts)| interrupts.clone())
                .ok_or(-minix_types::ENODEV)
        }
    }

    #[test]
    fn test_the_root_scan_reads_its_routing_table() {
        let mut interpreter = FakeInterpreter::new();
        // The root has one direct route (device 2, pin 1, IRQ 9) and one
        // route through a link device (device 3, pin 0, IRQ 11).
        interpreter.devices.push((
            1,
            0,
            0,
            vec![
                RoutingEntry {
                    address: 2 << 16,
                    pin: 1,
                    source: String::new(),
                    source_index: 9,
                },
                RoutingEntry {
                    address: 3 << 16,
                    pin: 0,
                    source: String::from("LNK0"),
                    source_index: 0,
                },
            ],
        ));
        interpreter.resources.push((100, vec![11]));
        let routing = Routing::scan(&mut interpreter).expect("the root answers");
        assert!(!routing.scan.multi_root);
        assert_eq!(routing.root.handle, 1);
        assert_eq!(routing.root.primary_bus, -1);
        assert_eq!(routing.root.secondary_bus, 0);
        assert_eq!(routing.root.irq(2, 1), 9, "the direct route");
        assert_eq!(routing.root.irq(3, 0), 11, "the linked route");
        assert_eq!(routing.root.irq(0, 0), -1, "an unrouted device");
    }

    #[test]
    fn test_a_second_root_is_reported_and_ignored() {
        struct TwoRoots;
        impl AcpiInterpreter for TwoRoots {
            fn initialize_subsystem(&mut self) -> Result<(), i32> {
                Ok(())
            }
            fn initialize_tables(&mut self, _s: u32) -> Result<(), i32> {
                Ok(())
            }
            fn load_tables(&mut self) -> Result<(), i32> {
                Ok(())
            }
            fn enable_subsystem(&mut self, _f: u32) -> Result<(), i32> {
                Ok(())
            }
            fn initialize_objects(&mut self, _f: u32) -> Result<(), i32> {
                Ok(())
            }
            fn evaluate_pic(&mut self, _a: bool) -> Result<(), i32> {
                Ok(())
            }
            fn devices(&mut self, id: &str) -> Result<Vec<i32>, i32> {
                if id == "PNP0A03" {
                    Ok(vec![1, 2])
                } else {
                    Ok(Vec::new())
                }
            }
            fn devices_with_parent(&mut self, _p: i32) -> Result<Vec<i32>, i32> {
                Ok(Vec::new())
            }
            fn parent(&mut self, _h: i32) -> Result<i32, i32> {
                Err(-minix_types::ENODEV)
            }
            fn irq_routing_table(&mut self, _h: i32) -> Result<Vec<RoutingEntry>, i32> {
                Ok(Vec::new())
            }
            fn object_info(&mut self, _h: i32) -> Result<DeviceInfo, i32> {
                Ok(DeviceInfo { address: 0 })
            }
            fn get_handle(&mut self, _p: i32, _n: &str) -> Result<i32, i32> {
                Err(-minix_types::ENODEV)
            }
            fn walk_resources(&mut self, _h: i32, _m: &str) -> Result<Vec<u32>, i32> {
                Err(-minix_types::ENODEV)
            }
        }
        let routing = Routing::scan(&mut TwoRoots).expect("the first root answers");
        assert!(routing.scan.multi_root, "the second root was seen");
        assert_eq!(routing.root.handle, 1, "the first root is the tree");
    }

    #[test]
    fn test_the_two_search_modes_match_c() {
        // A root whose secondary bus is zero, with a bridge at device 1.
        let mut root = PciBridge::new(1);
        root.secondary_bus = 0;
        let mut child = Box::new(PciBridge::new(2));
        child.device = 1;
        child.secondary_bus = 5;
        root.children[1] = Some(child);
        // By secondary bus: the child is found.
        assert_eq!(root.find(-1, -1, 5).map(|b| b.handle), Some(2));
        // By primary bus and device: the root's secondary bus is zero, so
        // device 1 of bus zero is the child.
        assert_eq!(root.find(0, 1, -1).map(|b| b.handle), Some(2));
        // A bus nobody carries has no bridge.
        assert!(root.find(-1, -1, 9).is_none());
        assert!(root.find(7, 1, -1).is_none());
    }

    #[test]
    fn test_the_routing_table_starts_at_minus_one() {
        let bridge = PciBridge::new(1);
        assert_eq!(bridge.irqtable.len(), IRQ_TABLE_ENTRIES);
        assert!(bridge.irqtable.iter().all(|entry| *entry == -1));
        assert!(routes(&bridge).is_empty());
    }

    #[test]
    fn test_the_children_carry_their_own_routing_tables() {
        let mut interpreter = FakeInterpreter::new();
        interpreter.devices.push((1, 0, 0, Vec::new()));
        // A bridge behind the root, at device 4.
        interpreter.devices.push((
            2,
            1,
            4 << 16,
            vec![RoutingEntry {
                address: 0,
                pin: 2,
                source: String::new(),
                source_index: 7,
            }],
        ));
        let routing = Routing::scan(&mut interpreter).expect("the root answers");
        let child = routing.root.find(0, 4, -1).expect("the bridge is there");
        assert_eq!(child.handle, 2);
        assert_eq!(child.irq(0, 2), 7);
    }
}
