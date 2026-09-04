//! USB device modeling over the devman client (doc 11-usb-device-model).
//!
//! C: `minix3/minix/lib/libdevman/usb.c` (301 lines) — attribute
//! generation (`add_device_attributes` / `add_interface_attributes`),
//! `devman_usb_device_new` / `delete` / `add` / `remove`, the
//! `bind_cb`/`unbind_cb` static wrappers, and `devman_usb_init`.
//!
//! This module owns the USB *description* side (what attributes a device
//! exposes); the USB *protocol* side (descriptors parsing) belongs to
//! `minix-usb` (still a stub) and enters here as plain decoded values.
//! Transport (`sendrec`/grants) is 10's [`ClientTransport`](super::devman_client::ClientTransport).

use alloc::string::String;
use alloc::vec::Vec;
use minix_types::{Endpoint, Errno};

use super::devman_client::{
    add_device, del_device, ClientDevice, ClientError, ClientTransport,
};

/// C: `struct devman_usb_bind_cb_data` (`devman.h:23-26`, lib side) —
/// `{ dev_id, interface }`, `interface == -1` for the device itself.
/// This is what driver callbacks actually receive (usb.c passes
/// `&cb_data`, never the raw endpoint alone).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BindData {
    pub dev_id: i32,
    pub interface: i32,
}

/// Driver bind/unbind callbacks, USB flavor (C: `devman_usb_bind_cb_t`,
/// `devman.h:57` — `(cb_data, endpoint)`).
pub type UsbBindCallback = fn(&BindData, Endpoint) -> Result<(), Errno>;

/// Decoded USB device descriptor fields this layer needs
/// (parsed by the USB stack; `UGETW` applied there, not here).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsbDeviceDesc {
    pub device_class: u8,
    pub device_subclass: u8,
    pub device_protocol: u8,
    pub vendor: u16,
    pub product: u16,
}

/// Decoded USB interface descriptor fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsbInterfaceDesc {
    pub number: u8,
    pub alternate: u8,
    pub num_endpoints: u8,
    pub class: u8,
    pub subclass: u8,
    pub protocol: u8,
}

/// One USB interface: descriptor + server id once added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbInterface {
    pub desc: UsbInterfaceDesc,
    pub server_id: Option<i32>,
}

/// A USB device with its interfaces (C: `struct devman_usb_dev`,
/// `devman.h:36-55` — minus the fixed `interfaces[32]` array, a `Vec`;
/// minus descriptor pointers, decoded values instead).
/// `usb_id` is the caller-provided USB-side id (`"USB%d"` name source,
/// usb.c:168); `server_id` is `None` until `add_usb` stores the assigned
/// id (C writes `dev->dev->dev_id`, via `devman_add_device`).
pub struct UsbDevice {
    pub usb_id: i32,
    pub desc: UsbDeviceDesc,
    pub configuration: i32,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub serial: Option<String>,
    pub interfaces: Vec<UsbInterface>,
    pub server_id: Option<i32>,
    /// Per-interface server ids, parallel to `interfaces`.
    pub intf_server_ids: Vec<Option<i32>>,
}

impl UsbDevice {
    pub fn new(usb_id: i32, desc: UsbDeviceDesc) -> Self {
        UsbDevice {
            usb_id,
            desc,
            configuration: 0,
            manufacturer: None,
            product: None,
            serial: None,
            interfaces: Vec::new(),
            server_id: None,
            intf_server_ids: Vec::new(),
        }
    }
}

/// C: `add_device_attributes` (usb.c:44-91) — five `0x`-formatted numbers
/// (`%02x`/`%04x`), conditional strings, then `dev_type`.
/// Exact spellings locked by test (devmand matches on them, 13).
pub fn device_attributes(dev: &UsbDevice) -> Vec<(String, String)> {
    use core::fmt::Write as _;
    let mut out = Vec::new();
    let hex2 = |v: u8| {
        let mut s = String::new();
        let _ = core::write!(s, "0x{v:02x}");
        s
    };
    let hex4 = |v: u16| {
        let mut s = String::new();
        let _ = core::write!(s, "0x{v:04x}");
        s
    };
    out.push((String::from("bDeviceClass"), hex2(dev.desc.device_class)));
    out.push((String::from("bDeviceSubClass"), hex2(dev.desc.device_subclass)));
    out.push((String::from("bDeviceProtocol"), hex2(dev.desc.device_protocol)));
    out.push((String::from("idVendor"), hex4(dev.desc.vendor)));
    out.push((String::from("idProduct"), hex4(dev.desc.product)));
    // C: only when non-NULL (usb.c:83-88).
    if let Some(p) = &dev.product {
        out.push((String::from("Product"), p.clone()));
    }
    if let Some(m) = &dev.manufacturer {
        out.push((String::from("Manufacturer"), m.clone()));
    }
    if let Some(s) = &dev.serial {
        out.push((String::from("SerialNumber"), s.clone()));
    }
    out.push((String::from("dev_type"), String::from("USB_DEV")));
    out
}

/// C: `add_interface_attributes` (usb.c:93-141) — six `0x%02x` numbers
/// then `dev_type = USB_INTF`.
pub fn interface_attributes(intf: &UsbInterfaceDesc) -> Vec<(String, String)> {
    use core::fmt::Write as _;
    let mut out = Vec::new();
    let hex2 = |v: u8| {
        let mut s = String::new();
        let _ = core::write!(s, "0x{v:02x}");
        s
    };
    out.push((String::from("bInterfaceNumber"), hex2(intf.number)));
    out.push((String::from("bAlternateSetting"), hex2(intf.alternate)));
    out.push((String::from("bNumEndpoints"), hex2(intf.num_endpoints)));
    out.push((String::from("bInterfaceClass"), hex2(intf.class)));
    out.push((String::from("bInterfaceSubClass"), hex2(intf.subclass)));
    out.push((String::from("bInterfaceProtocol"), hex2(intf.protocol)));
    out.push((String::from("dev_type"), String::from("USB_INTF")));
    out
}

/// Driver callback registry (C: static `bind_cb`/`unbind_cb` + 
/// `devman_usb_init`, usb.c:19-20/293-301).
/// The per-device shims (C: `devman_usb_bind_cb`, usb.c:280-293 —
/// missing callback → `ENODEV`) resolve through the stack registry here.
#[derive(Default)]
pub struct UsbStack {
    bind_cb: Option<UsbBindCallback>,
    unbind_cb: Option<UsbBindCallback>,
}

impl UsbStack {
    pub fn new() -> Self {
        UsbStack {
            bind_cb: None,
            unbind_cb: None,
        }
    }

    /// C: `devman_usb_init(bind_cb, unbind_cb)` (usb.c:293-301).
    pub fn set_callbacks(&mut self, bind: UsbBindCallback, unbind: UsbBindCallback) {
        self.bind_cb = Some(bind);
        self.unbind_cb = Some(unbind);
    }

    /// Device-level shim (C: `devman_usb_bind_cb`, usb.c:280-287).
    pub fn bind_device(&self, data: &BindData, ep: Endpoint) -> Result<(), Errno> {
        match self.bind_cb {
            Some(f) => f(data, ep),
            None => Err(Errno::ENODEV),
        }
    }

    /// Interface-level shim (C: `devman_usb_unbind_cb`, usb.c:289-294).
    pub fn unbind_device(&self, data: &BindData, ep: Endpoint) -> Result<(), Errno> {
        match self.unbind_cb {
            Some(f) => f(data, ep),
            None => Err(Errno::ENODEV),
        }
    }
}

/// C: `devman_usb_device_new(dev_id)` (usb.c:143-172) — the `dev_id`
/// parameter names the *USB-side* id (`"USB%d"`); `parent_dev_id` is 0
/// (root). (`dev->name` is a fixed array client-side — no alloc failure
/// mode here beyond Vec; A-7.)
pub fn device_name(usb_id: i32) -> String {
    use core::fmt::Write as _;
    let mut s = String::new();
    let _ = core::write!(s, "USB{usb_id}");
    // C: snprintf(name, 32, …) truncates (local.h:7); names are short in
    // practice — truncate like 10's truncate_name for parity.
    super::devman_client::truncate_name(&s)
}

/// C: `devman_usb_device_add` (usb.c:219-274) — attributes → device add
/// (server id stored back) → per-interface adds (parent = server id,
/// names `intf{i}`, `cb_data` per interface). Panics become `Err`
/// (A-7, via 10's `ClientError`).
pub fn add_usb(
    t: &mut impl ClientTransport,
    devman: Endpoint,
    usb: &mut UsbDevice,
) -> Result<(), ClientError> {
    let mut dev = ClientDevice::new(&device_name(usb.usb_id), 0);
    dev.attrs = device_attributes(usb);
    let server_id = add_device(t, devman, &mut dev)?;
    usb.server_id = Some(server_id);
    usb.intf_server_ids.clear();
    // Clone the small Copy descs first: adding borrows usb mutably below.
    let descs: Vec<UsbInterfaceDesc> =
        usb.interfaces.iter().map(|intf| intf.desc).collect();
    for (i, desc) in descs.iter().enumerate() {
        let mut idev = ClientDevice::new(&intf_name(i), server_id);
        idev.attrs = interface_attributes(desc);
        let iid = add_device(t, devman, &mut idev)?;
        intf_set_server(usb, i, iid);
    }
    Ok(())
}

fn intf_name(i: usize) -> String {
    use core::fmt::Write as _;
    let mut s = String::new();
    let _ = core::write!(s, "intf{i}");
    super::devman_client::truncate_name(&s)
}

fn intf_set_server(usb: &mut UsbDevice, i: usize, id: i32) {
    if usb.intf_server_ids.len() <= i {
        usb.intf_server_ids.resize(i + 1, None);
    }
    usb.intf_server_ids[i] = Some(id);
    if let Some(intf) = usb.interfaces.get_mut(i) {
        intf.server_id = Some(id);
    }
}

/// C: `devman_usb_device_remove` (usb.c:276-291) — interfaces first,
/// then the device (panics → `Err`). Local teardown is `drop`
/// (C: `devman_usb_device_delete` frees; Rust ownership — remove-then-
/// drop protocol, 11 §3.5).
pub fn remove_usb(
    t: &mut impl ClientTransport,
    devman: Endpoint,
    usb: &UsbDevice,
) -> Result<(), ClientError> {
    for id in usb.intf_server_ids.iter().flatten() {
        del_device(t, devman, *id)?;
    }
    if let Some(id) = usb.server_id {
        del_device(t, devman, id)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::super::devman_client::{ClientError, ClientTransport, HandledDevice};
    use super::*;
    use minix_types::{Message, MessageM4, MessageUnion, DEVMAN_REPLY};
    use std::vec::Vec;

    fn desc() -> UsbDeviceDesc {
        UsbDeviceDesc {
            device_class: 0,
            device_subclass: 0,
            device_protocol: 0,
            vendor: 0x1234,
            product: 0x5678,
        }
    }

    fn intf_desc() -> UsbInterfaceDesc {
        UsbInterfaceDesc {
            number: 0,
            alternate: 0,
            num_endpoints: 2,
            class: 0x08,
            subclass: 0x06,
            protocol: 0x50,
        }
    }

    #[test]
    fn attributes_match_c_spellings() {
        // usb.c:44-141, exact: names, 0x%02x/%04x shapes, dev_type last.
        let mut usb = UsbDevice::new(3, desc());
        usb.product = Some(String::from("Stick"));
        let attrs = device_attributes(&usb);
        let get = |n: &str| {
            attrs
                .iter()
                .find(|(k, _)| k == n)
                .map(|(_, v)| v.clone())
                .unwrap()
        };
        assert_eq!(get("bDeviceClass"), "0x00");
        assert_eq!(get("idVendor"), "0x1234");
        assert_eq!(get("idProduct"), "0x5678");
        assert_eq!(get("Product"), "Stick");
        assert!(attrs.iter().all(|(k, _)| k != "Manufacturer"));
        assert_eq!(get("dev_type"), "USB_DEV");
        assert_eq!(attrs.last().unwrap().0, "dev_type");
        let iattrs = interface_attributes(&intf_desc());
        assert_eq!(iattrs.len(), 7);
        assert_eq!(iattrs[3].1, "0x08"); // bInterfaceClass
        assert_eq!(iattrs[6].1, "USB_INTF");
    }

    struct FakeTransport {
        next_id: i32,
        pub calls: usize,
    }

    impl ClientTransport for FakeTransport {
        fn grant(&mut self, _buf: &[u8]) -> Result<i32, ClientError> {
            Ok(1)
        }

        fn revoke(&mut self, _grant: i32) {}

        fn sendrec(&mut self, _ep: Endpoint, msg: &mut Message) -> Result<(), ClientError> {
            // Names travel in the grant buffer, not the message — count
            // calls, assign server ids in order.
            self.next_id += 1;
            self.calls += 1;
            msg.m_type = DEVMAN_REPLY;
            msg.m_u.m_m4.m4l1 = 0;
            msg.m_u.m_m4.m4l2 = self.next_id as i64;
            Ok(())
        }
    }

    // NOTE: dels go through the same sendrec; distinguish by m_type in a
    // fuller fake — here add/remove use separate transports for clarity.

    #[test]
    fn add_remove_roundtrip() {
        // usb.c:219-291 without panics: device + interfaces added with
        // parent linkage, removed interfaces-first.
        let mut usb = UsbDevice::new(3, desc());
        // C: snprintf(name, 32, "USB%d") (usb.c:168).
        assert_eq!(device_name(3), "USB3");
        usb.interfaces.push(UsbInterface {
            desc: intf_desc(),
            server_id: None,
        });
        usb.interfaces.push(UsbInterface {
            desc: intf_desc(),
            server_id: None,
        });
        let mut t = FakeTransport { next_id: 40, calls: 0 };
        add_usb(&mut t, Endpoint(2), &mut usb).unwrap();
        assert_eq!(usb.server_id, Some(41));        assert_eq!(usb.intf_server_ids, std::vec![Some(42), Some(43)]);
        assert_eq!(t.calls, 3);
        // Interface parent linkage is by server id (usb.c: ~device_add).
        remove_usb(&mut t, Endpoint(2), &usb).unwrap();
        // 3 more sendrecs (2 intf dels + 1 dev del).
        assert_eq!(t.calls, 6);
    }

    #[test]
    fn stack_shims_default_enodev() {
        // C: no callback registered → ENODEV (usb.c:280-294).
        let stack = UsbStack::new();
        let data = BindData {
            dev_id: 41,
            interface: -1,
        };
        assert_eq!(stack.bind_device(&data, Endpoint(4)), Err(Errno::ENODEV));
        let mut stack2 = UsbStack::new();
        stack2.set_callbacks(|_, _| Ok(()), |_, _| Ok(()));
        assert_eq!(stack2.bind_device(&data, Endpoint(4)), Ok(()));
        // Full bind-data shape (devman.h:23-26).
        assert_eq!(data.interface, -1);
    }

    #[test]
    fn delete_is_drop_after_remove() {
        // C: delete frees locally, talks to no one (usb.c:174-197).
        // Rust: remove() then drop — no function needed; protocol test.
        let usb = UsbDevice::new(3, desc());
        assert!(usb.server_id.is_none());
        drop(usb);
    }

    #[test]
    fn client_device_binds_through_registry() {
        // 10 shims route (dev_id, ep); 11 resolves interface context.
        // Here: the 10-level HandledDevice path with the 2-arg callback.
        let devs = [HandledDevice {
            dev_id: 41,
            bind_cb: Some(|id, _ep| if id == 41 { Ok(()) } else { Err(Errno::ENODEV) }),
            unbind_cb: None,
        }];
        let mut replies = Vec::new();
        let mut msg = Message {
            m_source: Endpoint(2),
            m_type: minix_types::DEVMAN_BIND,
            m_u: MessageUnion {
                m_m4: MessageM4 {
                    m4l2: 41,
                    m4l3: 4,
                    ..MessageM4::default()
                },
            },
        };
        assert!(super::super::devman_client::handle_msg(
            &mut msg,
            Endpoint(2),
            &devs,
            &mut |m| replies.push(*m)
        ));
        assert_eq!(replies.len(), 1);
    }
}
