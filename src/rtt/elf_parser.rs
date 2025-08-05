//! ELF symbol parsing for RTT control block detection
//! Based on probe-rs implementation analysis

use crate::error::{DebugError, Result};
use std::path::Path;
use tracing::{debug, info, warn};

/// RTT symbol name as defined by SEGGER RTT implementation
const RTT_SYMBOL_NAME: &str = "_SEGGER_RTT";

/// Extract RTT control block address from ELF symbol table
/// This is the primary method used by probe-rs for RTT detection
pub fn get_rtt_symbol_from_elf(elf_path: &Path) -> Result<u64> {
    debug!("Parsing ELF file for RTT symbol: {}", elf_path.display());
    
    // Read ELF file
    let elf_data = std::fs::read(elf_path).map_err(|e| {
        DebugError::RttError(format!("Failed to read ELF file {}: {}", elf_path.display(), e))
    })?;
    
    // Parse ELF structure
    let elf = goblin::elf::Elf::parse(&elf_data).map_err(|e| {
        DebugError::RttError(format!("Failed to parse ELF file {}: {}", elf_path.display(), e))
    })?;
    
    info!("ELF file parsed successfully, searching for {} symbol", RTT_SYMBOL_NAME);
    debug!("ELF info - entry: 0x{:08X}, symbols: {}", elf.entry, elf.syms.len());
    
    // Search for _SEGGER_RTT symbol in symbol table
    for sym in elf.syms.iter() {
        if let Some(name) = elf.strtab.get_at(sym.st_name) {
            debug!("Found symbol: {} at 0x{:08X}", name, sym.st_value);
            
            if name == RTT_SYMBOL_NAME {
                let rtt_address = sym.st_value;
                info!("✅ Found {} symbol at address 0x{:08X}", RTT_SYMBOL_NAME, rtt_address);
                
                // Validate address is reasonable (should be in RAM)
                if is_valid_rtt_address(rtt_address) {
                    return Ok(rtt_address);
                } else {
                    warn!("RTT symbol address 0x{:08X} appears invalid (not in typical RAM range)", rtt_address);
                    return Err(DebugError::RttError(format!(
                        "RTT symbol found at invalid address 0x{:08X} (expected in RAM range 0x20000000-0x2FFFFFFF)", 
                        rtt_address
                    )));
                }
            }
        }
    }
    
    // Symbol not found
    debug!("RTT symbol search completed, {} not found in {} symbols", RTT_SYMBOL_NAME, elf.syms.len());
    Err(DebugError::RttError(format!(
        "{} symbol not found in ELF file {}. Firmware may not have RTT enabled or symbols may be stripped.", 
        RTT_SYMBOL_NAME, 
        elf_path.display()
    )))
}

/// Validate that RTT address is in a reasonable memory range
/// RTT control blocks are typically placed in RAM
fn is_valid_rtt_address(address: u64) -> bool {
    // Common ARM Cortex-M RAM ranges
    // STM32G4: 0x20000000 - 0x2000A000 (SRAM1 + SRAM2)
    // But allow broader range for different MCUs
    const RAM_START: u64 = 0x20000000;
    const RAM_END: u64 = 0x2FFFFFFF;
    
    address >= RAM_START && address <= RAM_END
}

/// Get comprehensive ELF information for debugging
pub fn get_elf_debug_info(elf_path: &Path) -> Result<ElfDebugInfo> {
    let elf_data = std::fs::read(elf_path).map_err(|e| {
        DebugError::RttError(format!("Failed to read ELF file: {}", e))
    })?;
    
    let elf = goblin::elf::Elf::parse(&elf_data).map_err(|e| {
        DebugError::RttError(format!("Failed to parse ELF file: {}", e))
    })?;
    
    let mut debug_symbols = Vec::new();
    for sym in elf.syms.iter() {
        if let Some(name) = elf.strtab.get_at(sym.st_name) {
            if name.contains("RTT") || name.contains("rtt") {
                debug_symbols.push(SymbolInfo {
                    name: name.to_string(),
                    address: sym.st_value,
                    size: sym.st_size,
                });
            }
        }
    }
    
    Ok(ElfDebugInfo {
        entry_point: elf.entry,
        symbol_count: elf.syms.len(),
        has_debug_info: !elf.section_headers.is_empty(),
        rtt_related_symbols: debug_symbols,
    })
}

#[derive(Debug)]
pub struct ElfDebugInfo {
    pub entry_point: u64,
    pub symbol_count: usize,
    pub has_debug_info: bool,
    pub rtt_related_symbols: Vec<SymbolInfo>,
}

#[derive(Debug)]
pub struct SymbolInfo {
    pub name: String,
    pub address: u64,
    pub size: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_valid_rtt_address() {
        // Valid STM32G4 RAM addresses
        assert!(is_valid_rtt_address(0x20000000)); // Start of SRAM1
        assert!(is_valid_rtt_address(0x20008000)); // Start of SRAM2
        assert!(is_valid_rtt_address(0x2000A000)); // End of SRAM2
        
        // Invalid addresses
        assert!(!is_valid_rtt_address(0x08000000)); // Flash
        assert!(!is_valid_rtt_address(0x00000000)); // Null
        assert!(!is_valid_rtt_address(0x40000000)); // Peripherals
    }
}