//! Socket power negotiation (`do_int`'s decision half).
//!
//! C correspondence: `do_int` (`ti1225.c:246-431`) — the card-present
//! guards in order (either card-detect bit means no card, then
//! "not a card", then "already powered"), the voltage negotiation in
//! C's order (5 V, 3 V, X.X V, Y.Y V, each requiring the card and the
//! socket to agree), and the control update
//! (`(csr_control & ~CC_VCCCTRL) | CC_VCC_*`). The bad-Vcc and
//! data-lost branches only print in C and do not return; they are noted
//! where the outcome is produced.

use crate::registers::{
    CC_VCCCTRL, CC_VCC_3V, CC_VCC_5V, CC_VCC_XV, CC_VCC_YV, CP_3VCARD, CP_3VSOCKET, CP_5VCARD,
    CP_5VSOCKET, CP_CDETECT1, CP_CDETECT2, CP_NOTACARD, CP_PWRCYCLE, CP_XVCARD, CP_XVSOCKET,
    CP_YVCARD, CP_YVSOCKET,
};

/// Which supply the socket should apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VccChoice {
    /// 5 V.
    Five,
    /// 3 V.
    Three,
    /// X.X V (the header leaves the value unnamed).
    X,
    /// Y.Y V.
    Y,
}

/// What one socket pass decides from the card-present word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketOutcome {
    /// Either card-detect bit is set: nothing in the socket.
    NoCard,
    /// The controller reports "not a card".
    NotACard,
    /// The socket is already powered.
    AlreadyPowered,
    /// Card and socket agree on this supply.
    Apply(VccChoice),
    /// No common supply.
    Incompatible,
}

/// The voltage field value for one choice (`CC_VCC_*`).
pub const fn vcc_bits(choice: VccChoice) -> u32 {
    match choice {
        VccChoice::Five => CC_VCC_5V,
        VccChoice::Three => CC_VCC_3V,
        VccChoice::X => CC_VCC_XV,
        VccChoice::Y => CC_VCC_YV,
    }
}

/// Replace the control register's voltage field (`do_int`'s update).
pub const fn with_vcc(control: u32, choice: VccChoice) -> u32 {
    (control & !CC_VCCCTRL) | vcc_bits(choice)
}

/// The socket pass's decision, in C's branch order.
pub const fn power_outcome(present: u32) -> SocketOutcome {
    if present & (CP_CDETECT1 | CP_CDETECT2) != 0 {
        return SocketOutcome::NoCard;
    }
    if present & CP_NOTACARD != 0 {
        return SocketOutcome::NotACard;
    }
    if present & CP_PWRCYCLE != 0 {
        return SocketOutcome::AlreadyPowered;
    }
    if present & CP_5VCARD != 0 && present & CP_5VSOCKET != 0 {
        return SocketOutcome::Apply(VccChoice::Five);
    }
    if present & CP_3VCARD != 0 && present & CP_3VSOCKET != 0 {
        return SocketOutcome::Apply(VccChoice::Three);
    }
    if present & CP_XVCARD != 0 && present & CP_XVSOCKET != 0 {
        return SocketOutcome::Apply(VccChoice::X);
    }
    if present & CP_YVCARD != 0 && present & CP_YVSOCKET != 0 {
        return SocketOutcome::Apply(VccChoice::Y);
    }
    SocketOutcome::Incompatible
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_card_detect_wins_over_everything() {
        let present = CP_CDETECT1 | CP_PWRCYCLE | CP_5VCARD | CP_5VSOCKET;
        assert_eq!(power_outcome(present), SocketOutcome::NoCard);
        assert_eq!(power_outcome(CP_CDETECT2), SocketOutcome::NoCard);
    }

    #[test]
    fn test_not_a_card_precedes_power_cycle() {
        let present = CP_NOTACARD | CP_PWRCYCLE | CP_5VCARD | CP_5VSOCKET;
        assert_eq!(power_outcome(present), SocketOutcome::NotACard);
        assert_eq!(power_outcome(CP_5VCARD | CP_5VSOCKET | CP_PWRCYCLE), SocketOutcome::AlreadyPowered);
    }

    #[test]
    fn test_negotiation_order_is_five_three_x_y() {
        let all = CP_5VCARD | CP_5VSOCKET | CP_3VCARD | CP_3VSOCKET | CP_XVCARD | CP_XVSOCKET
            | CP_YVCARD | CP_YVSOCKET;
        assert_eq!(power_outcome(all), SocketOutcome::Apply(VccChoice::Five));
        let three = CP_3VCARD | CP_3VSOCKET | CP_XVCARD | CP_XVSOCKET | CP_YVCARD | CP_YVSOCKET;
        assert_eq!(power_outcome(three), SocketOutcome::Apply(VccChoice::Three));
        let x = CP_XVCARD | CP_XVSOCKET | CP_YVCARD | CP_YVSOCKET;
        assert_eq!(power_outcome(x), SocketOutcome::Apply(VccChoice::X));
        assert_eq!(
            power_outcome(CP_YVCARD | CP_YVSOCKET),
            SocketOutcome::Apply(VccChoice::Y)
        );
    }

    #[test]
    fn test_card_without_matching_socket_is_incompatible() {
        assert_eq!(power_outcome(CP_5VCARD), SocketOutcome::Incompatible);
        assert_eq!(
            power_outcome(CP_5VSOCKET | CP_3VCARD),
            SocketOutcome::Incompatible
        );
    }

    #[test]
    fn test_control_update_replaces_the_voltage_field() {
        assert_eq!(with_vcc(0, VccChoice::Five), CC_VCC_5V);
        assert_eq!(with_vcc(0x7F, VccChoice::Three), 0x3F);
        assert_eq!(with_vcc(CC_VCC_YV | 0x08, VccChoice::X), CC_VCC_XV | 0x08);
    }
}
