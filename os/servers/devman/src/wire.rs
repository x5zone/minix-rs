//! Wire-format parsing for device descriptions (doc 03-devm-structs).
//!
//! C: `struct devman_device_info` / `devman_device_info_entry`
//! (`minix/include/minix/devman.h:8-20`, mirrored in
//! `servers/devman/devinfo.h:21-33`) as produced by `serialize_dev`
//! (`lib/libdevman/generic.c:36-99`).
//!
//! Layout (all little-endian, native x86 order):
//! ```text
//! offset 0:  count            i32   (entry count, ≥ 0)
//! offset 4:  parent_dev_id    i32
//! offset 8:  name_offset      u32   (from buffer start)
//! offset 12: subsystem_offset u32   (**never written** — `serialize_dev`
//!            leaves the malloc'd word uninitialized (generic.c has no
//!            assignment); the server never reads it. Decode ignores it;
//!            encode (10) writes 0. See 03 §2.6.)
//! offset 16: entries[count] × { type u32, name_offset u32,
//!            data_offset u32, req_nr u32 } (16 bytes each)
//! then:      NUL-terminated strings (offsets from buffer start)
//! ```
//! `type` is always 0 (`entry->type = 0; /* TODO: use macro */`,
//! generic.c:88); `req_nr` is never written by the serializer AND never
//! read by the server (`rg req_nr` hits only the `devinfo.h:32`
//! definition) — 4 garbage bytes per entry, preserved raw for roundtrip
//! fidelity. `type` classification is 07's job (A-6).
//!
//! [ARCH:A-4] explicit bounds-checked parsing, `no_std`-safe.

use alloc::string::String;
use alloc::vec::Vec;
use minix_types::Errno;

use crate::structs::DeviceId;

pub const WIRE_HEADER_LEN: usize = 16;
pub const WIRE_ENTRY_LEN: usize = 16;

/// C: `enum devman_inode_type` (`devman.h:47-51`).
/// Values locked by test (`static=0, dynamic=1, device=2`).
/// `DYNAMIC` is preserved raw here; 07 defers it (A-6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum EntryType {
    Static = 0,
    Dynamic = 1,
    Device = 2,
}

impl EntryType {
    pub fn from_u32(v: u32) -> Result<Self, Errno> {
        match v {
            0 => Ok(EntryType::Static),
            1 => Ok(EntryType::Dynamic),
            2 => Ok(EntryType::Device),
            _ => Err(Errno::EINVAL),
        }
    }
}

/// One decoded entry: classified type + owned strings + raw `req_nr`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireEntry {
    pub ty: EntryType,
    pub name: String,
    pub data: String,
    pub req_nr: u32,
}

/// A fully decoded device description (07 consumes this).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedDevice {
    pub parent: DeviceId,
    pub name: String,
    pub entries: Vec<WireEntry>,
}

/// Wire errors. Callers map to `EINVAL` (malformed client input).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireError {
    Truncated,
    BadOffset,
    BadCount,
    NonUtf8,
}

fn read_i32(buf: &[u8], off: usize) -> Result<i32, WireError> {
    let s = buf.get(off..off + 4).ok_or(WireError::Truncated)?;
    let b: [u8; 4] = s.try_into().map_err(|_| WireError::Truncated)?;
    Ok(i32::from_le_bytes(b))
}

fn read_u32(buf: &[u8], off: usize) -> Result<u32, WireError> {
    let s = buf.get(off..off + 4).ok_or(WireError::Truncated)?;
    let b: [u8; 4] = s.try_into().map_err(|_| WireError::Truncated)?;
    Ok(u32::from_le_bytes(b))
}

fn read_cstr(buf: &[u8], off: u32) -> Result<String, WireError> {
    let rest = buf.get(off as usize..).ok_or(WireError::BadOffset)?;
    let len = rest.iter().position(|&b| b == 0).ok_or(WireError::BadOffset)?;
    core::str::from_utf8(&rest[..len])
        .map(String::from)
        .map_err(|_| WireError::NonUtf8)
}

/// Decode one serialized device (see module docs for layout).
/// `parent` comes back as raw `i32` (validated to `DeviceId` by 07,
/// which owns id allocation); negative counts, out-of-range offsets,
/// and unterminated strings are all `WireError`.
pub fn parse_device(buf: &[u8]) -> Result<(i32, ParsedDevice), WireError> {
    if buf.len() < WIRE_HEADER_LEN {
        return Err(WireError::Truncated);
    }
    let count = read_i32(buf, 0)?;
    if count < 0 {
        return Err(WireError::BadCount);
    }
    let parent_raw = read_i32(buf, 4)?;
    let name_off = read_u32(buf, 8)?;
    // offset 12 (subsystem_offset): deliberately unread (see module docs).
    let count = count as usize;
    let entries_end = WIRE_HEADER_LEN
        .checked_add(count.checked_mul(WIRE_ENTRY_LEN).ok_or(WireError::BadCount)?)
        .ok_or(WireError::BadCount)?;
    if buf.len() < entries_end {
        return Err(WireError::Truncated);
    }
    let name = read_cstr(buf, name_off)?;
    let mut entries = Vec::new();
    entries.try_reserve(count).map_err(|_| WireError::Truncated)?;
    for i in 0..count {
        let base = WIRE_HEADER_LEN + i * WIRE_ENTRY_LEN;
        let ty = EntryType::from_u32(read_u32(buf, base)?).map_err(|_| WireError::BadCount)?;
        let ename = read_cstr(buf, read_u32(buf, base + 4)?)?;
        let edata = read_cstr(buf, read_u32(buf, base + 8)?)?;
        let req_nr = read_u32(buf, base + 12)?;
        entries.push(WireEntry {
            ty,
            name: ename,
            data: edata,
            req_nr,
        });
    }
    Ok((
        parent_raw,
        ParsedDevice {
            parent: DeviceId(parent_raw as u32),
            name,
            entries,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    /// Build a wire buffer in the exact `serialize_dev` layout.
    fn encode(name: &str, entries: &[(&str, &str)]) -> Vec<u8> {
        let mut buf = vec![0u8; WIRE_HEADER_LEN + entries.len() * WIRE_ENTRY_LEN];
        buf[0..4].copy_from_slice(&(entries.len() as i32).to_le_bytes());
        buf[4..8].copy_from_slice(&1i32.to_le_bytes()); // parent_dev_id
        let mut strings = Vec::new();
        let mut push = |s: &str| -> u32 {
            let off = (buf.len() + strings.len()) as u32;
            strings.extend_from_slice(s.as_bytes());
            strings.push(0);
            off
        };
        let name_off = push(name);
        let mut eoffs = Vec::new();
        for (n, d) in entries {
            eoffs.push((push(n), push(d)));
        }
        buf[8..12].copy_from_slice(&name_off.to_le_bytes());
        // offset 12 left 0 (encode writes 0; C leaves garbage — see docs).
        for (i, (no, dob)) in eoffs.iter().enumerate() {
            let base = WIRE_HEADER_LEN + i * WIRE_ENTRY_LEN;
            buf[base..base + 4].copy_from_slice(&0u32.to_le_bytes()); // STATIC
            buf[base + 4..base + 8].copy_from_slice(&no.to_le_bytes());
            buf[base + 8..base + 12].copy_from_slice(&dob.to_le_bytes());
            // req_nr left 0 (C leaves garbage).
        }
        buf.extend_from_slice(&strings);
        buf
    }

    #[test]
    fn entry_values_match_c() {
        // C: devman.h:47-51.
        assert_eq!(EntryType::Static as u32, 0);
        assert_eq!(EntryType::Dynamic as u32, 1);
        assert_eq!(EntryType::Device as u32, 2);
        assert_eq!(EntryType::from_u32(7), Err(Errno::EINVAL));
    }

    #[test]
    fn roundtrip_mirrors_serialize_dev() {
        let buf = encode("usb", &[("dev_type", "USB_DEV"), ("idVendor", "1234")]);
        let (parent, dev) = parse_device(&buf).unwrap();
        assert_eq!(parent, 1);
        assert_eq!(dev.name, "usb");
        assert_eq!(dev.entries.len(), 2);
        assert_eq!(dev.entries[0].ty, EntryType::Static);
        assert_eq!(dev.entries[0].name, "dev_type");
        assert_eq!(dev.entries[0].data, "USB_DEV");
        assert_eq!(dev.entries[1].name, "idVendor");
    }

    #[test]
    fn malformed_is_rejected() {
        assert_eq!(parse_device(&[]), Err(WireError::Truncated));
        assert_eq!(parse_device(&[0u8; 8]), Err(WireError::Truncated));
        // Negative count.
        let mut bad = encode("x", &[]);
        bad[0..4].copy_from_slice(&(-1i32).to_le_bytes());
        assert_eq!(parse_device(&bad), Err(WireError::BadCount));
        // Name offset past the end.
        let mut bad2 = encode("x", &[]);
        bad2[8..12].copy_from_slice(&9999u32.to_le_bytes());
        assert_eq!(parse_device(&bad2), Err(WireError::BadOffset));
        // Declared entry missing.
        let mut bad3 = encode("x", &[]);
        bad3[0..4].copy_from_slice(&3i32.to_le_bytes());
        assert_eq!(parse_device(&bad3), Err(WireError::Truncated));
    }

    #[test]
    fn dynamic_type_preserved_for_07() {
        // A-6: DYNAMIC is not rejected here; 07 defers it.
        let mut buf = encode("x", &[("a", "b")]);
        buf[WIRE_HEADER_LEN..WIRE_HEADER_LEN + 4].copy_from_slice(&1u32.to_le_bytes());
        let (_, dev) = parse_device(&buf).unwrap();
        assert_eq!(dev.entries[0].ty, EntryType::Dynamic);
    }
}
