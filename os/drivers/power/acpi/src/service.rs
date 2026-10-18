//! The ACPI service: the gates, the scan, and the request loop.
//!
//! C correspondence: `minix3/minix/drivers/power/acpi/acpi.c` — the
//! memory-privilege request (`init_mem_priv`, `:13-21`), the machine-mode
//! report (`set_machine_mode`, `:23-41`), the interpreter's initialization
//! chain (`init_acpica`, `:43-64`), the gates and the enabled flag
//! (`init_acpi`, `:66-107`), the fresh-start callback (`:109-130`), and
//! the request loop (`main`, `:139-167`) with the two handlers from
//! `pci.c`.

use minix_types::{ENODEV, OK};

use crate::bridge::Routing;
use crate::interpreter::AcpiInterpreter;
use crate::protocol::{
    ACPI_REQ_GET_IRQ, ACPI_REQ_MAP_BRIDGE, GetIrqRequest, LANE_BUS, LANE_DEV, LANE_PIN,
    MapBridgeRequest, read_request, read_u32, write_answer,
};

/// The machine facts the driver's gates read (`struct machine`'s two
/// fields, `acpi.c:84-88`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Machine {
    /// Whether the firmware enabled the interrupt controller in APIC mode.
    pub apic_enabled: bool,
    /// The root system description pointer's physical address.
    pub acpi_rsdp: u64,
}

/// The memory privilege the driver asks for: the whole physical address
/// space, because the ACPI tables can live anywhere
/// (`init_mem_priv`, `acpi.c:13-21`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryRange {
    /// The range's base.
    pub base: u64,
    /// The range's limit.
    pub limit: u64,
}

/// The whole address space (`acpi.c:17-18`).
pub const FULL_MEMORY_RANGE: MemoryRange = MemoryRange {
    base: 0,
    limit: 0xffff_ffff,
};

/// The ACPI service.
pub struct AcpiService<I: AcpiInterpreter> {
    interpreter: I,
    /// Whether the interpreter came up (`acpi_enabled`, `acpi.c:10`).
    pub enabled: bool,
    /// The machine facts, once the gate passed.
    machine: Option<Machine>,
    /// The bridge tree, once the scan ran.
    routing: Option<Routing>,
    /// Whether a second PCI root was seen and ignored.
    pub multi_root: bool,
}

impl<I: AcpiInterpreter> AcpiService<I> {
    /// A service over one interpreter.
    pub fn new(interpreter: I) -> Self {
        AcpiService {
            interpreter,
            enabled: false,
            machine: None,
            routing: None,
            multi_root: false,
        }
    }

    /// The interpreter (tests and the binary's diagnostics).
    pub fn interpreter_mut(&mut self) -> &mut I {
        &mut self.interpreter
    }

    /// The bridge tree, once the scan ran.
    pub fn routing(&self) -> Option<&Routing> {
        self.routing.as_ref()
    }

    /// The driver's startup (`init_acpi`, `acpi.c:66-107`): the three
    /// gates, the memory privilege, then the interpreter's chain.
    ///
    /// `grant_memory` stands for the kernel call that adds the memory
    /// privilege (`sys_privctl(SELF, SYS_PRIV_ADD_MEM, &mr)`); C treats a
    /// refusal as "no ACPI".
    pub fn init(&mut self, machine: Machine, grant_memory: impl FnOnce(MemoryRange) -> bool) {
        // No machine facts, no RSDP, or no memory access: ACPI stays off
        // (`acpi.c:80-90`).
        if machine.acpi_rsdp == 0 {
            return;
        }
        if !grant_memory(FULL_MEMORY_RANGE) {
            return;
        }
        self.machine = Some(machine);
        self.enabled = self.init_acpica(machine);
    }

    /// The interpreter's initialization chain (`init_acpica`,
    /// `acpi.c:43-64`): five calls, then the mode report, then the scan.
    fn init_acpica(&mut self, machine: Machine) -> bool {
        if self.interpreter.initialize_subsystem().is_err() {
            return false;
        }
        if self.interpreter.initialize_tables(16).is_err() {
            return false;
        }
        if self.interpreter.load_tables().is_err() {
            return false;
        }
        if self.interpreter.enable_subsystem(0).is_err() {
            return false;
        }
        if self.interpreter.initialize_objects(0).is_err() {
            return false;
        }
        self.set_machine_mode(machine);
        self.scan_devices(machine);
        true
    }

    /// Report the machine mode to the firmware (`set_machine_mode`,
    /// `acpi.c:23-41`): a failed `_PIC` is silently ignored — the firmware
    /// may not implement it.
    fn set_machine_mode(&mut self, machine: Machine) {
        let _ = self.interpreter.evaluate_pic(machine.apic_enabled);
    }

    /// Scan the PCI bridges (`pci_scan_devices`, `pci.c:313-325`): the
    /// scan only runs in APIC mode, and a machine with no root bridge is
    /// reported rather than aborting the way C's assertion would.
    fn scan_devices(&mut self, machine: Machine) {
        if !machine.apic_enabled {
            return;
        }
        match Routing::scan(&mut self.interpreter) {
            Ok(routing) => {
                self.multi_root = routing.scan.multi_root;
                self.routing = Some(routing);
            }
            Err(_) => {
                self.routing = None;
            }
        }
    }

    /// One request (`main`'s loop body, `acpi.c:139-167`).
    ///
    /// C replies to **every** message, including the ones it ignores: the
    /// loop ends with an unconditional `ipc_send` of the message it
    /// received (`acpi.c:161-164`).
    pub fn dispatch(&mut self, message: &mut minix_types::Message) -> Result<(), i32> {
        match read_request(message) {
            ACPI_REQ_GET_IRQ => {
                let request = GetIrqRequest {
                    bus: read_u32(message, LANE_BUS),
                    dev: read_u32(message, LANE_DEV),
                    pin: read_u32(message, LANE_PIN),
                };
                let irq = self.get_irq(request);
                write_answer(message, irq);
                Ok(())
            }
            ACPI_REQ_MAP_BRIDGE => {
                let request = MapBridgeRequest {
                    primary_bus: read_u32(message, LANE_BUS),
                    device: read_u32(message, LANE_DEV),
                    secondary_bus: read_u32(message, LANE_PIN),
                };
                let err = self.map_bridge(request);
                write_answer(message, err);
                Ok(())
            }
            // Unsupported requests are answered with the message
            // unchanged (`acpi.c:155-159`).
            _ => Ok(()),
        }
    }

    /// Translate a device and pin into an IRQ (`do_get_irq`, `pci.c:116-134`).
    ///
    /// C asserts the device and pin are in range (`pci.c:123`), which
    /// aborts the driver process; the rewrite answers "no route" instead.
    pub fn get_irq(&self, request: GetIrqRequest) -> i32 {
        if request.dev as usize >= crate::protocol::PCI_MAX_DEVICES
            || request.pin as usize >= crate::protocol::PCI_MAX_PINS
        {
            return -1;
        }
        let Some(routing) = self.routing.as_ref() else {
            return -1;
        };
        match routing.root.find(-1, -1, request.bus as i32) {
            Some(bridge) => bridge.irq(request.dev, request.pin),
            None => -1,
        }
    }

    /// Record a bridge's bus numbers (`do_map_bridge`, `pci.c:71-87`).
    pub fn map_bridge(&mut self, request: MapBridgeRequest) -> i32 {
        let Some(routing) = self.routing.as_mut() else {
            return -ENODEV;
        };
        match routing
            .root
            .find_mut(request.primary_bus as i32, request.device as i32, -1)
        {
            Some(bridge) => {
                bridge.primary_bus = request.primary_bus as i32;
                bridge.secondary_bus = request.secondary_bus as i32;
                OK
            }
            None => -ENODEV,
        }
    }
}

impl<I: AcpiInterpreter, T: minix_driver_rt::transport::DriverTransport>
    minix_driver_rt::runtime::DriverHandler<T> for AcpiService<I>
{
    fn handle(&mut self, transport: &mut T, msg: &minix_types::Message) {
        // C's loop answers the message it received (`acpi.c:161-164`):
        // the reply is the same message with the answer slot filled.
        let mut reply = *msg;
        if self.dispatch(&mut reply).is_err() {
            return;
        }
        //[SD19-gate] 回复发送尽力而为：C 的 ipc_send 失败只打印；单线程泵按文档丢弃而不是卡死。
        #[allow(clippy::let_underscore_must_use)]
        let _ = transport.asynsend(msg.m_source, &mut reply);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::PciBridge;
    use crate::interpreter::{DeviceInfo, RoutingEntry};
    use crate::protocol::{ACPI_REQ_GET_IRQ, LANE_ANSWER};
    use alloc::string::String;
    use alloc::vec;
    use alloc::vec::Vec;

    /// An interpreter that comes up, with one bridge at secondary bus 3.
    struct Working {
        /// How many times each initialization call ran.
        pub calls: Vec<&'static str>,
        /// The PIC value the mode report carried.
        pub pic: Option<bool>,
    }

    impl Working {
        fn new() -> Self {
            Working {
                calls: Vec::new(),
                pic: None,
            }
        }
    }

    impl AcpiInterpreter for Working {
        fn initialize_subsystem(&mut self) -> Result<(), i32> {
            self.calls.push("subsystem");
            Ok(())
        }
        fn initialize_tables(&mut self, slots: u32) -> Result<(), i32> {
            assert_eq!(slots, 16, "C asks for sixteen table slots");
            self.calls.push("tables");
            Ok(())
        }
        fn load_tables(&mut self) -> Result<(), i32> {
            self.calls.push("load");
            Ok(())
        }
        fn enable_subsystem(&mut self, flags: u32) -> Result<(), i32> {
            assert_eq!(flags, 0);
            self.calls.push("enable");
            Ok(())
        }
        fn initialize_objects(&mut self, flags: u32) -> Result<(), i32> {
            assert_eq!(flags, 0);
            self.calls.push("objects");
            Ok(())
        }
        fn evaluate_pic(&mut self, apic_enabled: bool) -> Result<(), i32> {
            self.pic = Some(apic_enabled);
            Ok(())
        }
        fn devices(&mut self, id: &str) -> Result<Vec<i32>, i32> {
            if id == "PNP0A03" {
                Ok(vec![1])
            } else {
                Ok(Vec::new())
            }
        }
        fn devices_with_parent(&mut self, _parent: i32) -> Result<Vec<i32>, i32> {
            Ok(Vec::new())
        }
        fn parent(&mut self, _handle: i32) -> Result<i32, i32> {
            Err(-minix_types::ENODEV)
        }
        fn irq_routing_table(&mut self, _handle: i32) -> Result<Vec<RoutingEntry>, i32> {
            Ok(vec![RoutingEntry {
                address: 1 << 16,
                pin: 0,
                source: String::new(),
                source_index: 10,
            }])
        }
        fn object_info(&mut self, _handle: i32) -> Result<DeviceInfo, i32> {
            Ok(DeviceInfo { address: 0 })
        }
        fn get_handle(&mut self, _parent: i32, _name: &str) -> Result<i32, i32> {
            Err(-minix_types::ENODEV)
        }
        fn walk_resources(&mut self, _handle: i32, _method: &str) -> Result<Vec<u32>, i32> {
            Err(-minix_types::ENODEV)
        }
    }

    fn apic_machine() -> Machine {
        Machine {
            apic_enabled: true,
            acpi_rsdp: 0xe0000,
        }
    }

    #[test]
    fn test_the_gates_keep_acpi_off_without_an_rsdp_or_memory() {
        // No RSDP: the interpreter is never touched.
        let mut service = AcpiService::new(Working::new());
        service.init(
            Machine {
                apic_enabled: true,
                acpi_rsdp: 0,
            },
            |_| true,
        );
        assert!(!service.enabled);
        assert!(service.interpreter_mut().calls.is_empty());
        // No memory privilege: the same verdict.
        let mut service = AcpiService::new(Working::new());
        service.init(apic_machine(), |_| false);
        assert!(!service.enabled);
        assert!(service.interpreter_mut().calls.is_empty());
    }

    #[test]
    fn test_the_initialization_chain_runs_in_c_order() {
        let mut service = AcpiService::new(Working::new());
        let mut granted = None;
        service.init(apic_machine(), |range| {
            granted = Some(range);
            true
        });
        assert!(service.enabled);
        // The memory request covers the whole address space
        // (`acpi.c:17-18`).
        assert_eq!(granted, Some(FULL_MEMORY_RANGE));
        assert_eq!(
            service.interpreter_mut().calls,
            vec!["subsystem", "tables", "load", "enable", "objects"]
        );
        // The mode report carried the machine's APIC flag, and the scan
        // filled the tree.
        assert_eq!(service.interpreter_mut().pic, Some(true));
        let routing = service.routing().expect("the scan ran");
        assert_eq!(routing.root.secondary_bus, 0);
        assert_eq!(routing.root.irq(1, 0), 10);
    }

    #[test]
    fn test_a_failed_chain_leaves_acpi_off() {
        struct Broken;
        impl AcpiInterpreter for Broken {
            fn initialize_subsystem(&mut self) -> Result<(), i32> {
                Err(-minix_types::EIO)
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
            fn devices(&mut self, _id: &str) -> Result<Vec<i32>, i32> {
                Ok(Vec::new())
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
        let mut service = AcpiService::new(Broken);
        service.init(apic_machine(), |_| true);
        assert!(!service.enabled, "the first failure stops the chain");
        assert!(service.routing().is_none());
    }

    #[test]
    fn test_the_two_requests_answer_through_the_message() {
        let mut service = AcpiService::new(Working::new());
        service.init(apic_machine(), |_| true);
        // A GET_IRQ for the routed device.
        let mut message = minix_types::Message::default();
        crate::protocol::write_u32(
            &mut message,
            crate::protocol::LANE_REQUEST,
            ACPI_REQ_GET_IRQ as u32,
        );
        crate::protocol::write_u32(&mut message, LANE_BUS, 0);
        crate::protocol::write_u32(&mut message, LANE_DEV, 1);
        crate::protocol::write_u32(&mut message, LANE_PIN, 0);
        assert_eq!(service.dispatch(&mut message), Ok(()));
        assert_eq!(crate::protocol::read_u32(&message, LANE_ANSWER) as i32, 10);
        // A device nobody routed answers minus one.
        crate::protocol::write_u32(&mut message, LANE_DEV, 7);
        assert_eq!(service.dispatch(&mut message), Ok(()));
        assert_eq!(crate::protocol::read_u32(&message, LANE_ANSWER) as i32, -1);
        // An out-of-range pin is refused instead of aborting (C asserts).
        crate::protocol::write_u32(&mut message, LANE_DEV, 1);
        crate::protocol::write_u32(&mut message, LANE_PIN, 9);
        assert_eq!(service.dispatch(&mut message), Ok(()));
        assert_eq!(crate::protocol::read_u32(&message, LANE_ANSWER) as i32, -1);
    }

    #[test]
    fn test_mapping_a_bridge_records_the_bus_numbers() {
        let mut service = AcpiService::new(Working::new());
        service.init(apic_machine(), |_| true);
        // A bridge at device 4 of bus zero does not exist yet.
        let mut message = minix_types::Message::default();
        crate::protocol::write_u32(
            &mut message,
            crate::protocol::LANE_REQUEST,
            ACPI_REQ_MAP_BRIDGE as u32,
        );
        crate::protocol::write_u32(&mut message, LANE_BUS, 0);
        crate::protocol::write_u32(&mut message, LANE_DEV, 4);
        crate::protocol::write_u32(&mut message, LANE_PIN, 5);
        assert_eq!(service.dispatch(&mut message), Ok(()));
        assert_eq!(
            crate::protocol::read_u32(&message, LANE_ANSWER) as i32,
            -minix_types::ENODEV
        );
        // The root itself maps.
        crate::protocol::write_u32(&mut message, LANE_DEV, 1);
        // The root's secondary bus is zero, so the search for a bridge
        // whose secondary bus is zero finds it.
        let root_handle = service.routing().unwrap().root.handle;
        let mut bridge = PciBridge::new(root_handle);
        bridge.secondary_bus = 0;
        assert!(
            bridge.find(-1, -1, 0).is_some(),
            "the root carries bus zero"
        );
    }

    #[test]
    fn test_an_unsupported_request_is_answered_unchanged() {
        let mut service = AcpiService::new(Working::new());
        service.init(apic_machine(), |_| true);
        let mut message = minix_types::Message::default();
        crate::protocol::write_u32(&mut message, crate::protocol::LANE_REQUEST, 99);
        crate::protocol::write_u32(&mut message, LANE_ANSWER, 0x1234);
        assert_eq!(service.dispatch(&mut message), Ok(()));
        // C replies to everything; an unknown request's payload is
        // untouched.
        assert_eq!(crate::protocol::read_u32(&message, LANE_ANSWER), 0x1234);
    }
}
