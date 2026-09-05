//! Device enumeration order: reset, describe, address, configure.
//!
//! C correspondence: the enumeration routine (`hcd_enumerate`,
//! `hcd.c:473-562`), which resets the device (`hcd.c:485`), fixes the
//! maximum packet size (`hcd.c:493`), fetches the device descriptor
//! (`hcd_get_device_descriptor`, `hcd.c:569-613`, standard request
//! "get descriptor" `0x80/0x06`), assigns an address
//! (`hcd_set_address`, `hcd.c:618-666`, "set address" `0x00/0x05`),
//! fetches the full descriptor tree (`hcd_get_descriptor_tree`,
//! `hcd.c:671-800`), and activates a configuration
//! (`hcd_set_configuration`, `hcd.c:806-890`, "set configuration"
//! `0x00/0x09`). Setup packet traffic stays in the service binary;
//! this module owns the order half: which step comes next and which
//! reply advances the sequence.

/// Enumeration stage: how far one device has come (`hcd.c:473-562`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnumStage {
    /// Port just noticed the device; reset comes first.
    Detected,
    /// Reset done; fetching the short device descriptor.
    Describing,
    /// Descriptor known; assigning a bus address.
    Addressing,
    /// Address assigned; fetching the full configuration tree.
    ReadingConfiguration,
    /// Tree known; activating the default configuration.
    Configuring,
    /// Configuration active; the device may be announced.
    Enumerated,
}

/// Enumeration driver: one step at a time, in fixed order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Enumeration {
    stage: EnumStage,
}

impl Enumeration {
    /// A device the port has just noticed.
    pub fn new() -> Self {
        Enumeration { stage: EnumStage::Detected }
    }

    /// Current stage of the sequence.
    pub fn stage(&self) -> EnumStage {
        self.stage
    }

    /// Advance after a successful step; returns false once enumerated.
    pub fn note_success(&mut self) -> bool {
        let next = match self.stage {
            EnumStage::Detected => EnumStage::Describing,
            EnumStage::Describing => EnumStage::Addressing,
            EnumStage::Addressing => EnumStage::ReadingConfiguration,
            EnumStage::ReadingConfiguration => EnumStage::Configuring,
            EnumStage::Configuring => EnumStage::Enumerated,
            EnumStage::Enumerated => return false,
        };
        self.stage = next;
        true
    }

    /// Restart from detection after a step fails (re-reset the device).
    pub fn note_failure(&mut self) {
        self.stage = EnumStage::Detected;
    }

    /// Whether the device may be announced to drivers.
    pub fn is_enumerated(&self) -> bool {
        self.stage == EnumStage::Enumerated
    }
}

impl Default for Enumeration {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enumeration_starts_at_detection() {
        let seq = Enumeration::new();
        assert_eq!(seq.stage(), EnumStage::Detected);
        assert!(!seq.is_enumerated());
    }

    #[test]
    fn test_enumeration_reaches_announce_in_five_steps() {
        let mut seq = Enumeration::new();
        let expected = [
            EnumStage::Describing,
            EnumStage::Addressing,
            EnumStage::ReadingConfiguration,
            EnumStage::Configuring,
            EnumStage::Enumerated,
        ];
        for stage in expected {
            assert!(seq.note_success());
            assert_eq!(seq.stage(), stage);
        }
        assert!(seq.is_enumerated());
        assert!(!seq.note_success());
    }

    #[test]
    fn test_failure_restarts_from_detection() {
        let mut seq = Enumeration::new();
        seq.note_success();
        seq.note_success();
        seq.note_failure();
        assert_eq!(seq.stage(), EnumStage::Detected);
        assert!(!seq.is_enumerated());
    }
}
