//! The hub class requests: the setup packets the hub driver sends.
//!
//! C correspondence: the feature and request enumerations
//! (`usb_hub.c:100-141`: `class_feature`, `class_code`) and
//! `hub_port_feature`'s validation (`usb_hub.c:565-610`: the port range
//! check, the code must be SET/CLEAR, and only four features are
//! accepted).

/// Hub class request codes (`class_code`).
pub mod code {
    /// `GET_STATUS`.
    pub const GET_STATUS: u8 = 0;
    /// `CLEAR_FEATURE`.
    pub const CLEAR_FEATURE: u8 = 1;
    /// `SET_FEATURE`.
    pub const SET_FEATURE: u8 = 3;
    /// `GET_DESCRIPTOR`.
    pub const GET_DESCRIPTOR: u8 = 6;
}

/// Hub class feature selectors (`class_feature`).
pub mod feature {
    /// `PORT_CONNECTION`.
    pub const PORT_CONNECTION: u16 = 0;
    /// `PORT_ENABLE`.
    pub const PORT_ENABLE: u16 = 1;
    /// `PORT_SUSPEND`.
    pub const PORT_SUSPEND: u16 = 2;
    /// `PORT_OVER_CURRENT`.
    pub const PORT_OVER_CURRENT: u16 = 3;
    /// `PORT_RESET`.
    pub const PORT_RESET: u16 = 4;
    /// `PORT_POWER`.
    pub const PORT_POWER: u16 = 8;
    /// `PORT_LOW_SPEED`.
    pub const PORT_LOW_SPEED: u16 = 9;
    /// `C_PORT_CONNECTION`.
    pub const C_PORT_CONNECTION: u16 = 16;
    /// `C_PORT_RESET`.
    pub const C_PORT_RESET: u16 = 20;
}

/// The class code a control request carries (the hub class is 9).
pub const HUB_CLASS: u8 = 9;

/// Build the setup packet for one port feature request
/// (`hub_port_feature`, `usb_hub.c:568-610`).
///
/// The validation is C's: the port must be within the hub's port count and
/// above zero, the code must be SET_FEATURE or CLEAR_FEATURE, and the
/// feature must be one of the four the driver uses.
pub fn port_feature_setup(
    port: u16,
    code_: u8,
    feature_: u16,
    num_ports: u16,
) -> Result<[u8; 8], i32> {
    if port == 0 || port > num_ports {
        return Err(-minix_types::EINVAL);
    }
    if code_ != code::SET_FEATURE && code_ != code::CLEAR_FEATURE {
        return Err(-minix_types::EINVAL);
    }
    if !matches!(
        feature_,
        feature::PORT_RESET
            | feature::PORT_POWER
            | feature::C_PORT_CONNECTION
            | feature::C_PORT_RESET
    ) {
        return Err(-minix_types::EINVAL);
    }
    // bmRequestType (class, other, device-to-host for a read),
    // bRequest, wValue, wIndex, wLength — the standard setup layout.
    let mut setup = [0u8; 8];
    setup[0] = 0x20 | HUB_CLASS; // class request, host to device
    setup[1] = code_;
    setup[2..4].copy_from_slice(&feature_.to_le_bytes());
    setup[4..6].copy_from_slice(&port.to_le_bytes());
    setup[6..8].copy_from_slice(&0u16.to_le_bytes());
    Ok(setup)
}

/// Build the setup packet for a port status read
/// (`hub_get_port_status`, `usb_hub.c`).
pub fn port_status_setup(port: u16, num_ports: u16) -> Result<[u8; 8], i32> {
    if port == 0 || port > num_ports {
        return Err(-minix_types::EINVAL);
    }
    let mut setup = [0u8; 8];
    setup[0] = 0xA0 | HUB_CLASS; // class request, device to host
    setup[1] = code::GET_STATUS;
    setup[2..4].copy_from_slice(&0u16.to_le_bytes());
    setup[4..6].copy_from_slice(&port.to_le_bytes());
    setup[6..8].copy_from_slice(&4u16.to_le_bytes()); // a port status word
    Ok(setup)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_feature_validation_matches_c() {
        assert!(port_feature_setup(1, code::SET_FEATURE, feature::PORT_POWER, 4).is_ok());
        assert!(port_feature_setup(1, code::CLEAR_FEATURE, feature::C_PORT_RESET, 4).is_ok());
        // Port out of range, or zero.
        assert_eq!(
            port_feature_setup(0, code::SET_FEATURE, feature::PORT_POWER, 4),
            Err(-minix_types::EINVAL)
        );
        assert_eq!(
            port_feature_setup(5, code::SET_FEATURE, feature::PORT_POWER, 4),
            Err(-minix_types::EINVAL)
        );
        // A code the driver does not use.
        assert_eq!(
            port_feature_setup(1, code::GET_STATUS, feature::PORT_POWER, 4),
            Err(-minix_types::EINVAL)
        );
        // A feature outside the four.
        assert_eq!(
            port_feature_setup(1, code::SET_FEATURE, feature::PORT_ENABLE, 4),
            Err(-minix_types::EINVAL)
        );
    }

    #[test]
    fn test_the_setup_packets_carry_the_class_and_fields() {
        let setup = port_feature_setup(2, code::SET_FEATURE, feature::PORT_RESET, 4).unwrap();
        assert_eq!(setup[0], 0x29, "class request, host to device");
        assert_eq!(setup[1], code::SET_FEATURE);
        assert_eq!(
            u16::from_le_bytes([setup[2], setup[3]]),
            feature::PORT_RESET
        );
        assert_eq!(u16::from_le_bytes([setup[4], setup[5]]), 2);
        assert_eq!(u16::from_le_bytes([setup[6], setup[7]]), 0);

        let status = port_status_setup(1, 4).unwrap();
        assert_eq!(status[0], 0xA9, "class request, device to host");
        assert_eq!(status[1], code::GET_STATUS);
        assert_eq!(
            u16::from_le_bytes([status[6], status[7]]),
            4,
            "one status word"
        );
    }
}
