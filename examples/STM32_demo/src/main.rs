#![no_std]
#![no_main]

use panic_halt as _;

use rtt_target::{rtt_init, ChannelMode::NoBlockSkip};
use core::fmt::Write;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_time::{Duration, Timer};

// Global system state
static mut LED_STATE: bool = false;
static mut CALCULATION_ACTIVE: bool = true;
static mut SPEED_MULTIPLIER: u32 = 1;
static mut FIB_INDEX: u32 = 0;

// Fibonacci calculation function
fn fibonacci(n: u32) -> u64 {
    if n <= 1 {
        n as u64
    } else {
        let mut a: u64 = 0;
        let mut b: u64 = 1;
        for _ in 2..=n {
            let temp = a.saturating_add(b);
            a = b;
            b = temp;
        }
        b
    }
}

// Command processing function - correct API for rtt-target v0.5.0
async fn process_commands(
    up_channels: &mut (rtt_target::UpChannel, rtt_target::UpChannel, rtt_target::UpChannel),
    down_channels: &mut (rtt_target::DownChannel, rtt_target::DownChannel),
    cmd_buf: &mut [u8],
    config_buf: &mut [u8],
) {
    // Process single-character commands from down channel 0
    let cmd_count = down_channels.0.read(cmd_buf);
    for i in 0..cmd_count {
        let cmd = cmd_buf[i] as char;
        match cmd {
            'L' => {
                unsafe { LED_STATE = !LED_STATE; }
                writeln!(up_channels.0, "LED toggled: {}", unsafe { LED_STATE }).ok();
            }
            'R' => {
                unsafe { FIB_INDEX = 0; }
                writeln!(up_channels.0, "Fibonacci counter reset to 0").ok();
            }
            'S' => {
                unsafe { CALCULATION_ACTIVE = !CALCULATION_ACTIVE; }
                writeln!(up_channels.0, "Calculation {}", 
                       if unsafe { CALCULATION_ACTIVE } { "started" } else { "stopped" }).ok();
            }
            'F' => {
                let current_fib = fibonacci(unsafe { FIB_INDEX });
                writeln!(up_channels.0, "Current: F({}) = {}", unsafe { FIB_INDEX }, current_fib).ok();
            }
            'I' => {
                writeln!(up_channels.0, "System: STM32G431CBTx RTT Bidirectional Demo").ok();
                writeln!(up_channels.2, "Status - Speed:{}, LED:{}, Calc:{}, Index:{}", 
                       unsafe { SPEED_MULTIPLIER }, 
                       unsafe { LED_STATE }, 
                       unsafe { CALCULATION_ACTIVE },
                       unsafe { FIB_INDEX }).ok();
            }
            'P' => {
                writeln!(up_channels.0, "Pausing for 5 seconds...").ok();
                Timer::after(Duration::from_secs(5)).await;
                writeln!(up_channels.0, "Pause complete").ok();
            }
            '0'..='9' => {
                let speed = (cmd_buf[i] - b'0' + 1) as u32;
                unsafe { SPEED_MULTIPLIER = speed; }
                writeln!(up_channels.0, "Speed set to: {}x", speed).ok();
            }
            _ => {
                writeln!(up_channels.2, "Unknown command: '{}' (0x{:02X})", cmd, cmd_buf[i]).ok();
            }
        }
    }
    
    // Process configuration commands from down channel 1
    let config_count = down_channels.1.read(config_buf);
    if config_count > 0 {
        if let Ok(config_str) = core::str::from_utf8(&config_buf[..config_count]) {
            writeln!(up_channels.0, "Config received: {}", config_str).ok();
            
            // Parse configuration commands
            if config_str.starts_with("SPEED:") {
                if let Ok(speed) = config_str[6..].parse::<u32>() {
                    if speed >= 1 && speed <= 10 {
                        unsafe { SPEED_MULTIPLIER = speed; }
                        writeln!(up_channels.0, "Speed configured to: {}x", speed).ok();
                    } else {
                        writeln!(up_channels.0, "Invalid speed: {} (range: 1-10)", speed).ok();
                    }
                }
            } else if config_str == "LED:ON" {
                unsafe { LED_STATE = true; }
                writeln!(up_channels.0, "LED configured ON").ok();
            } else if config_str == "LED:OFF" {
                unsafe { LED_STATE = false; }
                writeln!(up_channels.0, "LED configured OFF").ok();
            } else if config_str == "MODE:AUTO" {
                unsafe { CALCULATION_ACTIVE = true; }
                writeln!(up_channels.0, "Auto calculation mode enabled").ok();
            } else if config_str == "MODE:MANUAL" {
                unsafe { CALCULATION_ACTIVE = false; }
                writeln!(up_channels.0, "Manual calculation mode enabled").ok();
            } else if config_str == "RESET" {
                unsafe { 
                    FIB_INDEX = 0; 
                    CALCULATION_ACTIVE = true;
                    SPEED_MULTIPLIER = 1;
                    LED_STATE = false;
                }
                writeln!(up_channels.0, "System configuration reset").ok();
            } else {
                writeln!(up_channels.2, "Unknown config: {}", config_str).ok();
            }
        } else {
            writeln!(up_channels.2, "Invalid UTF-8 config data received").ok();
        }
    }
}

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    // Initialize STM32 hardware
    let p = embassy_stm32::init(Default::default());
    let mut led = Output::new(p.PB7, Level::Low, Speed::Low);
    
    // Initialize RTT with bidirectional channels
    let mut channels = rtt_init! {
        up: {
            0: { 
                size: 1024, 
                mode: NoBlockSkip, 
                name: "Terminal" 
            }
            1: { 
                size: 512, 
                mode: NoBlockSkip, 
                name: "Data" 
            }
            2: { 
                size: 256, 
                mode: NoBlockSkip, 
                name: "Debug" 
            }
        }
        down: {
            0: { 
                size: 64, 
                mode: NoBlockSkip, 
                name: "Commands" 
            }
            1: { 
                size: 128, 
                mode: NoBlockSkip, 
                name: "Config" 
            }
        }
    };

    // RTT channels already initialized above, no need for rtt_init_print!()

    // Startup messages
    writeln!(channels.up.0, "=== RTT Bidirectional Communication Demo ===").ok();
    writeln!(channels.up.0, "Target: STM32G431CBTx").ok();
    writeln!(channels.up.0, "Channels: 3 Up (Terminal/Data/Debug), 2 Down (Commands/Config)").ok();
    writeln!(channels.up.2, "RTT initialization complete").ok();
    
    writeln!(channels.up.0, "").ok();
    writeln!(channels.up.0, "Available Commands (send to down channel 0):").ok();
    writeln!(channels.up.0, "  L - Toggle LED").ok();
    writeln!(channels.up.0, "  R - Reset Fibonacci counter").ok();
    writeln!(channels.up.0, "  S - Start/Stop calculation").ok();
    writeln!(channels.up.0, "  F - Get current Fibonacci value").ok();
    writeln!(channels.up.0, "  I - System information").ok();
    writeln!(channels.up.0, "  P - Pause 5 seconds").ok();
    writeln!(channels.up.0, "  0-9 - Set speed (1x-10x)").ok();
    writeln!(channels.up.0, "").ok();
    writeln!(channels.up.0, "Config Commands (send to down channel 1):").ok();
    writeln!(channels.up.0, "  SPEED:n, LED:ON/OFF, MODE:AUTO/MANUAL, RESET").ok();
    writeln!(channels.up.0, "").ok();
    
    // Command and config buffers
    let mut cmd_buffer = [0u8; 64];
    let mut config_buffer = [0u8; 128];
    
    // System status counters
    let mut loop_counter = 0u32;
    let mut last_status_report = 0u32;

    writeln!(channels.up.0, "Starting main loop...").ok();

    loop {
        loop_counter += 1;
        
        // Process incoming commands
        process_commands(&mut channels.up, &mut channels.down, &mut cmd_buffer, &mut config_buffer).await;
        
        // Fibonacci calculation (if active)
        if unsafe { CALCULATION_ACTIVE } {
            let fib_value = fibonacci(unsafe { FIB_INDEX });
            writeln!(channels.up.1, "F({}) = {}", unsafe { FIB_INDEX }, fib_value).ok();
            
            // Milestone reporting
            let current_index = unsafe { FIB_INDEX };
            if current_index > 0 && current_index % 10 == 0 {
                writeln!(channels.up.0, "Milestone: {} Fibonacci numbers calculated", current_index).ok();
            }
            
            unsafe { FIB_INDEX += 1; }
            
            // Prevent overflow by resetting at very large numbers
            if unsafe { FIB_INDEX } > 100 {
                writeln!(channels.up.0, "Fibonacci counter reached 100, resetting to prevent overflow").ok();
                unsafe { FIB_INDEX = 0; }
            }
        }
        
        // LED control based on state
        if unsafe { LED_STATE } {
            led.set_high();
        } else {
            led.set_low();
        }
        
        // Periodic status report (every 50 loops)
        if loop_counter - last_status_report >= 50 {
            writeln!(channels.up.2, "Status: Loop#{}, Speed:{}x, LED:{}, Calc:{}, FibIdx:{}", 
                   loop_counter,
                   unsafe { SPEED_MULTIPLIER },
                   unsafe { LED_STATE },
                   unsafe { CALCULATION_ACTIVE },
                   unsafe { FIB_INDEX }).ok();
            last_status_report = loop_counter;
        }
        
        // Variable delay based on speed multiplier
        let delay_ms = 1000 / unsafe { SPEED_MULTIPLIER };
        Timer::after(Duration::from_millis(delay_ms.into())).await;
    }
}