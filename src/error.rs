//! Error types for the debugger MCP server

use thiserror::Error;

/// Main error type for the debugger MCP server
#[derive(Error, Debug)]
pub enum DebugError {
    #[error("Probe not found: {0}")]
    ProbeNotFound(String),

    #[error("Target not supported: {0}")]
    TargetNotSupported(String),

    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Invalid session ID: {0}")]
    InvalidSession(String),

    #[error("Session limit exceeded (max: {0})")]
    SessionLimitExceeded(usize),

    #[error("Operation timeout")]
    OperationTimeout,

    #[error("Invalid address: 0x{0:08x}")]
    InvalidAddress(u64),

    #[error("Memory access failed: {0}")]
    MemoryAccessFailed(String),

    #[error("Breakpoint limit exceeded")]
    BreakpointLimitExceeded,

    #[error("RTT not available")]
    RttNotAvailable,

    #[error("RTT error: {0}")]
    RttError(String),

    #[error("Flash operation failed: {0}")]
    FlashOperationFailed(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Probe error: {0}")]
    ProbeError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Internal error: {0}")]
    InternalError(String),
}

impl From<probe_rs::Error> for DebugError {
    fn from(error: probe_rs::Error) -> Self {
        DebugError::ProbeError(error.to_string())
    }
}

impl From<anyhow::Error> for DebugError {
    fn from(error: anyhow::Error) -> Self {
        DebugError::InternalError(error.to_string())
    }
}

/// Result type alias for convenience
pub type Result<T> = std::result::Result<T, DebugError>;

/// RTT specific errors
#[derive(Error, Debug)]
pub enum RttError {
    #[error("Control block not found")]
    ControlBlockNotFound,

    #[error("Channel not found: {0}")]
    ChannelNotFound(usize),

    #[error("Buffer overflow")]
    BufferOverflow,

    #[error("Attach failed: {0}")]
    AttachFailed(String),

    #[error("Read timeout")]
    ReadTimeout,
}

impl From<RttError> for DebugError {
    fn from(error: RttError) -> Self {
        DebugError::RttError(error.to_string())
    }
}

/// Target control errors
#[derive(Error, Debug)]
pub enum TargetError {
    #[error("Target not halted")]
    TargetNotHalted,

    #[error("Target not running")]
    TargetNotRunning,

    #[error("Reset failed: {0}")]
    ResetFailed(String),

    #[error("Halt failed: {0}")]
    HaltFailed(String),

    #[error("Step failed: {0}")]
    StepFailed(String),
}

impl From<TargetError> for DebugError {
    fn from(error: TargetError) -> Self {
        DebugError::InternalError(error.to_string())
    }
}

/// Memory operation errors
#[derive(Error, Debug)]
pub enum MemoryError {
    #[error("Read failed at address 0x{address:08x}: {reason}")]
    ReadFailed { address: u64, reason: String },

    #[error("Write failed at address 0x{address:08x}: {reason}")]
    WriteFailed { address: u64, reason: String },

    #[error("Invalid memory range: 0x{start:08x}-0x{end:08x}")]
    InvalidRange { start: u64, end: u64 },

    #[error("Memory region not accessible: 0x{address:08x}")]
    NotAccessible { address: u64 },
}

impl From<MemoryError> for DebugError {
    fn from(error: MemoryError) -> Self {
        DebugError::MemoryAccessFailed(error.to_string())
    }
}

/// Flash operation errors
#[derive(Error, Debug)]
pub enum FlashError {
    #[error("Flash erase failed: {0}")]
    EraseFailed(String),

    #[error("Flash program failed: {0}")]
    ProgramFailed(String),

    #[error("Flash verify failed: {0}")]
    VerifyFailed(String),

    #[error("Flash not writable")]
    NotWritable,

    #[error("Invalid flash address: 0x{0:08x}")]
    InvalidAddress(u64),
}

impl From<FlashError> for DebugError {
    fn from(error: FlashError) -> Self {
        DebugError::FlashOperationFailed(error.to_string())
    }
}