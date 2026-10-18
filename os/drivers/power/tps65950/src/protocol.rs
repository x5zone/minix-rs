//! RTCDEV protocol constants and wire offsets.
//!
//! C correspondence: the request and reply bases (`com.h:995-1011`:
//! `RTCDEV_RQ_BASE 0x1400`, `RTCDEV_RS_BASE 0x1480`, the two grant
//! variants the TPS65950 driver serves, and the general reply) and the
//! `mess_lc_readclock_rtcdev` payload (`ipc.h:593-600`: a grant id, an
//! eight-aligned buffer address, and the flags; the reply struct
//! `mess_readclock_lc_rtcdev`, `ipc.h:1845-1852`, carries the status).

/// Request base (`RTCDEV_RQ_BASE`).
pub const RTC_REQUEST_BASE: i32 = 0x1400;
/// Reply base (`RTCDEV_RS_BASE`).
pub const RTC_REPLY_BASE: i32 = 0x1480;

/// Read the time through a grant (`RTCDEV_GET_TIME_G`).
pub const GET_TIME_GRANT: i32 = RTC_REQUEST_BASE + 3;
/// Set the time through a grant (`RTCDEV_SET_TIME_G`).
pub const SET_TIME_GRANT: i32 = RTC_REQUEST_BASE + 4;
/// Power-off request (`RTCDEV_PWR_OFF`).
pub const PWR_OFF: i32 = RTC_REQUEST_BASE + 2;

/// No flags (`RTCDEV_NOFLAGS`).
pub const NO_FLAGS: i32 = 0x00;

/// The message union payload begins at byte 8 of `Message`.
pub const PAYLOAD: usize = 8;

/// Request and reply wire offsets (`mess_lc_readclock_rtcdev`).
pub mod cwire {
    use super::PAYLOAD;
    /// `cp_grant_id_t grant`.
    pub const GRANT: usize = PAYLOAD;
    /// `vir_bytes tm` (a `struct tm *` in the caller).
    pub const TM: usize = PAYLOAD + 8;
    /// `int flags`.
    pub const FLAGS: usize = PAYLOAD + 16;
    /// `int status` (the reply struct's first field).
    pub const STATUS: usize = PAYLOAD;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_numbers_match_com_header() {
        assert_eq!(RTC_REQUEST_BASE, 0x1400);
        assert_eq!(RTC_REPLY_BASE, 0x1480);
        assert_eq!(GET_TIME_GRANT, 0x1403);
        assert_eq!(SET_TIME_GRANT, 0x1404);
        assert_eq!(PWR_OFF, 0x1402);
        assert_eq!(NO_FLAGS, 0);
    }
}
