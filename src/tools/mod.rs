//! Embedded debugger MCP tools module
//! 
//! This module provides a unified tool handler for all embedded debugging operations
//! using the RMCP 0.3.2 API patterns, similar to the serial-mcp-rs implementation.

// Module declarations
pub mod debugger_tools;
pub mod types;

// Export all 18 tools (13 base debugging + 5 RTT communication)
pub use debugger_tools::*;
pub use types::*;