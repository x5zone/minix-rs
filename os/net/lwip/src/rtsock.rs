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
// RTM 帧面（第 20 篇 §2.5 登记的"服务主程序"半在此落地；判断与存储分离
// 的裁决不变——本节只是把消息的线形状收进本模块，保持"路由消息头不外泄
// 其他模块"的隔离规则）。消息头与地址数组的走查规则照 C：头之后按
// RTAX 位序排 sockaddr，每个按 8 字节向上取整推进（RT_ROUNDUP，0 也占
// 8 字节，`sys/net/route.h:272-274`）。
// ---------------------------------------------------------------------------

/// 消息类型：增路由（`RTM_ADD`，0x1，`route.h:210`）。
pub const RTM_ADD: u8 = 0x1;
/// 消息类型：删路由（`RTM_DELETE`，0x2，`route.h:211`）。
pub const RTM_DELETE: u8 = 0x2;
/// 消息类型：改路由（`RTM_CHANGE`，0x3，`route.h:212`；同键替换语义）。
pub const RTM_CHANGE: u8 = 0x3;
/// 消息类型：查路由（`RTM_GET`，0x4，`route.h:213`）。
pub const RTM_GET: u8 = 0x4;

/// 地址位：目的（`RTA_DST`，0x1，`route.h:248`）。
pub const RTA_DST: i32 = 0x1;
/// 地址位：网关（`RTA_GATEWAY`，0x2，`route.h:249`）。
pub const RTA_GATEWAY: i32 = 0x2;
/// 地址位：掩码（`RTA_NETMASK`，0x4，`route.h:250`）。
pub const RTA_NETMASK: i32 = 0x4;

/// 回执旗标：条目可用（`RTF_UP`，0x1，`route.h:140`）。
pub const RTF_UP: i32 = 0x1;
/// 回执旗标：经网关（`RTF_GATEWAY`，0x2，`route.h:141`）。
pub const RTF_GATEWAY: i32 = 0x2;
/// 回执旗标：主机条目（`RTF_HOST`，0x4，`route.h:142`）。
pub const RTF_HOST: i32 = 0x4;

/// LP64 下 `struct rt_msghdr` 的字节数（10 个 4 字节域 @0..40 加
/// 80 字节 metrics @40，头长 120 且天然 8 对齐）。
pub const HEADER_LEN: usize = 120;

/// 地址族：版本 4（`AF_INET`，2）。
const AF_INET: u8 = 2;
/// 地址族：版本 6（`AF_INET6`，24）。
const AF_INET6: u8 = 24;

/// 8 字节向上取整（`RT_ROUNDUP2(a, sizeof(uint64_t))`，`route.h:272-273`；
/// 0 也占一个槽位）。
fn roundup(len: usize) -> usize {
    if len == 0 {
        8
    } else {
        (len + 7) & !7
    }
}

/// 一条 sockaddr 的解析结果：族加至多 16 字节的地址本体（版本 4 的
/// `sin_addr`@4 与版本 6 的 `sin6_addr`@8 都装得下）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RtSockaddr {
    /// 地址族字节（`sa_family`，`sys/socket.h:229-233` 的第 2 字节）。
    pub family: u8,
    /// 地址本体（版本 4 取 4 字节、版本 6 取 16 字节）。
    pub addr: [u8; 16],
}

/// 解码后的 RTM 帧：命令与回执所需的最小域集。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RtFrame {
    /// 消息类型（`rtm_type`）。
    pub msg_type: u8,
    /// 出接口行号（`rtm_index`，落到路由条目的 ifdev 半）。
    pub index: u16,
    /// 旗标（`rtm_flags`）。
    pub flags: i32,
    /// 地址位掩码（`rtm_addrs`）。
    pub addrs: i32,
    /// 发送方进程号（`rtm_pid`，回执原样带回）。
    pub pid: i32,
    /// 发送方序号（`rtm_seq`，回执原样带回）。
    pub seq: i32,
    /// 目的地址（`RTA_DST` 位在场时解析）。
    pub dst: Option<RtSockaddr>,
    /// 网关（`RTA_GATEWAY` 位在场时解析；`None` = 直连）。
    pub gateway: Option<RtSockaddr>,
    /// 前缀长度（`RTA_NETMASK` 位在场时从掩码折算；位不在场 = 主机
    /// 条目，按族取满 32/128）。
    pub prefix: Option<u8>,
}

/// 从 `base` 起的 sockaddr 槽位取族与地址本体（版本 4 的地址在
/// `sin_addr`@4，版本 6 在 `sin6_addr`@8，`netinet6/in6.h:145-151`）。
/// 其余族在本模型的路由面无语义，按 C 的"目的族不认识"折
/// EAFNOSUPPORT（登记：C 在更晚的 route_process 阶段才拒）。
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

/// 从 `base` 起的 netmask 槽位折前缀长度。两种线上形态都收：完整
/// 形态（族 2/24，掩码在 sin_addr@4 / sin6_addr@8）与压缩形态
/// （掩码位从 `sa_data`@2 起，族字节可为 0——C 的
/// `rtsock_expand_netmask` 在 rtsock.c:598-605 展开同一形态）。位序
/// 大端（位 0 是首字节最高位），前导连续 1 计满后余位必须为零。
fn mask_prefix(buf: &[u8], base: usize, sa_len: usize) -> Result<u8, i32> {
    let family = *buf.get(base + 1).ok_or(minix_types::EINVAL)?;
    let (start, max_bits) = match (family, sa_len) {
        (AF_INET, l) if l >= 8 => (base + 4, 32u16),
        (AF_INET6, l) if l >= 24 => (base + 8, 128u16),
        (_, l) if l >= 2 => (base + 2, 128u16),
        // 只有头的空槽：前缀 0（默认路由）。
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

/// 解码一条 RTM 帧（C `rtsock_put` 的检查序，rtsock.c:528-570）：
/// ① `rtm_msglen` 必须等于缓冲长度；② 版本必须是 4（不对不解析类型）；
/// ③ 地址数组逐槽走查（界检查后取族与本体）。类型与身份的门在
/// [`crate::sockops::rt_road`]（要查根身份），表更新也在服务路。
pub fn parse_frame(buf: &[u8]) -> Result<RtFrame, i32> {
    if buf.len() < HEADER_LEN {
        return Err(minix_types::EINVAL);
    }
    // 头长已在门槛检查中保证，固定域位的切片不越界。
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
    // 地址数组走查：`rtm_addrs` 的位序即 RTAX 序（DST=0、GATEWAY=1、
    // NETMASK=2，route.h:261-263），每槽按 RT_ROUNDUP(sa_len) 推进。
    let mut off = HEADER_LEN;
    let mut mask_slot: Option<(usize, usize)> = None;
    for slot in 0..9 {
        if frame.addrs & (1 << slot) == 0 {
            continue;
        }
        // 槽位至少要放得下 sockaddr 的头两字节（C rtsock.c:572-575）。
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
        // 位不在场 = 主机条目；前缀在表更新时按族取满。
        None => None,
    };
    Ok(frame)
}

/// 在 `out` 的 `base` 偏移写一个完整形态 sockaddr（头两字节 +
/// 本体位；掩码槽把前缀掩码放到本体位），槽内余量清零。
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

/// 编码一条路由条目的 RTM_GET 帧（read 导出面）：头 + DST + GATEWAY
/// （在场时）+ NETMASK，全用完整形态 sockaddr（掩码不压缩——C 的
/// 导出面发压缩形态，完整形态是同一语义的未压缩表达，登记差异）。
/// 返回写入 `out` 的字节数；`out` 放不下整帧回 `None`。
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
    // 头：类型 GET、版本 4、接口行号、旗标、地址位、回执的 pid/seq。
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

/// 族对应的地址字节数（条目半）。
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

    /// 造一条 ADD 帧的原始字节（完整形态 sockaddr：DST+GATEWAY+
    /// NETMASK）。
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
        // DST 10.1.0.0。
        buf[HEADER_LEN] = 16;
        buf[HEADER_LEN + 1] = 2;
        buf[HEADER_LEN + 4..HEADER_LEN + 8].copy_from_slice(&[10, 1, 0, 0]);
        // GATEWAY 10.0.0.1。
        let gw = HEADER_LEN + 16;
        buf[gw] = 16;
        buf[gw + 1] = 2;
        buf[gw + 4..gw + 8].copy_from_slice(&[10, 0, 0, 1]);
        // NETMASK /16（完整形态）。
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

        // 检查序①：msglen 与缓冲长度不符 → EINVAL。
        let mut short = buf.clone();
        short[0..2].copy_from_slice(&99u16.to_le_bytes());
        assert_eq!(parse_frame(&short).unwrap_err(), minix_types::EINVAL);
        // 检查序②：版本不是 4 → EPROTONOSUPPORT。
        let mut alien = buf.clone();
        alien[2] = 5;
        assert_eq!(
            parse_frame(&alien).unwrap_err(),
            minix_types::EPROTONOSUPPORT
        );
        // 过短缓冲：EINVAL。
        assert_eq!(parse_frame(&buf[..40]).unwrap_err(), minix_types::EINVAL);
    }

    #[test]
    fn test_compressed_netmask_and_no_mask_host_route() {
        // 压缩形态：只有头两字节加掩码位（/24 → 3 个 0xFF）。
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
        buf[nm] = 5; // 2 字节头 + 3 掩码字节
        buf[nm + 1] = 0; // 压缩形态族字节可为 0
        buf[nm + 2..nm + 5].copy_from_slice(&[255, 255, 255]);
        let frame = parse_frame(&buf).expect("压缩掩码解析");
        assert_eq!(frame.prefix, Some(24));

        // 非连续掩码拒绝。
        buf[nm + 2..nm + 5].copy_from_slice(&[255, 0, 255]);
        assert_eq!(parse_frame(&buf).unwrap_err(), minix_types::EINVAL);

        // 掩码位不在场 = 主机条目（prefix None，表更新按族取满）。
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

        // 主机条目带主机旗标；缓冲放不下回 None。
        let host = crate::route::RouteEntry { prefix: 32, ..entry };
        let n = encode_entry_frame(&host, 0, 0, &mut out).expect("主机帧");
        assert_eq!(parse_frame(&out[..n]).unwrap().flags & RTF_HOST, RTF_HOST);
        let mut tiny = [0u8; 100];
        assert!(encode_entry_frame(&entry, 0, 0, &mut tiny).is_none());
    }
}
