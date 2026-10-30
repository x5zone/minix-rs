//! Routing socket policy: buffer bounds and message version check.
//!
//! C correspondence: `minix3/minix/net/lwip/rtsock.c` (1912 lines).
//! Socket storage, message parsing, and table updates stay in the service
//! binary. This module owns the portion that can be decided from numbers
//! alone: which send and receive sizes pass, and which message versions are
//! accepted.
//!
//! Routing messages and address structures must only cross the boundary in
//! this module. Other modules must not reference the routing header or the
//! stack address types directly; they go through the routing socket.

/// Largest routing send buffer (`RT_SNDBUF_MAX`, 512, `rtsock.c:28`; there
/// is deliberately no minimum or default because sends are single messages).
pub const SEND_BUFFER_MAX: usize = 512;

/// Smallest routing receive buffer (`RT_RCVBUF_MIN`, 0, `rtsock.c:30`).
pub const RECEIVE_BUFFER_MIN: usize = 0;

/// Default routing receive buffer (`RT_RCVBUF_DEF`, 16384, `rtsock.c:31`,
/// installed at creation in `rtsock.c:338`).
pub const RECEIVE_BUFFER_DEFAULT: usize = 16384;

/// Largest routing receive buffer (`RT_RCVBUF_MAX`, 65536, `rtsock.c:32`).
pub const RECEIVE_BUFFER_MAX: usize = 65536;

/// Expected routing message version (`RTM_VERSION`, 4,
/// `minix3/sys/net/route.h:208`, checked at `rtsock.c:535`: mismatched
/// versions are rejected before the type switch).
pub const MESSAGE_VERSION: u8 = 4;

/// Whether a send length passes (`rtsock_pre_send`, `rtsock.c:634-651`:
/// messages longer than the send maximum are refused).
pub fn send_length_allowed(length: usize) -> bool {
    length <= SEND_BUFFER_MAX
}

/// Whether a receive buffer size passes (the option range at
/// `rtsock.c:825-830`).
pub fn receive_buffer_allowed(size: usize) -> bool {
    (RECEIVE_BUFFER_MIN..=RECEIVE_BUFFER_MAX).contains(&size)
}

/// Whether a message version is accepted.
pub fn message_version_allowed(version: u8) -> bool {
    version == MESSAGE_VERSION
}

// ---------------------------------------------------------------------------
// RTM frame face: the "service main program" half for route messages lands
// here, and the ruling that decision and storage stay separate is unchanged —
// this section only takes the message's wire shape into this module, keeping the
// isolation rule that route message headers do not leak to other modules. The
// walk over the header and the address array follows C: after the header,
// sockaddrs sit in RTAX bit order, each advancing by an 8-byte round-up
// (RT_ROUNDUP, where 0 also occupies 8 bytes, `sys/net/route.h:272-274`).
// ---------------------------------------------------------------------------

/// Message type: add route (`RTM_ADD`, 0x1, `route.h:210`).
pub const RTM_ADD: u8 = 0x1;
/// Message type: delete route (`RTM_DELETE`, 0x2, `route.h:211`).
pub const RTM_DELETE: u8 = 0x2;
/// Message type: change route (`RTM_CHANGE`, 0x3, `route.h:212`; same-key replace semantics).
pub const RTM_CHANGE: u8 = 0x3;
/// Message type: get route (`RTM_GET`, 0x4, `route.h:213`).
pub const RTM_GET: u8 = 0x4;

/// Address bit: destination (`RTA_DST`, 0x1, `route.h:248`).
pub const RTA_DST: i32 = 0x1;
/// Address bit: gateway (`RTA_GATEWAY`, 0x2, `route.h:249`).
pub const RTA_GATEWAY: i32 = 0x2;
/// Address bit: netmask (`RTA_NETMASK`, 0x4, `route.h:250`).
pub const RTA_NETMASK: i32 = 0x4;

/// Receipt flag: the entry is usable (`RTF_UP`, 0x1, `route.h:140`).
pub const RTF_UP: i32 = 0x1;
/// Receipt flag: reached through a gateway (`RTF_GATEWAY`, 0x2, `route.h:141`).
pub const RTF_GATEWAY: i32 = 0x2;
/// Receipt flag: host entry (`RTF_HOST`, 0x4, `route.h:142`).
pub const RTF_HOST: i32 = 0x4;

/// Byte size of `struct rt_msghdr` under LP64 (ten 4-byte fields at 0..40 plus
/// 80 bytes of metrics at 40; the header is 120 bytes and naturally 8-aligned).
pub const HEADER_LEN: usize = 120;

/// Address family: version 4 (`AF_INET`, 2).
const AF_INET: u8 = 2;
/// Address family: version 6 (`AF_INET6`, 24).
const AF_INET6: u8 = 24;

/// Round up to 8 bytes (`RT_ROUNDUP2(a, sizeof(uint64_t))`, `route.h:272-273`;
/// 0 still occupies one slot).
fn roundup(len: usize) -> usize {
    if len == 0 {
        8
    } else {
        (len + 7) & !7
    }
}

/// One sockaddr's parse result: the family plus an address body of at most 16
/// bytes (which holds both version 4's `sin_addr`@4 and version 6's
/// `sin6_addr`@8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RtSockaddr {
    /// Address family byte (`sa_family`, the second byte of the object at `sys/socket.h:229-233`).
    pub family: u8,
    /// Address body (4 bytes for version 4, 16 bytes for version 6).
    pub addr: [u8; 16],
}

/// A decoded RTM frame: the minimal field set the command and the receipt need.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RtFrame {
    /// Message type (`rtm_type`).
    pub msg_type: u8,
    /// Egress interface index (`rtm_index`, landing in the route entry's ifdev half).
    pub index: u16,
    /// Flags (`rtm_flags`).
    pub flags: i32,
    /// Address bit mask (`rtm_addrs`).
    pub addrs: i32,
    /// Sender process number (`rtm_pid`, echoed back verbatim in the receipt).
    pub pid: i32,
    /// Sender sequence number (`rtm_seq`, echoed back verbatim in the receipt).
    pub seq: i32,
    /// Destination address (parsed when the `RTA_DST` bit is present).
    pub dst: Option<RtSockaddr>,
    /// Gateway (parsed when the `RTA_GATEWAY` bit is present; `None` = directly connected).
    pub gateway: Option<RtSockaddr>,
    /// Prefix length (derived from the netmask when the `RTA_NETMASK` bit is
    /// present; without the bit = a host entry, full 32/128 by family).
    pub prefix: Option<u8>,
}

/// Take the family and address body from the sockaddr slot at `base` (version 4
/// addresses at `sin_addr`@4, version 6 at `sin6_addr`@8,
/// `netinet6/in6.h:145-151`). Other families carry no meaning on this model's
/// routing face and fold to EAFNOSUPPORT the way C's "destination family not
/// understood" does (C rejects later, in the route_process stage).
fn parse_addr_slot(buf: &[u8], base: usize) -> Result<RtSockaddr, i32> {
    let family = *buf.get(base + 1).ok_or(minix_types::EINVAL)?;
    let mut addr = [0u8; 16];
    match family {
        AF_INET => {
            for (i, byte) in addr.iter_mut().take(4).enumerate() {
                *byte = *buf.get(base + 4 + i).ok_or(minix_types::EINVAL)?;
            }
        }
        AF_INET6 => {
            for (i, byte) in addr.iter_mut().enumerate() {
                *byte = *buf.get(base + 8 + i).ok_or(minix_types::EINVAL)?;
            }
        }
        _ => return Err(minix_types::EAFNOSUPPORT),
    }
    Ok(RtSockaddr { family, addr })
}

/// Fold the netmask slot at `base` into a prefix length. Both wire forms are
/// accepted: the full form (family 2/24 with the mask at sin_addr@4 /
/// sin6_addr@8) and the compressed form (mask bytes starting at `sa_data`@2,
/// whose family byte may be 0 — C's `rtsock_expand_netmask` expands the same
/// form at rtsock.c:598-605). Bit order is big-endian (bit 0 is the high bit of
/// the first byte), and after counting the leading run of ones every remaining
/// bit must be zero.
fn mask_prefix(buf: &[u8], base: usize, sa_len: usize) -> Result<u8, i32> {
    let family = *buf.get(base + 1).ok_or(minix_types::EINVAL)?;
    let (start, max_bits) = match (family, sa_len) {
        (AF_INET, l) if l >= 8 => (base + 4, 32u16),
        (AF_INET6, l) if l >= 24 => (base + 8, 128u16),
        (_, l) if l >= 2 => (base + 2, 128u16),
        // A head-only empty slot: prefix 0 (default route).
        _ => return Ok(0),
    };
    let end = (base + sa_len).min(start + 16);
    let mut prefix = 0u16;
    let mut ones_ended = false;
    for &byte in buf.get(start..end).ok_or(minix_types::EINVAL)? {
        let mut bit = 0x80u8;
        for _ in 0..8 {
            let set = byte & bit != 0;
            if set {
                if ones_ended {
                    return Err(minix_types::EINVAL);
                }
                prefix += 1;
            } else {
                ones_ended = true;
            }
            bit >>= 1;
        }
    }
    if prefix > max_bits {
        return Err(minix_types::EINVAL);
    }
    Ok(prefix as u8)
}

/// Decode one RTM frame (the check order of C `rtsock_put`, rtsock.c:528-570):
/// (1) `rtm_msglen` must equal the buffer length; (2) the version must be 4 (the
/// type is not parsed otherwise); (3) the address array is walked slot by slot
/// (bounds check, then family and body). The type and identity gates live in
/// [`crate::sockops::rt_road`] (which checks root identity), and table updates
/// stay on the service side too.
pub fn parse_frame(buf: &[u8]) -> Result<RtFrame, i32> {
    if buf.len() < HEADER_LEN {
        return Err(minix_types::EINVAL);
    }
    // The header length was guaranteed by the gate check, so slicing fixed field positions cannot go out of bounds.
    let word = |at: usize| i32::from_le_bytes(buf[at..at + 4].try_into().unwrap());
    let msglen = u16::from_le_bytes(buf[0..2].try_into().map_err(|_| minix_types::EINVAL)?)
        as usize;
    if msglen != buf.len() {
        return Err(minix_types::EINVAL);
    }
    if buf[2] != MESSAGE_VERSION {
        return Err(minix_types::EPROTONOSUPPORT);
    }
    let index = u16::from_le_bytes(buf[4..6].try_into().map_err(|_| minix_types::EINVAL)?);
    let mut frame = RtFrame {
        msg_type: buf[3],
        index,
        flags: word(8),
        addrs: word(12),
        pid: word(16),
        seq: word(20),
        dst: None,
        gateway: None,
        prefix: None,
    };
    // Address array walk: the bit order of `rtm_addrs` is the RTAX order
    // (DST=0, GATEWAY=1, NETMASK=2, route.h:261-263), and each slot advances by
    // RT_ROUNDUP(sa_len).
    let mut off = HEADER_LEN;
    let mut mask_slot: Option<(usize, usize)> = None;
    for slot in 0..9 {
        if frame.addrs & (1 << slot) == 0 {
            continue;
        }
        // A slot must at least hold the first two bytes of a sockaddr (C rtsock.c:572-575).
        if off + 2 > buf.len() {
            return Err(minix_types::EINVAL);
        }
        let sa_len = buf[off] as usize;
        if off + sa_len > buf.len() {
            return Err(minix_types::EINVAL);
        }
        match slot {
            0 => frame.dst = Some(parse_addr_slot(buf, off)?),
            1 => frame.gateway = Some(parse_addr_slot(buf, off)?),
            2 => mask_slot = Some((off, sa_len)),
            _ => {}
        }
        off += roundup(sa_len);
    }
    frame.prefix = match mask_slot {
        Some((base, sa_len)) => Some(mask_prefix(buf, base, sa_len)?),
        // Bit absent = a host entry; the prefix is filled to the family maximum at table update.
        None => None,
    };
    Ok(frame)
}

/// Write a full-form sockaddr at the `base` offset of `out` (two header bytes
/// plus the body bits; a mask slot puts the prefix mask in the body bits), and
/// zero the remaining room in the slot.
fn write_sockaddr(
    out: &mut [u8],
    base: usize,
    version: crate::route::IpVersion,
    addr: &[u8],
    prefix: Option<u8>,
) {
    let (family, sa_len, addr_at) = match version {
        crate::route::IpVersion::V4 => (AF_INET, 16usize, 4usize),
        crate::route::IpVersion::V6 => (AF_INET6, 28usize, 8usize),
    };
    let width = addr_len(version);
    for byte in &mut out[base..base + roundup(sa_len)] {
        *byte = 0;
    }
    out[base] = sa_len as u8;
    out[base + 1] = family;
    let mut body = [0u8; 16];
    match prefix {
        Some(p) => {
            let mask = crate::route::prefix_mask(version, p);
            body[..width].copy_from_slice(&mask[..width]);
        }
        None => body[..width].copy_from_slice(&addr[..width]),
    }
    out[base + addr_at..base + addr_at + width].copy_from_slice(&body[..width]);
}

/// Encode the RTM_GET frame of one route entry (the read export face): header +
/// DST + GATEWAY (when present) + NETMASK, all in full-form sockaddrs (the mask
/// is not compressed — C's export face sends the compressed form; the full form
/// is the same semantics expressed uncompressed, a noted difference). Returns the
/// number of bytes written to `out`; `None` when `out` cannot hold the whole
/// frame.
pub fn encode_entry_frame(
    entry: &crate::route::RouteEntry,
    pid: i32,
    seq: i32,
    out: &mut [u8],
) -> Option<usize> {
    let (sa_len, width) = match entry.version {
        crate::route::IpVersion::V4 => (16usize, 4usize),
        crate::route::IpVersion::V6 => (28usize, 16usize),
    };
    let gw_len = if entry.gateway.is_some() { roundup(sa_len) } else { 0 };
    let total = HEADER_LEN + roundup(sa_len) * 2 + gw_len;
    if out.len() < total {
        return None;
    }
    for byte in out[..total].iter_mut() {
        *byte = 0;
    }
    // Header: type GET, version 4, interface index, flags, address bits, receipt pid/seq.
    let mut flags = RTF_UP;
    if entry.gateway.is_some() {
        flags |= RTF_GATEWAY;
    }
    let max_prefix = match entry.version {
        crate::route::IpVersion::V4 => 32,
        crate::route::IpVersion::V6 => 128,
    };
    if entry.prefix == max_prefix {
        flags |= RTF_HOST;
    }
    let addrs = RTA_DST
        | RTA_NETMASK
        | if entry.gateway.is_some() { RTA_GATEWAY } else { 0 };
    out[0..2].copy_from_slice(&(total as u16).to_le_bytes());
    out[2] = MESSAGE_VERSION;
    out[3] = RTM_GET;
    out[4..6].copy_from_slice(&entry.ifdev.to_le_bytes());
    out[8..12].copy_from_slice(&flags.to_le_bytes());
    out[12..16].copy_from_slice(&addrs.to_le_bytes());
    out[16..20].copy_from_slice(&pid.to_le_bytes());
    out[20..24].copy_from_slice(&seq.to_le_bytes());
    let dest = &entry.dest[..width];
    let mut off = HEADER_LEN;
    write_sockaddr(out, off, entry.version, dest, None);
    off += roundup(sa_len);
    if let Some(gateway) = entry.gateway {
        write_sockaddr(out, off, entry.version, &gateway[..width], None);
        off += roundup(sa_len);
    }
    write_sockaddr(out, off, entry.version, dest, Some(entry.prefix));
    Some(total)
}

/// Address byte count for the family (the entry half).
fn addr_len(version: crate::route::IpVersion) -> usize {
    match version {
        crate::route::IpVersion::V4 => 4,
        crate::route::IpVersion::V6 => 16,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_bounds_match_socket_source() {
        assert_eq!(SEND_BUFFER_MAX, 512);
        assert_eq!(RECEIVE_BUFFER_DEFAULT, 16384);
        assert_eq!(RECEIVE_BUFFER_MAX, 65536);
        assert!(send_length_allowed(512));
        assert!(!send_length_allowed(513));
        assert!(receive_buffer_allowed(0));
        assert!(receive_buffer_allowed(65536));
        assert!(!receive_buffer_allowed(65537));
    }

    #[test]
    fn test_message_version_is_checked_first() {
        assert!(message_version_allowed(4));
        assert!(!message_version_allowed(3));
        assert!(!message_version_allowed(5));
    }

    /// Build the raw bytes of an ADD frame (full-form sockaddrs: DST+GATEWAY+NETMASK).
    fn sample_add_frame() -> alloc::vec::Vec<u8> {
        let mut buf = alloc::vec![0u8; HEADER_LEN + 16 * 3];
        let total = buf.len();
        buf[0..2].copy_from_slice(&(total as u16).to_le_bytes());
        buf[2] = MESSAGE_VERSION;
        buf[3] = RTM_ADD;
        buf[4..6].copy_from_slice(&3u16.to_le_bytes());
        buf[12..16].copy_from_slice(&(RTA_DST | RTA_GATEWAY | RTA_NETMASK).to_le_bytes());
        buf[16..20].copy_from_slice(&4242i32.to_le_bytes());
        buf[20..24].copy_from_slice(&7i32.to_le_bytes());
        // DST 10.1.0.0.
        buf[HEADER_LEN] = 16;
        buf[HEADER_LEN + 1] = 2;
        buf[HEADER_LEN + 4..HEADER_LEN + 8].copy_from_slice(&[10, 1, 0, 0]);
        // GATEWAY 10.0.0.1.
        let gw = HEADER_LEN + 16;
        buf[gw] = 16;
        buf[gw + 1] = 2;
        buf[gw + 4..gw + 8].copy_from_slice(&[10, 0, 0, 1]);
        // NETMASK /16 (full form).
        let nm = HEADER_LEN + 32;
        buf[nm] = 16;
        buf[nm + 1] = 2;
        buf[nm + 4..nm + 8].copy_from_slice(&[255, 255, 0, 0]);
        buf
    }

    #[test]
    fn test_parse_frame_walks_sockaddrs_and_checks_first() {
        let buf = sample_add_frame();
        let frame = parse_frame(&buf).expect("合法帧解析");
        assert_eq!(frame.msg_type, RTM_ADD);
        assert_eq!(frame.index, 3);
        assert_eq!(frame.pid, 4242);
        assert_eq!(frame.seq, 7);
        let dst = frame.dst.expect("目的在位");
        assert_eq!(dst.family, 2);
        assert_eq!(&dst.addr[..4], &[10, 1, 0, 0]);
        let gw = frame.gateway.expect("网关在位");
        assert_eq!(&gw.addr[..4], &[10, 0, 0, 1]);
        assert_eq!(frame.prefix, Some(16), "完整形态掩码折前缀 16");

        // Check order (1): msglen disagrees with the buffer length → EINVAL.
        let mut short = buf.clone();
        short[0..2].copy_from_slice(&99u16.to_le_bytes());
        assert_eq!(parse_frame(&short).unwrap_err(), minix_types::EINVAL);
        // Check order (2): version is not 4 → EPROTONOSUPPORT.
        let mut alien = buf.clone();
        alien[2] = 5;
        assert_eq!(
            parse_frame(&alien).unwrap_err(),
            minix_types::EPROTONOSUPPORT
        );
        // Too-short buffer: EINVAL.
        assert_eq!(parse_frame(&buf[..40]).unwrap_err(), minix_types::EINVAL);
    }

    #[test]
    fn test_compressed_netmask_and_no_mask_host_route() {
        // Compressed form: only the two header bytes plus the mask bytes (/24 → three 0xFF).
        let mut buf = alloc::vec![0u8; HEADER_LEN + 16 + 8];
        let total = buf.len();
        buf[0..2].copy_from_slice(&(total as u16).to_le_bytes());
        buf[2] = MESSAGE_VERSION;
        buf[3] = RTM_ADD;
        buf[12..16].copy_from_slice(&(RTA_DST | RTA_NETMASK).to_le_bytes());
        buf[HEADER_LEN] = 16;
        buf[HEADER_LEN + 1] = 2;
        buf[HEADER_LEN + 4..HEADER_LEN + 8].copy_from_slice(&[192, 168, 1, 1]);
        let nm = HEADER_LEN + 16;
        buf[nm] = 5; // 2-byte header + 3 mask bytes
        buf[nm + 1] = 0; // the compressed form's family byte may be 0
        buf[nm + 2..nm + 5].copy_from_slice(&[255, 255, 255]);
        let frame = parse_frame(&buf).expect("压缩掩码解析");
        assert_eq!(frame.prefix, Some(24));

        // Non-contiguous mask rejected.
        buf[nm + 2..nm + 5].copy_from_slice(&[255, 0, 255]);
        assert_eq!(parse_frame(&buf).unwrap_err(), minix_types::EINVAL);

        // Mask bit absent = host entry (prefix None; table update fills it to the family maximum).
        let mut host = alloc::vec![0u8; HEADER_LEN + 16];
        let total = host.len();
        host[0..2].copy_from_slice(&(total as u16).to_le_bytes());
        host[2] = MESSAGE_VERSION;
        host[3] = RTM_ADD;
        host[12..16].copy_from_slice(&RTA_DST.to_le_bytes());
        host[HEADER_LEN] = 16;
        host[HEADER_LEN + 1] = 2;
        host[HEADER_LEN + 4..HEADER_LEN + 8].copy_from_slice(&[10, 0, 0, 9]);
        let frame = parse_frame(&host).expect("主机条目解析");
        assert_eq!(frame.prefix, None);
    }

    #[test]
    fn test_encode_entry_frame_roundtrips_through_parse() {
        let entry = crate::route::RouteEntry {
            version: crate::route::IpVersion::V4,
            dest: {
                let mut d = [0u8; 16];
                d[..4].copy_from_slice(&[10, 1, 0, 0]);
                d
            },
            prefix: 16,
            gateway: Some({
                let mut g = [0u8; 16];
                g[..4].copy_from_slice(&[10, 0, 0, 1]);
                g
            }),
            ifdev: 5,
        };
        let mut out = [0u8; 256];
        let n = encode_entry_frame(&entry, 4242, 7, &mut out).expect("导出帧");
        let frame = parse_frame(&out[..n]).expect("导出帧可回解析");
        assert_eq!(frame.msg_type, RTM_GET);
        assert_eq!(frame.index, 5);
        assert_eq!(frame.flags & RTF_GATEWAY, RTF_GATEWAY, "经网关旗标在位");
        assert_eq!(frame.flags & RTF_HOST, 0, "前缀非满长，无主机旗标");
        assert_eq!(frame.prefix, Some(16));
        assert_eq!(&frame.dst.unwrap().addr[..4], &[10, 1, 0, 0]);
        assert_eq!(&frame.gateway.unwrap().addr[..4], &[10, 0, 0, 1]);

        // A host entry carries the host flag; a buffer too small returns None.
        let host = crate::route::RouteEntry { prefix: 32, ..entry };
        let n = encode_entry_frame(&host, 0, 0, &mut out).expect("主机帧");
        assert_eq!(parse_frame(&out[..n]).unwrap().flags & RTF_HOST, RTF_HOST);
        let mut tiny = [0u8; 100];
        assert!(encode_entry_frame(&entry, 0, 0, &mut tiny).is_none());
    }
}
