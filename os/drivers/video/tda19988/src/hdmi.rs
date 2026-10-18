//! The CEC and HDMI halves: register access, the page cache, and the
//! EDID read.
//!
//! C correspondence: `is_display_connected` (`tda19988.c:355-378`),
//! `enable_hdmi_module` (`tda19988.c:379-395`), `set_page` and the four
//! accessors (`tda19988.c:396-558`), `check_revision`
//! (`tda19988.c:559-590`), `hdmi_ddc_enable` (`tda19988.c:592-631`),
//! `hdmi_init` (`tda19988.c:633-662`), and `read_edid`
//! (`tda19988.c:664-780`).

use minix_driver_rt::transport::DriverTransport;
use minix_i2cdriver::client;
use minix_types::{Endpoint, EBUSY, EIO};

use crate::registers::{
    CEC_ENABLE_ALL_MASK, CEC_ENABLE_REG, CEC_STATUS_CONNECTED_MASK, CEC_STATUS_REG, DDC_RESET_DELAY_MICROS,
    EDID_LEN, EDID_POLL_DELAY_MICROS, EDID_POLL_TRIES, HDMI_CTRL_DDC_CLK_EN_MASK,
    HDMI_CTRL_DDC_CLK_REG, HDMI_CTRL_DDC_CTRL_REG, HDMI_CTRL_DDC_EN_MASK, HDMI_CTRL_INT_EDID_MASK,
    HDMI_CTRL_INT_REG, HDMI_CTRL_INTR_CTRL_REG, HDMI_CTRL_INTR_EN_GLO_MASK, HDMI_CTRL_PAGE,
    HDMI_CTRL_RESET_DDC_MASK, HDMI_CTRL_RESET_REG, HDMI_CTRL_REV_HI_REG, HDMI_CTRL_REV_LO_REG,
    HDMI_EDID_DATA_REG, HDMI_EDID_DEV_ADDR, HDMI_EDID_DEV_ADDR_REG, HDMI_EDID_OFFSET,
    HDMI_EDID_OFFSET_REG, HDMI_EDID_PAGE, HDMI_EDID_REQ_READ_MASK, HDMI_EDID_REQ_REG,
    HDMI_EDID_SEG_ADDR, HDMI_EDID_SEG_ADDR_REG, HDMI_EDID_SEG_PTR_ADDR,
    HDMI_EDID_SEG_PTR_ADDR_REG, HDMI_HDCP_OTP_DDC_CLK_MASK, HDMI_HDCP_OTP_DDC_CLK_REG,
    HDMI_HDCP_OTP_PAGE, HDMI_HDCP_OTP_SOME_MASK, HDMI_HDCP_OTP_SOME_REG, HDMI_PAGELESS,
    HDMI_PAGE_SELECT_REG, HDMI_REV_TDA19988,
};

/// One i2c binding: the bus driver's endpoint plus the device address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bus {
    /// The i2c bus driver's endpoint.
    pub endpoint: Endpoint,
    /// The device's slave address.
    pub address: u16,
}

/// Whether a revision pair is the expected chip (`check_revision`'s gate).
pub const fn revision_ok(revision: u16) -> bool {
    revision == HDMI_REV_TDA19988
}

/// Whether the CEC status word says a display is attached
/// (`is_display_connected`).
pub const fn display_connected(value: u8) -> bool {
    value & CEC_STATUS_CONNECTED_MASK != 0
}

/// Enable the HDMI module's clocks (`enable_hdmi_module`).
pub fn enable_hdmi_module<T: DriverTransport>(transport: &mut T, cec: Bus) -> Result<(), i32> {
    client::write_byte(
        transport,
        cec.endpoint,
        cec.address,
        CEC_ENABLE_REG,
        CEC_ENABLE_ALL_MASK,
    )
}

/// Whether a display is attached (`is_display_connected`).
pub fn is_display_connected<T: DriverTransport>(
    transport: &mut T,
    cec: Bus,
) -> Result<bool, i32> {
    let value = client::read_byte(transport, cec.endpoint, cec.address, CEC_STATUS_REG)?;
    Ok(display_connected(value))
}

/// The HDMI register access with C's page cache (`set_page`'s static).
pub struct Hdmi {
    page: u8,
}

impl Hdmi {
    /// A fresh accessor with no page selected.
    pub const fn new() -> Self {
        Hdmi { page: HDMI_PAGELESS }
    }

    /// The currently selected page (tests inspect the cache).
    pub const fn page(&self) -> u8 {
        self.page
    }

    /// Select one page, writing the selector only on a change
    /// (`set_page`).
    fn set_page<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        bus: Bus,
        page: u8,
    ) -> Result<(), i32> {
        if page != self.page {
            client::write_byte(
                transport,
                bus.endpoint,
                bus.address,
                HDMI_PAGE_SELECT_REG,
                page,
            )?;
            self.page = page;
        }
        Ok(())
    }

    /// Read one register, selecting its page first (`hdmi_read`).
    pub fn read<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        bus: Bus,
        page: u8,
        register: u8,
    ) -> Result<u8, i32> {
        if page != HDMI_PAGELESS {
            self.set_page(transport, bus, page)?;
        }
        client::read_byte(transport, bus.endpoint, bus.address, register)
    }

    /// Write one register, selecting its page first (`hdmi_write`).
    pub fn write<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        bus: Bus,
        page: u8,
        register: u8,
        value: u8,
    ) -> Result<(), i32> {
        if page != HDMI_PAGELESS {
            self.set_page(transport, bus, page)?;
        }
        client::write_byte(transport, bus.endpoint, bus.address, register, value)
    }

    /// Read-modify-write with an OR (`hdmi_set`).
    pub fn set<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        bus: Bus,
        page: u8,
        register: u8,
        mask: u8,
    ) -> Result<(), i32> {
        let value = self.read(transport, bus, page, register)?;
        self.write(transport, bus, page, register, value | mask)
    }

    /// Read-modify-write with an AND-NOT (`hdmi_clear`).
    pub fn clear<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        bus: Bus,
        page: u8,
        register: u8,
        mask: u8,
    ) -> Result<(), i32> {
        let value = self.read(transport, bus, page, register)?;
        self.write(transport, bus, page, register, value & !mask)
    }

    /// Read a block of registers (`hdmi_read_block`).
    pub fn read_block<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        bus: Bus,
        page: u8,
        register: u8,
        out: &mut [u8],
    ) -> Result<(), i32> {
        if page != HDMI_PAGELESS {
            self.set_page(transport, bus, page)?;
        }
        client::read_bytes(transport, bus.endpoint, bus.address, register, out)
    }

    /// Verify the chip revision (`check_revision`).
    pub fn check_revision<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        bus: Bus,
    ) -> Result<(), i32> {
        let low = self.read(transport, bus, HDMI_CTRL_PAGE, HDMI_CTRL_REV_LO_REG)? as u16;
        let high = self.read(transport, bus, HDMI_CTRL_PAGE, HDMI_CTRL_REV_HI_REG)? as u16;
        if revision_ok((high << 8) | low) {
            Ok(())
        } else {
            Err(-EIO)
        }
    }

    /// Bring up the DDC interface (`hdmi_ddc_enable`).
    pub fn ddc_enable<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        bus: Bus,
    ) -> Result<(), i32> {
        self.set(transport, bus, HDMI_CTRL_PAGE, HDMI_CTRL_RESET_REG, HDMI_CTRL_RESET_DDC_MASK)?;
        transport.delay_micros(DDC_RESET_DELAY_MICROS)?;
        self.clear(transport, bus, HDMI_CTRL_PAGE, HDMI_CTRL_RESET_REG, HDMI_CTRL_RESET_DDC_MASK)?;
        transport.delay_micros(DDC_RESET_DELAY_MICROS)?;
        self.write(transport, bus, HDMI_CTRL_PAGE, HDMI_CTRL_DDC_CTRL_REG, HDMI_CTRL_DDC_EN_MASK)?;
        self.write(transport, bus, HDMI_CTRL_PAGE, HDMI_CTRL_DDC_CLK_REG, HDMI_CTRL_DDC_CLK_EN_MASK)?;
        self.write(
            transport,
            bus,
            HDMI_HDCP_OTP_PAGE,
            HDMI_HDCP_OTP_DDC_CLK_REG,
            HDMI_HDCP_OTP_DDC_CLK_MASK,
        )
    }

    /// The full HDMI bring-up (`hdmi_init`): module clocks, revision
    /// check, and the DDC interface.
    pub fn init<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        cec: Bus,
        hdmi: Bus,
    ) -> Result<(), i32> {
        enable_hdmi_module(transport, cec)?;
        self.check_revision(transport, hdmi)?;
        self.ddc_enable(transport, hdmi)
    }

    /// Trigger and receive one EDID block (`read_edid`).
    pub fn read_edid<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        bus: Bus,
        out: &mut [u8; EDID_LEN],
    ) -> Result<(), i32> {
        self.clear(transport, bus, HDMI_HDCP_OTP_PAGE, HDMI_HDCP_OTP_SOME_REG, HDMI_HDCP_OTP_SOME_MASK)?;
        self.set(transport, bus, HDMI_CTRL_PAGE, HDMI_CTRL_INT_REG, HDMI_CTRL_INT_EDID_MASK)?;
        self.write(transport, bus, HDMI_CTRL_PAGE, HDMI_CTRL_INTR_CTRL_REG, HDMI_CTRL_INTR_EN_GLO_MASK)?;
        self.write(transport, bus, HDMI_EDID_PAGE, HDMI_EDID_DEV_ADDR_REG, HDMI_EDID_DEV_ADDR)?;
        self.write(transport, bus, HDMI_EDID_PAGE, HDMI_EDID_OFFSET_REG, HDMI_EDID_OFFSET)?;
        self.write(transport, bus, HDMI_EDID_PAGE, HDMI_EDID_SEG_PTR_ADDR_REG, HDMI_EDID_SEG_PTR_ADDR)?;
        self.write(transport, bus, HDMI_EDID_PAGE, HDMI_EDID_SEG_ADDR_REG, HDMI_EDID_SEG_ADDR)?;
        // Toggle the read-request bit.
        self.write(transport, bus, HDMI_EDID_PAGE, HDMI_EDID_REQ_REG, HDMI_EDID_REQ_READ_MASK)?;
        self.write(transport, bus, HDMI_EDID_PAGE, HDMI_EDID_REQ_REG, 0x00)?;

        // Poll the data-ready interrupt.
        let mut ready = false;
        for _ in 0..EDID_POLL_TRIES {
            let value = self.read(transport, bus, HDMI_CTRL_PAGE, HDMI_CTRL_INT_REG)?;
            if value & HDMI_CTRL_INT_EDID_MASK != 0 {
                ready = true;
                break;
            }
            transport.delay_micros(EDID_POLL_DELAY_MICROS)?;
        }
        if !ready {
            return Err(-EBUSY);
        }

        self.read_block(transport, bus, HDMI_EDID_PAGE, HDMI_EDID_DATA_REG, out)?;
        self.clear(transport, bus, HDMI_CTRL_PAGE, HDMI_CTRL_INT_REG, HDMI_CTRL_INT_EDID_MASK)?;
        self.set(transport, bus, HDMI_HDCP_OTP_PAGE, HDMI_HDCP_OTP_SOME_REG, HDMI_HDCP_OTP_SOME_MASK)
    }
}

impl Default for Hdmi {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_revision_and_connection_decodes() {
        assert!(revision_ok(0x0331));
        assert!(!revision_ok(0x0330));
        assert!(display_connected(0x02));
        assert!(display_connected(0x06));
        assert!(!display_connected(0x00));
    }
}
