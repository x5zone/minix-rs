//! Port-access abstraction for network card drivers.
//!
//! C correspondence: `minix3/minix/lib/libnetdriver/portio.c` (193 lines).
//! The C helpers move bytes between grant vectors and hardware ports with
//! `sys_sdevio` and panic when the transfer fails. Hardware port numbers
//! must never leak into the operating-system layer, so this module keeps
//! the copy algorithm (chunk walk across vector elements) behind a trait:
//! the operating-system side describes *what* to move, the board side
//! implements *how* a port moves it.

use super::protocol::NDEV_IOV_MAX;

/// One grant-backed packet fragment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PacketFragment {
    /// Grant identifying the caller buffer (opaque to this crate).
    pub grant: u64,
    /// Bytes available in this fragment.
    pub len: usize,
    /// Offset of this fragment's first byte in the packet.
    pub base: usize,
}

impl PacketFragment {
    /// True when a vector fits the C vector bound (`NDEV_IOV_MAX`).
    ///
    /// C: `iovec[NDEV_IOV_MAX]` in the library-internal data structure
    /// (`libnetdriver/netdriver.h`): longer vectors are never built.
    pub fn vector_fits(fragments: &[PacketFragment]) -> bool {
        fragments.len() <= NDEV_IOV_MAX
    }
}

/// Direction of a port transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortDirection {
    /// Hardware to caller buffer (port input).
    In,
    /// Caller buffer to hardware (port output).
    Out,
}

/// Width of one port transfer unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortWidth {
    /// One byte per transfer.
    Byte,
    /// Two bytes per transfer.
    Word,
}

/// Error of a port transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortError {
    /// The request runs past the end of the packet.
    OutOfRange,
    /// The board refused the transfer.
    BoardFault,
}

/// Port-access behavior for one board.
///
/// The default chunk-walk helper [`PortIo::chunk_plan`] implements the
/// `netdriver_portb` loop shape (`portio.c`): resolve the starting element
/// from a flat offset, then walk element by element. Board implementations
/// only provide [`PortIo::transfer_chunk`]; the walk itself is shared.
pub trait PortIo {
    /// Move one contiguous chunk between a port and one fragment.
    fn transfer_chunk(
        &mut self,
        direction: PortDirection,
        width: PortWidth,
        port: u32,
        fragment: &PacketFragment,
        fragment_offset: usize,
        length: usize,
    ) -> Result<(), PortError>;

    /// Walk a flat packet range across fragments, calling
    /// [`PortIo::transfer_chunk`] per element.
    ///
    /// C: the `while (size > 0)` loop in `netdriver_portb` (`portio.c`).
    /// An empty length succeeds without touching the board; a range past
    /// the packet end fails before any transfer, so a partial walk can
    /// never leave half a packet moved. The eight parameters mirror the C
    /// helper signature so the walk stays comparable line-for-line.
    #[allow(clippy::too_many_arguments)]
    fn transfer(
        &mut self,
        direction: PortDirection,
        width: PortWidth,
        port: u32,
        fragments: &[PacketFragment],
        packet_size: usize,
        offset: usize,
        length: usize,
    ) -> Result<(), PortError> {
        if length == 0 {
            return Ok(());
        }
        if offset.saturating_add(length) > packet_size {
            return Err(PortError::OutOfRange);
        }
        let mut remaining = length;
        let mut cursor = offset;
        while remaining > 0 {
            let (fragment, inner) = locate(fragments, cursor).ok_or(PortError::OutOfRange)?;
            let available = fragment.len.saturating_sub(inner);
            let chunk = remaining.min(available);
            if chunk == 0 {
                return Err(PortError::OutOfRange);
            }
            self.transfer_chunk(direction, width, port, fragment, inner, chunk)?;
            cursor += chunk;
            remaining -= chunk;
        }
        Ok(())
    }
}

/// Find the fragment holding a flat packet offset.
fn locate(fragments: &[PacketFragment], offset: usize) -> Option<(&PacketFragment, usize)> {
    let mut cursor = offset;
    for fragment in fragments {
        if cursor < fragment.len {
            return Some((fragment, cursor));
        }
        cursor -= fragment.len;
    }
    None
}

/// In-memory board for tests: records transfers instead of touching ports.
#[derive(Debug, Default)]
pub struct RecordingBoard {
    /// One entry per chunk: direction, width, port, length.
    pub log: alloc::vec::Vec<(PortDirection, PortWidth, u32, usize)>,
    /// When true, every chunk fails with a board fault.
    pub fail: bool,
}

impl RecordingBoard {
    /// Fresh board that accepts every transfer.
    pub fn new() -> RecordingBoard {
        RecordingBoard {
            log: alloc::vec::Vec::new(),
            fail: false,
        }
    }
}

impl PortIo for RecordingBoard {
    fn transfer_chunk(
        &mut self,
        direction: PortDirection,
        width: PortWidth,
        port: u32,
        _fragment: &PacketFragment,
        _fragment_offset: usize,
        length: usize,
    ) -> Result<(), PortError> {
        if self.fail {
            return Err(PortError::BoardFault);
        }
        self.log.push((direction, width, port, length));
        Ok(())
    }
}

/// Discarding board: accepts every chunk without recording.
///
/// Behavior differs from [`RecordingBoard`] (which logs for inspection):
/// this board uses no memory per chunk, modeling production boards whose
/// port access needs no trace. Out-of-range requests are still refused by
/// the shared [`PortIo::transfer`] walk before any chunk runs.
#[derive(Debug, Default, Clone, Copy)]
pub struct DiscardBoard;

impl PortIo for DiscardBoard {
    fn transfer_chunk(
        &mut self,
        _direction: PortDirection,
        _width: PortWidth,
        _port: u32,
        _fragment: &PacketFragment,
        _fragment_offset: usize,
        _length: usize,
    ) -> Result<(), PortError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_fragments() -> [PacketFragment; 2] {
        [
            PacketFragment {
                grant: 1,
                len: 10,
                base: 0,
            },
            PacketFragment {
                grant: 2,
                len: 10,
                base: 10,
            },
        ]
    }

    #[test]
    fn test_single_fragment_transfer_is_one_chunk() {
        let mut board = RecordingBoard::new();
        let fragments = two_fragments();
        board
            .transfer(
                PortDirection::Out,
                PortWidth::Byte,
                0x300,
                &fragments,
                20,
                0,
                10,
            )
            .unwrap();
        assert_eq!(board.log.len(), 1);
        assert_eq!(
            board.log[0],
            (PortDirection::Out, PortWidth::Byte, 0x300, 10)
        );
    }

    #[test]
    fn test_spanning_transfer_walks_two_fragments() {
        let mut board = RecordingBoard::new();
        let fragments = two_fragments();
        board
            .transfer(
                PortDirection::In,
                PortWidth::Word,
                0x300,
                &fragments,
                20,
                5,
                10,
            )
            .unwrap();
        assert_eq!(board.log.len(), 2);
        assert_eq!(board.log[0].3, 5);
        assert_eq!(board.log[1].3, 5);
    }

    #[test]
    fn test_overrun_fails_before_any_transfer() {
        let mut board = RecordingBoard::new();
        let fragments = two_fragments();
        let result = board.transfer(
            PortDirection::Out,
            PortWidth::Byte,
            0x300,
            &fragments,
            20,
            15,
            10,
        );
        assert_eq!(result, Err(PortError::OutOfRange));
        assert!(board.log.is_empty());
    }

    #[test]
    fn test_board_fault_propagates() {
        let mut board = RecordingBoard::new();
        board.fail = true;
        let fragments = two_fragments();
        let result = board.transfer(
            PortDirection::Out,
            PortWidth::Byte,
            0x300,
            &fragments,
            20,
            0,
            4,
        );
        assert_eq!(result, Err(PortError::BoardFault));
    }

    #[test]
    fn test_vector_bound_matches_c_header() {
        assert_eq!(super::super::protocol::NDEV_IOV_MAX, 8);
        assert!(PacketFragment::vector_fits(&two_fragments()));
        let long = [two_fragments()[0]; 9];
        assert!(!PacketFragment::vector_fits(&long));
    }

    #[test]
    fn test_discarding_board_accepts_without_recording() {
        let mut board = DiscardBoard;
        let fragments = two_fragments();
        board
            .transfer(
                PortDirection::Out,
                PortWidth::Byte,
                0x300,
                &fragments,
                20,
                0,
                20,
            )
            .unwrap();
        let result = board.transfer(
            PortDirection::Out,
            PortWidth::Byte,
            0x300,
            &fragments,
            20,
            19,
            2,
        );
        assert_eq!(result, Err(PortError::OutOfRange));
    }
}
