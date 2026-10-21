//! Embedded Debugger MCP Server - Main Entry Point

use clap::Parser;
use embedded_debugger_mcp::config::{
    Command as CliCommand, ConfigCommand, ProbeCommand, SkillCommand,
};
use embedded_debugger_mcp::debugger::discovery::ProbeDiscovery;
use rmcp::ServiceExt;
use serde::Serialize;
use std::process::Command as ProcessCommand;
use tracing::{debug, error, info};
use tracing_subscriber::{fmt, EnvFilter};

use embedded_debugger_mcp::{config::Args, tools::EmbeddedDebuggerToolHandler, Config, DebugError};
use skill_installer::{install_skill_bundle, DEFAULT_SKILL_PROMPT};

mod skill_installer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command line arguments
    let args = Args::parse();

    // Handle special flags first
    if args.generate_config
        || matches!(
            args.command,
            Some(CliCommand::Config {
                action: ConfigCommand::Generate
            })
        )
    {
        let config = Config::default();
        println!("{}", config.to_toml()?);
        return Ok(());
    }

    if let Some(CliCommand::Doctor { json }) = args.command.clone() {
        let mut config_result = Config::load_unvalidated(args.config.as_ref());
        if let Ok(config) = &mut config_result {
            config.merge_args(&args);
        }
        let report = DoctorReport::collect(config_result);
        print_doctor_report(&report, json)?;
        return Ok(());
    }

    // Load configuration
    let mut config = Config::load(args.config.as_ref())?;

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
    config.validate()?;

    // Initialize logging after config merge so file config is not overwritten by CLI defaults.
    init_logging(&config)?;

    info!(
        "Starting Debugger MCP Server v{}",
        env!("CARGO_PKG_VERSION")
    );
    debug!("Command line args: {:?}", args);

    info!("Configuration loaded and validated successfully");

    if let Some(command) = args.command.clone() {
        return run_cli_command(command, config).await;
    }

    run_mcp_server(config).await
}

async fn run_cli_command(
    command: CliCommand,
    config: Config,
) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        CliCommand::Serve => run_mcp_server(config).await,
        CliCommand::Config { action } => {
            match action {
                ConfigCommand::Generate => println!("{}", Config::default().to_toml()?),
                ConfigCommand::Validate => println!("Configuration is valid"),
                ConfigCommand::Show => println!("{}", config.to_toml()?),
            }
            Ok(())
        }
        CliCommand::Probes {
            action: ProbeCommand::List { json },
        } => {
            let probes = ProbeDiscovery::list_probes()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&probes)?);
            } else if probes.is_empty() {
                println!("No debug probes found");
            } else {
                for (index, probe) in probes.iter().enumerate() {
                    println!(
                        "{}. {} ({:04X}:{:04X}) {}",
                        index + 1,
                        probe.identifier,
                        probe.vendor_id,
                        probe.product_id,
                        probe.serial_number.as_deref().unwrap_or("no serial")
                    );
                }
            }
            Ok(())
        }
        CliCommand::Doctor { json } => {
            let report = DoctorReport::collect(Ok(config));
            print_doctor_report(&report, json)
        }
        CliCommand::Skill { action } => match action {
            SkillCommand::PrintPrompt => {
                print!("{}", DEFAULT_SKILL_PROMPT);
                Ok(())
            }
            SkillCommand::Install {
                target,
                home,
                dry_run,
                force,
                json,
            } => {
                let report = install_skill_bundle(target, home, dry_run, force)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else {
                    report.print_text();
                }
                Ok(())
            }
        },
    }
}

fn compat_stdio() -> (tokio::io::DuplexStream, tokio::io::DuplexStream) {
    compat_transport(tokio::io::stdin(), tokio::io::stdout())
}

fn compat_transport<R, W>(
    stdin: R,
    mut stdout: W,
) -> (tokio::io::DuplexStream, tokio::io::DuplexStream)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    use rmcp::model::ClientJsonRpcMessage;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::sync::mpsc;

    let (rmcp_in_rx, mut rmcp_in_tx) = tokio::io::duplex(65536);
    let (rmcp_out_rx, rmcp_out_tx) = tokio::io::duplex(65536);
    let (stdout_tx, mut stdout_rx) = mpsc::unbounded_channel::<Vec<u8>>();

    let stdout_tx_rmcp = stdout_tx.clone();
    tokio::spawn(async move {
        let mut reader = BufReader::new(rmcp_out_rx);
        let mut line = String::new();
        while let Ok(n) = reader.read_line(&mut line).await {
            if n == 0 {
                break;
            }
            if stdout_tx_rmcp.send(line.as_bytes().to_vec()).is_err() {
                break;
            }
            line.clear();
        }
    });

    tokio::spawn(async move {
        while let Some(bytes) = stdout_rx.recv().await {
            if stdout.write_all(&bytes).await.is_err() {
                break;
            }
            if stdout.flush().await.is_err() {
                break;
            }
        }
    });

    tokio::spawn(async move {
        let mut reader = BufReader::new(stdin);
        let mut line = String::new();
        let mut initialized = false;

        while let Ok(n) = reader.read_line(&mut line).await {
            if n == 0 {
                break;
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                line.clear();
                continue;
            }

            if let Ok(mut json_val) = serde_json::from_str::<serde_json::Value>(trimmed) {
                let method = json_val
                    .get("method")
                    .and_then(|m| m.as_str())
                    .map(str::to_owned);
                let has_id = json_val.get("id").is_some_and(|v| !v.is_null());

                if method.as_deref() == Some("initialize") {
                    if let Some(params) = json_val.get_mut("params").and_then(|p| p.as_object_mut())
                    {
                        if params.get("capabilities").is_none_or(|c| c.is_null()) {
                            params.insert("capabilities".to_string(), serde_json::json!({}));
                        }
                        if params.get("clientInfo").is_none_or(|c| c.is_null()) {
                            params.insert(
                                "clientInfo".to_string(),
                                serde_json::json!({"name": "mcp-client", "version": "1.0.0"}),
                            );
                        }
                    }
                }

                let normalized_str =
                    serde_json::to_string(&json_val).unwrap_or_else(|_| trimmed.to_string());

                if serde_json::from_str::<ClientJsonRpcMessage>(&normalized_str).is_ok() {
                    if !initialized {
                        match method.as_deref() {
                            Some("initialize") => {}
                            Some("notifications/initialized") => {
                                initialized = true;
                            }
                            _ => {
                                if !has_id {
                                    line.clear();
                                    continue;
                                }
                            }
                        }
                    }

                    let mut out = normalized_str.into_bytes();
                    out.push(b'\n');
                    if rmcp_in_tx.write_all(&out).await.is_err() {
                        break;
                    }
                    let _ = rmcp_in_tx.flush().await;
                } else if has_id {
                    let id = json_val
                        .get("id")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);
                    let err_resp = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {
                            "code": -32601,
                            "message": format!("Method not found: {}", method.unwrap_or_default())
                        }
                    });
                    if let Ok(mut bytes) = serde_json::to_vec(&err_resp) {
                        bytes.push(b'\n');
                        let _ = stdout_tx.send(bytes);
                    }
                }
            }

            line.clear();
        }
    });

    (rmcp_in_rx, rmcp_out_tx)
}

async fn run_mcp_server(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    // Create and serve the handler using rust-sdk standard pattern with compatibility stdio
    let service = EmbeddedDebuggerToolHandler::new(config)
        .serve(compat_stdio())
        .await
        .inspect_err(|e| {
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

#[derive(Debug, Serialize)]
struct DoctorReport {
    version: &'static str,
    rustc_version: Option<String>,
    config_valid: bool,
    config_error: Option<String>,
    probe_count: Option<usize>,
    probe_error: Option<String>,
}

impl DoctorReport {
    fn collect(config_result: Result<Config, DebugError>) -> Self {
        let config_result = config_result.and_then(|config| config.validate().map(|()| config));
        let probe_result = ProbeDiscovery::list_probes();

        Self {
            version: env!("CARGO_PKG_VERSION"),
            rustc_version: command_output("rustc", &["--version"]),
            config_valid: config_result.is_ok(),
            config_error: config_result.err().map(|error| error.to_string()),
            probe_count: probe_result.as_ref().ok().map(Vec::len),
            probe_error: probe_result.err().map(|error| error.to_string()),
        }
    }
}

fn print_doctor_report(
    report: &DoctorReport,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if json {
        println!("{}", serde_json::to_string_pretty(report)?);
        return Ok(());
    }

    println!("embedded-debugger-mcp doctor");
    println!("version: {}", report.version);
    println!(
        "rustc: {}",
        report.rustc_version.as_deref().unwrap_or("not found")
    );
    println!("config_valid: {}", report.config_valid);
    if let Some(error) = &report.config_error {
        println!("config_error: {}", error);
    }
    println!("probe_count: {}", report.probe_count.unwrap_or(0));
    if let Some(error) = &report.probe_error {
        println!("probe_error: {}", error);
    }
    println!("mcp_mode: use `embedded-debugger-mcp serve`");
    println!("cli_skill_mode: use `embedded-debugger-mcp skill print-prompt`");
    Ok(())
}

fn command_output(command: &str, args: &[&str]) -> Option<String> {
    let output = ProcessCommand::new(command).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    Some(stdout.trim().to_string())
}

/// Initialize logging system
fn init_logging(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.logging.level));

    let subscriber = fmt::Subscriber::builder()
        .with_env_filter(env_filter)
        .with_target(true)
        .with_thread_ids(true)
        .with_file(false)
        .with_line_number(false);

    // Configure output destination
    if let Some(log_file) = &config.logging.file {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_file)?;

        subscriber.with_writer(file).init();

        eprintln!("Logging to file: {}", log_file.display());
    } else {
        subscriber.with_writer(std::io::stderr).init();
    }

    debug!("Logging initialized with level: {}", config.logging.level);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_debugger_mcp::config::SkillInstallTarget;

    #[test]
    fn test_args_parsing() {
        let args = Args::parse_from([
            "embedded-debugger-mcp",
            "--log-level",
            "debug",
            "--max-sessions",
            "10",
        ]);

        assert_eq!(args.log_level.as_deref(), Some("debug"));
        assert_eq!(args.max_sessions, Some(10));
        assert!(args.command.is_none());
    }

    #[test]
    fn test_subcommand_parsing() {
        let args = Args::parse_from(["embedded-debugger-mcp", "probes", "list", "--json"]);
        assert_eq!(
            args.command,
            Some(CliCommand::Probes {
                action: ProbeCommand::List { json: true }
            })
        );
    }

    #[test]
    fn test_skill_install_subcommand_parsing() {
        let args = Args::parse_from([
            "embedded-debugger-mcp",
            "skill",
            "install",
            "--target",
            "codex",
            "--dry-run",
            "--json",
        ]);
        assert_eq!(
            args.command,
            Some(CliCommand::Skill {
                action: SkillCommand::Install {
                    target: SkillInstallTarget::Codex,
                    home: None,
                    dry_run: true,
                    force: false,
                    json: true,
                }
            })
        );
    }

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.validate().is_ok());
        assert_eq!(config.server.max_sessions, 5);
        assert_eq!(config.debugger.default_speed_khz, 4000);
    }

    #[tokio::test]
    async fn test_compat_transport_handles_discover_and_pre_handshake_notifications() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

        let (mut client_in_tx, client_in_rx) = tokio::io::duplex(4096);
        let (client_out_tx, client_out_rx) = tokio::io::duplex(4096);

        let (rmcp_in_rx, _rmcp_out_tx) = compat_transport(client_in_rx, client_out_tx);
        let mut client_out_reader = BufReader::new(client_out_rx);
        let mut rmcp_in_reader = BufReader::new(rmcp_in_rx);

        // 1. Send MCP 2026-07-28 `server/discover` request -> should get -32601 on stdout without closing rmcp stream
        client_in_tx
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"server/discover\",\"params\":{\"protocolVersion\":\"2026-07-28\"}}\n")
            .await
            .unwrap();
        client_in_tx.flush().await.unwrap();

        let mut out_line = String::new();
        client_out_reader.read_line(&mut out_line).await.unwrap();
        let resp: serde_json::Value = serde_json::from_str(&out_line).unwrap();
        assert_eq!(resp["id"], 1);
        assert_eq!(resp["error"]["code"], -32601);

        // 2. Send a pre-handshake notification (`notifications/roots/list_changed`) -> should be filtered out before `notifications/initialized`
        client_in_tx
            .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/roots/list_changed\"}\n")
            .await
            .unwrap();

        // 3. Send legacy `initialize` request -> should arrive first on rmcp_in_rx
        client_in_tx
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-06-18\"}}\n")
            .await
            .unwrap();
        client_in_tx.flush().await.unwrap();

        let mut rmcp_line = String::new();
        rmcp_in_reader.read_line(&mut rmcp_line).await.unwrap();
        let forwarded: serde_json::Value = serde_json::from_str(&rmcp_line).unwrap();
        assert_eq!(forwarded["id"], 2);
        assert_eq!(forwarded["method"], "initialize");
        assert!(forwarded["params"]["capabilities"].is_object());
        assert!(forwarded["params"]["clientInfo"].is_object());
    }
}
