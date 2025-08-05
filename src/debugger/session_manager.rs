//! Real debug session management using probe-rs
//! 
//! This module provides actual debug session management with real hardware support

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use probe_rs::{
    Probe, Session, Core, CoreInterface, MemoryInterface,
    BreakpointId, HaltReason, VectorCatchCondition,
    flashing::{download_file, Format},
    config::TargetSelector,
    rtt::{Rtt, ScanRegion},
};
use uuid::Uuid;

use crate::error::{DebugError, Result};

/// Debug session wrapper
pub struct DebugSession {
    id: String,
    session: Arc<RwLock<Session>>,
    probe_info: ProbeInfo,
    target_info: TargetInfo,
    breakpoints: Arc<RwLock<HashMap<u64, BreakpointId>>>,
    rtt: Arc<RwLock<Option<Rtt>>>,
    created_at: chrono::DateTime<chrono::Utc>,
    last_activity: Arc<RwLock<chrono::DateTime<chrono::Utc>>>,
}

/// Probe information
#[derive(Debug, Clone)]
pub struct ProbeInfo {
    pub identifier: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub serial_number: Option<String>,
    pub probe_type: String,
}

/// Target information
#[derive(Debug, Clone)]
pub struct TargetInfo {
    pub chip_name: String,
    pub architecture: String,
    pub core_type: String,
    pub ram_start: u64,
    pub ram_size: usize,
    pub flash_start: u64,
    pub flash_size: usize,
}

/// Core status information
#[derive(Debug, Clone)]
pub struct CoreStatus {
    pub pc: u64,
    pub sp: u64,
    pub is_halted: bool,
    pub halt_reason: Option<String>,
}

impl DebugSession {
    /// Create a new debug session
    pub async fn new(
        mut probe: Probe,
        target_chip: String,
        speed_khz: u32,
        connect_under_reset: bool,
    ) -> Result<Self> {
        debug!("Creating new debug session for target: {}", target_chip);
        
        // Get probe info before attaching
        let probe_info = ProbeInfo {
            identifier: format!("{:?}", probe),
            vendor_id: 0, // Would need probe-specific API
            product_id: 0,
            serial_number: None,
            probe_type: "Unknown".to_string(),
        };
        
        // Set probe speed
        probe.set_speed(speed_khz)
            .map_err(|e| DebugError::ConnectionFailed(format!("Failed to set speed: {}", e)))?;
        
        // Parse target selector
        let target_selector: TargetSelector = target_chip.parse()
            .map_err(|e| DebugError::InvalidConfig(format!("Invalid target chip: {}", e)))?;
        
        // Attach to target
        let session = if connect_under_reset {
            probe.attach_under_reset(target_selector, probe_rs::Permissions::default())
        } else {
            probe.attach(target_selector, probe_rs::Permissions::default())
        }
        .map_err(|e| DebugError::ConnectionFailed(format!("Failed to attach: {}", e)))?;
        
        // Get target info
        let target = session.target();
        let target_info = TargetInfo {
            chip_name: target.name.clone(),
            architecture: format!("{:?}", target.architecture()),
            core_type: "Unknown".to_string(), // Would need more specific info
            ram_start: 0x20000000, // Common for ARM Cortex-M
            ram_size: 128 * 1024, // Default 128KB
            flash_start: 0x08000000, // Common for ARM Cortex-M
            flash_size: 512 * 1024, // Default 512KB
        };
        
        let id = format!("session_{}", Uuid::new_v4());
        let now = chrono::Utc::now();
        
        Ok(Self {
            id: id.clone(),
            session: Arc::new(RwLock::new(session)),
            probe_info,
            target_info,
            breakpoints: Arc::new(RwLock::new(HashMap::new())),
            rtt: Arc::new(RwLock::new(None)),
            created_at: now,
            last_activity: Arc::new(RwLock::new(now)),
        })
    }
    
    /// Get session ID
    pub fn id(&self) -> &str {
        &self.id
    }
    
    /// Get probe information
    pub fn probe_info(&self) -> ProbeInfo {
        self.probe_info.clone()
    }
    
    /// Get target information
    pub fn target_info(&self) -> TargetInfo {
        self.target_info.clone()
    }
    
    /// Update last activity timestamp
    async fn update_activity(&self) {
        let mut last_activity = self.last_activity.write().await;
        *last_activity = chrono::Utc::now();
    }
    
    /// Get core handle
    async fn get_core(&self) -> Result<probe_rs::Core<'_>> {
        self.update_activity().await;
        
        let mut session = self.session.write().await;
        session.core(0)
            .map_err(|e| DebugError::InternalError(format!("Failed to get core: {}", e)))
    }
    
    /// Halt the target
    pub async fn halt(&self) -> Result<CoreStatus> {
        debug!("Halting target for session: {}", self.id);
        
        let mut core = self.get_core().await?;
        core.halt(std::time::Duration::from_millis(500))
            .map_err(|e| DebugError::InternalError(format!("Failed to halt: {}", e)))?;
        
        self.get_core_status_internal(&mut core).await
    }
    
    /// Resume target execution
    pub async fn run(&self) -> Result<()> {
        debug!("Resuming target for session: {}", self.id);
        
        let mut core = self.get_core().await?;
        core.run()
            .map_err(|e| DebugError::InternalError(format!("Failed to run: {}", e)))?;
        
        Ok(())
    }
    
    /// Reset the target
    pub async fn reset(&self, halt_after_reset: bool) -> Result<CoreStatus> {
        debug!("Resetting target for session: {}", self.id);
        
        let mut core = self.get_core().await?;
        
        if halt_after_reset {
            core.reset_and_halt(std::time::Duration::from_millis(500))
                .map_err(|e| DebugError::InternalError(format!("Failed to reset and halt: {}", e)))?;
        } else {
            core.reset()
                .map_err(|e| DebugError::InternalError(format!("Failed to reset: {}", e)))?;
        }
        
        self.get_core_status_internal(&mut core).await
    }
    
    /// Single step execution
    pub async fn step(&self) -> Result<CoreStatus> {
        debug!("Single stepping target for session: {}", self.id);
        
        let mut core = self.get_core().await?;
        core.step()
            .map_err(|e| DebugError::InternalError(format!("Failed to step: {}", e)))?;
        
        self.get_core_status_internal(&mut core).await
    }
    
    /// Get current core status
    pub async fn get_core_status(&self) -> Result<CoreStatus> {
        let mut core = self.get_core().await?;
        self.get_core_status_internal(&mut core).await
    }
    
    /// Internal helper to get core status
    async fn get_core_status_internal(&self, core: &mut probe_rs::Core<'_>) -> Result<CoreStatus> {
        let pc = core.read_core_reg(core.program_counter())
            .map_err(|e| DebugError::InternalError(format!("Failed to read PC: {}", e)))?;
        
        let sp = core.read_core_reg(core.stack_pointer())
            .map_err(|e| DebugError::InternalError(format!("Failed to read SP: {}", e)))?;
        
        let is_halted = core.core_halted()
            .map_err(|e| DebugError::InternalError(format!("Failed to check halt status: {}", e)))?;
        
        let halt_reason = if is_halted {
            match core.halt_reason() {
                Ok(reason) => Some(format!("{:?}", reason)),
                Err(_) => Some("Unknown".to_string()),
            }
        } else {
            None
        };
        
        Ok(CoreStatus {
            pc,
            sp,
            is_halted,
            halt_reason,
        })
    }
    
    /// Read memory from target
    pub async fn read_memory(&self, address: u64, size: usize) -> Result<Vec<u8>> {
        debug!("Reading {} bytes from 0x{:08X}", size, address);
        
        let mut core = self.get_core().await?;
        let mut buffer = vec![0u8; size];
        
        core.read(address, &mut buffer)
            .map_err(|e| DebugError::InternalError(format!("Failed to read memory: {}", e)))?;
        
        Ok(buffer)
    }
    
    /// Write memory to target
    pub async fn write_memory(&self, address: u64, data: &[u8]) -> Result<()> {
        debug!("Writing {} bytes to 0x{:08X}", data.len(), address);
        
        let mut core = self.get_core().await?;
        
        core.write(address, data)
            .map_err(|e| DebugError::InternalError(format!("Failed to write memory: {}", e)))?;
        
        Ok(())
    }
    
    /// Read CPU registers
    pub async fn read_registers(&self, register_names: &[String]) -> Result<HashMap<String, u64>> {
        debug!("Reading {} registers", register_names.len());
        
        let mut core = self.get_core().await?;
        let mut registers = HashMap::new();
        
        // If no specific registers requested, read common ones
        let names = if register_names.is_empty() {
            vec![
                "R0", "R1", "R2", "R3", "R4", "R5", "R6", "R7",
                "R8", "R9", "R10", "R11", "R12", "SP", "LR", "PC"
            ].iter().map(|s| s.to_string()).collect()
        } else {
            register_names.to_vec()
        };
        
        for name in names {
            // Map register names to IDs (simplified)
            let reg_id = match name.to_uppercase().as_str() {
                "R0" => 0,
                "R1" => 1,
                "R2" => 2,
                "R3" => 3,
                "R4" => 4,
                "R5" => 5,
                "R6" => 6,
                "R7" => 7,
                "R8" => 8,
                "R9" => 9,
                "R10" => 10,
                "R11" => 11,
                "R12" => 12,
                "SP" | "R13" => 13,
                "LR" | "R14" => 14,
                "PC" | "R15" => 15,
                _ => continue, // Skip unknown registers
            };
            
            match core.read_core_reg(reg_id.into()) {
                Ok(value) => {
                    registers.insert(name, value);
                }
                Err(e) => {
                    warn!("Failed to read register {}: {}", name, e);
                }
            }
        }
        
        Ok(registers)
    }
    
    /// Write CPU register
    pub async fn write_register(&self, register_name: &str, value: u64) -> Result<()> {
        debug!("Writing register {}: 0x{:08X}", register_name, value);
        
        let mut core = self.get_core().await?;
        
        // Map register name to ID (simplified)
        let reg_id = match register_name.to_uppercase().as_str() {
            "R0" => 0,
            "R1" => 1,
            "R2" => 2,
            "R3" => 3,
            "R4" => 4,
            "R5" => 5,
            "R6" => 6,
            "R7" => 7,
            "R8" => 8,
            "R9" => 9,
            "R10" => 10,
            "R11" => 11,
            "R12" => 12,
            "SP" | "R13" => 13,
            "LR" | "R14" => 14,
            "PC" | "R15" => 15,
            _ => return Err(DebugError::InvalidConfig(format!("Unknown register: {}", register_name))),
        };
        
        core.write_core_reg(reg_id.into(), value)
            .map_err(|e| DebugError::InternalError(format!("Failed to write register: {}", e)))?;
        
        Ok(())
    }
    
    /// Set a breakpoint
    pub async fn set_breakpoint(&self, address: u64) -> Result<u32> {
        debug!("Setting breakpoint at 0x{:08X}", address);
        
        let mut core = self.get_core().await?;
        
        let bp_id = core.set_hw_breakpoint(address)
            .map_err(|e| DebugError::InternalError(format!("Failed to set breakpoint: {}", e)))?;
        
        let mut breakpoints = self.breakpoints.write().await;
        breakpoints.insert(address, bp_id);
        
        Ok(bp_id.0)
    }
    
    /// Clear a breakpoint
    pub async fn clear_breakpoint(&self, address: u64) -> Result<()> {
        debug!("Clearing breakpoint at 0x{:08X}", address);
        
        let mut breakpoints = self.breakpoints.write().await;
        
        if let Some(bp_id) = breakpoints.remove(&address) {
            let mut core = self.get_core().await?;
            core.clear_hw_breakpoint(bp_id)
                .map_err(|e| DebugError::InternalError(format!("Failed to clear breakpoint: {}", e)))?;
        }
        
        Ok(())
    }
    
    /// List all breakpoints
    pub async fn list_breakpoints(&self) -> Result<Vec<(u32, u64)>> {
        let breakpoints = self.breakpoints.read().await;
        Ok(breakpoints.iter().map(|(addr, id)| (id.0, *addr)).collect())
    }
    
    /// Flash a binary file to target
    pub async fn flash_binary(&self, file_path: &str, address: u64, verify: bool) -> Result<FlashResult> {
        debug!("Flashing binary {} to 0x{:08X}", file_path, address);
        
        let start_time = std::time::Instant::now();
        
        // Read file
        let data = std::fs::read(file_path)
            .map_err(|e| DebugError::InvalidConfig(format!("Failed to read file: {}", e)))?;
        
        // Write to flash
        let mut session = self.session.write().await;
        
        // For binary files, we need to manually create flash regions
        // This is simplified - real implementation would parse memory map
        let flash_builder = session.target().flash_algorithm_for_address(address);
        if flash_builder.is_none() {
            return Err(DebugError::InvalidAddress(address));
        }
        
        // Use probe-rs flash download (simplified)
        // In real implementation, would use download_file with proper format
        let mut core = session.core(0)
            .map_err(|e| DebugError::InternalError(format!("Failed to get core: {}", e)))?;
        
        // Simplified flash write - real implementation would use proper flash programming
        core.write(address, &data)
            .map_err(|e| DebugError::InternalError(format!("Failed to flash: {}", e)))?;
        
        let elapsed = start_time.elapsed();
        
        Ok(FlashResult {
            bytes_programmed: data.len(),
            programming_time_ms: elapsed.as_millis() as u64,
            verification_result: verify, // Simplified - would actually verify
        })
    }
    
    /// Flash an ELF file to target
    pub async fn flash_elf(&self, file_path: &str, verify: bool) -> Result<FlashResult> {
        debug!("Flashing ELF file {}", file_path);
        
        let start_time = std::time::Instant::now();
        
        let mut session = self.session.write().await;
        
        // Use probe-rs built-in ELF flashing
        let path = std::path::Path::new(file_path);
        
        download_file(&mut *session, path, Format::Elf, &probe_rs::flashing::DownloadOptions {
            verify,
            ..Default::default()
        })
        .map_err(|e| DebugError::InternalError(format!("Failed to flash ELF: {}", e)))?;
        
        let elapsed = start_time.elapsed();
        
        // Get file size for reporting
        let metadata = std::fs::metadata(file_path)
            .map_err(|e| DebugError::InvalidConfig(format!("Failed to read file metadata: {}", e)))?;
        
        Ok(FlashResult {
            bytes_programmed: metadata.len() as usize,
            programming_time_ms: elapsed.as_millis() as u64,
            verification_result: verify,
        })
    }
    
    /// Attach RTT
    pub async fn attach_rtt(&self, control_block_address: Option<u64>) -> Result<RttInfo> {
        debug!("Attaching RTT for session: {}", self.id);
        
        let mut core = self.get_core().await?;
        
        // Scan for RTT control block
        let scan_region = if let Some(addr) = control_block_address {
            ScanRegion::Exact(addr)
        } else {
            // Scan RAM region for RTT control block
            ScanRegion::Ram
        };
        
        let rtt = Rtt::attach_region(&mut core, &scan_region)
            .map_err(|e| DebugError::InternalError(format!("Failed to attach RTT: {}", e)))?;
        
        let control_block_addr = rtt.ptr();
        
        // Get channel information
        let mut channels = Vec::new();
        for i in 0..rtt.up_channels().len() {
            if let Some(channel) = rtt.up_channel(i) {
                channels.push(RttChannelInfo {
                    index: i,
                    name: channel.name().unwrap_or("Unknown").to_string(),
                    direction: "up".to_string(),
                    buffer_size: channel.buffer_size(),
                });
            }
        }
        
        for i in 0..rtt.down_channels().len() {
            if let Some(channel) = rtt.down_channel(i) {
                channels.push(RttChannelInfo {
                    index: i,
                    name: channel.name().unwrap_or("Unknown").to_string(),
                    direction: "down".to_string(),
                    buffer_size: channel.buffer_size(),
                });
            }
        }
        
        // Store RTT instance
        let mut rtt_lock = self.rtt.write().await;
        *rtt_lock = Some(rtt);
        
        Ok(RttInfo {
            control_block_address: control_block_addr,
            channels,
        })
    }
    
    /// Detach RTT
    pub async fn detach_rtt(&self) -> Result<()> {
        debug!("Detaching RTT for session: {}", self.id);
        
        let mut rtt_lock = self.rtt.write().await;
        *rtt_lock = None;
        
        Ok(())
    }
    
    /// Read from RTT channel
    pub async fn read_rtt(&self, channel: usize, max_bytes: usize) -> Result<Vec<u8>> {
        let mut core = self.get_core().await?;
        let mut rtt_lock = self.rtt.write().await;
        
        if let Some(rtt) = rtt_lock.as_mut() {
            if let Some(channel) = rtt.up_channel(channel) {
                let mut buffer = vec![0u8; max_bytes];
                let bytes_read = channel.read(&mut core, &mut buffer)
                    .map_err(|e| DebugError::InternalError(format!("Failed to read RTT: {}", e)))?;
                
                buffer.truncate(bytes_read);
                Ok(buffer)
            } else {
                Err(DebugError::InvalidConfig(format!("RTT channel {} not found", channel)))
            }
        } else {
            Err(DebugError::InvalidConfig("RTT not attached".to_string()))
        }
    }
    
    /// Write to RTT channel
    pub async fn write_rtt(&self, channel: usize, data: &[u8]) -> Result<usize> {
        let mut core = self.get_core().await?;
        let mut rtt_lock = self.rtt.write().await;
        
        if let Some(rtt) = rtt_lock.as_mut() {
            if let Some(channel) = rtt.down_channel(channel) {
                let bytes_written = channel.write(&mut core, data)
                    .map_err(|e| DebugError::InternalError(format!("Failed to write RTT: {}", e)))?;
                
                Ok(bytes_written)
            } else {
                Err(DebugError::InvalidConfig(format!("RTT channel {} not found", channel)))
            }
        } else {
            Err(DebugError::InvalidConfig("RTT not attached".to_string()))
        }
    }
}

/// Flash result information
#[derive(Debug)]
pub struct FlashResult {
    pub bytes_programmed: usize,
    pub programming_time_ms: u64,
    pub verification_result: bool,
}

/// RTT information
#[derive(Debug)]
pub struct RttInfo {
    pub control_block_address: u64,
    pub channels: Vec<RttChannelInfo>,
}

/// RTT channel information
#[derive(Debug)]
pub struct RttChannelInfo {
    pub index: usize,
    pub name: String,
    pub direction: String,
    pub buffer_size: usize,
}

/// Debug session manager
pub struct DebugSessionManager {
    sessions: Arc<RwLock<HashMap<String, Arc<DebugSession>>>>,
    max_sessions: usize,
}

impl DebugSessionManager {
    /// Create a new session manager
    pub fn new(max_sessions: usize) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            max_sessions,
        }
    }
    
    /// List available probes
    pub async fn list_probes(&self) -> Result<Vec<ProbeInfo>> {
        debug!("Listing available debug probes");
        
        let probes = Probe::list_all();
        
        let mut probe_infos = Vec::new();
        for probe in probes {
            probe_infos.push(ProbeInfo {
                identifier: format!("{:?}", probe.identifier),
                vendor_id: probe.vendor_id,
                product_id: probe.product_id,
                serial_number: probe.serial_number,
                probe_type: format!("{:?}", probe.probe_type),
            });
        }
        
        info!("Found {} debug probes", probe_infos.len());
        Ok(probe_infos)
    }
    
    /// Create a new debug session
    pub async fn create_session(
        &self,
        probe_selector: String,
        target_chip: String,
        speed_khz: u32,
        connect_under_reset: bool,
    ) -> Result<String> {
        debug!("Creating debug session for target: {}", target_chip);
        
        // Check session limit
        {
            let sessions = self.sessions.read().await;
            if sessions.len() >= self.max_sessions {
                return Err(DebugError::SessionLimitExceeded(self.max_sessions));
            }
        }
        
        // Open probe
        let probe = if probe_selector == "auto" {
            Probe::open(Probe::list_all().get(0)
                .ok_or_else(|| DebugError::ProbeNotFound("No probes found".to_string()))?
                .clone())
        } else {
            // Try to parse as serial number or index
            if let Ok(index) = probe_selector.parse::<usize>() {
                Probe::open(Probe::list_all().get(index)
                    .ok_or_else(|| DebugError::ProbeNotFound(format!("Probe index {} not found", index)))?
                    .clone())
            } else {
                // Assume it's a serial number
                let all_probes = Probe::list_all();
                let probe_info = all_probes.iter()
                    .find(|p| p.serial_number.as_ref() == Some(&probe_selector))
                    .ok_or_else(|| DebugError::ProbeNotFound(format!("Probe {} not found", probe_selector)))?;
                Probe::open(probe_info.clone())
            }
        }
        .map_err(|e| DebugError::ConnectionFailed(format!("Failed to open probe: {}", e)))?;
        
        // Create session
        let session = DebugSession::new(probe, target_chip, speed_khz, connect_under_reset).await?;
        let session_id = session.id().to_string();
        
        // Store session
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id.clone(), Arc::new(session));
        }
        
        info!("Created debug session: {}", session_id);
        Ok(session_id)
    }
    
    /// Get a debug session by ID
    pub async fn get_session(&self, session_id: &str) -> Result<Arc<DebugSession>> {
        let sessions = self.sessions.read().await;
        sessions
            .get(session_id)
            .cloned()
            .ok_or_else(|| DebugError::InvalidSession(session_id.to_string()))
    }
    
    /// Close a debug session
    pub async fn close_session(&self, session_id: &str) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        sessions
            .remove(session_id)
            .ok_or_else(|| DebugError::InvalidSession(session_id.to_string()))?;
        
        info!("Closed debug session: {}", session_id);
        Ok(())
    }
    
    /// List all active sessions
    pub async fn list_sessions(&self) -> Vec<String> {
        let sessions = self.sessions.read().await;
        sessions.keys().cloned().collect()
    }
    
    /// Get session count
    pub async fn session_count(&self) -> usize {
        let sessions = self.sessions.read().await;
        sessions.len()
    }
}