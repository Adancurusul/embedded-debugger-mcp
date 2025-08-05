//! Embedded Debugger MCP Server
//! 
//! A Model Context Protocol server for embedded debugging using probe-rs.
//! Provides AI assistants with comprehensive debugging capabilities for
//! embedded systems including ARM Cortex-M, RISC-V, J-Link, DAPLink, ST-Link, and other debug probes.

pub mod config;
pub mod error;
pub mod utils;
pub mod debugger;
pub mod rtt;
pub mod flash;
pub mod tools;

pub use error::{DebugError, Result};
pub use config::Config;
pub use tools::EmbeddedDebuggerToolHandler;