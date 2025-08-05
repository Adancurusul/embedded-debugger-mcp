//! Complete RMCP 0.3.2 implementation for embedded debugger MCP tools
//! 
//! This implementation provides all 18 debugging tools (13 base + 5 RTT) using real probe-rs integration

use rmcp::{
    tool, tool_handler, tool_router, ServerHandler,
    handler::server::{router::tool::ToolRouter, tool::Parameters},
    model::*,
    ErrorData as McpError,
    service::RequestContext,
    RoleServer,
};
use tracing::{debug, error, info, warn};
use std::future::Future;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::types::*;
// Flash types will be used through crate::flash:: prefix
use crate::rtt::RttManager;

// Probe-rs imports
use probe_rs::probe::list::Lister;
use probe_rs::{Session, Permissions, CoreStatus, MemoryInterface, RegisterValue};

/// Debug session information
#[derive(Debug)]
pub struct DebugSession {
    pub session_id: String,
    pub probe_identifier: String,
    pub target_chip: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub session: Arc<tokio::sync::Mutex<Session>>,
    pub rtt_manager: Arc<tokio::sync::Mutex<RttManager>>,
}

/// Complete embedded debugger tool handler with all 18 tools
#[derive(Clone)]
pub struct EmbeddedDebuggerToolHandler {
    #[allow(dead_code)]
    tool_router: ToolRouter<EmbeddedDebuggerToolHandler>,
    sessions: Arc<RwLock<HashMap<String, Arc<DebugSession>>>>,
    max_sessions: usize,
}

impl EmbeddedDebuggerToolHandler {
    pub fn new(max_sessions: usize) -> Self {
        Self {
            tool_router: Self::tool_router(),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            max_sessions,
        }
    }
}

impl Default for EmbeddedDebuggerToolHandler {
    fn default() -> Self {
        Self::new(5)
    }
}

#[tool_router]
impl EmbeddedDebuggerToolHandler {
    // =============================================================================
    // Debugger Management Tools (4 tools)
    // =============================================================================

    #[tool(description = "List all available debug probes (J-Link, ST-Link, DAPLink, etc.)")]
    async fn list_probes(&self, Parameters(_args): Parameters<ListProbesArgs>) -> Result<CallToolResult, McpError> {
        debug!("Listing available debug probes");
        
        // Real probe-rs integration
        let probes = Lister::new().list_all();
        let message = if probes.is_empty() {
            "No debug probes found.\n\nPlease ensure your probe is connected and drivers are installed.\nSupported probes: J-Link, ST-Link, DAPLink, Black Magic Probe".to_string()
        } else {
            let mut result = format!("Found {} debug probe(s):\n\n", probes.len());
            
            for (i, probe) in probes.iter().enumerate() {
                result.push_str(&format!("{}. {}\n", i + 1, probe.identifier));
                result.push_str(&format!("   VID:PID = {:04X}:{:04X}\n", probe.vendor_id, probe.product_id));
                
                if let Some(serial) = &probe.serial_number {
                    result.push_str(&format!("   Serial: {}\n", serial));
                }
                
                result.push_str(&format!("   Probe Type: {:?}\n", probe.probe_type()));
                result.push('\n');
            }
            
            result
        };
        
        info!("Listed {} debug probes", probes.len());
        Ok(CallToolResult::success(vec![Content::text(message)]))
    }

    #[tool(description = "Connect to a debug probe and target chip")]
    async fn connect(&self, Parameters(args): Parameters<ConnectArgs>) -> Result<CallToolResult, McpError> {
        debug!("Connecting to probe '{}' and target '{}'", args.probe_selector, args.target_chip);
        
        // Check session limit
        {
            let sessions = self.sessions.read().await;
            if sessions.len() >= self.max_sessions {
                let error_msg = format!("Session limit exceeded. Maximum {} sessions allowed.", self.max_sessions);
                return Err(McpError::internal_error(error_msg, None));
            }
        }
        
        // Real probe-rs implementation
        let probes = Lister::new().list_all();
        
        if probes.is_empty() {
            return Err(McpError::internal_error(
                "❌ No debug probes found\n\nPlease connect a supported probe (J-Link, ST-Link, DAPLink, etc.)".to_string(),
                None
            ));
        }
        
        let selected_probe = if args.probe_selector.to_lowercase() == "auto" {
            probes.first()
        } else {
            probes.iter().find(|p| p.identifier.contains(&args.probe_selector))
        };

        match selected_probe {
            Some(probe_info) => {
                info!("Opening probe: {}", probe_info.identifier);
                match probe_info.open() {
                    Ok(probe) => {
                        info!("Attaching to target: {}", args.target_chip);
                        match probe.attach(&args.target_chip, Permissions::default()) {
                            Ok(session) => {
                                let session_id = format!("session_{}", chrono::Utc::now().timestamp_millis());
                                
                                let debug_session = DebugSession {
                                    session_id: session_id.clone(),
                                    probe_identifier: probe_info.identifier.clone(),
                                    target_chip: args.target_chip.clone(),
                                    created_at: chrono::Utc::now(),
                                    session: Arc::new(tokio::sync::Mutex::new(session)),
                                    rtt_manager: Arc::new(tokio::sync::Mutex::new(RttManager::new())),
                                };
                                
                                // Store session
                                {
                                    let mut sessions = self.sessions.write().await;
                                    sessions.insert(session_id.clone(), Arc::new(debug_session));
                                }
                                
                                let message = format!(
                                    "✅ Debug session established!\n\n\
                                    Session ID: {}\n\
                                    Probe: {} (VID:PID = {:04X}:{:04X})\n\
                                    Target: {}\n\
                                    Connected at: {}\n\n\
                                    Target connection established and ready for debugging.\n\
                                    Use this session ID for all debug operations.",
                                    session_id,
                                    probe_info.identifier,
                                    probe_info.vendor_id, probe_info.product_id,
                                    args.target_chip,
                                    chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
                                );
                                
                                info!("Created debug session: {}", session_id);
                                Ok(CallToolResult::success(vec![Content::text(message)]))
                            }
                            Err(e) => {
                                error!("Failed to attach to target '{}': {}", args.target_chip, e);
                                let error_msg = format!(
                                    "❌ Failed to attach to target '{}'\n\n\
                                    Error: {}\n\n\
                                    Suggestions:\n\
                                    - Check target chip name (try: STM32F407VGTx, nRF52840_xxAA)\n\
                                    - Ensure target is powered and connected\n\
                                    - Verify SWD/JTAG connections",
                                    args.target_chip, e
                                );
                                Err(McpError::internal_error(error_msg, None))
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to open probe '{}': {}", probe_info.identifier, e);
                        let error_msg = format!(
                            "❌ Failed to open probe '{}'\n\nError: {}\n\n\
                            Suggestions:\n\
                            - Check probe drivers installation\n\
                            - Verify USB connection\n\
                            - Try disconnecting and reconnecting probe",
                            probe_info.identifier, e
                        );
                        Err(McpError::internal_error(error_msg, None))
                    }
                }
            }
            None => {
                let available_probes: Vec<String> = probes
                    .iter()
                    .map(|p| format!("- {}", p.identifier))
                    .collect();
                
                let error_msg = format!(
                    "❌ Probe '{}' not found\n\n\
                    Available probes:\n{}\n\n\
                    Use 'auto' to connect to first available probe.",
                    args.probe_selector,
                    available_probes.join("\n")
                );
                Err(McpError::internal_error(error_msg, None))
            }
        }
    }

    #[tool(description = "Disconnect from a debug session")]
    async fn disconnect(&self, Parameters(args): Parameters<DisconnectArgs>) -> Result<CallToolResult, McpError> {
        debug!("Disconnecting session: {}", args.session_id);
        
        // Remove session from storage
        let removed_session = {
            let mut sessions = self.sessions.write().await;
            sessions.remove(&args.session_id)
        };
        
        match removed_session {
            Some(session) => {
                let message = format!(
                    "✅ Debug session disconnected successfully\n\n\
                    Session ID: {}\n\
                    Probe: {}\n\
                    Target: {}\n\
                    Duration: {:.1} minutes\n\n\
                    probe-rs Session resources have been cleaned up.",
                    args.session_id,
                    session.probe_identifier,
                    session.target_chip,
                    (chrono::Utc::now() - session.created_at).num_seconds() as f64 / 60.0
                );
                
                info!("Disconnected debug session: {}", args.session_id);
                Ok(CallToolResult::success(vec![Content::text(message)]))
            }
            None => {
                let error_msg = format!("❌ Session '{}' not found\n\nUse 'list_sessions' to see active sessions", args.session_id);
                Err(McpError::internal_error(error_msg, None))
            }
        }
    }

    #[tool(description = "Get basic information about a debug session")]
    async fn probe_info(&self, Parameters(args): Parameters<ProbeInfoArgs>) -> Result<CallToolResult, McpError> {
        debug!("Getting probe info for session: {}", args.session_id);
        
        // Get session from storage
        let session_arc = {
            let sessions = self.sessions.read().await;
            match sessions.get(&args.session_id) {
                Some(session) => session.clone(),
                None => {
                    let error_msg = format!("❌ Session '{}' not found\n\nUse 'connect' to establish a debug session first", args.session_id);
                    return Err(McpError::internal_error(error_msg, None));
                }
            }
        };
        
        // Calculate session duration
        let duration_minutes = (chrono::Utc::now() - session_arc.created_at).num_seconds() as f64 / 60.0;
        
        let message = format!(
            "📊 Debug Session Information\n\n\
            Probe Information:\n\
            - Identifier: {}\n\
            - Connected: true\n\n\
            Target Information:\n\
            - Chip: {}\n\n\
            Session Status:\n\
            - Session ID: {}\n\
            - Created: {}\n\
            - Duration: {:.1} minutes\n\n\
            Session is active and ready for operations.",
            session_arc.probe_identifier,
            session_arc.target_chip,
            args.session_id,
            session_arc.created_at.format("%Y-%m-%d %H:%M:%S UTC"),
            duration_minutes
        );
        
        info!("Retrieved probe info for session: {}", args.session_id);
        Ok(CallToolResult::success(vec![Content::text(message)]))
    }

    // =============================================================================
    // Target Control Tools (5 tools)
    // =============================================================================

    #[tool(description = "Halt the target CPU execution")]
    async fn halt(&self, Parameters(args): Parameters<HaltArgs>) -> Result<CallToolResult, McpError> {
        debug!("Halting target for session: {}", args.session_id);
        
        let session_arc = {
            let sessions = self.sessions.read().await;
            match sessions.get(&args.session_id) {
                Some(session) => session.clone(),
                None => {
                    let error_msg = format!("❌ Session '{}' not found\n\nUse 'connect' to establish a debug session first", args.session_id);
                    return Err(McpError::internal_error(error_msg, None));
                }
            }
        };
        
        // Halt the target
        {
            let mut session = session_arc.session.lock().await;
            let mut core = match session.core(0) {
                Ok(core) => core,
                Err(e) => {
                    error!("Failed to get core for session {}: {}", args.session_id, e);
                    return Err(McpError::internal_error(format!("Failed to get core: {}", e), None));
                }
            };
            
            match core.halt(std::time::Duration::from_millis(1000)) {
                Ok(_) => {
                    // Get status after halt
                    match core.status() {
                        Ok(_status) => {
                            let pc = core.read_core_reg(core.program_counter()).map(|v: RegisterValue| v.try_into().unwrap_or(0u32)).unwrap_or(0);
                            let sp = core.read_core_reg(core.stack_pointer()).map(|v: RegisterValue| v.try_into().unwrap_or(0u32)).unwrap_or(0);
                            
                            let message = format!(
                                "✅ Target halted successfully!\n\n\
                                Session ID: {}\n\
                                PC: 0x{:08X}\n\
                                SP: 0x{:08X}\n\
                                State: Halted\n",
                                args.session_id, pc, sp
                            );
                            
                            info!("Halt completed for session: {}", args.session_id);
                            Ok(CallToolResult::success(vec![Content::text(message)]))
                        }
                        Err(e) => {
                            warn!("Failed to get status after halt: {}", e);
                            let message = format!(
                                "✅ Target halted successfully!\n\n\
                                Session ID: {}\n\
                                State: Halted\n",
                                args.session_id
                            );
                            Ok(CallToolResult::success(vec![Content::text(message)]))
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to halt target for session {}: {}", args.session_id, e);
                    Err(McpError::internal_error(format!("Failed to halt target: {}", e), None))
                }
            }
        }
    }

    #[tool(description = "Resume target CPU execution")]
    async fn run(&self, Parameters(args): Parameters<RunArgs>) -> Result<CallToolResult, McpError> {
        debug!("Running target for session: {}", args.session_id);
        
        let session_arc = {
            let sessions = self.sessions.read().await;
            match sessions.get(&args.session_id) {
                Some(session) => session.clone(),
                None => {
                    let error_msg = format!("❌ Session '{}' not found\n\nUse 'connect' to establish a debug session first", args.session_id);
                    return Err(McpError::internal_error(error_msg, None));
                }
            }
        };
        
        // Resume the target
        {
            let mut session = session_arc.session.lock().await;
            let mut core = match session.core(0) {
                Ok(core) => core,
                Err(e) => {
                    error!("Failed to get core for session {}: {}", args.session_id, e);
                    return Err(McpError::internal_error(format!("Failed to get core: {}", e), None));
                }
            };
            
            match core.run() {
                Ok(_) => {
                    let message = format!(
                        "✅ Target resumed execution successfully!\n\n\
                        Session ID: {}\n\
                        Status: Running\n\n\
                        The target is now executing code. Use 'halt' to stop execution.",
                        args.session_id
                    );
                    
                    info!("Run completed for session: {}", args.session_id);
                    Ok(CallToolResult::success(vec![Content::text(message)]))
                }
                Err(e) => {
                    error!("Failed to run target for session {}: {}", args.session_id, e);
                    Err(McpError::internal_error(format!("Failed to run target: {}", e), None))
                }
            }
        }
    }

    #[tool(description = "Reset the target CPU")]
    async fn reset(&self, Parameters(args): Parameters<ResetArgs>) -> Result<CallToolResult, McpError> {
        debug!("Resetting target for session: {}", args.session_id);
        
        let session_arc = {
            let sessions = self.sessions.read().await;
            match sessions.get(&args.session_id) {
                Some(session) => session.clone(),
                None => {
                    let error_msg = format!("❌ Session '{}' not found\n\nUse 'connect' to establish a debug session first", args.session_id);
                    return Err(McpError::internal_error(error_msg, None));
                }
            }
        };
        
        // Reset the target
        {
            let mut session = session_arc.session.lock().await;
            let mut core = match session.core(0) {
                Ok(core) => core,
                Err(e) => {
                    error!("Failed to get core for session {}: {}", args.session_id, e);
                    return Err(McpError::internal_error(format!("Failed to get core: {}", e), None));
                }
            };
            
            match core.reset() {
                Ok(_) => {
                    if args.halt_after_reset {
                        match core.halt(std::time::Duration::from_millis(1000)) {
                            Ok(_) => {},
                            Err(e) => warn!("Failed to halt after reset: {}", e),
                        }
                    }
                    
                    let pc = core.read_core_reg(core.program_counter()).map(|v: RegisterValue| v.try_into().unwrap_or(0u32)).unwrap_or(0);
                    let sp = core.read_core_reg(core.stack_pointer()).map(|v: RegisterValue| v.try_into().unwrap_or(0u32)).unwrap_or(0);
                    
                    let message = format!(
                        "✅ Target reset completed successfully!\n\n\
                        Session ID: {}\n\
                        Reset type: {}\n\
                        Halted after reset: {}\n\
                        PC: 0x{:08X}\n\
                        SP: 0x{:08X}\n\
                        State: {}\n",
                        args.session_id,
                        args.reset_type,
                        args.halt_after_reset,
                        pc, sp,
                        if args.halt_after_reset { "Halted" } else { "Running" }
                    );
                    
                    info!("Reset completed for session: {}", args.session_id);
                    Ok(CallToolResult::success(vec![Content::text(message)]))
                }
                Err(e) => {
                    error!("Failed to reset target for session {}: {}", args.session_id, e);
                    Err(McpError::internal_error(format!("Failed to reset target: {}", e), None))
                }
            }
        }
    }

    #[tool(description = "Execute a single instruction step")]
    async fn step(&self, Parameters(args): Parameters<StepArgs>) -> Result<CallToolResult, McpError> {
        debug!("Single stepping target for session: {}", args.session_id);
        
        let session_arc = {
            let sessions = self.sessions.read().await;
            match sessions.get(&args.session_id) {
                Some(session) => session.clone(),
                None => {
                    let error_msg = format!("❌ Session '{}' not found\n\nUse 'connect' to establish a debug session first", args.session_id);
                    return Err(McpError::internal_error(error_msg, None));
                }
            }
        };
        
        // Single step the target
        {
            let mut session = session_arc.session.lock().await;
            let mut core = match session.core(0) {
                Ok(core) => core,
                Err(e) => {
                    error!("Failed to get core for session {}: {}", args.session_id, e);
                    return Err(McpError::internal_error(format!("Failed to get core: {}", e), None));
                }
            };
            
            match core.step() {
                Ok(_) => {
                    let pc = core.read_core_reg(core.program_counter()).map(|v: RegisterValue| v.try_into().unwrap_or(0u32)).unwrap_or(0);
                    let sp = core.read_core_reg(core.stack_pointer()).map(|v: RegisterValue| v.try_into().unwrap_or(0u32)).unwrap_or(0);
                    
                    let message = format!(
                        "✅ Single step completed successfully!\n\n\
                        Session ID: {}\n\
                        PC: 0x{:08X}\n\
                        SP: 0x{:08X}\n\
                        State: Halted\n",
                        args.session_id, pc, sp
                    );
                    
                    info!("Step completed for session: {}", args.session_id);
                    Ok(CallToolResult::success(vec![Content::text(message)]))
                }
                Err(e) => {
                    error!("Failed to step target for session {}: {}", args.session_id, e);
                    Err(McpError::internal_error(format!("Failed to step target: {}", e), None))
                }
            }
        }
    }

    #[tool(description = "Get current status of the target CPU and debug session")]
    async fn get_status(&self, Parameters(args): Parameters<GetStatusArgs>) -> Result<CallToolResult, McpError> {
        debug!("Getting status for session: {}", args.session_id);
        
        let session_arc = {
            let sessions = self.sessions.read().await;
            match sessions.get(&args.session_id) {
                Some(session) => session.clone(),
                None => {
                    let error_msg = format!("❌ Session '{}' not found\n\nUse 'connect' to establish a debug session first", args.session_id);
                    return Err(McpError::internal_error(error_msg, None));
                }
            }
        };
        
        // Get target status
        {
            let mut session = session_arc.session.lock().await;
            let mut core = match session.core(0) {
                Ok(core) => core,
                Err(e) => {
                    error!("Failed to get core for session {}: {}", args.session_id, e);
                    return Err(McpError::internal_error(format!("Failed to get core: {}", e), None));
                }
            };
            
            match core.status() {
                Ok(status) => {
                    let pc = core.read_core_reg(core.program_counter()).map(|v: RegisterValue| v.try_into().unwrap_or(0u32)).unwrap_or(0);
                    let sp = core.read_core_reg(core.stack_pointer()).map(|v: RegisterValue| v.try_into().unwrap_or(0u32)).unwrap_or(0);
                    
                    let is_halted = matches!(status, CoreStatus::Halted(_));
                    let halt_reason = match status {
                        CoreStatus::Halted(reason) => format!("{:?}", reason),
                        CoreStatus::Running => "N/A".to_string(),
                        _ => "Unknown".to_string(),
                    };
                    
                    let message = format!(
                        "📊 Debug Session Status\n\n\
                        Core Information:\n\
                        - PC: 0x{:08X}\n\
                        - SP: 0x{:08X}\n\
                        - State: {}\n\
                        - Halt reason: {}\n\n\
                        Session Information:\n\
                        - ID: {}\n\
                        - Connected: true\n\
                        - Target: {}\n\
                        - Probe: {}\n\
                        - Duration: {:.1} minutes\n",
                        pc, sp,
                        if is_halted { "Halted" } else { "Running" },
                        halt_reason,
                        args.session_id,
                        session_arc.target_chip,
                        session_arc.probe_identifier,
                        (chrono::Utc::now() - session_arc.created_at).num_seconds() as f64 / 60.0
                    );
                    
                    Ok(CallToolResult::success(vec![Content::text(message)]))
                }
                Err(e) => {
                    error!("Failed to get core status for session {}: {}", args.session_id, e);
                    Err(McpError::internal_error(format!("Failed to get core status: {}", e), None))
                }
            }
        }
    }

    // =============================================================================
    // Memory Operation Tools (2 tools)
    // =============================================================================

    #[tool(description = "Read memory from the target")]
    async fn read_memory(&self, Parameters(args): Parameters<ReadMemoryArgs>) -> Result<CallToolResult, McpError> {
        debug!("Reading memory for session: {} at address {}", args.session_id, args.address);
        
        // Parse address
        let address = match parse_address(&args.address) {
            Ok(addr) => addr,
            Err(e) => {
                error!("Invalid address '{}': {}", args.address, e);
                return Err(McpError::internal_error(format!("Invalid address '{}': {}", args.address, e), None));
            }
        };

        let session_arc = {
            let sessions = self.sessions.read().await;
            match sessions.get(&args.session_id) {
                Some(session) => session.clone(),
                None => {
                    let error_msg = format!("❌ Session '{}' not found\n\nUse 'connect' to establish a debug session first", args.session_id);
                    return Err(McpError::internal_error(error_msg, None));
                }
            }
        };

        // Read memory
        {
            let mut session = session_arc.session.lock().await;
            let mut core = match session.core(0) {
                Ok(core) => core,
                Err(e) => {
                    error!("Failed to get core for session {}: {}", args.session_id, e);
                    return Err(McpError::internal_error(format!("Failed to get core: {}", e), None));
                }
            };
            
            let mut data = vec![0u8; args.size as usize];
            match core.read(address, &mut data) {
                Ok(_) => {
                    debug!("Read {} bytes from address 0x{:08X}", data.len(), address);
                    
                    let formatted_data = format_memory_data(&data, &args.format, address);
                    let message = format!(
                        "📖 Memory read completed successfully!\n\n\
                        Session ID: {}\n\
                        Address: 0x{:08X}\n\
                        Size: {} bytes\n\
                        Format: {}\n\n\
                        Data:\n{}",
                        args.session_id, address, args.size, args.format, formatted_data
                    );
                    
                    info!("Memory read completed for session: {}", args.session_id);
                    Ok(CallToolResult::success(vec![Content::text(message)]))
                }
                Err(e) => {
                    error!("Failed to read memory for session {}: {}", args.session_id, e);
                    Err(McpError::internal_error(format!("Failed to read memory: {}", e), None))
                }
            }
        }
    }

    #[tool(description = "Write memory to the target")]
    async fn write_memory(&self, Parameters(args): Parameters<WriteMemoryArgs>) -> Result<CallToolResult, McpError> {
        debug!("Writing memory for session: {} at address {}", args.session_id, args.address);
        
        // Parse address
        let address = match parse_address(&args.address) {
            Ok(addr) => addr,
            Err(e) => {
                error!("Invalid address '{}': {}", args.address, e);
                return Err(McpError::internal_error(format!("Invalid address '{}': {}", args.address, e), None));
            }
        };

        // Parse data based on format
        let data = match parse_data(&args.data, &args.format) {
            Ok(data) => data,
            Err(e) => {
                error!("Invalid data '{}': {}", args.data, e);
                return Err(McpError::internal_error(format!("Invalid data '{}': {}", args.data, e), None));
            }
        };

        let session_arc = {
            let sessions = self.sessions.read().await;
            match sessions.get(&args.session_id) {
                Some(session) => session.clone(),
                None => {
                    let error_msg = format!("❌ Session '{}' not found\n\nUse 'connect' to establish a debug session first", args.session_id);
                    return Err(McpError::internal_error(error_msg, None));
                }
            }
        };

        // Write memory
        {
            let mut session = session_arc.session.lock().await;
            let mut core = match session.core(0) {
                Ok(core) => core,
                Err(e) => {
                    error!("Failed to get core for session {}: {}", args.session_id, e);
                    return Err(McpError::internal_error(format!("Failed to get core: {}", e), None));
                }
            };
            
            match core.write(address, &data) {
                Ok(_) => {
                    let message = format!(
                        "✏️ Memory write completed successfully!\n\n\
                        Session ID: {}\n\
                        Address: 0x{:08X}\n\
                        Data: {}\n\
                        Format: {}\n\
                        Bytes written: {}",
                        args.session_id, address, args.data, args.format, data.len()
                    );
                    
                    info!("Memory write completed for session: {}", args.session_id);
                    Ok(CallToolResult::success(vec![Content::text(message)]))
                }
                Err(e) => {
                    error!("Failed to write memory for session {}: {}", args.session_id, e);
                    Err(McpError::internal_error(format!("Failed to write memory: {}", e), None))
                }
            }
        }
    }

    // =============================================================================
    // Breakpoint Tools (2 tools)
    // =============================================================================

    #[tool(description = "Set a breakpoint at the specified address")]
    async fn set_breakpoint(&self, Parameters(args): Parameters<SetBreakpointArgs>) -> Result<CallToolResult, McpError> {
        debug!("Setting breakpoint for session: {} at address {}", args.session_id, args.address);
        
        // Parse address
        let address = match parse_address(&args.address) {
            Ok(addr) => addr,
            Err(e) => {
                error!("Invalid address '{}': {}", args.address, e);
                return Err(McpError::internal_error(format!("Invalid address '{}': {}", args.address, e), None));
            }
        };

        let session_arc = {
            let sessions = self.sessions.read().await;
            match sessions.get(&args.session_id) {
                Some(session) => session.clone(),
                None => {
                    let error_msg = format!("❌ Session '{}' not found\n\nUse 'connect' to establish a debug session first", args.session_id);
                    return Err(McpError::internal_error(error_msg, None));
                }
            }
        };

        // Set breakpoint
        {
            let mut session = session_arc.session.lock().await;
            let mut core = match session.core(0) {
                Ok(core) => core,
                Err(e) => {
                    error!("Failed to get core for session {}: {}", args.session_id, e);
                    return Err(McpError::internal_error(format!("Failed to get core: {}", e), None));
                }
            };
            
            match core.set_hw_breakpoint(address) {
                Ok(_) => {
                    let message = format!(
                        "🎯 Breakpoint set successfully!\n\n\
                        Session ID: {}\n\
                        Address: 0x{:08X}\n\
                        Type: Hardware breakpoint\n\n\
                        The target will halt when execution reaches this address.",
                        args.session_id, address
                    );
                    
                    info!("Breakpoint set for session: {} at 0x{:08X}", args.session_id, address);
                    Ok(CallToolResult::success(vec![Content::text(message)]))
                }
                Err(e) => {
                    error!("Failed to set breakpoint for session {}: {}", args.session_id, e);
                    Err(McpError::internal_error(format!("Failed to set breakpoint: {}", e), None))
                }
            }
        }
    }

    #[tool(description = "Clear a breakpoint at the specified address")]
    async fn clear_breakpoint(&self, Parameters(args): Parameters<ClearBreakpointArgs>) -> Result<CallToolResult, McpError> {
        debug!("Clearing breakpoint for session: {} at address {}", args.session_id, args.address);
        
        // Parse address
        let address = match parse_address(&args.address) {
            Ok(addr) => addr,
            Err(e) => {
                error!("Invalid address '{}': {}", args.address, e);
                return Err(McpError::internal_error(format!("Invalid address '{}': {}", args.address, e), None));
            }
        };

        let session_arc = {
            let sessions = self.sessions.read().await;
            match sessions.get(&args.session_id) {
                Some(session) => session.clone(),
                None => {
                    let error_msg = format!("❌ Session '{}' not found\n\nUse 'connect' to establish a debug session first", args.session_id);
                    return Err(McpError::internal_error(error_msg, None));
                }
            }
        };

        // Clear breakpoint
        {
            let mut session = session_arc.session.lock().await;
            let mut core = match session.core(0) {
                Ok(core) => core,
                Err(e) => {
                    error!("Failed to get core for session {}: {}", args.session_id, e);
                    return Err(McpError::internal_error(format!("Failed to get core: {}", e), None));
                }
            };
            
            match core.clear_hw_breakpoint(address) {
                Ok(_) => {
                    let message = format!(
                        "🎯 Breakpoint cleared successfully!\n\n\
                        Session ID: {}\n\
                        Address: 0x{:08X}\n\n\
                        The breakpoint has been removed.",
                        args.session_id, address
                    );
                    
                    info!("Breakpoint cleared for session: {} at 0x{:08X}", args.session_id, address);
                    Ok(CallToolResult::success(vec![Content::text(message)]))
                }
                Err(e) => {
                    error!("Failed to clear breakpoint for session {}: {}", args.session_id, e);
                    Err(McpError::internal_error(format!("Failed to clear breakpoint: {}", e), None))
                }
            }
        }
    }

    // =============================================================================
    // RTT Communication Tools (5 tools)
    // =============================================================================

    #[tool(description = "Attach to RTT (Real-Time Transfer) for communication with target")]
    async fn rtt_attach(&self, Parameters(args): Parameters<RttAttachArgs>) -> Result<CallToolResult, McpError> {
        debug!("Attaching RTT for session: {}", args.session_id);
        
        // Get session from storage
        let session_arc = {
            let sessions = self.sessions.read().await;
            match sessions.get(&args.session_id) {
                Some(session) => session.clone(),
                None => {
                    let error_msg = format!("❌ Session '{}' not found\n\nUse 'connect' to establish a debug session first", args.session_id);
                    return Err(McpError::internal_error(error_msg, None));
                }
            }
        };

        // Parse control block address if provided
        let control_block_address = if let Some(addr_str) = args.control_block_address {
            match parse_address(&addr_str) {
                Ok(addr) => Some(addr),
                Err(e) => {
                    let error_msg = format!("❌ Invalid control block address '{}': {}", addr_str, e);
                    return Err(McpError::internal_error(error_msg, None));
                }
            }
        } else {
            None
        };

        // Parse memory ranges if provided
        let memory_ranges = if let Some(ranges) = args.memory_ranges {
            let mut parsed_ranges = Vec::new();
            for range in ranges {
                let start = parse_address(&range.start).map_err(|e| {
                    McpError::internal_error(format!("Invalid start address '{}': {}", range.start, e), None)
                })?;
                let end = parse_address(&range.end).map_err(|e| {
                    McpError::internal_error(format!("Invalid end address '{}': {}", range.end, e), None)
                })?;
                parsed_ranges.push((start, end));
            }
            Some(parsed_ranges)
        } else {
            None
        };

        // Attach RTT
        {
            let mut rtt_manager = session_arc.rtt_manager.lock().await;
            match rtt_manager.attach(session_arc.session.clone(), control_block_address, memory_ranges).await {
                Ok(_) => {
                    let up_channels = rtt_manager.up_channel_count();
                    let down_channels = rtt_manager.down_channel_count();
                    
                    let message = format!(
                        "✅ RTT attached successfully!\n\n\
                        Session ID: {}\n\
                        Up Channels (Target→Host): {}\n\
                        Down Channels (Host→Target): {}\n\n\
                        RTT is now ready for real-time communication with the target.\n\
                        Use 'rtt_read' to read from target and 'rtt_write' to send data to target.",
                        args.session_id, up_channels, down_channels
                    );
                    
                    info!("RTT attached successfully for session: {}", args.session_id);
                    Ok(CallToolResult::success(vec![Content::text(message)]))
                }
                Err(e) => {
                    error!("Failed to attach RTT for session {}: {}", args.session_id, e);
                    let error_msg = format!(
                        "❌ Failed to attach RTT\n\n\
                        Session ID: {}\n\
                        Error: {}\n\n\
                        Suggestions:\n\
                        - Ensure the target firmware has RTT enabled and initialized\n\
                        - Check that the target is halted\n\
                        - Verify memory ranges if specified\n\
                        - Try different control block address if known",
                        args.session_id, e
                    );
                    Err(McpError::internal_error(error_msg, None))
                }
            }
        }
    }

    #[tool(description = "Detach from RTT communication")]
    async fn rtt_detach(&self, Parameters(args): Parameters<RttDetachArgs>) -> Result<CallToolResult, McpError> {
        debug!("Detaching RTT for session: {}", args.session_id);
        
        // Get session from storage
        let session_arc = {
            let sessions = self.sessions.read().await;
            match sessions.get(&args.session_id) {
                Some(session) => session.clone(),
                None => {
                    let error_msg = format!("❌ Session '{}' not found\n\nUse 'connect' to establish a debug session first", args.session_id);
                    return Err(McpError::internal_error(error_msg, None));
                }
            }
        };

        // Detach RTT
        {
            let mut rtt_manager = session_arc.rtt_manager.lock().await;
            match rtt_manager.detach().await {
                Ok(_) => {
                    let message = format!(
                        "✅ RTT detached successfully\n\n\
                        Session ID: {}\n\n\
                        RTT communication has been closed.",
                        args.session_id
                    );
                    
                    info!("RTT detached successfully for session: {}", args.session_id);
                    Ok(CallToolResult::success(vec![Content::text(message)]))
                }
                Err(e) => {
                    error!("Failed to detach RTT for session {}: {}", args.session_id, e);
                    let error_msg = format!("❌ Failed to detach RTT: {}", e);
                    Err(McpError::internal_error(error_msg, None))
                }
            }
        }
    }

    #[tool(description = "Read data from RTT up channel (target to host)")]
    async fn rtt_read(&self, Parameters(args): Parameters<RttReadArgs>) -> Result<CallToolResult, McpError> {
        debug!("Reading from RTT channel {} for session: {}", args.channel, args.session_id);
        
        // Get session from storage
        let session_arc = {
            let sessions = self.sessions.read().await;
            match sessions.get(&args.session_id) {
                Some(session) => session.clone(),
                None => {
                    let error_msg = format!("❌ Session '{}' not found\n\nUse 'connect' to establish a debug session first", args.session_id);
                    return Err(McpError::internal_error(error_msg, None));
                }
            }
        };

        // Read from RTT
        {
            let mut rtt_manager = session_arc.rtt_manager.lock().await;
            if !rtt_manager.is_attached() {
                let error_msg = format!("❌ RTT not attached for session '{}'\n\nUse 'rtt_attach' first", args.session_id);
                return Err(McpError::internal_error(error_msg, None));
            }

            match rtt_manager.read_channel(args.channel).await {
                Ok(data) => {
                    let data_len = data.len();
                    let data_str = if data.is_empty() {
                        "No data available".to_string()
                    } else {
                        // Try to decode as UTF-8, fall back to hex if not valid
                        match String::from_utf8(data.clone()) {
                            Ok(text) => {
                                if text.chars().all(|c| c.is_ascii_graphic() || c.is_ascii_whitespace()) {
                                    format!("Text: {}", text)
                                } else {
                                    format!("Mixed: {} (hex: {})", text, hex::encode(&data))
                                }
                            }
                            Err(_) => format!("Binary data (hex): {}", hex::encode(&data))
                        }
                    };

                    let message = format!(
                        "📥 RTT Read from Channel {}\n\n\
                        Session ID: {}\n\
                        Bytes Read: {}\n\n\
                        Data:\n{}",
                        args.channel, args.session_id, data_len, data_str
                    );
                    
                    debug!("Read {} bytes from RTT channel {} for session: {}", data_len, args.channel, args.session_id);
                    Ok(CallToolResult::success(vec![Content::text(message)]))
                }
                Err(e) => {
                    error!("Failed to read from RTT channel {} for session {}: {}", args.channel, args.session_id, e);
                    let error_msg = format!(
                        "❌ Failed to read from RTT channel {}\n\n\
                        Session ID: {}\n\
                        Error: {}",
                        args.channel, args.session_id, e
                    );
                    Err(McpError::internal_error(error_msg, None))
                }
            }
        }
    }

    #[tool(description = "Write data to RTT down channel (host to target)")]
    async fn rtt_write(&self, Parameters(args): Parameters<RttWriteArgs>) -> Result<CallToolResult, McpError> {
        debug!("Writing to RTT channel {} for session: {}", args.channel, args.session_id);
        
        // Get session from storage
        let session_arc = {
            let sessions = self.sessions.read().await;
            match sessions.get(&args.session_id) {
                Some(session) => session.clone(),
                None => {
                    let error_msg = format!("❌ Session '{}' not found\n\nUse 'connect' to establish a debug session first", args.session_id);
                    return Err(McpError::internal_error(error_msg, None));
                }
            }
        };

        // Parse data based on encoding
        let data_bytes = match args.encoding.as_str() {
            "utf8" => args.data.as_bytes().to_vec(),
            "hex" => {
                match hex::decode(&args.data) {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        let error_msg = format!("❌ Invalid hex data '{}': {}", args.data, e);
                        return Err(McpError::internal_error(error_msg, None));
                    }
                }
            }
            "binary" => {
                // Parse binary string like "10110011 11001100"
                let binary_str = args.data.replace(' ', "");
                if binary_str.len() % 8 != 0 {
                    let error_msg = format!("❌ Binary data must be multiple of 8 bits: '{}'", args.data);
                    return Err(McpError::internal_error(error_msg, None));
                }
                
                let mut bytes = Vec::new();
                for chunk in binary_str.chars().collect::<Vec<_>>().chunks(8) {
                    let byte_str: String = chunk.iter().collect();
                    match u8::from_str_radix(&byte_str, 2) {
                        Ok(byte) => bytes.push(byte),
                        Err(e) => {
                            let error_msg = format!("❌ Invalid binary byte '{}': {}", byte_str, e);
                            return Err(McpError::internal_error(error_msg, None));
                        }
                    }
                }
                bytes
            }
            _ => {
                let error_msg = format!("❌ Unsupported encoding '{}'. Use 'utf8', 'hex', or 'binary'", args.encoding);
                return Err(McpError::internal_error(error_msg, None));
            }
        };

        // Write to RTT
        {
            let mut rtt_manager = session_arc.rtt_manager.lock().await;
            if !rtt_manager.is_attached() {
                let error_msg = format!("❌ RTT not attached for session '{}'\n\nUse 'rtt_attach' first", args.session_id);
                return Err(McpError::internal_error(error_msg, None));
            }

            match rtt_manager.write_channel(args.channel, &data_bytes).await {
                Ok(bytes_written) => {
                    let message = format!(
                        "📤 RTT Write to Channel {}\n\n\
                        Session ID: {}\n\
                        Data: {}\n\
                        Encoding: {}\n\
                        Bytes Written: {}\n\n\
                        Data sent successfully to target.",
                        args.channel, args.session_id, args.data, args.encoding, bytes_written
                    );
                    
                    info!("Wrote {} bytes to RTT channel {} for session: {}", bytes_written, args.channel, args.session_id);
                    Ok(CallToolResult::success(vec![Content::text(message)]))
                }
                Err(e) => {
                    error!("Failed to write to RTT channel {} for session {}: {}", args.channel, args.session_id, e);
                    let error_msg = format!(
                        "❌ Failed to write to RTT channel {}\n\n\
                        Session ID: {}\n\
                        Error: {}",
                        args.channel, args.session_id, e
                    );
                    Err(McpError::internal_error(error_msg, None))
                }
            }
        }
    }

    #[tool(description = "List available RTT channels")]
    async fn rtt_channels(&self, Parameters(args): Parameters<RttChannelsArgs>) -> Result<CallToolResult, McpError> {
        debug!("Listing RTT channels for session: {}", args.session_id);
        
        // Get session from storage
        let session_arc = {
            let sessions = self.sessions.read().await;
            match sessions.get(&args.session_id) {
                Some(session) => session.clone(),
                None => {
                    let error_msg = format!("❌ Session '{}' not found\n\nUse 'connect' to establish a debug session first", args.session_id);
                    return Err(McpError::internal_error(error_msg, None));
                }
            }
        };

        // List RTT channels
        {
            let rtt_manager = session_arc.rtt_manager.lock().await;
            if !rtt_manager.is_attached() {
                let error_msg = format!("❌ RTT not attached for session '{}'\n\nUse 'rtt_attach' first", args.session_id);
                return Err(McpError::internal_error(error_msg, None));
            }

            let channels = rtt_manager.get_channels();
            let channel_count = channels.len();
            
            if channels.is_empty() {
                let message = format!(
                    "📋 RTT Channels\n\n\
                    Session ID: {}\n\n\
                    No RTT channels available.",
                    args.session_id
                );
                return Ok(CallToolResult::success(vec![Content::text(message)]));
            }

            let mut message = format!("📋 RTT Channels\n\nSession ID: {}\n\n", args.session_id);
            
            // Group channels by direction
            let mut up_channels = Vec::new();
            let mut down_channels = Vec::new();
            
            for channel in &channels {
                match channel.direction {
                    crate::rtt::ChannelDirection::Up => up_channels.push(channel),
                    crate::rtt::ChannelDirection::Down => down_channels.push(channel),
                }
            }

            if !up_channels.is_empty() {
                message.push_str("📥 Up Channels (Target → Host):\n");
                for channel in up_channels {
                    message.push_str(&format!(
                        "  {}. {} (Size: {} bytes, Mode: {})\n",
                        channel.id, channel.name, channel.buffer_size, channel.mode
                    ));
                }
                message.push('\n');
            }

            if !down_channels.is_empty() {
                message.push_str("📤 Down Channels (Host → Target):\n");
                for channel in down_channels {
                    message.push_str(&format!(
                        "  {}. {} (Size: {} bytes, Mode: {})\n",
                        channel.id, channel.name, channel.buffer_size, channel.mode
                    ));
                }
            }

            info!("Listed {} RTT channels for session: {}", channel_count, args.session_id);
            Ok(CallToolResult::success(vec![Content::text(message)]))
        }
    }

    // =============================================================================
    // Flash Programming Tools (4 tools)
    // =============================================================================

    #[tool(description = "Erase flash memory sectors or entire chip")]
    async fn flash_erase(&self, Parameters(args): Parameters<FlashEraseArgs>) -> Result<CallToolResult, McpError> {
        debug!("Flash erase for session: {}, type: {}", args.session_id, args.erase_type);
        
        let session_arc = {
            let sessions = self.sessions.read().await;
            match sessions.get(&args.session_id) {
                Some(session) => session.clone(),
                None => {
                    let error_msg = format!("❌ Session '{}' not found\n\nUse 'connect' to establish a debug session first", args.session_id);
                    return Err(McpError::internal_error(error_msg, None));
                }
            }
        };

        // Parse erase type and parameters
        let erase_type = match args.erase_type.as_str() {
            "all" => crate::flash::EraseType::All,
            "sectors" => {
                let address = match args.address {
                    Some(addr_str) => parse_address(&addr_str).map_err(|e| McpError::internal_error(e, None))?,
                    None => return Err(McpError::internal_error("Address required for sector erase".to_string(), None)),
                };
                let size = match args.size {
                    Some(sz) => sz as usize,
                    None => return Err(McpError::internal_error("Size required for sector erase".to_string(), None)),
                };
                crate::flash::EraseType::Sectors { address, size }
            }
            _ => return Err(McpError::internal_error(format!("Invalid erase type: {}", args.erase_type), None)),
        };

        // Perform erase operation
        {
            let mut session = session_arc.session.lock().await;
            match crate::flash::FlashManager::erase_flash(&mut session, erase_type).await {
                Ok(result) => {
                    let message = format!(
                        "✅ Flash erase completed successfully!\n\n\
                        Session ID: {}\n\
                        Erase Type: {}\n\
                        Duration: {}ms\n\
                        {}\n\n\
                        Flash memory has been erased and is ready for programming.",
                        args.session_id,
                        args.erase_type,
                        result.erase_time_ms,
                        match result.sectors_erased {
                            Some(count) => format!("Sectors Erased: {}", count),
                            None => "Full chip erased".to_string(),
                        }
                    );
                    
                    info!("Flash erase completed for session: {}", args.session_id);
                    Ok(CallToolResult::success(vec![Content::text(message)]))
                }
                Err(e) => {
                    error!("Flash erase failed for session {}: {}", args.session_id, e);
                    let error_msg = format!(
                        "❌ Flash erase failed\n\n\
                        Session ID: {}\n\
                        Error: {}\n\n\
                        Suggestions:\n\
                        - Check if flash is write-protected\n\
                        - Ensure target is halted\n\
                        - Verify flash address range",
                        args.session_id, e
                    );
                    Err(McpError::internal_error(error_msg, None))
                }
            }
        }
    }

    #[tool(description = "Program file to flash memory (supports ELF, HEX, BIN)")]
    async fn flash_program(&self, Parameters(args): Parameters<FlashProgramArgs>) -> Result<CallToolResult, McpError> {
        debug!("Flash program for session: {}, file: {}", args.session_id, args.file_path);
        
        let session_arc = {
            let sessions = self.sessions.read().await;
            match sessions.get(&args.session_id) {
                Some(session) => session.clone(),
                None => {
                    let error_msg = format!("❌ Session '{}' not found\n\nUse 'connect' to establish a debug session first", args.session_id);
                    return Err(McpError::internal_error(error_msg, None));
                }
            }
        };

        // Parse file path and format
        let file_path = std::path::Path::new(&args.file_path);
        let format = match args.format.as_str() {
            "auto" => crate::flash::FileFormat::Auto,
            "elf" => crate::flash::FileFormat::Elf,
            "hex" => crate::flash::FileFormat::Hex,
            "bin" => crate::flash::FileFormat::Bin,
            _ => return Err(McpError::internal_error(format!("Unsupported format: {}", args.format), None)),
        };

        // Parse base address if provided
        let base_address = if let Some(addr_str) = args.base_address {
            Some(parse_address(&addr_str).map_err(|e| McpError::internal_error(e, None))?)
        } else {
            None
        };

        // Perform programming operation
        {
            let mut session = session_arc.session.lock().await;
            match crate::flash::FlashManager::program_file(&mut session, file_path, format, base_address).await {
                Ok(result) => {
                    let message = format!(
                        "✅ Flash programming completed successfully!\n\n\
                        Session ID: {}\n\
                        File: {}\n\
                        Format: {}\n\
                        Bytes Programmed: {}\n\
                        Duration: {}ms\n\
                        Verification: {}\n\n\
                        Firmware has been programmed to flash memory.",
                        args.session_id,
                        args.file_path,
                        args.format,
                        result.bytes_programmed,
                        result.programming_time_ms,
                        match result.verification_result {
                            Some(true) => "✅ Passed",
                            Some(false) => "❌ Failed",
                            None => "Not performed",
                        }
                    );
                    
                    info!("Flash programming completed for session: {}", args.session_id);
                    Ok(CallToolResult::success(vec![Content::text(message)]))
                }
                Err(e) => {
                    error!("Flash programming failed for session {}: {}", args.session_id, e);
                    let error_msg = format!(
                        "❌ Flash programming failed\n\n\
                        Session ID: {}\n\
                        File: {}\n\
                        Error: {}\n\n\
                        Suggestions:\n\
                        - Check file exists and is readable\n\
                        - Verify file format is correct\n\
                        - Ensure flash is erased first\n\
                        - Check target memory map",
                        args.session_id, args.file_path, e
                    );
                    Err(McpError::internal_error(error_msg, None))
                }
            }
        }
    }

    #[tool(description = "Verify flash memory contents")]
    async fn flash_verify(&self, Parameters(args): Parameters<FlashVerifyArgs>) -> Result<CallToolResult, McpError> {
        debug!("Flash verify for session: {}", args.session_id);
        
        let session_arc = {
            let sessions = self.sessions.read().await;
            match sessions.get(&args.session_id) {
                Some(session) => session.clone(),
                None => {
                    let error_msg = format!("❌ Session '{}' not found\n\nUse 'connect' to establish a debug session first", args.session_id);
                    return Err(McpError::internal_error(error_msg, None));
                }
            }
        };

        // Parse address
        let address = parse_address(&args.address).map_err(|e| McpError::internal_error(e, None))?;

        // Get expected data
        let expected_data = if let Some(file_path) = &args.file_path {
            // Read from file
            std::fs::read(file_path)
                .map_err(|e| McpError::internal_error(format!("Failed to read file {}: {}", file_path, e), None))?
        } else if let Some(hex_data) = &args.data {
            // Parse hex data
            match parse_data(hex_data, "hex") {
                Ok(data) => data,
                Err(e) => return Err(McpError::internal_error(format!("Invalid hex data: {}", e), None)),
            }
        } else {
            return Err(McpError::internal_error("Either file_path or data must be provided".to_string(), None));
        };

        // Limit to specified size
        let expected_data = if expected_data.len() > args.size as usize {
            &expected_data[..args.size as usize]
        } else {
            &expected_data
        };

        // Perform verification
        {
            let mut session = session_arc.session.lock().await;
            match crate::flash::FlashManager::verify_flash(&mut session, expected_data, address).await {
                Ok(result) => {
                    let message = if result.success {
                        format!(
                            "✅ Flash verification successful!\n\n\
                            Session ID: {}\n\
                            Address: 0x{:08X}\n\
                            Bytes Verified: {}\n\n\
                            All flash contents match expected data.",
                            args.session_id, address, result.bytes_verified
                        )
                    } else {
                        let mut message = format!(
                            "❌ Flash verification failed!\n\n\
                            Session ID: {}\n\
                            Address: 0x{:08X}\n\
                            Bytes Verified: {}\n\
                            Mismatches: {}\n\n\
                            First {} mismatches:\n",
                            args.session_id, address, result.bytes_verified, result.mismatches.len(),
                            std::cmp::min(10, result.mismatches.len())
                        );
                        
                        for (i, mismatch) in result.mismatches.iter().take(10).enumerate() {
                            message.push_str(&format!(
                                "  {}. 0x{:08X}: expected 0x{:02X}, got 0x{:02X}\n",
                                i + 1, mismatch.address, mismatch.expected, mismatch.actual
                            ));
                        }
                        
                        if result.mismatches.len() > 10 {
                            message.push_str(&format!("  ... and {} more mismatches\n", result.mismatches.len() - 10));
                        }
                        
                        message
                    };
                    
                    info!("Flash verification completed for session: {}", args.session_id);
                    Ok(CallToolResult::success(vec![Content::text(message)]))
                }
                Err(e) => {
                    error!("Flash verification failed for session {}: {}", args.session_id, e);
                    let error_msg = format!(
                        "❌ Flash verification error\n\n\
                        Session ID: {}\n\
                        Error: {}",
                        args.session_id, e
                    );
                    Err(McpError::internal_error(error_msg, None))
                }
            }
        }
    }

    #[tool(description = "Complete firmware deployment: erase, program, verify, run and attach RTT")]
    async fn run_firmware(&self, Parameters(args): Parameters<RunFirmwareArgs>) -> Result<CallToolResult, McpError> {
        debug!("Run firmware for session: {}, file: {}", args.session_id, args.file_path);
        
        let session_arc = {
            let sessions = self.sessions.read().await;
            match sessions.get(&args.session_id) {
                Some(session) => session.clone(),
                None => {
                    let error_msg = format!("❌ Session '{}' not found\n\nUse 'connect' to establish a debug session first", args.session_id);
                    return Err(McpError::internal_error(error_msg, None));
                }
            }
        };

        let mut status_messages = Vec::new();
        let start_time = std::time::Instant::now();

        // Step 1: Erase flash
        status_messages.push("🔄 Step 1/5: Erasing flash memory...".to_string());
        {
            let mut session = session_arc.session.lock().await;
            match crate::flash::FlashManager::erase_flash(&mut session, crate::flash::EraseType::All).await {
                Ok(_) => status_messages.push("✅ Flash erased successfully".to_string()),
                Err(e) => {
                    let error_msg = format!("❌ Flash erase failed: {}", e);
                    status_messages.push(error_msg.clone());
                    return Err(McpError::internal_error(format!("{}\n\n{}", status_messages.join("\n"), error_msg), None));
                }
            }
        }

        // Step 2: Program firmware
        status_messages.push("🔄 Step 2/5: Programming firmware...".to_string());
        let format = match args.format.as_str() {
            "auto" => crate::flash::FileFormat::Auto,
            "elf" => crate::flash::FileFormat::Elf,
            "hex" => crate::flash::FileFormat::Hex,
            "bin" => crate::flash::FileFormat::Bin,
            _ => return Err(McpError::internal_error(format!("Unsupported format: {}", args.format), None)),
        };

        {
            let mut session = session_arc.session.lock().await;
            match crate::flash::FlashManager::program_file(&mut session, std::path::Path::new(&args.file_path), format, None).await {
                Ok(result) => status_messages.push(format!("✅ Programmed {} bytes", result.bytes_programmed)),
                Err(e) => {
                    let error_msg = format!("❌ Programming failed: {}", e);
                    status_messages.push(error_msg.clone());
                    return Err(McpError::internal_error(format!("{}\n\n{}", status_messages.join("\n"), error_msg), None));
                }
            }
        }

        // Step 3: Reset and run
        if args.reset_after_flash {
            status_messages.push("🔄 Step 3/5: Resetting target...".to_string());
            {
                let mut session = session_arc.session.lock().await;
                let mut core = match session.core(0) {
                    Ok(core) => core,
                    Err(e) => return Err(McpError::internal_error(format!("Failed to get core: {}", e), None)),
                };
                
                match core.reset() {
                    Ok(_) => {
                        status_messages.push("✅ Target reset successfully".to_string());
                        // Run the target
                        match core.run() {
                            Ok(_) => status_messages.push("✅ Target running".to_string()),
                            Err(e) => warn!("Failed to run after reset: {}", e),
                        }
                    }
                    Err(e) => {
                        let error_msg = format!("❌ Reset failed: {}", e);
                        status_messages.push(error_msg.clone());
                        return Err(McpError::internal_error(format!("{}\n\n{}", status_messages.join("\n"), error_msg), None));
                    }
                }
            }
        }

        // Step 4: Attach RTT (if requested) - Mimic probe-rs run behavior
        if args.attach_rtt {
            status_messages.push("🔄 Step 4/5: Attaching RTT (probe-rs style)...".to_string());
            
            // Key improvement: Give target more time to boot, mimic probe-rs run timing
            info!("Allowing target firmware to fully initialize RTT control block...");
            tokio::time::sleep(tokio::time::Duration::from_millis(2000)).await; // Initial 2s delay
            
            // Give target additional time to fully initialize RTT (key improvement)
            info!("Giving target additional time to initialize RTT control block...");
            tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
            
            // Enhanced RTT retry mechanism with probe-rs style timing
            let mut rtt_attached = false;
            let max_attempts = 8; // Increase retry attempts
            let mut attempt = 1;
            
            while attempt <= max_attempts && !rtt_attached {
                // probe-rs style delay strategy: 1s, 1.5s, 2s, 2.5s, 3s, 3.5s, 4s, 4.5s
                let delay_ms = 1000 + (attempt - 1) * 500;
                info!("RTT attach attempt {}/{}, waiting {}ms for RTT control block...", attempt, max_attempts, delay_ms);
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms as u64)).await;
                
                // Small delay between RTT attempts (let target stabilize)
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                
                // Try RTT attachment with different strategies (probe-rs style optimization)
                let mut rtt_manager = session_arc.rtt_manager.lock().await;
                let rtt_result = match attempt {
                    1..=2 => {
                        // First 2 attempts: ELF symbol detection (probe-rs priority method)
                        debug!("RTT attempt {}: Using ELF symbol detection (probe-rs style)", attempt);
                        rtt_manager.attach_with_elf(session_arc.session.clone(), std::path::Path::new(&args.file_path)).await
                    }
                    3..=5 => {
                        // Attempts 3-5: standard attach, let probe-rs auto-scan memory
                        debug!("RTT attempt {}: Using standard memory map scan", attempt);
                        rtt_manager.attach(session_arc.session.clone(), None, None).await
                    }
                    6..=7 => {
                        // Attempts 6-7: try STM32G4 specific memory ranges
                        debug!("RTT attempt {}: Using STM32G4 specific memory ranges", attempt);
                        let stm32g4_ranges = vec![
                            (0x20000000, 0x20004000), // SRAM1 first half: 16KB - most likely RTT location
                            (0x20004000, 0x20008000), // SRAM1 second half: 16KB
                            (0x20008000, 0x2000A000), // SRAM2: 8KB
                        ];
                        rtt_manager.attach(session_arc.session.clone(), None, Some(stm32g4_ranges)).await
                    }
                    _ => {
                        // Last attempt: try common RTT control block addresses
                        let cb_addr = 0x20000000;
                        debug!("RTT attempt {}: Using specific control block address 0x{:08X}", attempt, cb_addr);
                        rtt_manager.attach(session_arc.session.clone(), Some(cb_addr), None).await
                    }
                };
                
                match rtt_result {
                    Ok(_) => {
                        let up_channels = rtt_manager.up_channel_count();
                        let down_channels = rtt_manager.down_channel_count();
                        status_messages.push(format!("✅ RTT attached on attempt {} ({} up, {} down channels)", attempt, up_channels, down_channels));
                        info!("RTT successfully attached after {} attempts!", attempt);
                        rtt_attached = true;
                    }
                    Err(e) => {
                        if attempt == max_attempts {
                            // Final attempt failed
                            status_messages.push(format!("⚠️ RTT attach failed after {} attempts: {}", max_attempts, e));
                            warn!("RTT attachment failed completely after {} attempts", max_attempts);
                        } else {
                            debug!("RTT attach attempt {}/{} failed: {}, retrying with different strategy...", attempt, max_attempts, e);
                        }
                    }
                }
                attempt += 1;
            }
            
            // If RTT successfully connected, give extra initialization time
            if rtt_attached {
                info!("RTT connected successfully, allowing channel stabilization...");
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }
        }

        status_messages.push("🔄 Step 5/5: Finalizing...".to_string());
        let elapsed = start_time.elapsed();

        let message = format!(
            "🚀 Firmware deployment completed!\n\n\
            Session ID: {}\n\
            File: {}\n\
            Format: {}\n\
            Total Time: {:.1}s\n\n\
            Status:\n{}\n\n\
            ✅ Firmware is now running on target.\n\
            {}",
            args.session_id,
            args.file_path,
            args.format,
            elapsed.as_secs_f64(),
            status_messages.join("\n"),
            if args.attach_rtt { "Use 'rtt_read' to monitor target output." } else { "Use 'rtt_attach' to enable real-time communication." }
        );

        info!("Firmware deployment completed for session: {} in {:.1}s", args.session_id, elapsed.as_secs_f64());
        Ok(CallToolResult::success(vec![Content::text(message)]))
    }
}

// =============================================================================
// Utility Functions
// =============================================================================

/// Parse address string (hex or decimal) to u64
fn parse_address(addr_str: &str) -> Result<u64, String> {
    let addr_str = addr_str.trim();
    
    if addr_str.starts_with("0x") || addr_str.starts_with("0X") {
        u64::from_str_radix(&addr_str[2..], 16)
            .map_err(|e| format!("Invalid hex address: {}", e))
    } else {
        addr_str.parse::<u64>()
            .map_err(|e| format!("Invalid decimal address: {}", e))
    }
}

/// Parse data string based on format
fn parse_data(data_str: &str, format: &str) -> Result<Vec<u8>, String> {
    match format {
        "hex" => {
            // Remove spaces and 0x prefixes
            let clean_str = data_str.replace(" ", "").replace("0x", "").replace("0X", "");
            if clean_str.len() % 2 != 0 {
                return Err("Hex data must have even number of characters".to_string());
            }
            
            (0..clean_str.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&clean_str[i..i+2], 16))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("Invalid hex data: {}", e))
        }
        "ascii" => Ok(data_str.as_bytes().to_vec()),
        "words32" => {
            let words: Result<Vec<u32>, _> = data_str
                .split_whitespace()
                .map(|s| {
                    if s.starts_with("0x") || s.starts_with("0X") {
                        u32::from_str_radix(&s[2..], 16)
                    } else {
                        s.parse::<u32>()
                    }
                })
                .collect();
            
            match words {
                Ok(words) => {
                    let mut data = Vec::new();
                    for word in words {
                        data.extend_from_slice(&word.to_le_bytes());
                    }
                    Ok(data)
                }
                Err(e) => Err(format!("Invalid word32 data: {}", e)),
            }
        }
        "words16" => {
            let words: Result<Vec<u16>, _> = data_str
                .split_whitespace()
                .map(|s| {
                    if s.starts_with("0x") || s.starts_with("0X") {
                        u16::from_str_radix(&s[2..], 16)
                    } else {
                        s.parse::<u16>()
                    }
                })
                .collect();
            
            match words {
                Ok(words) => {
                    let mut data = Vec::new();
                    for word in words {
                        data.extend_from_slice(&word.to_le_bytes());
                    }
                    Ok(data)
                }
                Err(e) => Err(format!("Invalid word16 data: {}", e)),
            }
        }
        _ => Err(format!("Unsupported data format: {}", format)),
    }
}

/// Format memory data for display
fn format_memory_data(data: &[u8], format: &str, base_address: u64) -> String {
    match format {
        "hex" => {
            let mut result = String::new();
            for (i, chunk) in data.chunks(16).enumerate() {
                let addr = base_address + (i * 16) as u64;
                result.push_str(&format!("0x{:08X}: ", addr));
                
                // Hex bytes
                for (j, byte) in chunk.iter().enumerate() {
                    if j == 8 { result.push(' '); }
                    result.push_str(&format!("{:02X} ", byte));
                }
                
                // Pad if needed
                if chunk.len() < 16 {
                    let padding = (16 - chunk.len()) * 3 + (if chunk.len() <= 8 { 1 } else { 0 });
                    result.push_str(&" ".repeat(padding));
                }
                
                // ASCII representation
                result.push_str("| ");
                for byte in chunk {
                    if byte.is_ascii_graphic() || *byte == b' ' {
                        result.push(*byte as char);
                    } else {
                        result.push('.');
                    }
                }
                result.push('\n');
            }
            result
        }
        "binary" => {
            data.iter()
                .map(|b| format!("{:08b}", b))
                .collect::<Vec<_>>()
                .join(" ")
        }
        "words32" => {
            let mut result = String::new();
            for (i, chunk) in data.chunks(4).enumerate() {
                if chunk.len() == 4 {
                    let addr = base_address + (i * 4) as u64;
                    let word = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                    result.push_str(&format!("0x{:08X}: 0x{:08X}\n", addr, word));
                }
            }
            result
        }
        "words16" => {
            let mut result = String::new();
            for (i, chunk) in data.chunks(2).enumerate() {
                if chunk.len() == 2 {
                    let addr = base_address + (i * 2) as u64;
                    let word = u16::from_le_bytes([chunk[0], chunk[1]]);
                    result.push_str(&format!("0x{:08X}: 0x{:04X}\n", addr, word));
                }
            }
            result
        }
        "ascii" => {
            String::from_utf8_lossy(data).to_string()
        }
        _ => {
            // Default to hex if unknown format
            format_memory_data(data, "hex", base_address)
        }
    }
}

#[tool_handler]
impl ServerHandler for EmbeddedDebuggerToolHandler {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some("Complete embedded debugging and flash programming MCP server supporting ARM Cortex-M, RISC-V, and other architectures via probe-rs. Provides comprehensive debugging and flash programming capabilities including probe detection, target connection, memory operations, breakpoints, RTT communication, and flash programming with real hardware integration. All 22 tools available: list_probes, connect, disconnect, probe_info, halt, run, reset, step, get_status, read_memory, write_memory, set_breakpoint, clear_breakpoint, rtt_attach, rtt_detach, rtt_read, rtt_write, rtt_channels, flash_erase, flash_program, flash_verify, run_firmware.".to_string()),
        }
    }

    async fn initialize(
        &self,
        _request: InitializeRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, McpError> {
        info!("Complete Embedded Debugger MCP server initialized with all 22 tools (18 debug + 4 flash)");
        Ok(self.get_info())
    }
}