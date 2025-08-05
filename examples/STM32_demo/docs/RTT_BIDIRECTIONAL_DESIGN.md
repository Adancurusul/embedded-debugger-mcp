# RTT Bidirectional Communication Implementation Design

## Overview

This document outlines the design and implementation of bidirectional RTT (Real-Time Transfer) communication for testing MCP embedded debugger functionality. The implementation will support both up channels (target→host) and down channels (host→target) to enable comprehensive testing of RTT read/write operations.

## Current Status Analysis

### Existing Implementation
- **Current demo**: Uses defmt-rtt for logging only (up channel)
- **Channels**: 1 up channel (defmt logging), 0 down channels
- **Target**: STM32G431CBTx (noted: Embed.toml shows STM32G031G8Ux - needs update)
- **MCP Server**: Supports RTT read/write but write currently fails due to missing down channels

### Limitations
- No bidirectional communication capability
- Cannot test MCP RTT write functionality
- Limited to logging-only use case

## Design Goals

1. **Bidirectional Communication**: Enable both host→target and target→host data flow
2. **Multiple Channels**: Implement multiple up and down channels for testing
3. **Command Processing**: Simple command interface via down channels
4. **Data Streaming**: Multiple data types via up channels
5. **MCP Testing**: Comprehensive validation of all RTT MCP tools
6. **Real-time**: Maintain real-time characteristics without blocking

## Implementation Architecture

### Channel Configuration

```rust
// Target-side channel setup using rtt-target crate
let channels = rtt_init! {
    up: {
        0: { 
            size: 1024, 
            mode: NoBlockSkip, 
            name: "Terminal" 
        }      // General logging & status
        1: { 
            size: 512, 
            mode: NoBlockSkip, 
            name: "Data" 
        }         // Fibonacci results & telemetry
        2: { 
            size: 256, 
            mode: NoBlockSkip, 
            name: "Debug" 
        }        // Debug information
    }
    down: {
        0: { 
            size: 64, 
            mode: NoBlockSkip, 
            name: "Commands" 
        }      // Single-character commands
        1: { 
            size: 128, 
            mode: NoBlockSkip, 
            name: "Config" 
        }       // Configuration data
    }
};
```

### Channel Purpose

| Channel | Direction | Purpose | Size | Format |
|---------|-----------|---------|------|--------|
| Up 0 | Target→Host | Terminal logging | 1024B | String |
| Up 1 | Target→Host | Fibonacci & telemetry | 512B | String |
| Up 2 | Target→Host | Debug information | 256B | String |
| Down 0 | Host→Target | Single-char commands | 64B | Raw bytes |
| Down 1 | Host→Target | Configuration data | 128B | Raw bytes |

### Command Interface Design

#### Simple Commands (Down Channel 0)
```
'L' - Toggle LED
'R' - Reset Fibonacci counter
'S' - Start/Stop Fibonacci calculation  
'F' - Request current Fibonacci value
'I' - Request system information
'P' - Pause for 5 seconds
'0'-'9' - Set calculation speed (0=fastest, 9=slowest)
```

#### Configuration Commands (Down Channel 1)
```
"SPEED:n" - Set speed multiplier (n=1-10)
"LED:ON/OFF" - Control LED state
"MODE:AUTO/MANUAL" - Set calculation mode
"RESET" - Full system reset
```

## Implementation Plan

### Phase 1: Dependencies and Setup
1. Add `rtt-target` dependency to Cargo.toml
2. Remove `defmt-rtt` dependency (replaced by rtt-target)
3. Update target chip in Embed.toml
4. Import necessary RTT modules

### Phase 2: Core RTT Implementation
1. Initialize RTT with multiple channels
2. Replace defmt logging with RTT channel writes
3. Implement command reading infrastructure
4. Add command processing logic

### Phase 3: Enhanced Features
1. Add bidirectional data flow
2. Implement various command types
3. Add telemetry and status reporting
4. Integrate with existing Fibonacci calculation

### Phase 4: Testing and Validation
1. Test each RTT channel individually
2. Validate MCP RTT read operations
3. Validate MCP RTT write operations
4. Performance and stability testing

## Detailed Implementation

### Dependencies Update

```toml
# Remove defmt-rtt, add rtt-target
[dependencies]
# defmt-rtt = { version = "1.0.0", optional = true }  # Remove
rtt-target = "0.5"  # Add

# Remove defmt feature from default
[features]
default = ["panic-probe"]  # Remove defmt-rtt
```

### Main Application Structure

```rust
#![no_std]
#![no_main]

use rtt_target::{rtt_init, rprintln};
use core::fmt::Write;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_time::{Duration, Timer};

// Global state for commands
static mut LED_STATE: bool = false;
static mut CALCULATION_ACTIVE: bool = true;
static mut SPEED_MULTIPLIER: u32 = 1;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
    let mut led = Output::new(p.PB7, Level::Low, Speed::Low);
    
    // Initialize RTT with bidirectional channels
    let channels = rtt_init! {
        up: {
            0: { size: 1024, mode: NoBlockSkip, name: "Terminal" }
            1: { size: 512, mode: NoBlockSkip, name: "Data" }
            2: { size: 256, mode: NoBlockSkip, name: "Debug" }
        }
        down: {
            0: { size: 64, mode: NoBlockSkip, name: "Commands" }
            1: { size: 128, mode: NoBlockSkip, name: "Config" }
        }
    };

    writeln!(channels.up.0, "RTT Bidirectional Demo Started").ok();
    writeln!(channels.up.2, "Channels: 3 Up, 2 Down").ok();
    
    let mut fib_index = 0u32;
    let mut cmd_buffer = [0u8; 64];
    let mut config_buffer = [0u8; 128];

    loop {
        // Process incoming commands
        process_commands(&channels, &mut cmd_buffer, &mut config_buffer, &mut led).await;
        
        // Fibonacci calculation (if active)
        if unsafe { CALCULATION_ACTIVE } {
            let fib_value = fibonacci(fib_index);
            writeln!(channels.up.1, "F({}) = {}", fib_index, fib_value).ok();
            
            if fib_index % 10 == 0 {
                writeln!(channels.up.0, "Milestone: {} Fibonacci numbers calculated", fib_index).ok();
            }
            
            fib_index += 1;
        }
        
        // LED control
        if unsafe { LED_STATE } {
            led.set_high();
        } else {
            led.set_low();
        }
        
        // Variable delay based on speed setting
        let delay_ms = 1000 * unsafe { SPEED_MULTIPLIER };
        Timer::after(Duration::from_millis(delay_ms.into())).await;
    }
}
```

### Command Processing Implementation

```rust
async fn process_commands(
    channels: &rtt_target::Channels,
    cmd_buf: &mut [u8],
    config_buf: &mut [u8], 
    led: &mut Output<'_>
) {
    // Process single-character commands
    if let Ok(bytes) = channels.down.0.read(cmd_buf) {
        for i in 0..bytes {
            match cmd_buf[i] as char {
                'L' => {
                    unsafe { LED_STATE = !LED_STATE; }
                    writeln!(channels.up.0, "LED toggled: {}", unsafe { LED_STATE }).ok();
                }
                'R' => {
                    // Reset handled in main loop
                    writeln!(channels.up.0, "Fibonacci counter reset requested").ok();
                }
                'S' => {
                    unsafe { CALCULATION_ACTIVE = !CALCULATION_ACTIVE; }
                    writeln!(channels.up.0, "Calculation {}", 
                           if unsafe { CALCULATION_ACTIVE } { "started" } else { "stopped" }).ok();
                }
                'F' => {
                    writeln!(channels.up.0, "Current Fibonacci index: {}", /* current index */).ok();
                }
                'I' => {
                    writeln!(channels.up.0, "System: STM32G431CBTx, RTT Bidirectional Demo").ok();
                    writeln!(channels.up.2, "Speed: {}, LED: {}, Calc: {}", 
                           unsafe { SPEED_MULTIPLIER }, 
                           unsafe { LED_STATE }, 
                           unsafe { CALCULATION_ACTIVE }).ok();
                }
                'P' => {
                    writeln!(channels.up.0, "Pausing for 5 seconds...").ok();
                    Timer::after(Duration::from_secs(5)).await;
                    writeln!(channels.up.0, "Pause complete").ok();
                }
                '0'..='9' => {
                    let speed = (cmd_buf[i] - b'0' + 1) as u32;
                    unsafe { SPEED_MULTIPLIER = speed; }
                    writeln!(channels.up.0, "Speed set to: {}", speed).ok();
                }
                _ => {
                    writeln!(channels.up.2, "Unknown command: 0x{:02X}", cmd_buf[i]).ok();
                }
            }
        }
    }
    
    // Process configuration commands
    if let Ok(bytes) = channels.down.1.read(config_buf) {
        if bytes > 0 {
            if let Ok(s) = core::str::from_utf8(&config_buf[..bytes]) {
                writeln!(channels.up.0, "Config received: {}", s).ok();
                // Parse and process configuration
                process_config_string(s, channels);
            }
        }
    }
}
```

## Testing Strategy

### MCP Tool Testing Sequence

1. **RTT Attachment**: Verify all channels are detected
2. **Channel Discovery**: Confirm 3 up, 2 down channels
3. **Up Channel Reading**: Test reading from channels 0, 1, 2
4. **Down Channel Writing**: Test writing commands to channels 0, 1
5. **Command Processing**: Verify commands are executed
6. **Bidirectional Flow**: Test simultaneous read/write operations

### Test Commands for MCP Validation

```bash
# Test single commands
mcp__embedded-debugger__rtt_write --channel=0 --data="L"  # Toggle LED
mcp__embedded-debugger__rtt_write --channel=0 --data="I"  # System info
mcp__embedded-debugger__rtt_write --channel=0 --data="5"  # Set speed

# Test configuration commands  
mcp__embedded-debugger__rtt_write --channel=1 --data="SPEED:3"
mcp__embedded-debugger__rtt_write --channel=1 --data="LED:ON"

# Read responses
mcp__embedded-debugger__rtt_read --channel=0  # Terminal
mcp__embedded-debugger__rtt_read --channel=1  # Data
mcp__embedded-debugger__rtt_read --channel=2  # Debug
```

## Performance Considerations

### Buffer Sizing
- **Up channels**: Larger buffers for continuous data flow
- **Down channels**: Smaller buffers for command efficiency
- **Total memory**: ~2KB RTT buffer usage

### Real-time Characteristics
- Non-blocking RTT operations
- Command processing in main loop
- No interrupt-based RTT handling
- Graceful degradation under load

## Success Criteria

1. **✅ RTT Initialization**: 3 up + 2 down channels detected
2. **✅ MCP Read Operations**: All up channels readable via MCP
3. **✅ MCP Write Operations**: All down channels writable via MCP  
4. **✅ Command Processing**: Commands executed correctly
5. **✅ Bidirectional Flow**: Simultaneous read/write operations
6. **✅ Stability**: Continuous operation without errors
7. **✅ Performance**: Real-time response to commands

## Implementation Files

### Modified Files
- `Cargo.toml` - Update dependencies
- `Embed.toml` - Correct target chip
- `src/main.rs` - Complete RTT bidirectional implementation
- `src/fmt.rs` - Update logging interface

### New Files
- `RTT_BIDIRECTIONAL_DESIGN.md` - This design document
- `RTT_TESTING_GUIDE.md` - MCP testing procedures

---

**Next Steps**: Begin implementation with Phase 1 (Dependencies and Setup), then proceed through each phase with testing validation at each step.