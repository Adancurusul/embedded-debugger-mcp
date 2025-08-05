//! RTT (Real-Time Transfer) communication
//! 
//! This module provides RTT integration using probe-rs for embedded debugging
//! with enhanced ELF symbol detection based on probe-rs implementation analysis.

pub mod manager;
pub mod elf_parser;

// Export RTT components
pub use manager::{RttManager, ChannelInfo, ChannelDirection};
pub use elf_parser::{get_rtt_symbol_from_elf, get_elf_debug_info, ElfDebugInfo, SymbolInfo};