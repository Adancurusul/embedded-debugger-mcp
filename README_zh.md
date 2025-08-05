# 嵌入式调试器 MCP 服务器

[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://rust-lang.org)
[![RMCP](https://img.shields.io/badge/RMCP-0.3.2-blue.svg)](https://github.com/modelcontextprotocol/rust-sdk)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

专业的模型上下文协议 (MCP) 嵌入式调试服务器，基于 probe-rs 构建。为 AI 助手提供包括 ARM Cortex-M、RISC-V 微控制器在内的全面嵌入式系统调试功能，支持真实硬件集成。

> 📖 **语言版本**: [English](README.md) | [中文](README_zh.md)

## ✨ 功能特性

- 🚀 **生产就绪**: 真实硬件集成，提供22个综合调试工具
- 🔌 **多探针支持**: J-Link, ST-Link V2/V3, DAPLink, Black Magic Probe
- 🎯 **完整调试控制**: 连接、暂停、运行、复位、单步执行
- 💾 **内存操作**: 支持多种数据格式的Flash和RAM读写
- 🛑 **断点管理**: 硬件和软件断点的实时控制
- 📱 **Flash编程**: 完整的Flash操作 - 擦除、编程、验证
- 📡 **RTT双向通信**: 实时传输，支持交互式命令/响应系统
- 🏗️ **多架构支持**: ARM Cortex-M, RISC-V，经过STM32集成测试
- 🤖 **AI集成**: 与Claude和其他AI助手完美兼容
- 🧪 **全面测试**: 所有22个工具在真实STM32G431CBTx硬件上验证通过

## 🏗️ 架构

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   MCP 客户端    │◄──►│  嵌入式调试器    │◄──►│  调试探针       │
│   (Claude/AI)   │    │  MCP 服务器      │    │  硬件           │
└─────────────────┘    └──────────────────┘    └─────────────────┘
                              │
                              ▼
                       ┌──────────────────┐
                       │  目标设备        │
                       │  (ARM/RISC-V)    │
                       └──────────────────┘
```

## 🚀 快速开始

### 前置要求

**硬件要求:**
- **调试探针**: ST-Link V2/V3, J-Link, 或 DAPLink 兼容探针
- **目标板**: STM32 或其他支持的微控制器
- **连接线**: 用于探针和目标板的USB线

**软件要求:**
- Rust 1.70+ 
- probe-rs 兼容的调试探针驱动程序

### 安装

```bash
# 克隆并从源码构建
git clone https://github.com/adancurusul/embedded-debugger-mcp.git
cd embedded-debugger-mcp
cargo build --release
```

### 基本使用

1. **启动 MCP 服务器**
```bash
# 运行 MCP 服务器
./target/release/embedded-debugger-mcp

# 带调试日志
RUST_LOG=debug ./target/release/embedded-debugger-mcp

# 使用自定义配置
./target/release/embedded-debugger-mcp --config config.toml
```

2. **配置 MCP 客户端**

#### Claude Desktop 配置示例

添加到 Claude Desktop 配置文件:

**Windows 示例:**
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

**macOS/Linux 示例:**
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

其他例如cursor ,claude code 等参考对应工具文档

## 🎯 试试 STM32 演示

我们提供了一个全面的 **STM32 RTT 双向通信演示**，展示了所有功能：

```bash
# 进入示例目录
cd examples/STM32_demo

# 构建固件
cargo build --release

# 与 MCP 服务器配合使用，获得完整的调试体验
```

**演示内容:**
- ✅ **交互式 RTT 通信**: 发送命令并获得实时响应
- ✅ **全部 22 个 MCP 工具**: 在真实 STM32 硬件上完整验证
- ✅ **斐波那契计算器**: 实时数据流与控制命令
- ✅ **硬件集成**: 在 STM32G431CBTx + ST-Link V2 上测试

[📖 查看 STM32 演示文档 →](examples/STM32_demo/README.md)

### AI 助手使用示例

#### 列出可用的调试探针
```
请列出系统上可用的调试探针
```

#### 连接并烧录固件
```
使用 ST-Link 探针连接到我的 STM32G431CBTx，然后烧录位于 examples/STM32_demo/target/thumbv7em-none-eabi/release/STM32_demo 的固件
```

#### 交互式 RTT 通信
```
请连接 RTT 并显示终端通道的数据。然后发送命令 'L' 来切换 LED。
```

#### 内存分析
```
读取地址 0x08000000 处的 64 字节内存并分析数据格式
```

#### 测试全部 22 个 MCP 工具
```
请帮我测试所有 22 个 MCP 嵌入式调试工具与我的 STM32 开发板。先连接到探针，然后系统性地测试每个工具类别：探针管理、内存操作、调试控制、断点、Flash 操作、RTT 通信和会话管理。
```

## 🛠️ 完整工具集 (22个工具)

所有工具均通过真实 STM32 硬件测试和验证：

### 🔌 探针管理 (3个工具)
| 工具 | 描述 | 状态 |
|------|------|------|
| `list_probes` | 发现可用的调试探针 | ✅ 生产就绪 |
| `connect` | 连接到探针和目标芯片 | ✅ 生产就绪 |
| `probe_info` | 获取详细会话信息 | ✅ 生产就绪 |

### 💾 内存操作 (2个工具) 
| 工具 | 描述 | 状态 |
|------|------|------|
| `read_memory` | 支持多种格式的Flash/RAM读取 | ✅ 生产就绪 |
| `write_memory` | 向目标内存写入数据 | ✅ 生产就绪 |

### 🎯 调试控制 (4个工具)
| 工具 | 描述 | 状态 |
|------|------|------|
| `halt` | 停止目标执行 | ✅ 生产就绪 |
| `run` | 恢复目标执行 | ✅ 生产就绪 |
| `reset` | 硬件/软件复位 | ✅ 生产就绪 |
| `step` | 单指令步进 | ✅ 生产就绪 |

### 🛑 断点管理 (2个工具)
| 工具 | 描述 | 状态 |
|------|------|------|
| `set_breakpoint` | 设置硬件/软件断点 | ✅ 生产就绪 |
| `clear_breakpoint` | 移除断点 | ✅ 生产就绪 |

### 📱 Flash 操作 (3个工具)
| 工具 | 描述 | 状态 |
|------|------|------|
| `flash_erase` | 擦除Flash内存扇区/芯片 | ✅ 生产就绪 |
| `flash_program` | 编程 ELF/HEX/BIN 文件 | ✅ 生产就绪 |
| `flash_verify` | 验证Flash内容 | ✅ 生产就绪 |

### 📡 RTT 通信 (6个工具)
| 工具 | 描述 | 状态 |
|------|------|------|
| `rtt_attach` | 连接到RTT通信 | ✅ 生产就绪 |
| `rtt_detach` | 断开RTT连接 | ✅ 生产就绪 |
| `rtt_channels` | 列出可用的RTT通道 | ✅ 生产就绪 |
| `rtt_read` | 从RTT上行通道读取 | ✅ 生产就绪 |
| `rtt_write` | 向RTT下行通道写入 | ✅ 生产就绪 |
| `run_firmware` | 完整部署 + RTT | ✅ 生产就绪 |

### 📊 会话管理 (2个工具)
| 工具 | 描述 | 状态 |
|------|------|------|
| `get_status` | 获取当前调试状态 | ✅ 生产就绪 |
| `disconnect` | 清理会话终止 | ✅ 生产就绪 |

**✅ 22/22 工具 - 真实硬件 100% 成功率**

## 🌍 支持的硬件

### 调试探针
- **J-Link**: Segger J-Link (所有变体)
- **ST-Link**: ST-Link/V2, ST-Link/V3
- **DAPLink**: ARM DAPLink 兼容探针
- **Black Magic Probe**: Black Magic Probe
- **FTDI**: 基于 FTDI 的调试探针

### 目标架构
- **ARM Cortex-M**: M0, M0+, M3, M4, M7, M23, M33
- **RISC-V**: 各种 RISC-V 核心
- **ARM Cortex-A**: 基本支持


## 🏆 生产状态

### ✅ 完全实现并测试

**当前状态: 生产就绪**

- ✅ **完整的 probe-rs 集成**: 所有22个工具的真实硬件调试
- ✅ **硬件验证**: 在 STM32G431CBTx + ST-Link V2 上测试
- ✅ **RTT 双向通信**: 完整的交互式通信与实时命令
- ✅ **Flash 操作**: 完整的擦除、编程、验证工作流
- ✅ **会话管理**: 多会话支持与强大的错误处理
- ✅ **AI 集成**: 完美的 MCP 协议兼容性

## 🙏 致谢

感谢以下开源项目：

- [probe-rs](https://probe.rs/) - 嵌入式调试工具包
- [rmcp](https://github.com/modelcontextprotocol/rust-sdk) - Rust MCP SDK  
- [tokio](https://tokio.rs/) - 异步运行时




## 📄 许可证

本项目采用 MIT 许可证。详细信息请参阅 [LICENSE](LICENSE) 文件。
---

⭐ 如果这个项目对你有帮助，请给我们一个 Star！

