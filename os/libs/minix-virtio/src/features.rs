//! Feature negotiation: host offers, guest accepts the intersection.
//!
//! C correspondence: `exchange_features` plus `virtio_host_supports`
//! and `virtio_guest_supports` in `minix3/minix/lib/libvirtio/virtio.c:
//! 223-246,806-831`, over the `struct virtio_feature` table
//! (`virtio.h`).

/// One negotiable feature: bit plus who supports it.
///
/// C: `struct virtio_feature` with name, bit, host and guest support
/// (`virtio.h`). Names stay in the service crate (diagnostics); bits
/// drive the policy here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Feature {
    /// Feature bit (zero through thirty-one).
    pub bit: u8,
    /// Host offers it.
    pub host: bool,
    /// Guest wants it.
    pub guest: bool,
}

impl Feature {
    /// True when both sides agree (the negotiated set).
    pub const fn agreed(self) -> bool {
        self.host && self.guest
    }
}

/// Negotiate a feature table: guest keeps a bit only when the host
/// offers it.
///
/// C: `exchange_features` (`virtio.c:223-246`): read host bits, mask the
/// guest request, write back the intersection. Unknown host bits are
/// ignored (forward compatibility: old guests on new hosts).
pub fn negotiate(features: &mut [Feature], host_bits: u32) {
    for feature in features.iter_mut() {
        if feature.bit < 32 {
            let offered = host_bits & (1 << feature.bit) != 0;
            feature.host = offered;
        }
    }
}

/// Bits of the agreed set, for the write-back.
pub fn agreed_bits(features: &[Feature]) -> u32 {
    let mut bits = 0u32;
    for feature in features {
        if feature.agreed() && feature.bit < 32 {
            bits |= 1 << feature.bit;
        }
    }
    bits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> [Feature; 3] {
        [
            Feature {
                bit: 0,
                host: false,
                guest: true,
            },
            Feature {
                bit: 5,
                host: false,
                guest: true,
            },
            Feature {
                bit: 9,
                host: false,
                guest: false,
            },
        ]
    }

    #[test]
    fn test_negotiation_keeps_intersection_only() {
        let mut features = table();
        negotiate(&mut features, (1 << 0) | (1 << 9));
        assert!(features[0].agreed());
        assert!(!features[1].agreed());
        assert!(!features[2].agreed());
        assert_eq!(agreed_bits(&features), 1);
    }

    #[test]
    fn test_unknown_host_bits_are_ignored() {
        let mut features = table();
        negotiate(&mut features, 0xFFFF_FFFF);
        assert!(features[0].agreed());
        assert!(features[1].agreed());
    }
}
