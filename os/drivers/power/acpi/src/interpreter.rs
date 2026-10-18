//! The ACPICA seam: the interpreter surface this driver consumes.
//!
//! C correspondence: the calls `acpi.c` and `pci.c` make into the vendored
//! ACPI Component Architecture — `AcpiInitializeSubsystem`,
//! `AcpiInitializeTables`, `AcpiLoadTables`, `AcpiEnableSubsystem`,
//! `AcpiInitializeObjects`, `AcpiEvaluateObject` (the `_PIC` call),
//! `AcpiGetDevices`, `AcpiGetParent`, `AcpiGetIrqRoutingTable`,
//! `AcpiGetObjectInfo`, `AcpiGetHandle`, and `AcpiWalkResources`
//! (`acpi.c:26-77`, `pci.c:156-325`).
//!
//! # Why this is a seam (registered `E-ACPICA`)
//!
//! The C driver is 83,279 lines across 212 files: the interpreter is
//! Intel's ACPICA vendored wholesale (`hardware/`, `parser/`, `executer/`,
//! `namespace/`, `utilities/`, `dispatcher/`, `events/`, `tables/`,
//! `resources/`), with 1,548 lines of MINIX glue on top (`acpi.c`,
//! `osminixxf.c`'s forty-eight OS-service functions, `pci.c`). The
//! rewrite models the glue — the gates, the bridge tree, the routing
//! table, and the request loop — and puts the interpreter behind this
//! trait. The three porting options (vendor ACPICA as a C library,
//! reimplement the used subset, or keep the seam with a production leg
//! that wraps one of those) are adjudicated in document
//! `24-misc-drivers.md`; until one lands, [`UnboundAcpi`] refuses every
//! call so the driver reports "ACPI failed" instead of pretending to have
//! read tables it never parsed.

use alloc::string::String;
use alloc::vec::Vec;

/// One device's identity, as much of it as the routing code reads
/// (`ACPI_DEVICE_INFO`, of which `pci.c:186` uses `Address`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceInfo {
    /// The device's address; the upper half is the device number.
    pub address: u32,
}

/// One entry of a bridge's interrupt routing table
/// (`ACPI_PCI_ROUTING_TABLE`, `pci.c:203-228`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutingEntry {
    /// The packed address; the upper half is the device number.
    pub address: u32,
    /// The interrupt pin.
    pub pin: u32,
    /// The source device's name, empty when the entry names its own
    /// interrupt (`pci.c:210`).
    pub source: String,
    /// Which interrupt of the source's resource list applies.
    pub source_index: u32,
}

/// The interpreter surface the driver consumes.
///
/// Every method is fallible: the C calls return `ACPI_STATUS` and the
/// driver branches on `ACPI_SUCCESS` everywhere.
pub trait AcpiInterpreter {
    /// `AcpiInitializeSubsystem`.
    fn initialize_subsystem(&mut self) -> Result<(), i32>;
    /// `AcpiInitializeTables(NULL, 16, FALSE)`: at least sixteen table
    /// slots, no dynamic table allocation.
    fn initialize_tables(&mut self, initial_slots: u32) -> Result<(), i32>;
    /// `AcpiLoadTables`.
    fn load_tables(&mut self) -> Result<(), i32>;
    /// `AcpiEnableSubsystem(0)`.
    fn enable_subsystem(&mut self, flags: u32) -> Result<(), i32>;
    /// `AcpiInitializeObjects(0)`.
    fn initialize_objects(&mut self, flags: u32) -> Result<(), i32>;
    /// `AcpiEvaluateObject(ACPI_ROOT_OBJECT, "_PIC", args)`: report the
    /// machine mode to the firmware.
    fn evaluate_pic(&mut self, apic_enabled: bool) -> Result<(), i32>;
    /// `AcpiGetDevices(id, ...)`: the handles whose hardware identifier
    /// is `id` (or every device when the identifier is empty).
    fn devices(&mut self, id: &str) -> Result<Vec<i32>, i32>;
    /// The devices whose parent is `parent` (`AcpiGetDevices(NULL, ...)`
    /// filtered by `AcpiGetParent`, which is what `add_pci_dev` selects,
    /// `pci.c:250-262`).
    fn devices_with_parent(&mut self, parent: i32) -> Result<Vec<i32>, i32>;
    /// `AcpiGetParent`.
    fn parent(&mut self, handle: i32) -> Result<i32, i32>;
    /// `AcpiGetIrqRoutingTable`.
    fn irq_routing_table(&mut self, handle: i32) -> Result<Vec<RoutingEntry>, i32>;
    /// `AcpiGetObjectInfo`, of which the routing code reads the address.
    fn object_info(&mut self, handle: i32) -> Result<DeviceInfo, i32>;
    /// `AcpiGetHandle(parent, name)`.
    fn get_handle(&mut self, parent: i32, name: &str) -> Result<i32, i32>;
    /// `AcpiWalkResources(handle, "_CRS", ...)`: the interrupts each
    /// resource carries (`pci.c:144-155` reads the first of a source's
    /// interrupts).
    fn walk_resources(&mut self, handle: i32, method: &str) -> Result<Vec<u32>, i32>;
}

/// The production stand-in: every call refuses.
///
/// Registered `E-ACPICA`: with no interpreter in the tree the driver must
/// report "ACPI failed" rather than claim to have parsed tables. C's
/// equivalent failure path is `ACPI: ACPI failed with err %d`
/// (`acpi.c:100-103`).
pub struct UnboundAcpi;

impl AcpiInterpreter for UnboundAcpi {
    fn initialize_subsystem(&mut self) -> Result<(), i32> {
        Err(-minix_types::ENOSYS)
    }
    fn initialize_tables(&mut self, _initial_slots: u32) -> Result<(), i32> {
        Err(-minix_types::ENOSYS)
    }
    fn load_tables(&mut self) -> Result<(), i32> {
        Err(-minix_types::ENOSYS)
    }
    fn enable_subsystem(&mut self, _flags: u32) -> Result<(), i32> {
        Err(-minix_types::ENOSYS)
    }
    fn initialize_objects(&mut self, _flags: u32) -> Result<(), i32> {
        Err(-minix_types::ENOSYS)
    }
    fn evaluate_pic(&mut self, _apic_enabled: bool) -> Result<(), i32> {
        Err(-minix_types::ENOSYS)
    }
    fn devices(&mut self, _id: &str) -> Result<Vec<i32>, i32> {
        Err(-minix_types::ENOSYS)
    }
    fn devices_with_parent(&mut self, _parent: i32) -> Result<Vec<i32>, i32> {
        Err(-minix_types::ENOSYS)
    }
    fn parent(&mut self, _handle: i32) -> Result<i32, i32> {
        Err(-minix_types::ENOSYS)
    }
    fn irq_routing_table(&mut self, _handle: i32) -> Result<Vec<RoutingEntry>, i32> {
        Err(-minix_types::ENOSYS)
    }
    fn object_info(&mut self, _handle: i32) -> Result<DeviceInfo, i32> {
        Err(-minix_types::ENOSYS)
    }
    fn get_handle(&mut self, _parent: i32, _name: &str) -> Result<i32, i32> {
        Err(-minix_types::ENOSYS)
    }
    fn walk_resources(&mut self, _handle: i32, _method: &str) -> Result<Vec<u32>, i32> {
        Err(-minix_types::ENOSYS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_unbound_interpreter_refuses_every_call() {
        let mut interpreter = UnboundAcpi;
        assert_eq!(
            interpreter.initialize_subsystem(),
            Err(-minix_types::ENOSYS)
        );
        assert_eq!(interpreter.initialize_tables(16), Err(-minix_types::ENOSYS));
        assert_eq!(interpreter.load_tables(), Err(-minix_types::ENOSYS));
        assert_eq!(interpreter.enable_subsystem(0), Err(-minix_types::ENOSYS));
        assert_eq!(interpreter.initialize_objects(0), Err(-minix_types::ENOSYS));
        assert_eq!(interpreter.evaluate_pic(true), Err(-minix_types::ENOSYS));
        assert_eq!(interpreter.devices("PNP0A03"), Err(-minix_types::ENOSYS));
        assert_eq!(
            interpreter.devices_with_parent(1),
            Err(-minix_types::ENOSYS)
        );
        assert_eq!(interpreter.parent(1), Err(-minix_types::ENOSYS));
        assert_eq!(interpreter.irq_routing_table(1), Err(-minix_types::ENOSYS));
        assert_eq!(interpreter.object_info(1), Err(-minix_types::ENOSYS));
        assert_eq!(interpreter.get_handle(1, "LNK0"), Err(-minix_types::ENOSYS));
        assert_eq!(
            interpreter.walk_resources(1, "_CRS"),
            Err(-minix_types::ENOSYS)
        );
    }
}
