//! The VirtualBox VMMDev device protocol: request numbers, result codes,
//! and the byte layout of the one-page request buffer.
//!
//! C correspondence: `minix3/minix/drivers/vmm_guest/vbox/vmmdev.h` (the
//! device's request header, the seven request types the driver issues,
//! the HGCM structures, and the 4096-byte buffer) and
//! `minix3/minix/include/minix/vboxtype.h` (the caller-side parameter
//! shape that rides the caller's grant).
//!
//! The buffer is the device's view of memory: the driver writes a request
//! header plus a body into its contiguous page, hands the page's physical
//! address to the device through the backdoor port, and reads the result
//! back out of the same page. The structs below mirror the C layouts so
//! the offsets can be pinned against them, and [`wire`] derives the byte
//! offsets every reader and writer uses; the two structures the device
//! reads at unaligned offsets (the parameter array and the page list) are
//! accessed through the byte helpers instead of a struct reference.

/// The VMMDev PCI vendor identifier (`VMMDEV_PCI_VID`).
pub const PCI_VENDOR: u16 = 0x80ee;
/// The VMMDev PCI device identifier (`VMMDEV_PCI_DID`).
pub const PCI_DEVICE: u16 = 0xcafe;

/// Request type: host time (`VMMDEV_REQ_HOSTTIME`).
pub const REQ_HOST_TIME: u32 = 10;
/// Request type: acknowledge and retrieve events
/// (`VMMDEV_REQ_ACKNOWLEDGEEVENTS`).
pub const REQ_ACKNOWLEDGE_EVENTS: u32 = 41;
/// Request type: report guest information (`VMMDEV_REQ_REPORTGUESTINFO`).
pub const REQ_REPORT_GUEST_INFO: u32 = 50;
/// Request type: open an HGCM connection (`VMMDEV_REQ_HGCMCONNECT`).
pub const REQ_HGCM_CONNECT: u32 = 60;
/// Request type: close an HGCM connection (`VMMDEV_REQ_HGCMDISCONNECT`).
pub const REQ_HGCM_DISCONNECT: u32 = 61;
/// Request type: perform an HGCM call (`VMMDEV_REQ_HGCMCALL`).
pub const REQ_HGCM_CALL: u32 = 62;
/// Request type: cancel an HGCM call (`VMMDEV_REQ_HGMCANCEL`).
pub const REQ_HGCM_CANCEL: u32 = 64;

/// Result code: success (`VMMDEV_ERR_OK`).
pub const ERR_OK: i32 = 0;
/// Result code: general failure (`VMMDEV_ERR_GENERIC`).
pub const ERR_GENERIC: i32 = -1;
/// Result code: HGCM service not found (`VMMDEV_ERR_HGCM_NOT_FOUND`).
pub const ERR_HGCM_NOT_FOUND: i32 = -2900;
/// Result code: HGCM client rejected (`VMMDEV_ERR_HGCM_DENIED`; positive).
pub const ERR_HGCM_DENIED: i32 = 2901;
/// Result code: HGCM invalid address (`VMMDEV_ERR_HGCM_INVALID_ADDR`).
pub const ERR_HGCM_INVALID_ADDR: i32 = -2902;
/// Result code: HGCM call still in progress (`VMMDEV_ERR_HGCM_ASYNC_EXEC`;
/// positive).
pub const ERR_HGCM_ASYNC_EXEC: i32 = 2903;
/// Result code: HGCM internal error (`VMMDEV_ERR_HGCM_INTERNAL`).
pub const ERR_HGCM_INTERNAL: i32 = -2904;
/// Result code: HGCM invalid client identifier
/// (`VMMDEV_ERR_HGCM_INVALID_ID`).
pub const ERR_HGCM_INVALID_ID: i32 = -2905;

/// Builds the version word the device expects (`VMMDEV_MAKEWORD`).
pub const fn make_word(major: u32, minor: u32) -> u32 {
    (major << 16) | minor
}

/// The backdoor protocol version the driver speaks
/// (`VMMDEV_BACKDOOR_VERSION` = 1.1).
pub const BACKDOOR_VERSION: u32 = make_word(1, 1);
/// The guest version reported at startup (`VMMDEV_GUEST_VERSION` = 1.4).
pub const GUEST_VERSION: u32 = make_word(1, 4);
/// The guest OS type reported at startup (`VMMDEV_GUEST_OS_OTHER`, "L4 —
/// close enough" in the C comment).
pub const GUEST_OS_TYPE: u32 = 0x90000;

/// The HGCM event bit (`VMMDEV_EVENT_HGCM`): an interrupt carrying it
/// means at least one HGCM request completed.
pub const EVENT_HGCM: u32 = 1 << 1;

/// Request-completed flag the device sets in an HGCM header
/// (`VMMDEV_HGCM_REQ_DONE`).
pub const HGCM_REQ_DONE: u32 = 1 << 0;

/// Service-location code for an existing service on the local host
/// (`VMMDEV_HGCM_SVCLOC_LOCALHOST_EXISTING`).
pub const HGCM_SVCLOC_LOCALHOST_EXISTING: u32 = 2;

/// The service name field's size (`VMMDEV_HGCM_NAME_SIZE`).
pub const HGCM_NAME_SIZE: usize = 128;

/// Page-list direction flag: the host writes into the pages
/// (`VMMDEV_HGCM_FLAG_FROM_HOST`).
pub const HGCM_FLAG_FROM_HOST: u32 = 0x02;
/// Page-list direction flag: the host reads out of the pages
/// (`VMMDEV_HGCM_FLAG_TO_HOST`).
pub const HGCM_FLAG_TO_HOST: u32 = 0x01;

/// Device-side parameter type: 32-bit value (`VMMDEV_HGCM_PARAM_U32`).
pub const HGCM_PARAM_U32: u32 = 1;
/// Device-side parameter type: 64-bit value (`VMMDEV_HGCM_PARAM_U64`).
pub const HGCM_PARAM_U64: u32 = 2;
/// Device-side parameter type: page list (`VMMDEV_HGCM_PARAM_PAGELIST`).
pub const HGCM_PARAM_PAGELIST: u32 = 10;

/// The request buffer's size (`VMMDEV_BUF_SIZE`, "just one page").
pub const BUF_SIZE: usize = 4096;

/// The request header every request starts with
/// (`struct VMMDevRequestHeader`).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RequestHeader {
    /// Total request size in bytes.
    pub size: u32,
    /// Protocol version ([`BACKDOOR_VERSION`]).
    pub version: u32,
    /// Request type ([`REQ_HOST_TIME`] and friends).
    pub type_: u32,
    /// Device result code ([`ERR_OK`] and friends).
    pub result: i32,
    /// Reserved words.
    pub reserved: [u32; 2],
}

/// The startup request reporting guest information
/// (`struct VMMDevReportGuestInfo`).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReportGuestInfo {
    /// The request header.
    pub header: RequestHeader,
    /// The guest interface version ([`GUEST_VERSION`]).
    pub add_version: u32,
    /// The guest OS type ([`GUEST_OS_TYPE`]).
    pub os_type: u32,
}

/// The host-time request (`struct VMMDevReqHostTime`): the device fills
/// `time` with the host's milliseconds since the epoch.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReqHostTime {
    /// The request header.
    pub header: RequestHeader,
    /// Host time in milliseconds.
    pub time: u64,
}

/// The event request (`struct VMMDevEvents`): asking for it
/// acknowledges the pending events and returns their mask in `events`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Events {
    /// The request header.
    pub header: RequestHeader,
    /// Pending event mask.
    pub events: u32,
}

/// The header every HGCM request starts with
/// (`struct VMMDevHGCMHeader`): a request header plus the completion flag
/// and result the device updates asynchronously.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HgcmHeader {
    /// The request header.
    pub header: RequestHeader,
    /// Completion flags ([`HGCM_REQ_DONE`]).
    pub flags: u32,
    /// Device result code, updated when the request completes.
    pub result: i32,
}

/// The connection request (`struct VMMDevHGCMConnect`).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HgcmConnect {
    /// The HGCM header.
    pub header: HgcmHeader,
    /// Service location code
    /// ([`HGCM_SVCLOC_LOCALHOST_EXISTING`]).
    pub type_: u32,
    /// The service name, zero-padded.
    pub name: [u8; HGCM_NAME_SIZE],
    /// The client identifier the device assigns.
    pub client_id: u32,
}

impl Default for HgcmConnect {
    fn default() -> Self {
        HgcmConnect {
            header: HgcmHeader::default(),
            type_: 0,
            name: [0; HGCM_NAME_SIZE],
            client_id: 0,
        }
    }
}

/// The disconnection request (`struct VMMDevHGCMDisconnect`).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HgcmDisconnect {
    /// The HGCM header.
    pub header: HgcmHeader,
    /// The client identifier to release.
    pub client_id: u32,
}

/// The call request (`struct VMMDevHGCMCall`); the parameter array
/// follows it at [`wire::CALL_PARAMS`].
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HgcmCall {
    /// The HGCM header.
    pub header: HgcmHeader,
    /// The client identifier.
    pub client_id: u32,
    /// The function number within the service.
    pub function: u32,
    /// The number of parameters that follow.
    pub count: u32,
}

/// The cancellation request (`struct VMMDevHGCMCancel`): the physical
/// address of the request to cancel identifies it, so the body is empty.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HgcmCancel {
    /// The HGCM header.
    pub header: HgcmHeader,
}

/// The page list the device reads for a buffer parameter
/// (`struct VMMDevHGCMPageList`).
///
/// The first entry sits at [`wire::PAGE_LIST_ADDR`], which is not
/// eight-byte aligned once the list starts behind a parameter array, so
/// the service writes entries through the byte helpers.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HgcmPageList {
    /// Direction flags ([`HGCM_FLAG_FROM_HOST`] / [`HGCM_FLAG_TO_HOST`]).
    pub flags: u32,
    /// Offset into the first page.
    pub offset: u16,
    /// Number of page addresses that follow.
    pub count: u16,
    /// The first page address.
    pub addr: [u64; 1],
}

/// One device-side call parameter (`struct VMMDevHGCMParam`): a type word
/// and an eight-byte value slot holding a 32-bit value, a 64-bit value, or
/// the page-list location.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HgcmParam {
    /// Parameter type ([`HGCM_PARAM_U32`] and friends).
    pub type_: u32,
    /// Reserved padding before the eight-byte value slot.
    pub _padding: u32,
    /// The value: a `u32`, a `u64`, or `{size, offset}` for a page list.
    pub value: u64,
}

/// Byte offsets inside the request buffer.
///
/// Where a mirror struct exists the constant is derived from it with
/// `offset_of!`, so a change to the mirror cannot silently move the wire
/// layout; the two unaligned structures (the parameter array and the page
/// list entries) are explicit arithmetic and are pinned by tests against
/// the C sizes.
pub mod wire {
    use super::*;

    /// [`RequestHeader`] offsets.
    pub const HEADER_SIZE: usize = core::mem::size_of::<RequestHeader>();
    /// The request's total size field.
    pub const HEADER_SIZE_FIELD: usize = core::mem::offset_of!(RequestHeader, size);
    /// The protocol version field.
    pub const HEADER_VERSION: usize = core::mem::offset_of!(RequestHeader, version);
    /// The request type field.
    pub const HEADER_TYPE: usize = core::mem::offset_of!(RequestHeader, type_);
    /// The result field.
    pub const HEADER_RESULT: usize = core::mem::offset_of!(RequestHeader, result);

    /// [`ReportGuestInfo`] offsets.
    pub const REPORT_SIZE: usize = core::mem::size_of::<ReportGuestInfo>();
    /// The guest interface version field.
    pub const REPORT_ADD_VERSION: usize = core::mem::offset_of!(ReportGuestInfo, add_version);
    /// The guest OS type field.
    pub const REPORT_OS_TYPE: usize = core::mem::offset_of!(ReportGuestInfo, os_type);

    /// [`ReqHostTime`] offsets.
    pub const HOST_TIME_SIZE: usize = core::mem::size_of::<ReqHostTime>();
    /// The host-time field.
    pub const HOST_TIME_TIME: usize = core::mem::offset_of!(ReqHostTime, time);

    /// [`Events`] offsets.
    pub const EVENTS_SIZE: usize = core::mem::size_of::<Events>();
    /// The event-mask field.
    pub const EVENTS_MASK: usize = core::mem::offset_of!(Events, events);

    /// [`HgcmHeader`] offsets.
    pub const HGCM_HEADER_SIZE: usize = core::mem::size_of::<HgcmHeader>();
    /// The completion-flag field.
    pub const HGCM_FLAGS: usize = core::mem::offset_of!(HgcmHeader, flags);
    /// The HGCM result field.
    pub const HGCM_RESULT: usize = core::mem::offset_of!(HgcmHeader, result);

    /// [`HgcmConnect`] offsets.
    pub const CONNECT_SIZE: usize = core::mem::size_of::<HgcmConnect>();
    /// The service-location field.
    pub const CONNECT_TYPE: usize = core::mem::offset_of!(HgcmConnect, type_);
    /// The name field.
    pub const CONNECT_NAME: usize = core::mem::offset_of!(HgcmConnect, name);
    /// The name field's size.
    pub const CONNECT_NAME_SIZE: usize = HGCM_NAME_SIZE;
    /// The client-identifier field.
    pub const CONNECT_CLIENT_ID: usize = core::mem::offset_of!(HgcmConnect, client_id);

    /// [`HgcmDisconnect`] offsets.
    pub const DISCONNECT_SIZE: usize = core::mem::size_of::<HgcmDisconnect>();
    /// The client-identifier field.
    pub const DISCONNECT_CLIENT_ID: usize = core::mem::offset_of!(HgcmDisconnect, client_id);

    /// [`HgcmCall`] offsets.
    pub const CALL_SIZE: usize = core::mem::size_of::<HgcmCall>();
    /// The client-identifier field.
    pub const CALL_CLIENT_ID: usize = core::mem::offset_of!(HgcmCall, client_id);
    /// The function field.
    pub const CALL_FUNCTION: usize = core::mem::offset_of!(HgcmCall, function);
    /// The parameter-count field.
    pub const CALL_COUNT: usize = core::mem::offset_of!(HgcmCall, count);
    /// Where the parameter array starts: C's `&callreq[1]`, which is the
    /// struct's size (`sizeof(*callreq)`), not its alignment.
    pub const CALL_PARAMS: usize = CALL_SIZE;

    /// [`HgcmParam`] stride and fields.
    ///
    /// The C array is indexed by stride `sizeof(struct VMMDevHGCMParam)`
    /// and starts at [`CALL_PARAMS`], so an element reference would be
    /// misaligned; the value slot sits eight bytes in, matching C's union
    /// alignment.
    pub const PARAM_SIZE: usize = core::mem::size_of::<HgcmParam>();
    /// The parameter type word.
    pub const PARAM_TYPE: usize = core::mem::offset_of!(HgcmParam, type_);
    /// The parameter value slot.
    pub const PARAM_VALUE: usize = core::mem::offset_of!(HgcmParam, value);

    /// [`HgcmPageList`] fields.
    pub const PAGE_LIST_HEADER: usize = core::mem::offset_of!(HgcmPageList, addr);
    /// The direction-flag word.
    pub const PAGE_LIST_FLAGS: usize = core::mem::offset_of!(HgcmPageList, flags);
    /// The first-page offset (16-bit).
    pub const PAGE_LIST_OFFSET: usize = core::mem::offset_of!(HgcmPageList, offset);
    /// The page count (16-bit).
    pub const PAGE_LIST_COUNT: usize = core::mem::offset_of!(HgcmPageList, count);
    /// The first page address.
    pub const PAGE_LIST_ADDR: usize = core::mem::offset_of!(HgcmPageList, addr);
    /// The stride of the page addresses.
    pub const PAGE_LIST_ADDR_SIZE: usize = core::mem::size_of::<u64>();

    /// [`HgcmCancel`] offsets.
    pub const CANCEL_SIZE: usize = core::mem::size_of::<HgcmCancel>();

    /// The caller-side parameter stride (`vbox_param_t`).
    ///
    /// C: `minix3/minix/include/minix/vboxtype.h:31-44` — the type word,
    /// then a union holding `u32`, `u64`, or the pointer descriptor
    /// `{grant, off, size, dir}`. On 64-bit the descriptor's four fields
    /// are `cp_grant_id_t` (4), `size_t` (8), `size_t` (8), `unsigned
    /// int` (4) with padding: the union is 32 bytes, so the stride is 40.
    pub const CALLER_PARAM_SIZE: usize = 40;
    /// The caller parameter's type word.
    pub const CALLER_PARAM_TYPE: usize = 0;
    /// The caller parameter's value slot (`u32`/`u64`/descriptor).
    pub const CALLER_PARAM_VALUE: usize = 8;
    /// The pointer descriptor's grant field.
    pub const CALLER_PTR_GRANT: usize = 8;
    /// The pointer descriptor's offset field.
    pub const CALLER_PTR_OFF: usize = 16;
    /// The pointer descriptor's size field.
    pub const CALLER_PTR_SIZE: usize = 24;
    /// The pointer descriptor's direction field.
    pub const CALLER_PTR_DIR: usize = 32;
}

/// Caller-side parameter types (`vbox_type_t`).
pub mod caller_type {
    /// Invalid type (`VBOX_TYPE_INVALID`).
    pub const INVALID: i32 = 0;
    /// 32-bit value (`VBOX_TYPE_U32`).
    pub const U32: i32 = 1;
    /// 64-bit value (`VBOX_TYPE_U64`).
    pub const U64: i32 = 2;
    /// Granted memory area (`VBOX_TYPE_PTR`).
    pub const PTR: i32 = 3;
}

/// Caller-side transfer directions (`VBOX_DIR_*`).
pub mod caller_dir {
    /// Host to guest (`VBOX_DIR_IN`).
    pub const IN: u32 = 0x01;
    /// Guest to host (`VBOX_DIR_OUT`).
    pub const OUT: u32 = 0x02;
}

/// Little-endian byte readers and writers for the request buffer.
///
/// The buffer is the device's memory: every field is read and written at
/// an explicit byte offset in little-endian order, which is the native
/// order of all three targets and of the C structs the device expects.
pub mod bytes {
    /// Reads a 16-bit word.
    pub fn read_u16(buffer: &[u8], offset: usize) -> u16 {
        let mut word = [0u8; 2];
        word.copy_from_slice(&buffer[offset..offset + 2]);
        u16::from_le_bytes(word)
    }

    /// Reads a 32-bit word.
    pub fn read_u32(buffer: &[u8], offset: usize) -> u32 {
        let mut word = [0u8; 4];
        word.copy_from_slice(&buffer[offset..offset + 4]);
        u32::from_le_bytes(word)
    }

    /// Reads a 64-bit word.
    pub fn read_u64(buffer: &[u8], offset: usize) -> u64 {
        let mut word = [0u8; 8];
        word.copy_from_slice(&buffer[offset..offset + 8]);
        u64::from_le_bytes(word)
    }

    /// Writes a 16-bit word.
    pub fn write_u16(buffer: &mut [u8], offset: usize, value: u16) {
        buffer[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    /// Writes a 32-bit word.
    pub fn write_u32(buffer: &mut [u8], offset: usize, value: u32) {
        buffer[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    /// Writes a 64-bit word.
    pub fn write_u64(buffer: &mut [u8], offset: usize, value: u64) {
        buffer[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mirror_structs_match_the_c_sizes() {
        // vmmdev.h: RequestHeader 6 words, ReportGuestInfo +2,
        // ReqHostTime header + u64, Events header + u32, HgcmHeader +2,
        // HgcmConnect +type+name[128]+client_id, HgcmDisconnect +client_id,
        // HgcmCall +3 words, HgcmCancel = HgcmHeader.
        assert_eq!(core::mem::size_of::<RequestHeader>(), 24);
        assert_eq!(core::mem::size_of::<ReportGuestInfo>(), 32);
        assert_eq!(core::mem::size_of::<ReqHostTime>(), 32);
        assert_eq!(core::mem::size_of::<Events>(), 28);
        assert_eq!(core::mem::size_of::<HgcmHeader>(), 32);
        assert_eq!(core::mem::size_of::<HgcmConnect>(), 168);
        assert_eq!(core::mem::size_of::<HgcmDisconnect>(), 36);
        assert_eq!(core::mem::size_of::<HgcmCall>(), 44);
        assert_eq!(core::mem::size_of::<HgcmCancel>(), 32);
        // The page list: flags + u16 + u16 + u64[1] = 16, header 8.
        assert_eq!(core::mem::size_of::<HgcmPageList>(), 16);
        assert_eq!(wire::PAGE_LIST_HEADER, 8);
        // The device-side parameter: type word, four bytes of padding, and
        // the eight-byte union.
        assert_eq!(core::mem::size_of::<HgcmParam>(), 16);
        assert_eq!(wire::PARAM_VALUE, 8);
    }

    #[test]
    fn test_derived_offsets_pin_the_c_numbers() {
        assert_eq!(wire::HEADER_SIZE, 24);
        assert_eq!(wire::HEADER_SIZE_FIELD, 0);
        assert_eq!(wire::HEADER_VERSION, 4);
        assert_eq!(wire::HEADER_TYPE, 8);
        assert_eq!(wire::HEADER_RESULT, 12);
        assert_eq!(wire::REPORT_ADD_VERSION, 24);
        assert_eq!(wire::REPORT_OS_TYPE, 28);
        assert_eq!(wire::HOST_TIME_TIME, 24);
        assert_eq!(wire::EVENTS_MASK, 24);
        assert_eq!(wire::HGCM_FLAGS, 24);
        assert_eq!(wire::HGCM_RESULT, 28);
        assert_eq!(wire::CONNECT_TYPE, 32);
        assert_eq!(wire::CONNECT_NAME, 36);
        assert_eq!(wire::CONNECT_CLIENT_ID, 164);
        assert_eq!(wire::DISCONNECT_CLIENT_ID, 32);
        assert_eq!(wire::CALL_CLIENT_ID, 32);
        assert_eq!(wire::CALL_FUNCTION, 36);
        assert_eq!(wire::CALL_COUNT, 40);
        assert_eq!(wire::CALL_PARAMS, 44);
        assert_eq!(wire::PARAM_SIZE, 16);
        assert_eq!(wire::PARAM_TYPE, 0);
        assert_eq!(wire::PAGE_LIST_FLAGS, 0);
        assert_eq!(wire::PAGE_LIST_OFFSET, 4);
        assert_eq!(wire::PAGE_LIST_COUNT, 6);
        assert_eq!(wire::PAGE_LIST_ADDR, 8);
        assert_eq!(wire::CANCEL_SIZE, 32);
        // The caller-side stride and descriptor (vboxtype.h).
        assert_eq!(wire::CALLER_PARAM_SIZE, 40);
        assert_eq!(wire::CALLER_PTR_GRANT, 8);
        assert_eq!(wire::CALLER_PTR_OFF, 16);
        assert_eq!(wire::CALLER_PTR_SIZE, 24);
        assert_eq!(wire::CALLER_PTR_DIR, 32);
    }

    #[test]
    fn test_protocol_numbers_match_vmmdev_header() {
        assert_eq!(PCI_VENDOR, 0x80ee);
        assert_eq!(PCI_DEVICE, 0xcafe);
        assert_eq!(REQ_HOST_TIME, 10);
        assert_eq!(REQ_ACKNOWLEDGE_EVENTS, 41);
        assert_eq!(REQ_REPORT_GUEST_INFO, 50);
        assert_eq!(REQ_HGCM_CONNECT, 60);
        assert_eq!(REQ_HGCM_DISCONNECT, 61);
        assert_eq!(REQ_HGCM_CALL, 62);
        assert_eq!(REQ_HGCM_CANCEL, 64);
        assert_eq!(ERR_OK, 0);
        assert_eq!(ERR_GENERIC, -1);
        assert_eq!(ERR_HGCM_NOT_FOUND, -2900);
        assert_eq!(ERR_HGCM_DENIED, 2901);
        assert_eq!(ERR_HGCM_INVALID_ADDR, -2902);
        assert_eq!(ERR_HGCM_ASYNC_EXEC, 2903);
        assert_eq!(ERR_HGCM_INTERNAL, -2904);
        assert_eq!(ERR_HGCM_INVALID_ID, -2905);
        assert_eq!(BACKDOOR_VERSION, 0x0001_0001);
        assert_eq!(GUEST_VERSION, 0x0001_0004);
        assert_eq!(GUEST_OS_TYPE, 0x90000);
        assert_eq!(EVENT_HGCM, 2);
        assert_eq!(HGCM_REQ_DONE, 1);
        assert_eq!(HGCM_SVCLOC_LOCALHOST_EXISTING, 2);
        assert_eq!(HGCM_NAME_SIZE, 128);
        assert_eq!(HGCM_FLAG_TO_HOST, 0x01);
        assert_eq!(HGCM_FLAG_FROM_HOST, 0x02);
        assert_eq!(HGCM_PARAM_U32, 1);
        assert_eq!(HGCM_PARAM_U64, 2);
        assert_eq!(HGCM_PARAM_PAGELIST, 10);
        assert_eq!(BUF_SIZE, 4096);
        // vboxtype.h: the caller's four parameter types and directions.
        assert_eq!(caller_type::INVALID, 0);
        assert_eq!(caller_type::U32, 1);
        assert_eq!(caller_type::U64, 2);
        assert_eq!(caller_type::PTR, 3);
        assert_eq!(caller_dir::IN, 0x01);
        assert_eq!(caller_dir::OUT, 0x02);
    }

    #[test]
    fn test_byte_helpers_round_trip_at_unaligned_offsets() {
        let mut buffer = [0u8; 64];
        bytes::write_u16(&mut buffer, 45, 0x1234);
        bytes::write_u32(&mut buffer, 47, 0xdead_beef);
        bytes::write_u64(&mut buffer, 51, 0x0123_4567_89ab_cdef);
        assert_eq!(bytes::read_u16(&buffer, 45), 0x1234);
        assert_eq!(bytes::read_u32(&buffer, 47), 0xdead_beef);
        assert_eq!(bytes::read_u64(&buffer, 51), 0x0123_4567_89ab_cdef);
        // Little-endian on the wire: the first byte is the low byte.
        assert_eq!(buffer[45], 0x34);
        assert_eq!(buffer[47], 0xef);
    }
}
