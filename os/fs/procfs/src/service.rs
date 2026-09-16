//! Service directory: the `/proc/service` listing of registered system
//! services.
//!
//! C correspondence: `minix3/minix/fs/procfs/service.c` — the server asks
//! the reconstruction service for the registered service labels and their
//! properties, then renders one row per service. The real query belongs to
//! the RS transport; this module keeps the listing data and its rendering,
//! so tests drive the seam directly.

/// One registered service: label, endpoint, and flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceEntry {
    /// Service label (the RS registration name).
    pub label: &'static str,
    /// Service endpoint number.
    pub endpoint: i32,
    /// RS publish flags.
    pub flags: u32,
}

/// The service directory seam: where entries come from is a transport
/// question; the listing only renders what the seam yields.
pub trait ServiceDirectory {
    /// All registered services, in registration order.
    fn services(&self) -> alloc::vec::Vec<ServiceEntry>;
}

/// Render the service listing: one `label endpoint flags` row per entry,
/// the shape the C `service_getdents`/`service_read` pair produced.
pub fn render_services(buf: &mut crate::buf::ProcBuf, entries: &[ServiceEntry]) {
    for entry in entries {
        buf.push_str(entry.label);
        buf.push_str(" ");
        buf.push_u64(entry.endpoint as u64);
        buf.push_str(" ");
        buf.push_u64(entry.flags as u64);
        buf.push_str("\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed;

    impl ServiceDirectory for Fixed {
        fn services(&self) -> alloc::vec::Vec<ServiceEntry> {
            alloc::vec::Vec::new()
        }
    }

    #[test]
    fn test_service_seam_starts_empty() {
        let directory = Fixed;
        assert!(directory.services().is_empty());
    }
}
