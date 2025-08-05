//! Embedded Debugger MCP Server - Main Entry Point

use clap::Parser;
use tracing::{info, error, debug};
use tracing_subscriber::{EnvFilter, fmt};
use rmcp::{ServiceExt, transport::stdio};

use embedded_debugger_mcp::{
    Config,
    config::Args,
    tools::EmbeddedDebuggerToolHandler,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command line arguments
    let args = Args::parse();

    // Handle special flags first
    if args.generate_config {
        let config = Config::default();
        println!("{}", config.to_toml()?);
        return Ok(());
    }

    // Initialize logging
    init_logging(&args)?;

    info!("Starting Debugger MCP Server v{}", env!("CARGO_PKG_VERSION"));
    debug!("Command line args: {:?}", args);

    // Load configuration
    let mut config = Config::load(args.config.as_ref())
        .map_err(|e| {
            error!("Failed to load configuration: {}", e);
            e
        })?;

    // Merge command line arguments into configuration
    config.merge_args(&args);

    if args.validate_config {
        config.validate()?;
        println!("Configuration is valid");
        return Ok(());
    }

    if args.show_config {
        println!("{}", config.to_toml()?);
        return Ok(());
    }

    // Validate final configuration
    config.validate()
        .map_err(|e| {
            error!("Configuration validation failed: {}", e);
            e
        })?;

    info!("Configuration loaded and validated successfully");

    // Create and serve the handler using rust-sdk standard pattern
    let service = EmbeddedDebuggerToolHandler::new(config.server.max_sessions)
        .serve(stdio()).await.inspect_err(|e| {
            error!("Serving error: {:?}", e);
        })?;
    
    info!("Embedded Debugger MCP Server started successfully");
    
    // Wait for the service to complete
    service.waiting().await?;

    // Cleanup (simplified - no sessions to manage)
    info!("Cleaning up resources...");

    info!("Embedded Debugger MCP Server stopped");
    Ok(())
}

/// Initialize logging system
fn init_logging(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&args.log_level));

    let subscriber = fmt::Subscriber::builder()
        .with_env_filter(env_filter)
        .with_target(true)
        .with_thread_ids(true)
        .with_file(false)
        .with_line_number(false);

    // Configure output destination
    if let Some(log_file) = &args.log_file {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_file)?;
        
        subscriber
            .with_writer(file)
            .init();
        
        println!("Logging to file: {}", log_file.display());
    } else {
        subscriber
            .with_writer(std::io::stderr)
            .init();
    }

    debug!("Logging initialized with level: {}", args.log_level);
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_args_parsing() {
        let args = Args::parse_from(&[
            "debugger-mcp-rs",
            "--log-level", "debug",
            "--max-sessions", "10",
        ]);
        
        assert_eq!(args.log_level, "debug");
        assert_eq!(args.max_sessions, 10);
    }

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.validate().is_ok());
        assert_eq!(config.server.max_sessions, 5);
        assert_eq!(config.debugger.default_speed_khz, 4000);
    }

}