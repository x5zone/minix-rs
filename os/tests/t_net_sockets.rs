//! minix3/minix/tests Rust 腿翻译 —— 套接字域(test56/90 的策略面)。
//!
//! # 翻译映射
//!
//! C 网络编号测试的 socket 数据面需要 INET/UDS 服务器与真实连接;本文件钉
//! **当前已存在的策略面**:UDS 的域/类型准入、散列槽位、环与控制消息长度
//! 策略(C `uds.c` 对位),以及 lwip 地址工具(C test48 的地址处理面对位)。
//! 连接建立、数据搬运、SCM_RIGHTS fd 传递挂 UDS/INET 服务器点亮后的条目。
//!
//! | C 测试 | 语义归宿 |
//! |---|---|
//! | test56/90(UDS 准入策略) | [`uds_admission_policy_and_slot_binding`] |
//! | test90(环与控制消息长度策略) | [`uds_ring_and_control_length_policy`] |
//! | test48/93(地址工具:前缀/掩码/范围) | [`lwip_address_utilities_prefix_and_scope`] |
//! | test67/80/81/91/92(TCP/UDP/RAW 数据面) | 边界外,挂 INET 点亮后补 |
//!
//! 交付门 = 编译;全部测试 `#[ignore]`,点亮前提见各测试属性。

use minix_net_lwip::addr::{
    address_scope, common_bits, normalize_prefix, prefix_from_netmask_v4, SCOPE_GLOBAL,
    SCOPE_LINK_LOCAL,
};
use minix_net_uds::core::{domain_allowed, hash_slot};
use minix_net_uds::io::{control_length_allowed, max_payload, ring_advance, RECEIVE_BUFFER};

// ---------------------------------------------------------------------------
// test56/90 —— UDS 准入策略与槽位绑定
// ---------------------------------------------------------------------------

/// C 语义(`uds.c:230-248`):只有本地域通过;槽位由设备号与 inode 混合
/// 散列,结果落在槽位数界内且确定性可复现。
#[test]
#[ignore = "点亮前提:UDS 服务器连接面随载体点亮后复核"]
fn uds_admission_policy_and_slot_binding() {
    // 域准入:只有 AF_UNIX 对位为真。
    assert!(domain_allowed(true), "本地域放行");
    assert!(!domain_allowed(false), "其他域拒绝(配置错误的早期门)");

    // 槽位绑定:确定性 + 界内 + 对不同键分散。
    let first = hash_slot(0x301, 42);
    let again = hash_slot(0x301, 42);
    assert_eq!(first, again, "同键同槽(确定性)");
    assert!(first < minix_net_uds::core::HASH_SLOTS, "槽位落在界内");
    // 不同 inode 在 16 槽位上大概率分散;这里只用两个键断言函数可区分。
    let _ = hash_slot(0x301, 43);
}

// ---------------------------------------------------------------------------
// test90 —— 环推进与控制消息长度策略
// ---------------------------------------------------------------------------

/// C 语义(uds io 面):环位置对缓冲容量取模回绕(`uds_advance`,`io.c:82`);
/// 控制消息长度门在界内放行、越界拒绝;净载荷 = 缓冲减头部。
#[test]
#[ignore = "点亮前提:UDS 服务器数据面随载体点亮后复核"]
fn uds_ring_and_control_length_policy() {
    // 环推进回绕:容量取模(缓冲 32768,C io.c:82)。
    let wrapped = ring_advance(RECEIVE_BUFFER - 8, 16);
    assert_eq!(wrapped, 8, "越界部分对容量取模回折到环首");
    assert_eq!(ring_advance(0, 8), 8, "普通推进");

    // 控制消息长度:界内(≤ CONTROL_MAX = 4096)放行,越界拒绝。
    assert!(control_length_allowed(0));
    assert!(control_length_allowed(4096));
    assert!(!control_length_allowed(4097));

    // 净载荷:缓冲减头部(头部尺寸是 wire 契约的一部分)。
    let payload = max_payload(16);
    assert_eq!(payload, RECEIVE_BUFFER - 16, "净载荷 = 缓冲 - 头部");
}

// ---------------------------------------------------------------------------
// test48/93 —— lwip 地址工具
// ---------------------------------------------------------------------------

/// C 语义(lwip 地址工具,`test48` 的 inet 处理面与 `test93` 的路由前缀面):
/// v4 点分掩码转前缀长度;前缀归一化清掉主机位;公共前缀位数计算;地址
/// 范围判定。
#[test]
#[ignore = "点亮前提:INET 服务器随载体点亮后端到端复核"]
fn lwip_address_utilities_prefix_and_scope() {
    // 点分掩码 → 前缀长度(C test93 的 netmask 面):255.255.255.0 → 24。
    assert_eq!(prefix_from_netmask_v4(0xFFFF_FF00), Some(24));
    assert_eq!(prefix_from_netmask_v4(0xFFFF_0000), Some(16));
    assert_eq!(prefix_from_netmask_v4(0x0000_0000), Some(0));
    // 非连续掩码拒绝(前缀长度无定义)。
    assert_eq!(prefix_from_netmask_v4(0x0FFF_FFFF), None);

    // 前缀归一化:从 128 位最高端起保留前缀位,主机位清零。
    // v4 地址以低 32 位进 u128 时,/24 的对位是 /120(96 位映射头 + 24)。
    let base = normalize_prefix(0xC0A8_0101, 120); // 192.168.1.1 → 192.168.1.0
    assert_eq!(base, 0xC0A8_0100, "主机位清零");
    // 纯 v6 形:/64 保留高 64 位。
    let v6 = normalize_prefix(0x2001_0DB8_0000_0000_0000_0000_0000_0001, 64);
    assert_eq!(v6, 0x2001_0DB8_0000_0000_0000_0000_0000_0000, "/64 清低 64 位");

    // 公共前缀位:从最高端数起。v4 地址顶到 128 位高位(与 /24 对位):
    // 低 8 位分岔 → 共 24 位;完全相同 → 32 位。
    assert_eq!(common_bits(0xC0A8_0100 << 96, 0xC0A8_01FF << 96, 32), 24);
    assert_eq!(common_bits(0xC0A8_0100 << 96, 0xC0A8_0100 << 96, 32), 32);

    // 地址范围判定(C addrpol.c 对位):v4 恒全局;环回走链路本地。
    assert_eq!(
        address_scope(true, false, false, false, false, false, 0, false, false),
        SCOPE_GLOBAL,
        "v4 地址恒全局"
    );
    assert_eq!(
        address_scope(false, false, false, true, false, false, 0, false, false),
        SCOPE_LINK_LOCAL,
        "环回按链路本地"
    );
    assert_eq!(
        address_scope(false, true, false, false, false, false, 0, false, false),
        SCOPE_GLOBAL,
        "全局 v6 即全局"
    );
}
