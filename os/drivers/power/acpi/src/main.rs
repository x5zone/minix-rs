//! The ACPI service binary.
//!
//! C: `main` runs the SEF startup (`acpi.c:109-136`: the gates, the
//! memory privilege, the interpreter's initialization chain, the machine
//! mode report, and the PCI bridge scan), then the request loop
//! (`acpi.c:139-167`). The loop shell and the RS birth handshake live in
//! the shared driver runtime; this binary builds the service.
//!
//! C announces no character key: ACPI is an RS service with its own two
//! requests (`ACPI_REQ_GET_IRQ`, `ACPI_REQ_MAP_BRIDGE`), so the loop is
//! entered through the announce-free serve path.
//!
//! The interpreter is the registered seam (`E-ACPICA`): with the unbound
//! interpreter in place the service reports "ACPI failed" (C's
//! `ACPI: ACPI failed with err %d`, `acpi.c:100-103`) instead of claiming
//! to have parsed tables, and then answers the two requests with "no
//! route" and "no device" respectively.

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

/// The machine record's wire layout: `processors_count` at zero,
/// `bsp_id` at four, padding at eight, `apic_enabled` at twelve,
/// `acpi_rsdp` at sixteen, `board_id` at twenty-four
/// (`struct machine`, the `GET_MACHINE` record the kernel fills).
fn read_machine() -> Option<minix_driver_acpi::service::Machine> {
    let mut buffer = [0u8; 32];
    minix_sys::syscall::sys_get_machine(
        &minix_sys::syscall::DirectKernelCallTransport,
        &mut buffer,
    )
    .ok()?;
    let apic_enabled = i32::from_le_bytes(buffer[12..16].try_into().unwrap()) != 0;
    let acpi_rsdp = u64::from_le_bytes(buffer[16..24].try_into().unwrap());
    Some(minix_driver_acpi::service::Machine {
        apic_enabled,
        acpi_rsdp,
    })
}

/// Adds one memory range to this process's privileges
/// (`sys_privctl(SELF, SYS_PRIV_ADD_MEM, &mr)`, `acpi.c:19`).
///
/// The request number is `SYS_PRIV_ADD_MEM` (`com.h:347`); the argument
/// is a `struct minix_mem_range` — two `phys_bytes` fields — in this
/// process's own memory.
fn add_memory_privilege(range: minix_driver_acpi::service::MemoryRange) -> bool {
    /// `SYS_PRIV_ADD_MEM` (`com.h:347`).
    const SYS_PRIV_ADD_MEM: i32 = 6;

    /// The C argument structure (`struct minix_mem_range`: two
    /// `phys_bytes` fields).
    #[repr(C)]
    struct MinixMemRange {
        mr_base: u64,
        mr_limit: u64,
    }

    let range = MinixMemRange {
        mr_base: range.base,
        mr_limit: range.limit,
    };
    minix_sys::syscall::sys_privctl(
        &minix_sys::syscall::DirectKernelCallTransport,
        minix_types::Endpoint::SELF.0,
        SYS_PRIV_ADD_MEM,
        &range as *const MinixMemRange as u64,
    )
    .is_ok()
}

fn main() {
    // C `sef_startup` asks the kernel who we are (`sef.c:76-87`).
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("acpi: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    let mut service = minix_driver_acpi::init();

    // C `sys_getmachine` supplies the two gate facts (`acpi.c:84-88`);
    // without them ACPI stays off.
    let machine = match read_machine() {
        Some(machine) => machine,
        None => return,
    };
    // C asks the kernel for memory access before touching any table
    // (`acpi.c:80-84`).
    service.init(machine, add_memory_privilege);

    let mut runtime = DriverRuntime::new(transport, "acpi");
    // `serve_quiet`: birth handshake then the loop, without publishing a
    // label C never publishes. The loop returns only when the transport
    // dies; C turns the same receive failure into a print-and-continue.
    let _birth = runtime.serve_quiet(&mut service, |_| Ok(()));
}
