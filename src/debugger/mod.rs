//! Debugger session management

pub mod discovery;

/// Configuration for a debug session
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// Connection speed in kHz
    pub speed_khz: u32,
    /// Whether to connect under reset
    pub connect_under_reset: bool,
    /// Whether to halt after connecting
    pub halt_after_connect: bool,
}