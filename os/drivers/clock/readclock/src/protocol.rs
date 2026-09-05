//! Real-time-clock protocol: request numbers, flags, and time shape.
//!
//! C correspondence: `RTCDEV_RQ_BASE 0x1400`, `RTCDEV_RS_BASE 0x1480`,
//! the five request numbers plus the general reply in
//! `minix3/minix/include/minix/com.h:995-1012`, and the broken-down time
//! (`struct tm`) carried by value in the message paths of
//! `minix3/minix/drivers/clock/readclock/readclock.c:70-114`.

/// Base of the real-time-clock request range.
///
/// C: `RTCDEV_RQ_BASE 0x1400` (`com.h:995`).
pub const RTC_REQUEST_BASE: i32 = 0x1400;

/// Base of the real-time-clock reply range.
///
/// C: `RTCDEV_RS_BASE 0x1480` (`com.h:996`).
pub const RTC_REPLY_BASE: i32 = 0x1480;

/// Read the time from the hardware clock (caller buffer addressing).
///
/// C: `RTCDEV_GET_TIME` (`com.h:1002`).
pub const GET_TIME: i32 = RTC_REQUEST_BASE;
/// Write the time into the hardware clock.
///
/// C: `RTCDEV_SET_TIME` (`com.h:1003`).
pub const SET_TIME: i32 = RTC_REQUEST_BASE + 1;
/// Program the power-off time (power management only).
///
/// C: `RTCDEV_PWR_OFF` (`com.h:1004`).
pub const POWER_OFF: i32 = RTC_REQUEST_BASE + 2;
/// Read the time through a grant (forward path).
///
/// C: `RTCDEV_GET_TIME_G` (`com.h:1007`).
pub const GET_TIME_GRANT: i32 = RTC_REQUEST_BASE + 3;
/// Write the time through a grant (forward path).
///
/// C: `RTCDEV_SET_TIME_G` (`com.h:1008`).
pub const SET_TIME_GRANT: i32 = RTC_REQUEST_BASE + 4;

/// General reply code.
///
/// C: `RTCDEV_REPLY` (`com.h:1011`).
pub const REPLY: i32 = RTC_REPLY_BASE;

/// No special flags on a clock request.
///
/// C: `RTCDEV_NOFLAGS` as used by the forward power-off call
/// (`forward.c:113`).
pub const NO_FLAGS: i32 = 0;

/// Real-time-clock request kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtcRequest {
    /// Read the time.
    GetTime,
    /// Write the time.
    SetTime,
    /// Program the power-off time.
    PowerOff,
    /// Read the time through a grant.
    GetTimeGrant,
    /// Write the time through a grant.
    SetTimeGrant,
}

impl RtcRequest {
    /// Full message type of the request.
    pub const fn message_type(self) -> i32 {
        match self {
            RtcRequest::GetTime => GET_TIME,
            RtcRequest::SetTime => SET_TIME,
            RtcRequest::PowerOff => POWER_OFF,
            RtcRequest::GetTimeGrant => GET_TIME_GRANT,
            RtcRequest::SetTimeGrant => SET_TIME_GRANT,
        }
    }

    /// Decode a raw message type; `None` means "not a clock request".
    pub const fn decode(message_type: i32) -> Option<RtcRequest> {
        match message_type - RTC_REQUEST_BASE {
            0 => Some(RtcRequest::GetTime),
            1 => Some(RtcRequest::SetTime),
            2 => Some(RtcRequest::PowerOff),
            3 => Some(RtcRequest::GetTimeGrant),
            4 => Some(RtcRequest::SetTimeGrant),
            _ => None,
        }
    }
}

/// Returns true for a clock request or the general reply.
///
/// C: `IS_RTCDEV_RQ` and `IS_RTCDEV_RS` (`com.h:998-999`).
pub const fn is_clock_message(message_type: i32) -> bool {
    (message_type & !0x7f) == RTC_REQUEST_BASE || (message_type & !0x7f) == RTC_REPLY_BASE
}

/// Broken-down calendar time (year through second).
///
/// C: `struct tm` as carried by `fetch_t`/`store_t`
/// (`readclock.c:179-191`). Weekday and yearday ride along untouched;
/// the driver never interprets them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrokenTime {
    /// Seconds after the minute (zero to sixty-one, leap included).
    pub seconds: i32,
    /// Minutes after the hour.
    pub minutes: i32,
    /// Hours since midnight.
    pub hours: i32,
    /// Day of the month (one-based).
    pub day: i32,
    /// Months since January (zero-based).
    pub month: i32,
    /// Years since nineteen hundred.
    pub year: i32,
}

impl BrokenTime {
    /// Range-check the six interpreted fields (leap second allowed).
    pub const fn is_plausible(self) -> bool {
        self.seconds >= 0
            && self.seconds <= 61
            && self.minutes >= 0
            && self.minutes <= 59
            && self.hours >= 0
            && self.hours <= 23
            && self.day >= 1
            && self.day <= 31
            && self.month >= 0
            && self.month <= 11
            && self.year >= 70
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_numbers_match_com_header() {
        assert_eq!(RtcRequest::GetTime.message_type(), 0x1400);
        assert_eq!(RtcRequest::SetTime.message_type(), 0x1401);
        assert_eq!(RtcRequest::PowerOff.message_type(), 0x1402);
        assert_eq!(RtcRequest::GetTimeGrant.message_type(), 0x1403);
        assert_eq!(RtcRequest::SetTimeGrant.message_type(), 0x1404);
        assert_eq!(REPLY, 0x1480);
    }

    #[test]
    fn test_decode_round_trips_all_five_requests() {
        let all = [
            RtcRequest::GetTime,
            RtcRequest::SetTime,
            RtcRequest::PowerOff,
            RtcRequest::GetTimeGrant,
            RtcRequest::SetTimeGrant,
        ];
        for request in all {
            assert_eq!(RtcRequest::decode(request.message_type()), Some(request));
        }
        assert_eq!(RtcRequest::decode(0x1405), None);
        assert_eq!(RtcRequest::decode(0x400), None);
    }

    #[test]
    fn test_plausibility_bounds() {
        let good = BrokenTime {
            seconds: 30,
            minutes: 15,
            hours: 10,
            day: 5,
            month: 8,
            year: 126,
        };
        assert!(good.is_plausible());
        let bad = BrokenTime { hours: 24, ..good };
        assert!(!bad.is_plausible());
        let leap = BrokenTime {
            seconds: 60,
            ..good
        };
        assert!(leap.is_plausible());
    }
}
