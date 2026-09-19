//! Routing table policy: prefix assumption, lookup order, miss handling.
//!
//! C correspondence: `minix3/minix/net/lwip/rttree.c` (744 lines) with the
//! route management in `minix3/minix/net/lwip/route.c` (1654 lines) and the
//! gateway hooks in the lightweight stack glue. Table storage, message
//! traffic, and stack overrides stay in the service binary. This module owns
//! the portion that can be decided from numbers alone: which prefix lengths
//! are valid, how bit positions map to bytes, and which lookup outcome
//! applies.
//!
//! The tree assumes every mask can be expressed as a prefix length: some
//! leading number of set bits followed by all clear bits. Entries live at
//! the node matching their bit count, and that node may still have children
//! that refine the prefix. There are no pure leaf-or-internal nodes, only
//! data nodes (with an entry, zero to two children) and link nodes (without
//! an entry, exactly two children).

use alloc::vec::Vec;

/// Largest version 4 prefix length (32 address bits).
pub const VERSION4_BITS: u8 = 32;

/// Largest version 6 prefix length (128 address bits).
pub const VERSION6_BITS: u8 = 128;

/// Byte index holding a bit position (`RTTREE_BITS_TO_BYTE`, `rttree.c:38`).
pub fn bit_to_byte(bit: u32) -> usize {
    (bit >> 3) as usize
}

/// Shift selecting a bit inside its byte (`RTTREE_BITS_TO_SHIFT`,
/// `rttree.c:39`: bit 0 is the most significant bit of byte 0).
pub fn bit_to_shift(bit: u32) -> u32 {
    7 - (bit & 7)
}

/// Bytes needed to hold a bit count (`RTTREE_BITS_TO_BYTES`, `rttree.c:40`).
pub fn bits_to_bytes(bits: u32) -> usize {
    ((bits + 7) >> 3) as usize
}

/// Whether a version 4 prefix length is valid.
pub fn version4_prefix_allowed(prefix: u8) -> bool {
    prefix <= VERSION4_BITS
}

/// Whether a version 6 prefix length is valid.
pub fn version6_prefix_allowed(prefix: u8) -> bool {
    prefix <= VERSION6_BITS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bit_mapping_matches_tree_macros() {
        assert_eq!(bit_to_byte(0), 0);
        assert_eq!(bit_to_byte(8), 1);
        assert_eq!(bit_to_shift(0), 7);
        assert_eq!(bit_to_shift(7), 0);
        assert_eq!(bit_to_shift(8), 7);
        assert_eq!(bits_to_bytes(0), 0);
        assert_eq!(bits_to_bytes(1), 1);
        assert_eq!(bits_to_bytes(8), 1);
        assert_eq!(bits_to_bytes(9), 2);
    }

    #[test]
    fn test_prefix_bounds_match_address_sizes() {
        assert_eq!(VERSION4_BITS, 32);
        assert_eq!(VERSION6_BITS, 128);
        assert!(version4_prefix_allowed(32));
        assert!(!version4_prefix_allowed(33));
        assert!(version6_prefix_allowed(128));
    }
}


// ---------------------------------------------------------------------------
// 路由表（C `rttree.c` 的存储半 + `route.c:371-738` 的增删查；19 篇 §2.5
// 登记的"判断与存储分离"在此收敛：树实现换扁平最长匹配，外部契约一致
// ——前缀合法性、最长匹配获胜、默认路由即前缀 0 条目）。
// ---------------------------------------------------------------------------

/// 地址族（路由条目按族分行；地址按族取 4 或 16 字节定长表示）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpVersion {
    /// 版本 4：地址取前 4 字节。
    V4,
    /// 版本 6：地址取全 16 字节。
    V6,
}

/// 一条路由条目：网络（按前缀规范化）、下一跳（`None` 即直连）、
/// 出接口行号。默认网关与默认路由的配合见 19 篇 §1.3（前缀 0 的条目
/// 配一个下一跳地址实现全网可达）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteEntry {
    /// 地址族。
    pub version: IpVersion,
    /// 目的网络（加入时按前缀规范化，主机位清零）。
    pub dest: [u8; 16],
    /// 前缀长度（0 = 默认路由）。
    pub prefix: u8,
    /// 下一跳；`None` = 直连（目的地即本地链路）。
    pub gateway: Option<[u8; 16]>,
    /// 出接口行号（服务侧接口表，第 14 篇）。
    pub ifdev: u16,
}

/// 路由表：扁平条目组，查找按最长前缀获胜。C 的 `rttree` 用前缀树是
/// 存储策略，对外可观察的语义只有"增、删、最长匹配查"，本表以同语义
/// 的扁平实现承载（19 篇 §2.5 已登记该差异）。
#[derive(Debug, Default)]
pub struct RouteTable {
    entries: Vec<RouteEntry>,
}

/// 按族与前缀长度造掩码（大端位序：位 0 是第 0 字节最高位，19 篇 §1.2）。
fn prefix_mask(version: IpVersion, prefix: u8) -> [u8; 16] {
    let mut mask = [0u8; 16];
    let bits = match version {
        IpVersion::V4 => prefix.min(32) as usize,
        IpVersion::V6 => prefix.min(128) as usize,
    };
    let whole = bits / 8;
    for byte in mask.iter_mut().take(whole) {
        *byte = 0xff;
    }
    if whole < 16 && bits % 8 != 0 {
        mask[whole] = 0xffu8 << (8 - bits % 8);
    }
    mask
}

fn masked(address: &[u8], mask: &[u8; 16]) -> [u8; 16] {
    let mut out = [0u8; 16];
    for (o, (a, m)) in out.iter_mut().zip(address.iter().zip(mask.iter())) {
        *o = a & m;
    }
    out
}

impl RouteTable {
    /// `route_init`（`route.c:248`）：空表起步。
    pub fn new() -> Self {
        RouteTable { entries: Vec::new() }
    }

    /// 在册条数。
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 增或改：同键（族、网络、前缀）条目被替换（C 的增删改查里的
    /// "改"半）。前缀按族校验（32/128，`route.rs` 既有合法性函数），
    /// 目的地址按前缀规范化（主机位清零，树假设的前提）。
    pub fn add(&mut self, mut entry: RouteEntry) -> Result<(), i32> {
        let allowed = match entry.version {
            IpVersion::V4 => version4_prefix_allowed(entry.prefix),
            IpVersion::V6 => version6_prefix_allowed(entry.prefix),
        };
        if !allowed {
            return Err(minix_types::EINVAL);
        }
        let mask = prefix_mask(entry.version, entry.prefix);
        entry.dest = masked(&entry.dest, &mask);
        self.entries.retain(|e| {
            e.version != entry.version || e.dest != entry.dest || e.prefix != entry.prefix
        });
        self.entries.push(entry);
        Ok(())
    }

    /// 删：同键精确匹配，返回是否确实在册。
    pub fn remove(&mut self, version: IpVersion, dest: [u8; 16], prefix: u8) -> bool {
        let mask = prefix_mask(version, prefix);
        let dest = masked(&dest, &mask);
        let before = self.entries.len();
        self.entries
            .retain(|e| !(e.version == version && e.dest == dest && e.prefix == prefix));
        self.entries.len() != before
    }

    /// 查：最长前缀获胜；无匹配（含无默认路由）返回 `None`——C 的
    /// 选路覆盖返回"不可达"由调用方折错。
    pub fn lookup(&self, version: IpVersion, address: &[u8]) -> Option<&RouteEntry> {
        let mut best: Option<&RouteEntry> = None;
        for entry in self.entries.iter().filter(|e| e.version == version) {
            let mask = prefix_mask(entry.version, entry.prefix);
            if masked(address, &mask) == entry.dest {
                let better = best
                    .map(|b| entry.prefix > b.prefix)
                    .unwrap_or(true);
                if better {
                    best = Some(entry);
                }
            }
        }
        best
    }
}

#[cfg(test)]
mod table_tests {
    use super::*;

    fn v4_entry(dest: [u8; 4], prefix: u8, gateway: Option<[u8; 4]>) -> RouteEntry {
        let mut d = [0u8; 16];
        d[..4].copy_from_slice(&dest);
        let g = gateway.map(|g| {
            let mut t = [0u8; 16];
            t[..4].copy_from_slice(&g);
            t
        });
        RouteEntry { version: IpVersion::V4, dest: d, prefix, gateway: g, ifdev: 0 }
    }

    fn v4_addr(dest: [u8; 4]) -> [u8; 16] {
        let mut d = [0u8; 16];
        d[..4].copy_from_slice(&dest);
        d
    }

    #[test]
    fn test_longest_prefix_wins() {
        let mut table = RouteTable::new();
        table.add(v4_entry([0, 0, 0, 0], 0, Some([10, 0, 0, 1]))).unwrap();
        table.add(v4_entry([192, 168, 0, 0], 16, None)).unwrap();
        let hit = table.lookup(IpVersion::V4, &[192, 168, 1, 5]).unwrap();
        assert_eq!(hit.prefix, 16, "更长前缀获胜");
        assert_eq!(hit.gateway, None, "直连条目无下一跳");
        // 落在 16 位前缀之外的地址走默认路由。
        let miss = table.lookup(IpVersion::V4, &[8, 8, 8, 8]).unwrap();
        assert_eq!(miss.prefix, 0);
    }

    #[test]
    fn test_dest_normalized_on_add() {
        let mut table = RouteTable::new();
        // 主机位非零：按前缀规范化后与规范化形式同键（改语义覆盖）。
        table.add(v4_entry([192, 168, 1, 1], 16, None)).unwrap();
        assert_eq!(table.len(), 1);
        table.add(v4_entry([192, 168, 9, 9], 16, Some([10, 0, 0, 1]))).unwrap();
        assert_eq!(table.len(), 1, "同键条目被替换（改半）");
        let hit = table.lookup(IpVersion::V4, &[192, 168, 7, 7]).unwrap();
        assert_eq!(hit.gateway.map(|g| g[..4].try_into().unwrap()), Some([10, 0, 0, 1]), "替换后的下一跳生效");
    }

    #[test]
    fn test_prefix_validation_and_remove() {
        let mut table = RouteTable::new();
        assert_eq!(
            table.add(v4_entry([10, 0, 0, 0], 33, None)).unwrap_err(),
            minix_types::EINVAL,
            "版本 4 前缀超 32 拒绝"
        );
        table.add(v4_entry([10, 0, 0, 0], 8, None)).unwrap();
        assert!(table.remove(IpVersion::V4, v4_addr([10, 0, 0, 0]), 8));
        assert!(!table.remove(IpVersion::V4, v4_addr([10, 0, 0, 0]), 8), "双删返回假");
        assert!(table.lookup(IpVersion::V4, &[10, 1, 2, 3]).is_none(), "删后无匹配");
    }

    #[test]
    fn test_v6_family_separate() {
        let mut table = RouteTable::new();
        let mut v6 = RouteEntry {
            version: IpVersion::V6,
            dest: [0u8; 16],
            prefix: 128,
            gateway: None,
            ifdev: 1,
        };
        // 版本 6 前缀超 128 拒绝。
        v6.prefix = 129;
        assert_eq!(table.add(v6).unwrap_err(), minix_types::EINVAL);
        v6.prefix = 128;
        table.add(v6).unwrap();
        // 族隔离：版本 4 的查询不命中版本 6 条目。
        assert!(table.lookup(IpVersion::V4, &[0u8; 4]).is_none());
    }
}

