//! BMP085 sensor transactions and compensation math.
//!
//! C correspondence: the constants and calibration layout
//! (`bmp085.c:8-120`), `read_cal_coef` (`bmp085.c:214-300`), and
//! `measure` (`bmp085.c:300-430`) — chip-id check, the eleven
//! calibration registers, the temperature trigger plus big-endian
//! sixteen-bit read, the pressure trigger with its oversampling mode
//! plus twenty-four-bit read, and the datasheet formulas in C's integer
//! arithmetic.
//!
//! The conversion waits in C are `micro_delay`; this module asks the
//! transport for a delay, whose production implementation rides the
//! kernel-call seam (not wired yet — a transport without it reports
//! `ENOSYS`, so a real read fails honestly rather than reading a
//! conversion that has not finished).

use minix_driver_rt::transport::DriverTransport;
use minix_i2cdriver::client;
use minix_types::Endpoint;

/// Control register (`CTRL_REG`, `bmp085.c:10`).
pub const CTRL_REG: u8 = 0xF4;
/// Chip identification register (`CHIPID_REG`).
pub const CHIPID_REG: u8 = 0xD0;
/// The only accepted chip id (`BMP085_CHIPID`).
pub const BMP085_CHIPID: u8 = 0x55;
/// First calibration register (`AC1_MSB_REG`).
pub const CALIBRATION_FIRST: u8 = 0xAA;
/// Last calibration register (`MD_LSB_REG`).
pub const CALIBRATION_LAST: u8 = 0xBF;
/// Raw sensor value, most significant byte (`SENSOR_VAL_MSB_REG`).
pub const SENSOR_VALUE_MSB: u8 = 0xF6;
/// Temperature conversion trigger (`CMD_TRIG_T`).
pub const CMD_TRIGGER_TEMPERATURE: u8 = 0x2E;
/// Temperature conversion time in microseconds (`UDELAY_T`).
pub const UDELAY_TEMPERATURE: u32 = 4_500;

/// One pressure measurement mode (`struct pressure_cmd`, `bmp085.c:38-42`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PressureCommand {
    /// Conversion trigger byte.
    pub command: u8,
    /// Oversampling mode (zero through three).
    pub mode: u8,
    /// Conversion time in microseconds.
    pub udelay: u32,
}

/// The four pressure modes (`pressure_cmds`, `bmp085.c:45-50`).
pub const PRESSURE_COMMANDS: [PressureCommand; 4] = [
    PressureCommand {
        command: 0x34,
        mode: 0,
        udelay: 4_500,
    },
    PressureCommand {
        command: 0x74,
        mode: 1,
        udelay: 7_500,
    },
    PressureCommand {
        command: 0xB4,
        mode: 2,
        udelay: 13_500,
    },
    PressureCommand {
        command: 0xF4,
        mode: 3,
        udelay: 25_500,
    },
];

/// The default mode (`p_cmd = &pressure_cmds[MODE_STD]`).
pub const MODE_STANDARD: usize = 1;

/// Raw calibration words from the datasheet's worked example, in
/// register order (`0xAA` through `0xBF`).
pub fn datasheet_calibration_values() -> [u16; 11] {
    [
        408,
        (-72i16) as u16,
        (-14386i16) as u16,
        32741,
        32757,
        23153,
        6190,
        4,
        (-32768i16) as u16,
        (-8711i16) as u16,
        2868,
    ]
}

/// Calibration coefficients (`struct calibration`, `bmp085.c:74-88`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Calibration {
    pub ac1: i16,
    pub ac2: i16,
    pub ac3: i16,
    pub ac4: u16,
    pub ac5: u16,
    pub ac6: u16,
    pub b1: i16,
    pub b2: i16,
    pub mb: i16,
    pub mc: i16,
    pub md: i16,
}

impl Calibration {
    /// Read and validate the eleven coefficients (`read_cal_coef`).
    ///
    /// C rejects a table where any raw value is zero or all ones
    /// (`CAL_COEF_IS_VALID`, `bmp085.c:299-312`).
    pub fn read<T: DriverTransport>(
        transport: &mut T,
        bus: Endpoint,
        address: u16,
    ) -> Result<Calibration, i32> {
        let mut raw = [0u16; 11];
        for (index, slot) in raw.iter_mut().enumerate() {
            let register = CALIBRATION_FIRST + (index as u8) * 2;
            *slot = client::read_u16(transport, bus, address, register)?;
        }
        if raw.iter().any(|value| *value == 0 || *value == 0xFFFF) {
            return Err(-minix_types::EIO);
        }
        Ok(Calibration {
            ac1: raw[0] as i16,
            ac2: raw[1] as i16,
            ac3: raw[2] as i16,
            ac4: raw[3],
            ac5: raw[4],
            ac6: raw[5],
            b1: raw[6] as i16,
            b2: raw[7] as i16,
            mb: raw[8] as i16,
            mc: raw[9] as i16,
            md: raw[10] as i16,
        })
    }
}

/// Read the chip id (`bmp085.c:190-208`).
pub fn chip_id<T: DriverTransport>(
    transport: &mut T,
    bus: Endpoint,
    address: u16,
) -> Result<u8, i32> {
    client::read_byte(transport, bus, address, CHIPID_REG)
}

/// Trigger one temperature and one pressure conversion and compensate
/// (`measure`, `bmp085.c:300-430`).
pub fn measure<T: DriverTransport>(
    transport: &mut T,
    bus: Endpoint,
    address: u16,
    calibration: &Calibration,
    command: PressureCommand,
) -> Result<(i32, i32), i32> {
    client::write_byte(transport, bus, address, CTRL_REG, CMD_TRIGGER_TEMPERATURE)?;
    transport.delay_micros(UDELAY_TEMPERATURE)?;
    let raw_temperature = client::read_u16(transport, bus, address, SENSOR_VALUE_MSB)? as i16;

    client::write_byte(transport, bus, address, CTRL_REG, command.command)?;
    transport.delay_micros(command.udelay)?;
    let raw_pressure = client::read_u24(transport, bus, address, SENSOR_VALUE_MSB)? as i32;
    let up = raw_pressure >> (8 - command.mode);

    Ok(compensate(calibration, raw_temperature, up, command.mode))
}

/// The datasheet compensation (`bmp085.c:375-430`), in C's integer
/// arithmetic (division truncates toward zero, shifts are arithmetic).
/// For the datasheet's worked example this reproduces its printed
/// 15.0 C and 69964 Pa.
pub fn compensate(calibration: &Calibration, ut: i16, up: i32, mode: u8) -> (i32, i32) {
    let cal = calibration;
    // Temperature (`bmp085.c:375-385`).
    let x1 = ((ut as i32 - cal.ac6 as i32) * cal.ac5 as i32) >> 15;
    let x2 = ((cal.mc as i32) << 11) / (x1 + cal.md as i32);
    let b5 = x1 + x2;
    let temperature = (b5 + 8) / (1 << 4);

    // Pressure (`bmp085.c:391-430`).
    let b6 = b5 - 4000;
    let x1 = ((cal.b2 as i32).wrapping_mul((b6.wrapping_mul(b6)) >> 12)) >> 11;
    let x2 = ((cal.ac2 as i32).wrapping_mul(b6)) >> 11;
    let x3 = x1 + x2;
    let b3 = ((((cal.ac1 as i32).wrapping_mul(4) + x3) << mode) + 2) >> 2;
    let x1 = ((cal.ac3 as i32).wrapping_mul(b6)) >> 13;
    let x2 = ((cal.b1 as i32).wrapping_mul((b6.wrapping_mul(b6)) >> 12)) >> 16;
    let x3 = ((x1 + x2) + 2) >> 2;
    let b4 = (cal.ac4 as u32).wrapping_mul((x3 + 32768) as u32) >> 15;
    let b7 = (up as u32)
        .wrapping_sub(b3 as u32)
        .wrapping_mul(50000u32 >> mode);
    let p = if b7 < 0x8000_0000 {
        (b7.wrapping_mul(2) / b4) as i32
    } else {
        ((b7 / b4).wrapping_mul(2)) as i32
    };
    let x1 = (p >> 8).wrapping_mul(p >> 8);
    let x1 = x1.wrapping_mul(3038) >> 16;
    let x2 = (-7357i32).wrapping_mul(p) >> 16;
    let pressure = p.wrapping_add((x1.wrapping_add(x2).wrapping_add(3791)) >> 4);

    (temperature, pressure)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec;
    use std::vec::Vec;

    /// The datasheet example calibration.
    fn datasheet_calibration() -> Calibration {
        Calibration {
            ac1: 408,
            ac2: -72,
            ac3: -14386,
            ac4: 32741,
            ac5: 32757,
            ac6: 23153,
            b1: 6190,
            b2: 4,
            mb: -32768,
            mc: -8711,
            md: 2868,
        }
    }

    #[test]
    fn test_datasheet_example_compensates() {
        // ut 27898, up 23843 at mode zero: 15.0 C and 69964 Pa, the
        // datasheet's worked example.
        let (temperature, pressure) =
            compensate(&datasheet_calibration(), 27898, 23843, 0);
        assert_eq!(temperature, 150);
        assert_eq!(pressure, 69964);
    }

    #[test]
    fn test_mode_table_matches_the_c_table() {
        assert_eq!(PRESSURE_COMMANDS.len(), 4);
        assert_eq!(PRESSURE_COMMANDS[0].command, 0x34);
        assert_eq!(PRESSURE_COMMANDS[3].mode, 3);
        assert_eq!(PRESSURE_COMMANDS[3].udelay, 25_500);
        assert_eq!(MODE_STANDARD, 1);
        assert_eq!(CALIBRATION_LAST - CALIBRATION_FIRST, 21);
    }

    /// Scripted bus answering the calibration registers.
    struct Scripted {
        reads: Vec<u8>,
        values: Vec<u16>,
        block: u64,
    }

    impl DriverTransport for Scripted {
        fn receive(&mut self, _msg: &mut minix_types::Message) -> Result<(), i32> {
            Err(-minix_types::EIO)
        }
        fn send(&mut self, _dst: Endpoint, _msg: &mut minix_types::Message) -> Result<(), i32> {
            Ok(())
        }
        fn send_rec(&mut self, _dst: Endpoint, msg: &mut minix_types::Message) -> Result<(), i32> {
            assert_eq!(msg.m_type, client::BUSC_I2C_EXEC);
            let block = self.block as *mut client::ExecBlock;
            // SAFETY: the test granted its own exec block.
            let block = unsafe { &mut *block };
            assert_eq!(block.op, client::I2C_OP_READ_WITH_STOP);
            self.reads.push(block.cmd[0]);
            let value = self.values.remove(0);
            block.buf[..2].copy_from_slice(&value.to_be_bytes());
            msg.m_type = 0;
            Ok(())
        }
        fn asynsend(&mut self, _dst: Endpoint, _msg: &mut minix_types::Message) -> Result<(), i32> {
            Ok(())
        }
        fn copy_from_grant(
            &mut self,
            _granter: Endpoint,
            _grant: i32,
            _offset: u64,
            _buf: &mut [u8],
        ) -> Result<(), i32> {
            Ok(())
        }
        fn copy_to_grant(
            &mut self,
            _granter: Endpoint,
            _grant: i32,
            _offset: u64,
            _buf: &[u8],
        ) -> Result<(), i32> {
            Ok(())
        }
        fn publish_label(&mut self, _label: &str) -> Result<(), i32> {
            Ok(())
        }
        fn grant_direct(
            &mut self,
            _dest: Endpoint,
            address: u64,
            _len: usize,
            _write: bool,
        ) -> Result<i32, i32> {
            self.block = address;
            Ok(1)
        }
        fn grant_revoke(&mut self, _grant: i32) -> Result<(), i32> {
            Ok(())
        }
    }

    #[test]
    fn test_calibration_reads_eleven_registers_in_order() {
        let values = vec![408u16, (-72i16) as u16, (-14386i16) as u16, 32741, 32757, 23153, 6190, 4, (-32768i16) as u16, (-8711i16) as u16, 2868];
        let mut bus = Scripted {
            reads: Vec::new(),
            values,
            block: 0,
        };
        let calibration = Calibration::read(&mut bus, Endpoint(42), 0x77).unwrap();
        assert_eq!(calibration, datasheet_calibration());
        assert_eq!(
            bus.reads,
            vec![0xAA, 0xAC, 0xAE, 0xB0, 0xB2, 0xB4, 0xB6, 0xB8, 0xBA, 0xBC, 0xBE]
        );
    }

    #[test]
    fn test_calibration_rejects_zero_and_all_ones() {
        let mut values = vec![408u16, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1];
        let mut bus = Scripted {
            reads: Vec::new(),
            values: values.clone(),
            block: 0,
        };
        assert_eq!(
            Calibration::read(&mut bus, Endpoint(42), 0x77),
            Err(-minix_types::EIO)
        );
        values[1] = 0xFFFF;
        let mut bus = Scripted {
            reads: Vec::new(),
            values,
            block: 0,
        };
        assert_eq!(
            Calibration::read(&mut bus, Endpoint(42), 0x77),
            Err(-minix_types::EIO)
        );
    }
}
