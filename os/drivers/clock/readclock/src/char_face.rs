//! Chardriver wiring: the readclock driver as a chardriver device.
//!
//! C correspondence: `readclock.c` main structure — RTCDEV requests
//! dispatched through the permission gates (`device::dispatch`), the
//! clock chip behind the [`RealTimeClock`] trait, replies carrying the
//! broken-down time.
//!
//! RTCDEV is a dedicated protocol family (com.h:995-1012) rather than
//! CDEV; the face here routes the family's requests over the framework
//! machinery (notifications drop, permissions gate before any copy).
//! Grant-vs-value request variants stay visible to the service, which
//! picks the safecopy transport.

use minix_chardriver::protocol::OpenDeviceSet;

use crate::clock::RealTimeClock;
use crate::device::{dispatch, Caller, Dispatch};
use crate::protocol::RtcRequest;

/// The readclock driver's face over one clock chip.
pub struct ReadclockFace<R: RealTimeClock> {
    pub clock: R,
    /// Opened minors since the last announce (the restart gate).
    pub opened: OpenDeviceSet,
}

impl<R: RealTimeClock> ReadclockFace<R> {
    /// A face over the given clock.
    pub fn new(clock: R) -> Self {
        ReadclockFace {
            clock,
            opened: OpenDeviceSet::new(),
        }
    }

    /// Serve one RTCDEV request; returns what to reply.
    ///
    /// C: `main` (`readclock.c:59-114`) — the dispatcher gates first
    /// (permissions before copies), then the chip reads or writes.
    /// `Get` answers the broken-down time or the chip's error; refusals
    /// carry their code unchanged.
    pub fn serve(
        &mut self,
        is_notify: bool,
        request: Option<RtcRequest>,
        caller: Caller,
    ) -> Result<Option<crate::protocol::BrokenTime>, i32> {
        match dispatch(is_notify, request, caller) {
            Dispatch::Drop => Ok(None),
            Dispatch::Unknown => Err(crate::device::unknown_call()),
            Dispatch::Get => match self.clock.get_time(crate::protocol::NO_FLAGS) {
                Ok(time) => Ok(Some(time)),
                Err(code) => Err(code),
            },
            Dispatch::Set(Ok(())) => {
                // The time value arrives via the grant copy in the
                // service; the face-level set path is exercised by the
                // service tests with a decoded value.
                Ok(None)
            }
            Dispatch::Set(Err(code)) => Err(code),
            Dispatch::PowerOff(Ok(())) => Ok(None),
            Dispatch::PowerOff(Err(code)) => Err(code),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::MemClock;
    use crate::device::test_time;

    #[test]
    fn test_get_returns_chip_time_after_init() {
        // MemClock::new is born initialized and holds the given time.
        let mut face = ReadclockFace::new(MemClock::new(test_time()));
        let reply = face
            .serve(false, Some(RtcRequest::GetTime), Caller::ordinary())
            .unwrap();
        assert_eq!(reply, Some(test_time()));
    }

    #[test]
    fn test_set_permission_gates_before_touching_the_chip() {
        let mut face = ReadclockFace::new(MemClock::new(test_time()));
        // An ordinary caller's set is refused before the chip is touched.
        assert_eq!(
            face.serve(false, Some(RtcRequest::SetTime), Caller::ordinary()),
            Err(-minix_types::EPERM)
        );
        assert_eq!(face.clock.stored(), test_time()); // unchanged
        // Super user passes (the value copy is the service's job).
        assert_eq!(
            face.serve(false, Some(RtcRequest::SetTime), Caller::root()),
            Ok(None)
        );
    }

    #[test]
    fn test_notifications_drop_and_unknown_is_einval() {
        let mut face = ReadclockFace::new(MemClock::new(test_time()));
        assert_eq!(face.serve(true, None, Caller::root()), Ok(None));
        assert_eq!(
            face.serve(false, None, Caller::root()),
            Err(crate::device::unknown_call())
        );
    }
}
