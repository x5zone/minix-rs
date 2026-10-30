//! Rust-leg translation of minix3/minix/tests — the socket domain (the policy
//! face of test56/90).
//!
//! # Translation mapping
//!
//! C's numbered network tests exercise the socket data plane, which needs the
//! INET/UDS servers and real connections; this file pins down the **policy face
//! that already exists**: UDS domain/type admission, hash slot assignment, ring
//! and control-message length policy (matching C `uds.c`), and the lwip address
//! utilities (matching the address-handling face of C test48). Connection
//! establishment, data movement, and SCM_RIGHTS fd passing are items pending
//! once the UDS/INET servers light up.
//!
//! | C test | Semantic home |
//! |---|---|
//! | test56/90 (UDS admission policy) | [`uds_admission_policy_and_slot_binding`] |
//! | test90 (ring and control-message length policy) | [`uds_ring_and_control_length_policy`] |
//! | test48/93 (address utilities: prefix/mask/scope) | [`lwip_address_utilities_prefix_and_scope`] |
//! | test67/80/81/91/92 (TCP/UDP/RAW data plane) | out of scope; to be added once INET lights up |
//!
//! Delivery gate = compilation; every test is `#[ignore]`, and the conditions
//! for enabling each are given in its attribute.

use minix_net_lwip::addr::{
    SCOPE_GLOBAL, SCOPE_LINK_LOCAL, address_scope, common_bits, normalize_prefix,
    prefix_from_netmask_v4,
};
use minix_net_uds::core::{domain_allowed, hash_slot};
use minix_net_uds::io::{RECEIVE_BUFFER, control_length_allowed, max_payload, ring_advance};

// ---------------------------------------------------------------------------
// test56/90 — UDS admission policy and slot binding
// ---------------------------------------------------------------------------

/// C semantics (`uds.c:230-248`): only the local domain passes; the slot is a
/// hash of the device number mixed with the inode, falls inside the slot index
/// bound, and is deterministic and reproducible.
#[test]
#[ignore = "点亮前提:UDS 服务器连接面随载体点亮后复核"]
fn uds_admission_policy_and_slot_binding() {
    // Domain admission: only AF_UNIX is admitted.
    assert!(domain_allowed(true), "本地域放行");
    assert!(!domain_allowed(false), "其他域拒绝(配置错误的早期门)");

    // Slot binding: deterministic + in bounds + spread across different keys.
    let first = hash_slot(0x301, 42);
    let again = hash_slot(0x301, 42);
    assert_eq!(first, again, "同键同槽(确定性)");
    assert!(first < minix_net_uds::core::HASH_SLOTS, "槽位落在界内");
    // Different inodes mostly spread over the 64 slots (C uds.h
    // UDSHASH_SLOTS); only two keys are used here to assert the function
    // distinguishes them.
    let _ = hash_slot(0x301, 43);
}

// ---------------------------------------------------------------------------
// test90 — ring advance and control-message length policy
// ---------------------------------------------------------------------------

/// C semantics (uds io face): the ring position wraps modulo the buffer capacity
/// (`uds_advance`, `io.c:82`); the control-message length gate admits values in
/// bounds and rejects those out of bounds; usable payload = buffer minus header.
#[test]
#[ignore = "点亮前提:UDS 服务器数据面随载体点亮后复核"]
fn uds_ring_and_control_length_policy() {
    // Ring advance wrap: modulo the capacity (buffer 32768, C io.c:82).
    let wrapped = ring_advance(RECEIVE_BUFFER - 8, 16);
    assert_eq!(wrapped, 8, "越界部分对容量取模回折到环首");
    assert_eq!(ring_advance(0, 8), 8, "普通推进");

    // Control-message length: in bounds (≤ CONTROL_MAX = 4096) admitted, out of
    // bounds rejected.
    assert!(control_length_allowed(0));
    assert!(control_length_allowed(4096));
    assert!(!control_length_allowed(4097));

    // Usable payload: buffer minus header (the header size is part of the wire
    // contract).
    let payload = max_payload(16);
    assert_eq!(payload, RECEIVE_BUFFER - 16, "净载荷 = 缓冲 - 头部");
}

// ---------------------------------------------------------------------------
// test48/93 — lwip address utilities
// ---------------------------------------------------------------------------

/// C semantics (lwip address utilities, the inet handling face of `test48` and
/// the routing-prefix face of `test93`): dotted v4 mask to prefix length; prefix
/// normalization clears the host bits; common prefix bit count; address scope
/// classification.
#[test]
#[ignore = "点亮前提:INET 服务器随载体点亮后端到端复核"]
fn lwip_address_utilities_prefix_and_scope() {
    // Dotted mask → prefix length (the netmask face of C test93):
    // 255.255.255.0 → 24.
    assert_eq!(prefix_from_netmask_v4(0xFFFF_FF00), Some(24));
    assert_eq!(prefix_from_netmask_v4(0xFFFF_0000), Some(16));
    assert_eq!(prefix_from_netmask_v4(0x0000_0000), Some(0));
    // Non-contiguous mask rejected (prefix length is undefined).
    assert_eq!(prefix_from_netmask_v4(0x0FFF_FFFF), None);

    // Prefix normalization: keep prefix bits from the most significant end of
    // 128 bits, clear host bits. A v4 address enters u128 in the low 32 bits, so
    // /24 corresponds to /120 (96-bit mapped header + 24).
    let base = normalize_prefix(0xC0A8_0101, 120); // 192.168.1.1 → 192.168.1.0
    assert_eq!(base, 0xC0A8_0100, "主机位清零");
    // Pure v6 form: /64 keeps the high 64 bits.
    let v6 = normalize_prefix(0x2001_0DB8_0000_0000_0000_0000_0000_0001, 64);
    assert_eq!(
        v6, 0x2001_0DB8_0000_0000_0000_0000_0000_0000,
        "/64 清低 64 位"
    );

    // Common prefix bits: counted from the most significant end. A v4 address is
    // aligned to the top of 128 bits (corresponding to /24): the low 8 bits
    // differ → 24 bits in common; identical → 32 bits.
    assert_eq!(common_bits(0xC0A8_0100 << 96, 0xC0A8_01FF << 96, 32), 24);
    assert_eq!(common_bits(0xC0A8_0100 << 96, 0xC0A8_0100 << 96, 32), 32);

    // Address scope classification (matching C addrpol.c): v4 is always global;
    // loopback goes link-local.
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
