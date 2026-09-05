//! Network device consumption policy: slot count, queue depths, active rule.
//!
//! C correspondence: `minix3/minix/net/lwip/ndev.c` (1019 lines) with the
//! public surface in `minix3/minix/net/lwip/ndev.h` (33 lines). Driver
//! processes, message traffic, and packet storage stay in the service binary.
//! This module owns the portion that can be decided from numbers alone: how
//! many device slots exist, how deep each queue is guaranteed to be, when a
//! slot counts as active, and how many request objects the pool holds.
//!
//! The consumption side mirrors the driver side defined with the 16-stage
//! driver framework. The driver owns the hardware; this module owns the
//! client-side bounds that keep one misbehaving driver from exhausting the
//! service.

/// Largest network device drivers (`NR_NDEV`, `ndev.h:5`).
pub const MAX_DEVICES: usize = 8;

/// Minimum guaranteed send queue depth per device (`NDEV_SENDQ`,
/// `ndev.c:72`).
pub const SEND_QUEUE_GUARANTEE: usize = 2;

/// Guaranteed receive queue depth per device (`NDEV_RECVQ`, `ndev.c:73`).
pub const RECEIVE_QUEUE_GUARANTEE: usize = 2;

/// Spare request objects beyond the guaranteed queues (`NREQ_SPARES`,
/// `ndev.c:74`).
pub const SPARE_REQUESTS: usize = 8;

/// Total request objects (`NR_NREQ`, `ndev.c:75`: both queues times the
/// device count plus spares).
pub const TOTAL_REQUESTS: usize =
    (SEND_QUEUE_GUARANTEE + RECEIVE_QUEUE_GUARANTEE) * MAX_DEVICES + SPARE_REQUESTS;

/// Whether a device identifier names a valid slot.
pub fn device_id_valid(id: usize) -> bool {
    id < MAX_DEVICES
}

/// Whether a slot counts as active (`NDEV_ACTIVE`, `ndev.c:108`: the send
/// queue maximum depth is greater than zero).
///
/// A slot becomes active once the driver initialization reply arrives and
/// reports queue depths. Before that, the slot is tracked but has no
/// interface attached, which is why initialization requests are handled
/// without an interface object.
pub fn device_active(send_queue_max: usize) -> bool {
    send_queue_max > 0
}

/// Whether a send queue has room for one more non-receive request
/// (`ndev_queue_add`, `ndev.c:293-296`: requests other than receives are
/// refused once the count reaches the send guarantee).
pub fn send_queue_has_room(queued: usize, is_receive: bool) -> bool {
    if is_receive {
        return true;
    }
    queued < SEND_QUEUE_GUARANTEE
}

/// Whether a receive count needs clamping to the guarantee
/// (`ndev_init_reply`, `ndev.c:596-597`: the driver-reported maximum is
/// capped at the receive guarantee).
pub fn clamp_receive_max(driver_reported: usize) -> usize {
    driver_reported.min(RECEIVE_QUEUE_GUARANTEE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slot_count_matches_header() {
        assert_eq!(MAX_DEVICES, 8);
        assert!(device_id_valid(0));
        assert!(device_id_valid(7));
        assert!(!device_id_valid(8));
    }

    #[test]
    fn test_queue_guarantees_match_source() {
        assert_eq!(SEND_QUEUE_GUARANTEE, 2);
        assert_eq!(RECEIVE_QUEUE_GUARANTEE, 2);
        assert_eq!(SPARE_REQUESTS, 8);
        assert_eq!(TOTAL_REQUESTS, 40);
    }

    #[test]
    fn test_active_means_send_queue_advertised() {
        assert!(!device_active(0));
        assert!(device_active(1));
        assert!(device_active(2));
    }

    #[test]
    fn test_send_room_applies_only_to_non_receives() {
        assert!(send_queue_has_room(2, true));
        assert!(send_queue_has_room(1, false));
        assert!(!send_queue_has_room(2, false));
    }

    #[test]
    fn test_receive_max_is_clamped_to_guarantee() {
        assert_eq!(clamp_receive_max(1), 1);
        assert_eq!(clamp_receive_max(2), 2);
        assert_eq!(clamp_receive_max(5), 2);
    }
}
