//! Request dispatch: permission gates for the three operations.
//!
//! C correspondence: the receive loop in `main`
//! (`readclock.c:50-131`): notifications are dropped without reply, get
//! is open to everyone, set needs the super user, power-off needs the
//! power manager, unknown calls are invalid, and replies go out
//! non-blocking.

use super::protocol::{BrokenTime, RtcRequest};
use minix_types::{EINVAL, EPERM, OK};

/// Caller identity for one request: endpoint plus super-user flag.
///
/// C: `getnuid(caller) == SUPER_USER` (`readclock.c:85`) and
/// `caller == PM_PROC_NR` (`readclock.c:103`). Numeric endpoint values
/// stay in the service crate; this type carries the two verdicts the
/// policy needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caller {
    /// True for the super user.
    pub super_user: bool,
    /// True for the power manager endpoint.
    pub power_manager: bool,
}

impl Caller {
    /// Ordinary caller (neither super user nor power manager).
    pub const fn ordinary() -> Caller {
        Caller {
            super_user: false,
            power_manager: false,
        }
    }

    /// Super-user caller.
    pub const fn root() -> Caller {
        Caller {
            super_user: true,
            power_manager: false,
        }
    }

    /// Power-manager caller.
    pub const fn power() -> Caller {
        Caller {
            super_user: true,
            power_manager: true,
        }
    }
}

/// What the dispatcher decides for one message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dispatch {
    /// Dropped without reply (notifications).
    Drop,
    /// Read the time (any caller).
    Get,
    /// Write the time (super user only; refusal carries the code).
    Set(Result<(), i32>),
    /// Power off (power manager only; refusal carries the code).
    PowerOff(Result<(), i32>),
    /// Unknown call: invalid argument.
    Unknown,
}

/// Classify one message before touching any clock.
///
/// C: the branch structure of `main` (`readclock.c:59-114`). Permission
/// checks happen before any copy or chip access: a refused set never
/// reads the caller buffer, a refused power-off never touches the chip.
pub const fn dispatch(is_notify: bool, request: Option<RtcRequest>, caller: Caller) -> Dispatch {
    if is_notify {
        return Dispatch::Drop;
    }
    let Some(request) = request else {
        return Dispatch::Unknown;
    };
    match request {
        RtcRequest::GetTime | RtcRequest::GetTimeGrant => Dispatch::Get,
        RtcRequest::SetTime | RtcRequest::SetTimeGrant => {
            if caller.super_user {
                Dispatch::Set(Ok(()))
            } else {
                Dispatch::Set(Err(-EPERM))
            }
        }
        RtcRequest::PowerOff => {
            if caller.power_manager {
                Dispatch::PowerOff(Ok(()))
            } else {
                Dispatch::PowerOff(Err(-EPERM))
            }
        }
    }
}

/// Reply status for an unknown call.
pub const fn unknown_call() -> i32 {
    -EINVAL
}

/// Success marker.
pub const SUCCESS: i32 = OK;

/// Time value used by tests (midnight, plausible).
pub const fn test_time() -> BrokenTime {
    BrokenTime {
        seconds: 0,
        minutes: 0,
        hours: 0,
        day: 1,
        month: 0,
        year: 126,
    }
}

#[cfg(test)]
mod tests {
    use super::super::protocol::RtcRequest;
    use super::*;

    #[test]
    fn test_notifications_drop_without_reply() {
        assert_eq!(
            dispatch(true, Some(RtcRequest::GetTime), Caller::ordinary()),
            Dispatch::Drop
        );
    }

    #[test]
    fn test_get_is_open_to_everyone() {
        assert_eq!(
            dispatch(false, Some(RtcRequest::GetTime), Caller::ordinary()),
            Dispatch::Get
        );
        assert_eq!(
            dispatch(false, Some(RtcRequest::GetTimeGrant), Caller::ordinary()),
            Dispatch::Get
        );
    }

    #[test]
    fn test_set_needs_super_user() {
        assert_eq!(
            dispatch(false, Some(RtcRequest::SetTime), Caller::root()),
            Dispatch::Set(Ok(()))
        );
        assert_eq!(
            dispatch(false, Some(RtcRequest::SetTime), Caller::ordinary()),
            Dispatch::Set(Err(-EPERM))
        );
    }

    #[test]
    fn test_power_off_needs_power_manager() {
        assert_eq!(
            dispatch(false, Some(RtcRequest::PowerOff), Caller::power()),
            Dispatch::PowerOff(Ok(()))
        );
        assert_eq!(
            dispatch(false, Some(RtcRequest::PowerOff), Caller::root()),
            Dispatch::PowerOff(Err(-EPERM))
        );
    }

    #[test]
    fn test_unknown_calls_are_invalid() {
        assert_eq!(dispatch(false, None, Caller::root()), Dispatch::Unknown);
        assert_eq!(unknown_call(), -EINVAL);
        assert_eq!(SUCCESS, OK);
    }
}
