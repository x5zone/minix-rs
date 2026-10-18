//! TDA19988 register map: CEC registers, HDMI pages, and the EDID path.
//!
//! C correspondence: `tda19988.c:11-136` — the device count and EDID
//! length, the copy buffer size, the CEC status/enable registers, the
//! seven HDMI pages plus the pageless marker, the control-page
//! registers and masks, the EDID-page registers, the HDCP/OTP pair, the
//! page-select register, and the expected revision `0x0331`.

/// Devices this driver handles (`NR_DEVS`).
pub const NR_DEVS: usize = 1;
/// The TDA19988's device index (`TDA19988_DEV`).
pub const TDA19988_DEV: usize = 0;
/// Standard EDID block length (`EDID_LEN`).
pub const EDID_LEN: usize = 128;
/// Copy buffer size (`COPYBUF_SIZE`, four kilobytes).
pub const COPYBUF_SIZE: usize = 0x1000;
/// The exported device's size in bytes (`dv_size = 128`).
pub const DEVICE_SIZE: u64 = 128;

/// CEC status register (`CEC_STATUS_REG`).
pub const CEC_STATUS_REG: u8 = 0xFE;
/// CEC display-connected bit (`CEC_STATUS_CONNECTED_MASK`).
pub const CEC_STATUS_CONNECTED_MASK: u8 = 0x02;
/// CEC enable register (`CEC_ENABLE_REG`).
pub const CEC_ENABLE_REG: u8 = 0xFF;
/// CEC enable-all mask (`CEC_ENABLE_ALL_MASK`).
pub const CEC_ENABLE_ALL_MASK: u8 = 0x87;

/// HDMI control page (`HDMI_CTRL_PAGE`).
pub const HDMI_CTRL_PAGE: u8 = 0x00;
/// HDMI PPL page (`HDMI_PPL_PAGE`).
pub const HDMI_PPL_PAGE: u8 = 0x02;
/// HDMI EDID page (`HDMI_EDID_PAGE`).
pub const HDMI_EDID_PAGE: u8 = 0x09;
/// HDMI info page (`HDMI_INFO_PAGE`).
pub const HDMI_INFO_PAGE: u8 = 0x10;
/// HDMI audio page (`HDMI_AUDIO_PAGE`).
pub const HDMI_AUDIO_PAGE: u8 = 0x11;
/// HDMI HDCP/OTP page (`HDMI_HDCP_OTP_PAGE`).
pub const HDMI_HDCP_OTP_PAGE: u8 = 0x12;
/// HDMI gamut page (`HDMI_GAMUT_PAGE`).
pub const HDMI_GAMUT_PAGE: u8 = 0x13;
/// Marker meaning "no page is selected" (`HDMI_PAGELESS`).
pub const HDMI_PAGELESS: u8 = 0xFF;

/// Control page: revision, low byte (`HDMI_CTRL_REV_LO_REG`).
pub const HDMI_CTRL_REV_LO_REG: u8 = 0x00;
/// Control page: revision, high byte (`HDMI_CTRL_REV_HI_REG`).
pub const HDMI_CTRL_REV_HI_REG: u8 = 0x02;
/// Control page: reset register (`HDMI_CTRL_RESET_REG`).
pub const HDMI_CTRL_RESET_REG: u8 = 0x0A;
/// Control page: DDC reset bit (`HDMI_CTRL_RESET_DDC_MASK`).
pub const HDMI_CTRL_RESET_DDC_MASK: u8 = 0x02;
/// Control page: DDC control register (`HDMI_CTRL_DDC_CTRL_REG`).
pub const HDMI_CTRL_DDC_CTRL_REG: u8 = 0x0B;
/// Control page: DDC enable value (`HDMI_CTRL_DDC_EN_MASK`, zero).
pub const HDMI_CTRL_DDC_EN_MASK: u8 = 0x00;
/// Control page: DDC clock register (`HDMI_CTRL_DDC_CLK_REG`).
pub const HDMI_CTRL_DDC_CLK_REG: u8 = 0x0C;
/// Control page: DDC clock enable mask (`HDMI_CTRL_DDC_CLK_EN_MASK`).
pub const HDMI_CTRL_DDC_CLK_EN_MASK: u8 = 0x01;
/// Control page: interrupt control register (`HDMI_CTRL_INTR_CTRL_REG`).
pub const HDMI_CTRL_INTR_CTRL_REG: u8 = 0x0F;
/// Control page: global interrupt enable mask (`HDMI_CTRL_INTR_EN_GLO_MASK`).
pub const HDMI_CTRL_INTR_EN_GLO_MASK: u8 = 0x04;
/// Control page: interrupt status register (`HDMI_CTRL_INT_REG`).
pub const HDMI_CTRL_INT_REG: u8 = 0x11;
/// Control page: EDID-block interrupt mask (`HDMI_CTRL_INT_EDID_MASK`).
pub const HDMI_CTRL_INT_EDID_MASK: u8 = 0x02;

/// EDID page: data register (`HDMI_EDID_DATA_REG`).
pub const HDMI_EDID_DATA_REG: u8 = 0x00;
/// EDID page: device address register (`HDMI_EDID_DEV_ADDR_REG`).
pub const HDMI_EDID_DEV_ADDR_REG: u8 = 0xFB;
/// EDID page: device address value (`HDMI_EDID_DEV_ADDR`).
pub const HDMI_EDID_DEV_ADDR: u8 = 0xA0;
/// EDID page: offset register (`HDMI_EDID_OFFSET_REG`).
pub const HDMI_EDID_OFFSET_REG: u8 = 0xFC;
/// EDID page: offset value (`HDMI_EDID_OFFSET`).
pub const HDMI_EDID_OFFSET: u8 = 0x00;
/// EDID page: segment-pointer address register
/// (`HDMI_EDID_SEG_PTR_ADDR_REG`).
pub const HDMI_EDID_SEG_PTR_ADDR_REG: u8 = 0xFC;
/// EDID page: segment-pointer address value (`HDMI_EDID_SEG_PTR_ADDR`).
pub const HDMI_EDID_SEG_PTR_ADDR: u8 = 0x00;
/// EDID page: segment address register (`HDMI_EDID_SEG_ADDR_REG`).
pub const HDMI_EDID_SEG_ADDR_REG: u8 = 0xFE;
/// EDID page: segment address value (`HDMI_EDID_SEG_ADDR`).
pub const HDMI_EDID_SEG_ADDR: u8 = 0x00;
/// EDID page: read-request register (`HDMI_EDID_REQ_REG`).
pub const HDMI_EDID_REQ_REG: u8 = 0xFA;
/// EDID page: read-request bit (`HDMI_EDID_REQ_READ_MASK`).
pub const HDMI_EDID_REQ_READ_MASK: u8 = 0x01;

/// HDCP/OTP page: DDC clock register (`HDMI_HDCP_OTP_DDC_CLK_REG`).
pub const HDMI_HDCP_OTP_DDC_CLK_REG: u8 = 0x9A;
/// HDCP/OTP page: DDC clock mask (`HDMI_HDCP_OTP_DDC_CLK_MASK`).
pub const HDMI_HDCP_OTP_DDC_CLK_MASK: u8 = 0x27;
/// HDCP/OTP page: undocumented register (`HDMI_HDCP_OTP_SOME_REG`).
pub const HDMI_HDCP_OTP_SOME_REG: u8 = 0x9B;
/// HDCP/OTP page: undocumented mask (`HDMI_HDCP_OTP_SOME_MASK`).
pub const HDMI_HDCP_OTP_SOME_MASK: u8 = 0x02;

/// Page-select register, outside any page (`HDMI_PAGE_SELECT_REG`).
pub const HDMI_PAGE_SELECT_REG: u8 = 0xFF;

/// The revision the driver accepts (`HDMI_REV_TDA19988`).
pub const HDMI_REV_TDA19988: u16 = 0x0331;

/// Polls before the EDID-ready interrupt gives up (`read_edid`).
pub const EDID_POLL_TRIES: usize = 100;
/// Delay between EDID polls, in microseconds.
pub const EDID_POLL_DELAY_MICROS: u32 = 1000;
/// Delay around the DDC reset pulse, in microseconds (`hdmi_ddc_enable`).
pub const DDC_RESET_DELAY_MICROS: u32 = 100_000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants_match_the_driver_header() {
        assert_eq!(EDID_LEN, 128);
        assert_eq!(COPYBUF_SIZE, 0x1000);
        assert_eq!(DEVICE_SIZE, 128);
        assert_eq!(CEC_STATUS_REG, 0xFE);
        assert_eq!(CEC_STATUS_CONNECTED_MASK, 0x02);
        assert_eq!(CEC_ENABLE_ALL_MASK, 0x87);
        assert_eq!(HDMI_CTRL_PAGE, 0x00);
        assert_eq!(HDMI_EDID_PAGE, 0x09);
        assert_eq!(HDMI_HDCP_OTP_PAGE, 0x12);
        assert_eq!(HDMI_PAGELESS, 0xFF);
        assert_eq!(HDMI_EDID_DEV_ADDR, 0xA0);
        assert_eq!(HDMI_REV_TDA19988, 0x0331);
        assert_eq!(HDMI_PAGE_SELECT_REG, 0xFF);
    }
}
