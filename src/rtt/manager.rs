//! RTT manager implementation using probe-rs RTT API

use crate::error::{DebugError, Result};
use std::collections::HashMap;
use std::sync::Arc;
use std::path::Path;
use tokio::sync::Mutex;
use tracing::{debug, info, error, warn};
use probe_rs::{Session, rtt::{Rtt, ScanRegion}, MemoryInterface};

/// RTT manager for hardware communication with embedded targets  
#[derive(Debug)]
pub struct RttManager {
    /// RTT attachment status
    attached: bool,
    /// Real RTT instance from probe-rs
    rtt: Option<Rtt>,
    /// Session reference for RTT operations
    session: Option<Arc<Mutex<Session>>>,
    /// Cached channel information from RTT
    channels: HashMap<u32, ChannelInfo>,
    /// Number of up channels discovered
    up_channel_count: usize,
    /// Number of down channels discovered
    down_channel_count: usize,
}

#[derive(Debug, Clone)]
pub struct ChannelInfo {
    pub id: u32,
    pub name: String,
    pub direction: ChannelDirection,
    pub mode: String,
    pub buffer_size: usize,
}

#[derive(Debug, Clone)]
pub enum ChannelDirection {
    Up,   // Target to Host
    Down, // Host to Target
}

impl Default for RttManager {
    fn default() -> Self {
        Self::new()
    }
}

impl RttManager {
    /// Create a new RTT manager
    pub fn new() -> Self {
        Self {
            attached: false,
            rtt: None,
            session: None,
            channels: HashMap::new(),
            up_channel_count: 0,
            down_channel_count: 0,
        }
    }

    /// Enhanced attach method with ELF symbol detection first (probe-rs style)
    /// This is the recommended method that follows probe-rs best practices
    pub async fn attach_with_elf(
        &mut self,
        session: Arc<Mutex<Session>>,
        firmware_path: &Path,
    ) -> Result<()> {
        info!("Starting enhanced RTT attachment with ELF symbol detection first");
        debug!("Firmware path: {}", firmware_path.display());

        // Phase 1: Try ELF symbol detection (primary method)
        match crate::rtt::elf_parser::get_rtt_symbol_from_elf(firmware_path) {
            Ok(symbol_addr) => {
                info!("✅ Found _SEGGER_RTT symbol at 0x{:08X}, attempting direct connection", symbol_addr);
                
                // Try direct connection at symbol address
                match self.try_rtt_at_address(session.clone(), symbol_addr).await {
                    Ok(_) => {
                        info!("🎯 RTT connected successfully using ELF symbol address!");
                        return Ok(());
                    }
                    Err(e) => {
                        warn!("RTT connection failed at symbol address 0x{:08X}: {}", symbol_addr, e);
                        info!("Falling back to memory scanning...");
                    }
                }
            }
            Err(e) => {
                info!("ELF symbol detection failed: {}", e);
                info!("Proceeding with memory scanning fallback...");
            }
        }

        // Phase 2: Memory scanning fallback (original method)
        info!("Using memory scanning fallback approach");
        self.attach(session, None, None).await
    }

    /// Try RTT connection at specific address (used for ELF symbol detection)
    async fn try_rtt_at_address(
        &mut self,
        session: Arc<Mutex<Session>>,
        address: u64,
    ) -> Result<()> {
        debug!("Attempting RTT connection at specific address: 0x{:08X}", address);

        // Store session reference
        self.session = Some(session.clone());

        let mut session_guard = session.lock().await;
        let mut core = session_guard.core(0).map_err(|e| {
            error!("Failed to get core for RTT attachment: {}", e);
            DebugError::RttError(format!("Failed to get core: {}", e))
        })?;

        // Validate control block at address first
        let is_valid = self.validate_rtt_control_block_sync(&mut core, address)?;
        if !is_valid {
            return Err(DebugError::RttError(format!(
                "Invalid RTT control block at address 0x{:08X} (magic identifier not found)",
                address
            )));
        }
        debug!("✅ RTT control block validated at 0x{:08X}", address);

        // CRITICAL FIX: Use ScanRegion::Exact for direct address connection
        debug!("Using ScanRegion::Exact for direct address connection...");
        
        let scan_region = ScanRegion::Exact(address);
        let rtt_result = Rtt::attach_region(&mut core, &scan_region);

        match rtt_result {
            Ok(rtt) => {
                info!("Successfully attached RTT at ELF symbol address 0x{:08X}!", address);
                self.complete_attachment_sync(rtt)
            }
            Err(e) => {
                error!("RTT attachment failed at address 0x{:08X}: {}", address, e);
                Err(DebugError::RttError(format!(
                    "RTT attachment failed at symbol address 0x{:08X}: {}",
                    address, e
                )))
            }
        }
    }

    /// Validate RTT control block at given address (probe-rs style validation)
    fn validate_rtt_control_block_sync(
        &self,
        core: &mut probe_rs::Core<'_>,
        address: u64,
    ) -> Result<bool> {
        debug!("Validating RTT control block at address 0x{:08X}", address);

        // Read 16 bytes for RTT magic identifier
        let mut id_buffer = [0u8; 16];
        core.read(address, &mut id_buffer).map_err(|e| {
            DebugError::RttError(format!("Failed to read RTT control block at 0x{:08X}: {}", address, e))
        })?;

        // Check for "SEGGER RTT" magic identifier
        const RTT_ID: &[u8] = b"SEGGER RTT\0\0\0\0\0\0";
        let is_valid = id_buffer == RTT_ID;

        if is_valid {
            debug!("✅ Valid RTT control block found at 0x{:08X}", address);
        } else {
            debug!("❌ Invalid RTT control block at 0x{:08X}, found: {:02X?}", address, &id_buffer[..10]);
        }

        Ok(is_valid)
    }

    /// Attach to RTT on target using probe-rs RTT API with enhanced detection
    /// Priority: ELF symbol detection first, then memory scanning fallback
    pub async fn attach(
        &mut self, 
        session: Arc<Mutex<Session>>,
        control_block_address: Option<u64>,
        memory_ranges: Option<Vec<(u64, u64)>>
    ) -> Result<()> {
        debug!("Attaching to RTT using probe-rs integration with enhanced detection");
        
        // Store session reference
        self.session = Some(session.clone());
        
        // Note: memory_map not needed for probe-rs 0.25 attach_region API
        
        // Get the session and core to perform RTT attachment
        let mut session_guard = session.lock().await;
        let mut core = session_guard.core(0).map_err(|e| {
            error!("Failed to get core for RTT attachment: {}", e);
            DebugError::RttError(format!("Failed to get core: {}", e))
        })?;
        
        // Check if target is running (important for RTT initialization)
        let core_status = core.status().map_err(|e| {
            error!("Failed to get core status: {}", e);
            DebugError::RttError(format!("Failed to get core status: {}", e))
        })?;
        debug!("Core status before RTT attach: {:?}", core_status);
        
        // Build ScanRegion based on parameters
        let scan_region = if let Some(cb_addr) = control_block_address {
            info!("RTT scan: Using exact address: 0x{:08X}", cb_addr);
            ScanRegion::Exact(cb_addr)
        } else if let Some(ranges) = memory_ranges {
            info!("RTT scan: Using custom memory ranges: {:?}", ranges);
            let ranges = ranges.into_iter()
                .map(|(start, end)| start..end)
                .collect();
            ScanRegion::Ranges(ranges)
        } else {
            info!("RTT scan: Using RAM scan (probe-rs default)");
            ScanRegion::Ram
        };
        
        // Try RTT attachment with appropriate scan region
        debug!("Attempting RTT attach with scan region: {:?}", scan_region);
        let rtt_result = Rtt::attach_region(&mut core, &scan_region);
        
        match rtt_result {
            Ok(rtt) => {
                info!("Successfully attached to RTT control block!");
                self.complete_attachment_sync(rtt)
            }
            Err(e) => {
                error!("RTT attachment failed: {}", e);
                
                // Provide detailed debugging information
                let detailed_error = format!(
                    "RTT attachment failed: {}\n\n\
                    Debug Information:\n\
                    - Core Status: {:?}\n\
                    - Scan Region: {:?}\n\
                    - Control Block Address: {:?}\n\n\
                    Common Solutions:\n\
                    - Make sure RTT is initialized on the target (defmt-rtt or rtt-target)\n\
                    - Ensure target is running (not halted) during RTT initialization\n\
                    - Check that firmware has sufficient time to initialize RTT\n\
                    - Verify memory regions contain RTT control block\n\
                    - For defmt: ensure defmt-rtt feature is enabled in firmware", 
                    e, 
                    core_status,
                    scan_region,
                    control_block_address
                );
                
                Err(DebugError::RttError(detailed_error))
            }
        }
    }
    
    /// Complete RTT attachment by discovering channels (synchronous version)
    fn complete_attachment_sync(&mut self, mut rtt: Rtt) -> Result<()> {
        // Clear any previous state
        self.channels.clear();
        
        // Discover up channels (target to host)
        let up_channels = rtt.up_channels();
        self.up_channel_count = up_channels.len();
        for i in 0..up_channels.len() {
            if let Some(up_channel) = up_channels.get(i) {
                let channel_info = ChannelInfo {
                    id: i as u32,
                    name: up_channel.name().unwrap_or(&format!("Up{}", i)).to_string(),
                    direction: ChannelDirection::Up,
                    mode: "RTT".to_string(), // Simplified as mode() requires &mut Core
                    buffer_size: up_channel.buffer_size(),
                };
                self.channels.insert(i as u32, channel_info);
                debug!("Discovered up channel {}: {} (size: {} bytes)", 
                       i, up_channel.name().unwrap_or("unnamed"), up_channel.buffer_size());
            }
        }
        
        // Discover down channels (host to target)
        let down_channels = rtt.down_channels();
        self.down_channel_count = down_channels.len();
        for i in 0..down_channels.len() {
            if let Some(down_channel) = down_channels.get(i) {
                let channel_info = ChannelInfo {
                    id: i as u32,
                    name: down_channel.name().unwrap_or(&format!("Down{}", i)).to_string(),
                    direction: ChannelDirection::Down,
                    mode: "RTT".to_string(), // Simplified as mode() requires &mut Core
                    buffer_size: down_channel.buffer_size(),
                };
                // Use offset for down channels to avoid ID conflicts
                self.channels.insert(1000 + i as u32, channel_info);
                debug!("Discovered down channel {}: {} (size: {} bytes)", 
                       i, down_channel.name().unwrap_or("unnamed"), down_channel.buffer_size());
            }
        }
        
        // Store the RTT instance
        self.rtt = Some(rtt);
        self.attached = true;
        
        info!("RTT attachment completed: {} up channels, {} down channels", 
              self.up_channel_count, self.down_channel_count);
        Ok(())
    }

    /// Detach from RTT
    pub async fn detach(&mut self) -> Result<()> {
        debug!("Detaching from RTT");
        
        self.attached = false;
        self.rtt = None;
        self.session = None;
        self.channels.clear();
        self.up_channel_count = 0;
        self.down_channel_count = 0;
        
        info!("RTT detached successfully");
        Ok(())
    }

    /// Read from RTT up channel using probe-rs RTT API
    pub async fn read_channel(&mut self, channel: u32) -> Result<Vec<u8>> {
        if !self.attached {
            return Err(DebugError::RttError("RTT not attached".to_string()));
        }

        let session = self.session.as_ref()
            .ok_or_else(|| DebugError::RttError("No session available".to_string()))?;
        
        let rtt = self.rtt.as_mut()
            .ok_or_else(|| DebugError::RttError("No RTT instance available".to_string()))?;
        
        // Lock session and get core
        let mut session_guard = session.lock().await;
        let mut core = session_guard.core(0).map_err(|e| {
            DebugError::RttError(format!("Failed to get core: {}", e))
        })?;
        
        // Get the up channel (mutable reference)
        let up_channels = rtt.up_channels();
        let up_channel = up_channels.get_mut(channel as usize)
            .ok_or_else(|| DebugError::RttError(format!("Up channel {} not found", channel)))?;
        
        // Read from RTT channel
        let mut buffer = vec![0u8; 1024]; // Buffer for reading
        match up_channel.read(&mut core, &mut buffer) {
            Ok(bytes_read) => {
                buffer.truncate(bytes_read);
                if bytes_read > 0 {
                    debug!("Read {} bytes from RTT up channel {}", bytes_read, channel);
                }
                Ok(buffer)
            }
            Err(e) => {
                error!("Failed to read from RTT up channel {}: {}", channel, e);
                Err(DebugError::RttError(format!("RTT read failed: {}", e)))
            }
        }
    }

    /// Write to RTT down channel using probe-rs RTT API
    pub async fn write_channel(&mut self, channel: u32, data: &[u8]) -> Result<usize> {
        if !self.attached {
            return Err(DebugError::RttError("RTT not attached".to_string()));
        }

        let session = self.session.as_ref()
            .ok_or_else(|| DebugError::RttError("No session available".to_string()))?;
        
        let rtt = self.rtt.as_mut()
            .ok_or_else(|| DebugError::RttError("No RTT instance available".to_string()))?;
        
        // Lock session and get core
        let mut session_guard = session.lock().await;
        let mut core = session_guard.core(0).map_err(|e| {
            DebugError::RttError(format!("Failed to get core: {}", e))
        })?;
        
        // Get the down channel (mutable reference)
        let down_channels = rtt.down_channels();
        let down_channel = down_channels.get_mut(channel as usize)
            .ok_or_else(|| DebugError::RttError(format!("Down channel {} not found", channel)))?;
        
        // Write to RTT channel
        match down_channel.write(&mut core, data) {
            Ok(bytes_written) => {
                debug!("Wrote {} bytes to RTT down channel {}", bytes_written, channel);
                info!("RTT Write Channel {}: {:?}", channel, String::from_utf8_lossy(&data[..bytes_written]));
                Ok(bytes_written)
            }
            Err(e) => {
                error!("Failed to write to RTT down channel {}: {}", channel, e);
                Err(DebugError::RttError(format!("RTT write failed: {}", e)))
            }
        }
    }

    /// Get information about all RTT channels
    pub fn get_channels(&self) -> Vec<&ChannelInfo> {
        self.channels.values().collect()
    }

    /// Check if RTT is attached
    pub fn is_attached(&self) -> bool {
        self.attached
    }

    /// Get the number of available up channels
    pub fn up_channel_count(&self) -> usize {
        self.up_channel_count
    }

    /// Get the number of available down channels
    pub fn down_channel_count(&self) -> usize {
        self.down_channel_count
    }
}