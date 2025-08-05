//! Utility functions and helper types for the debugger MCP server

/// Probe type enumeration
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeType {
    JLink,
    DapLink,
    StLink,
    Blackmagic,
    Ftdi,
    Unknown,
}

impl ProbeType {
    /// Detect probe type from vendor and product IDs
    pub fn from_vid_pid(vendor_id: u16, product_id: u16) -> Self {
        match (vendor_id, product_id) {
            (0x1366, _) => ProbeType::JLink,
            (0x0D28, _) => ProbeType::DapLink,
            (0x0483, 0x374B) | (0x0483, 0x3748) | (0x0483, 0x374A) => ProbeType::StLink,
            (0x1D50, 0x6018) => ProbeType::Blackmagic,
            (0x0403, _) => ProbeType::Ftdi,
            _ => ProbeType::Unknown,
        }
    }
}

impl std::fmt::Display for ProbeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProbeType::JLink => write!(f, "J-Link"),
            ProbeType::DapLink => write!(f, "DAPLink"),
            ProbeType::StLink => write!(f, "ST-Link"),
            ProbeType::Blackmagic => write!(f, "Black Magic Probe"),
            ProbeType::Ftdi => write!(f, "FTDI"),
            ProbeType::Unknown => write!(f, "Unknown"),
        }
    }
}