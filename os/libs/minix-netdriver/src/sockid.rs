//! Socket identifier namespace: class bases, index field, safe decode.
//!
//! C correspondence: `sockid_t` is `int32_t` and doubles as the negative
//! error channel on call returns (`minix3/minix/include/minix/sockdriver.h:27-28`);
//! the five class bases live in `minix3/minix/net/lwip/lwip.h:58-62`
//! (`SOCKID_TCP 0x0`, `SOCKID_UDP 0x00100000`, `SOCKID_RAW 0x00200000`,
//! `SOCKID_RT 0x00400000`, `SOCKID_LNK 0x00800000`); an id is minted as the
//! class base OR the owner's array slot (`tcpsock.c:140`,
//! `udpsock.c:155`, `rawsock.c:343`, `rtsock.c:344`, `lnksock.c:60`). The
//! bases step by `0x00100000`, which fixes the index field at twenty bits.
//! The UNIX-domain server mints bare indices with no class base at all
//! (`uds.c:97-101`), so numerically small ids sit inside the TCP range:
//! interpretation belongs to the driver that issued the id, and the virtual
//! file system treats ids as opaque.
//!
//! This module owns the naming half of that namespace, next to the event
//! hash in [`crate::sockevent`] that consumes raw ids. Message packing
//! stays in the service binary.

/// Width of the per-class index field: class bases step by `0x00100000`
/// (`1 << 20`), so an index must fit in twenty bits.
pub const INDEX_BITS: u32 = 20;

/// Mask isolating the index field of a raw identifier.
pub const INDEX_MASK: u32 = (1 << INDEX_BITS) - 1;

/// Class base for transmission-control sockets (`SOCKID_TCP`).
pub const SOCKID_TCP: i32 = 0x00000000;
/// Class base for datagram sockets (`SOCKID_UDP`).
pub const SOCKID_UDP: i32 = 0x00100000;
/// Class base for raw IP sockets (`SOCKID_RAW`).
pub const SOCKID_RAW: i32 = 0x00200000;
/// Class base for route sockets (`SOCKID_RT`).
pub const SOCKID_RT: i32 = 0x00400000;
/// Class base for link-layer sockets (`SOCKID_LNK`).
pub const SOCKID_LNK: i32 = 0x00800000;

/// One socket class: a class base paired with its name.
///
/// The enum exists so constructors take a class instead of a raw base
/// number; `from_base` decodes in the other direction for callers that
/// received a raw id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SockClass {
    /// Transmission-control sockets (`SOCKID_TCP`).
    Tcp,
    /// Datagram sockets (`SOCKID_UDP`).
    Udp,
    /// Raw IP sockets (`SOCKID_RAW`).
    Raw,
    /// Route sockets (`SOCKID_RT`).
    Rt,
    /// Link-layer sockets (`SOCKID_LNK`).
    Lnk,
}

impl SockClass {
    /// The class base value ORed into identifiers (`lwip.h:58-62`).
    pub const fn base(self) -> i32 {
        match self {
            SockClass::Tcp => SOCKID_TCP,
            SockClass::Udp => SOCKID_UDP,
            SockClass::Raw => SOCKID_RAW,
            SockClass::Rt => SOCKID_RT,
            SockClass::Lnk => SOCKID_LNK,
        }
    }

    /// Decode a raw class base into a class; `None` for values outside
    /// the five bases.
    pub const fn from_base(base: i32) -> Option<SockClass> {
        match base {
            SOCKID_TCP => Some(SockClass::Tcp),
            SOCKID_UDP => Some(SockClass::Udp),
            SOCKID_RAW => Some(SockClass::Raw),
            SOCKID_RT => Some(SockClass::Rt),
            SOCKID_LNK => Some(SockClass::Lnk),
            _ => None,
        }
    }
}

/// A socket identifier: the wire form is a non-negative `int32_t`
/// (`sockid_t`, `sockdriver.h:28`).
///
/// Negative values are the error channel, not identifiers — the
/// constructors here reject them so a call site cannot confuse the two.
/// The type deliberately wraps the raw number instead of enumerating
/// classes: a UNIX-domain bare index and a TCP-class id share the same
/// numeric range by C design, and only the issuing driver knows which it
/// minted, so an enum here would encode a falsehood.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SockId(i32);

impl SockId {
    /// Mint an identifier from a class and an owner-array index
    /// (`SOCKID_TCP | slot` shape, `tcpsock.c:140`). `None` when the
    /// index overflows the twenty-bit field and would bleed into class
    /// bits.
    pub const fn from_class(class: SockClass, index: u32) -> Option<SockId> {
        if index > INDEX_MASK {
            return None;
        }
        Some(SockId(class.base() | index as i32))
    }

    /// Mint a bare identifier with no class base (`uds.c:97-101` shape).
    /// `None` when the index overflows the twenty-bit field or reaches
    /// into negative territory reserved for the error channel.
    pub const fn bare(index: u32) -> Option<SockId> {
        if index > INDEX_MASK {
            return None;
        }
        Some(SockId(index as i32))
    }

    /// Adopt a raw identifier from the wire; `None` for negative values,
    /// which are error codes on call returns (`sockdriver.h:27`).
    pub const fn from_raw(raw: i32) -> Option<SockId> {
        if raw < 0 {
            return None;
        }
        Some(SockId(raw))
    }

    /// The wire form.
    pub const fn raw(self) -> i32 {
        self.0
    }

    /// The class bits of the identifier. Bare UNIX-domain ids decode as
    /// the TCP base because they share its numeric range; the issuing
    /// driver's meaning wins over this arithmetic decode.
    pub const fn class_base(self) -> i32 {
        self.0 & !INDEX_MASK as i32
    }

    /// The index field of the identifier.
    pub const fn index(self) -> u32 {
        (self.0 as u32) & INDEX_MASK
    }

    /// The class if the class bits name one of the five bases; bare ids
    /// below the first base decode as [`SockClass::Tcp`], per the
    /// arithmetic above.
    pub const fn class(self) -> Option<SockClass> {
        SockClass::from_base(self.class_base())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sockevent;

    #[test]
    fn test_class_bases_match_lwip_header() {
        assert_eq!(SOCKID_TCP, 0x00000000);
        assert_eq!(SOCKID_UDP, 0x00100000);
        assert_eq!(SOCKID_RAW, 0x00200000);
        assert_eq!(SOCKID_RT, 0x00400000);
        assert_eq!(SOCKID_LNK, 0x00800000);
        assert_eq!(INDEX_BITS, 20);
        assert_eq!(INDEX_MASK, 0xFFFFF);
    }

    #[test]
    fn test_ids_are_base_or_slot_like_the_c_allocators() {
        let tcp = SockId::from_class(SockClass::Tcp, 5).expect("tcp id");
        assert_eq!(tcp.raw(), 5);
        assert_eq!(tcp.class(), Some(SockClass::Tcp));
        assert_eq!(tcp.index(), 5);

        let udp = SockId::from_class(SockClass::Udp, 3).expect("udp id");
        assert_eq!(udp.raw(), 0x00100003);
        assert_eq!(udp.raw(), SOCKID_UDP | 3, "与 C 的 OR 分配同形");

        let rt = SockId::from_class(SockClass::Rt, 7).expect("rt id");
        assert_eq!(rt.class_base(), SOCKID_RT);
        assert_eq!(SockClass::from_base(rt.class_base()), Some(SockClass::Rt));
    }

    #[test]
    fn test_index_overflow_and_negatives_are_refused() {
        assert!(SockId::from_class(SockClass::Lnk, 1 << INDEX_BITS).is_none());
        assert!(SockId::bare(1 << INDEX_BITS).is_none());
        assert!(SockId::from_raw(-1).is_none(), "负数是错误通道不是 id");
        assert!(SockId::from_raw(i32::MIN).is_none());
    }

    #[test]
    fn test_wire_round_trip_preserves_the_number() {
        let id = SockId::from_class(SockClass::Raw, 0xF_EF01).expect("raw id");
        assert_eq!(SockId::from_raw(id.raw()), Some(id));
        assert_eq!(id.raw() as u32 & INDEX_MASK, 0xF_EF01);
    }

    #[test]
    fn test_ids_hash_through_the_sockevent_slots() {
        let id = SockId::from_class(SockClass::Udp, 2).expect("udp id");
        let slot = sockevent::hash_slot(id.raw() as u32);
        assert!(slot < sockevent::HASH_SLOTS);
        assert_eq!(
            sockevent::hash_slot((SOCKID_UDP | 2) as u32),
            slot,
            "哈希消费裸编号，与类基值位形一致"
        );
    }
}
