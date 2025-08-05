# Embedded Debugger MCP Server

[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://rust-lang.org)
[![RMCP](https://img.shields.io/badge/RMCP-0.3.2-blue.svg)](https://github.com/modelcontextprotocol/rust-sdk)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

A professional Model Context Protocol (MCP) server for embedded debugging with probe-rs. Provides AI assistants with comprehensive debugging capabilities for embedded systems including ARM Cortex-M, RISC-V microcontrollers with real hardware integration.

> 📖 **Language Versions**: [English](README.md) | [中文](README_zh.md)

## ✨ Features

- 🚀 **Production Ready**: Real hardware integration with 22 comprehensive debugging tools
- 🔌 **Multi-Probe Support**: J-Link, ST-Link V2/V3, DAPLink, Black Magic Probe
- 🎯 **Complete Debug Control**: Connect, halt, run, reset, single-step execution  
- 💾 **Memory Operations**: Read/write flash and RAM with multiple data formats
- 🛑 **Breakpoint Management**: Hardware and software breakpoints with real-time control
- 📱 **Flash Programming**: Complete flash operations - erase, program, verify
- 📡 **RTT Bidirectional**: Real-Time Transfer with interactive command/response system
- 🏗️ **Multi-Architecture**: ARM Cortex-M, RISC-V with tested STM32 integration
- 🤖 **AI Integration**: Perfect compatibility with Claude and other AI assistants
- 🧪 **Comprehensive Testing**: All 22 tools validated with real STM32G431CBTx hardware

## 🏗️ Architecture

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   MCP Client    │◄──►│  Embedded        │◄──►│  Debug Probe    │
│   (Claude/AI)   │    │  Debugger MCP    │    │  Hardware       │
└─────────────────┘    └──────────────────┘    └─────────────────┘
                              │
                              ▼
                       ┌──────────────────┐
                       │  Target Device   │
                       │  (ARM/RISC-V)    │
                       └──────────────────┘
```

## 🚀 Quick Start

### Prerequisites

**Hardware Requirements:**
- **Debug Probe**: ST-Link V2/V3, J-Link, or DAPLink compatible probe
- **Target Board**: STM32 or other supported microcontroller
- **Connection**: USB cables for probe and target board

**Software Requirements:**
- Rust 1.70+ 
- probe-rs compatible debug probe drivers

### Installation

```bash
# Clone and build from source
git clone https://github.com/adancurusul/embedded-debugger-mcp.git
cd embedded-debugger-mcp
cargo build --release
```

### Basic Usage

**Configure MCP Clients**

#### Claude Desktop Configuration Example

Add to Claude Desktop configuration file:

**Windows Example:**
```json
{
  "mcpServers": {
    "embedded-debugger": {
      "command": "C:\\path\\to\\debugger-mcp-rs\\target\\release\\embedded-debugger-mcp.exe",
      "args": [],
      "env": {
        "RUST_LOG": "info"
      }
    }
  }
}
```

**macOS/Linux Example:**
```json
{
  "mcpServers": {
    "embedded-debugger": {
      "command": "/path/to/debugger-mcp-rs/target/release/embedded-debugger-mcp",
      "args": [],
      "env": {
        "RUST_LOG": "info"
      }
    }
  }
}
```

Other examples for other tools like cursor ,claude code  etc. please refer to the corresponding tool documentation

## 🎯 Try the STM32 Demo

We provide a comprehensive **STM32 RTT Bidirectional Demo** that showcases all capabilities:

```bash
# Navigate to the example
cd examples/STM32_demo

# Build the firmware  
cargo build --release

# Use with MCP server for complete debugging experience
```

**What the demo shows:**
- ✅ **Interactive RTT Communication**: Send commands and get real-time responses
- ✅ **All 22 MCP Tools**: Complete validation with real STM32 hardware
- ✅ **Fibonacci Calculator**: Live data streaming with control commands
- ✅ **Hardware Integration**: Tested with STM32G431CBTx + ST-Link V2

[📖 View STM32 Demo Documentation →](examples/STM32_demo/README.md)

### Usage Examples with AI Assistants

#### List Available Debug Probes
```
Please list available debug probes on the system
```

#### Connect and Flash Firmware
```
Connect to my STM32G431CBTx using ST-Link probe, then flash the firmware at examples/STM32_demo/target/thumbv7em-none-eabi/release/STM32_demo
```

#### Interactive RTT Communication
```
Please attach RTT and show me the data from the terminal channel. Then send a command 'L' to toggle the LED.
```

#### Memory Analysis
```
Read 64 bytes of memory from address 0x08000000 and analyze the data format
```

#### Test All 22 MCP Tools
```
Please help me test all 22 MCP embedded debugger tools with my STM32 board. Start by connecting to the probe, then systematically test each tool category: probe management, memory operations, debug control, breakpoints, flash operations, RTT communication, and session management.
```

## 🛠️ Complete Tool Set (22 Tools)

All tools tested and validated with real STM32 hardware:

### 🔌 Probe Management (3 tools)
| Tool | Description | Status |
|------|-------------|---------|
| `list_probes` | Discover available debug probes | ✅ Production Ready |
| `connect` | Connect to probe and target chip | ✅ Production Ready |
| `probe_info` | Get detailed session information | ✅ Production Ready |

### 💾 Memory Operations (2 tools) 
| Tool | Description | Status |
|------|-------------|---------|
| `read_memory` | Read flash/RAM with multiple formats | ✅ Production Ready |
| `write_memory` | Write to target memory | ✅ Production Ready |

### 🎯 Debug Control (4 tools)
| Tool | Description | Status |
|------|-------------|---------|
| `halt` | Stop target execution | ✅ Production Ready |
| `run` | Resume target execution | ✅ Production Ready |
| `reset` | Hardware/software reset | ✅ Production Ready |
| `step` | Single instruction stepping | ✅ Production Ready |

### 🛑 Breakpoint Management (2 tools)
| Tool | Description | Status |
|------|-------------|---------|
| `set_breakpoint` | Set hardware/software breakpoints | ✅ Production Ready |
| `clear_breakpoint` | Remove breakpoints | ✅ Production Ready |

### 📱 Flash Operations (3 tools)
| Tool | Description | Status |
|------|-------------|---------|
| `flash_erase` | Erase flash memory sectors/chip | ✅ Production Ready |
| `flash_program` | Program ELF/HEX/BIN files | ✅ Production Ready |
| `flash_verify` | Verify flash contents | ✅ Production Ready |

### 📡 RTT Communication (6 tools)
| Tool | Description | Status |
|------|-------------|---------|
| `rtt_attach` | Connect to RTT communication | ✅ Production Ready |
| `rtt_detach` | Disconnect RTT | ✅ Production Ready |
| `rtt_channels` | List available RTT channels | ✅ Production Ready |
| `rtt_read` | Read from RTT up channels | ✅ Production Ready |
| `rtt_write` | Write to RTT down channels | ✅ Production Ready |
| `run_firmware` | Complete deployment + RTT | ✅ Production Ready |

### 📊 Session Management (2 tools)
| Tool | Description | Status |
|------|-------------|---------|
| `get_status` | Get current debug status | ✅ Production Ready |
| `disconnect` | Clean session termination | ✅ Production Ready |

**✅ 22/22 Tools - 100% Success Rate with Real Hardware**

## 🌍 Supported Hardware

### Debug Probes
- **J-Link**: Segger J-Link (all variants)
- **ST-Link**: ST-Link/V2, ST-Link/V3
- **DAPLink**: ARM DAPLink compatible probes
- **Black Magic Probe**: Black Magic Probe
- **FTDI**: FTDI-based debug probes

### Target Architectures
- **ARM Cortex-M**: M0, M0+, M3, M4, M7, M23, M33
- **RISC-V**: Various RISC-V cores
- **ARM Cortex-A**: Basic support

## 🏆 Production Status

### ✅ Fully Implemented and Tested

**Current Status: PRODUCTION READY**

- ✅ **Complete probe-rs Integration**: Real hardware debugging with all 22 tools
- ✅ **Hardware Validation**: Tested with STM32G431CBTx + ST-Link V2
- ✅ **RTT Bidirectional**: Full interactive communication with real-time commands
- ✅ **Flash Operations**: Complete erase, program, verify workflow
- ✅ **Session Management**: Multi-session support with robust error handling
- ✅ **AI Integration**: Perfect MCP protocol compatibility

## 🙏 Acknowledgments

Thanks to the following open source projects:

- [probe-rs](https://probe.rs/) - Embedded debugging toolkit
- [rmcp](https://github.com/modelcontextprotocol/rust-sdk) - Rust MCP SDK
- [tokio](https://tokio.rs/) - Async runtime

## 📄 License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.

---

⭐ If this project helps you, please give us a Star!