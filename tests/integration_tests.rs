//! Integration tests for debugger MCP server

use embedded_debugger_mcp::{Config, DebugSessionManager};

#[tokio::test]
async fn test_session_manager_initialization() {
    let config = Config::default();
    let manager = DebugSessionManager::new(config.server.max_sessions);
    
    // Test basic functionality
    assert_eq!(manager.session_count().await, 0);
    
    let sessions = manager.list_sessions().await;
    assert!(sessions.is_empty());
    
    let stats = manager.get_statistics().await;
    assert_eq!(stats.total_sessions, 0);
    assert_eq!(stats.max_sessions, config.server.max_sessions);
}

#[tokio::test]
async fn test_config_validation() {
    let config = Config::default();
    assert!(config.validate().is_ok());
    
    // Test TOML serialization
    let toml_str = config.to_toml().unwrap();
    assert!(!toml_str.is_empty());
    assert!(toml_str.contains("[server]"));
    assert!(toml_str.contains("[debugger]"));
}

#[tokio::test]
async fn test_probe_discovery() {
    // Test probe discovery (this will work even without hardware)
    let result = embedded_debugger_mcp::debugger::ProbeDiscovery::list_probes();
    assert!(result.is_ok());
    
    // The result might be empty if no probes are connected, which is fine
    let probes = result.unwrap();
    println!("Found {} probes", probes.len());
}

#[test]
fn test_error_types() {
    use embedded_debugger_mcp::DebugError;
    
    let error = DebugError::ProbeNotFound("test".to_string());
    assert!(error.to_string().contains("Probe not found"));
    
    let error = DebugError::SessionLimitExceeded(5);
    assert!(error.to_string().contains("Session limit exceeded"));
}

#[test]
fn test_utils_encoding() {
    use embedded_debugger_mcp::utils::encoding;
    
    let data = vec![0x01, 0x02, 0x03, 0x04];
    
    // Test hex encoding
    let hex = encoding::bytes_to_hex(&data);
    assert_eq!(hex, "01020304");
    
    let decoded = encoding::hex_to_bytes(&hex).unwrap();
    assert_eq!(decoded, data);
    
    // Test base64 encoding
    let base64 = encoding::bytes_to_base64(&data);
    let decoded = encoding::base64_to_bytes(&base64).unwrap();
    assert_eq!(decoded, data);
}

#[test]
fn test_utils_address() {
    use embedded_debugger_mcp::utils::address;
    
    // Test address parsing
    assert_eq!(address::parse_address("0x1000").unwrap(), 0x1000);
    assert_eq!(address::parse_address("1000").unwrap(), 1000);
    
    // Test address formatting
    assert_eq!(address::format_address(0x1000), "0x00001000");
    
    // Test alignment
    assert!(address::is_aligned(0x1000, 4));
    assert!(!address::is_aligned(0x1001, 4));
    assert_eq!(address::align_down(0x1003, 4), 0x1000);
    assert_eq!(address::align_up(0x1001, 4), 0x1004);
}

#[test]
fn test_data_format_parsing() {
    use embedded_debugger_mcp::utils::DataFormat;
    
    assert_eq!("hex".parse::<DataFormat>().unwrap(), DataFormat::Hex);
    assert_eq!("binary".parse::<DataFormat>().unwrap(), DataFormat::Binary);
    assert_eq!("ascii".parse::<DataFormat>().unwrap(), DataFormat::Ascii);
    assert_eq!("words".parse::<DataFormat>().unwrap(), DataFormat::Words);
    
    assert!("invalid".parse::<DataFormat>().is_err());
}

#[test]
fn test_reset_type_parsing() {
    use embedded_debugger_mcp::utils::ResetType;
    
    assert_eq!("hardware".parse::<ResetType>().unwrap(), ResetType::Hardware);
    assert_eq!("software".parse::<ResetType>().unwrap(), ResetType::Software);
    assert_eq!("system".parse::<ResetType>().unwrap(), ResetType::System);
    
    assert!("invalid".parse::<ResetType>().is_err());
}

#[tokio::test]
async fn test_mcp_tool_schemas() {
    use embedded_debugger_mcp::mcp::types::create_tool_schemas;
    
    let tools = create_tool_schemas();
    assert!(!tools.is_empty());
    
    // Check that we have the expected tools
    let tool_names: Vec<_> = tools.iter().map(|t| &t.name).collect();
    
    assert!(tool_names.contains(&&"list_probes".to_string()));
    assert!(tool_names.contains(&&"connect".to_string()));
    assert!(tool_names.contains(&&"halt".to_string()));
    assert!(tool_names.contains(&&"run".to_string()));
    assert!(tool_names.contains(&&"read_memory".to_string()));
    assert!(tool_names.contains(&&"write_memory".to_string()));
    assert!(tool_names.contains(&&"set_breakpoint".to_string()));
    assert!(tool_names.contains(&&"flash_binary".to_string()));
    assert!(tool_names.contains(&&"rtt_attach".to_string()));
    
    println!("Found {} MCP tools", tools.len());
    for tool in &tools {
        println!("  - {}: {}", tool.name, tool.description);
    }
}