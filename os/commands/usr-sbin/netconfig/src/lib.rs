#![cfg_attr(not(test), no_std)]

//! Network configuration and diagnostics core for Minix-RS commands.
//!
//! Covers `notes/rewrite/fork-syscall-rewrite/18-stage-commands/18-network-config.md`:
//! interface configuration (`minix3/sbin/ifconfig/ifconfig.c`, interface flags
//! read with `SIOCGIFFLAGS` near line 1050 and written with `SIOCSIFFLAGS`
//! near line 1059, maximum transmission unit written with `SIOCSIFMTU` near
//! line 1173), routing (`minix3/sbin/route/route.c`, the routing socket verbs
//! `RTM_ADD`/`RTM_DELETE`/`RTM_GET` and the version stamp `RTM_VERSION` near
//! line 642), address resolution (`minix3/usr.sbin/arp/arp.c`, entry lookup
//! with `RTM_GET` near line 315, entry creation with `RTM_ADD` near line 350,
//! entry removal with `RTM_DELETE` near line 434), echo probing
//! (`minix3/sbin/ping/ping.c`, the checksum function `in_cksum` at line 1266,
//! request construction with `ICMP_ECHO` near line 897, reply matching with
//! `ICMP_ECHOREPLY` near line 1029), path tracing
//! (`minix3/usr.sbin/traceroute/traceroute.c`, default probes use
//! `IPPROTO_UDP` near line 721, hop limit raised with `IP_TTL` near line 1350,
//! default hop limit read from `IPCTL_DEFTTL` near line 472), socket state
//! display (`minix3/usr.bin/netstat/main.c`, usage near line 866), and the
//! host, service, and protocol databases (`minix3/etc/hosts`,
//! `minix3/etc/services`, `minix3/etc/protocols`).
//!
//! # Design
//!
//! Configuration commands translate words into kernel requests; diagnosis
//! commands translate replies into human lines. What is pure here lives in
//! this crate, what opens sockets, issues input and output control calls, or
//! reads the routing socket stays with the execution layer:
//!
//! - [`iface`]: interface flag words (`up`, `down`), maximum transmission
//!   unit parsing, and the interface request record.
//! - [`route`]: routing verbs (add, delete, change, get, flush, monitor) and
//!   the route entry record (destination, gateway, outgoing interface).
//! - [`arp`]: neighbor table verbs (add, delete, get, set) and the neighbor
//!   entry record (network address, link address, interface, expiry).
//! - [`ping`]: Internet Control Message Protocol echo arithmetic (the
//!   ones-complement checksum from `ping.c:1266`, identifier and sequence
//!   matching, round trip timing) and probe option parsing.
//! - [`hosts`]: host, service, and protocol database lookup over the
//!   [`hosts::AddressDb`] trait (a slice backend plus an empty backend so
//!   tests run without files).
//!
//! Everything borrows from the input and uses fixed size buffers: no heap,
//! `no_std` throughout. Socket calls, control calls, and file reads stay with
//! the execution layer behind [`hosts::AddressDb`] and [`route::RouteTable`].

pub mod arp;
pub mod hosts;
pub mod iface;
pub mod ping;
pub mod route;

/// Errors produced by this crate, mapped to classic Unix error numbers.
///
/// 22 marks malformed input (`EINVAL`): unknown verbs, bad addresses, bad
/// counts. 3 marks a missing entry (`ESRCH`, the same number the routing
/// commands report when a route or neighbor entry is absent). 51 marks an
/// unreachable network (`ENETUNREACH`, the same number echo probing reports
/// when no reply path exists).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetconfigError {
    /// Malformed input.
    InvalidArgument,
    /// No such route, neighbor, or database entry.
    NotFound,
    /// Network unreachable.
    Unreachable,
}

impl NetconfigError {
    /// The classic Unix error number for this failure.
    pub fn as_errno(self) -> i32 {
        match self {
            NetconfigError::InvalidArgument => 22,
            NetconfigError::NotFound => 3,
            NetconfigError::Unreachable => 51,
        }
    }
}
