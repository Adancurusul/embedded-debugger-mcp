//! Debug probe discovery and enumeration

use probe_rs::probe::list::Lister;
use crate::error::{DebugError, Result};
use crate::utils::ProbeType;
use tracing::{debug, info, warn};

/// Information about discovered debug probe
#[derive(Debug, Clone)]
pub struct ProbeInfo {
    pub identifier: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub serial_number: Option<String>,
    pub probe_type: String,
    pub speed_khz: u32,
    pub version: Option<String>,
}

/// Debug probe discovery utility
pub struct ProbeDiscovery;

impl ProbeDiscovery {
    /// List all available debug probes
    pub fn list_probes() -> Result<Vec<ProbeInfo>> {
        debug!("Discovering debug probes");
        
        let lister = Lister::new();
        
        let probes = lister
            .list_all()
            .into_iter()
            .map(|probe_info| {
                let probe_type = ProbeType::from_vid_pid(probe_info.vendor_id, probe_info.product_id);
                
                ProbeInfo {
                    identifier: probe_info.identifier.clone(),
                    vendor_id: probe_info.vendor_id,
                    product_id: probe_info.product_id,
                    serial_number: probe_info.serial_number.clone(),
                    probe_type: probe_type.to_string(),
                    speed_khz: 4000, // Default speed
                    version: Some("USB".to_string()),
                }
            })
            .collect::<Vec<_>>();

        info!("Found {} debug probes", probes.len());
        for probe in &probes {
            debug!("  {} - {} ({})", 
                   probe.identifier, 
                   probe.probe_type, 
                   probe.serial_number.as_deref().unwrap_or("no serial"));
        }

        Ok(probes)
    }

    /// Find a specific probe by selector criteria
    pub fn find_probe(
        serial_number: Option<&str>,
        vendor_id: Option<u16>,
        product_id: Option<u16>,
        probe_type: Option<&str>,
    ) -> Result<ProbeInfo> {
        debug!("Finding probe with criteria: serial={:?}, vid={:?}, pid={:?}, type={:?}",
               serial_number, vendor_id, product_id, probe_type);
        
        let all_probes = Self::list_probes()?;
        
        if all_probes.is_empty() {
            return Err(DebugError::ProbeNotFound("No debug probes found".to_string()));
        }

        // If no criteria specified, return the first probe
        if serial_number.is_none() && vendor_id.is_none() && product_id.is_none() && probe_type.is_none() {
            return Ok(all_probes[0].clone());
        }

        // Filter probes based on criteria
        let matching_probes: Vec<_> = all_probes
            .into_iter()
            .filter(|probe| {
                // Check serial number
                if let Some(serial) = serial_number {
                    if probe.serial_number.as_deref() != Some(serial) {
                        return false;
                    }
                }

                // Check vendor ID
                if let Some(vid) = vendor_id {
                    if probe.vendor_id != vid {
                        return false;
                    }
                }

                // Check product ID
                if let Some(pid) = product_id {
                    if probe.product_id != pid {
                        return false;
                    }
                }

                // Check probe type
                if let Some(ptype) = probe_type {
                    if probe.probe_type.to_lowercase() != ptype.to_lowercase() {
                        return false;
                    }
                }

                true
            })
            .collect();

        if matching_probes.is_empty() {
            return Err(DebugError::ProbeNotFound(
                "No probe found matching the specified criteria".to_string()
            ));
        }

        if matching_probes.len() > 1 {
            warn!("Multiple probes match criteria, using the first one");
            for (i, probe) in matching_probes.iter().enumerate() {
                debug!("  {}: {} ({})", i, probe.identifier, 
                       probe.serial_number.as_deref().unwrap_or("no serial"));
            }
        }

        Ok(matching_probes[0].clone())
    }

    /// Auto-select the best available probe
    pub fn auto_select_probe() -> Result<ProbeInfo> {
        debug!("Auto-selecting debug probe");
        
        let all_probes = Self::list_probes()?;
        
        if all_probes.is_empty() {
            return Err(DebugError::ProbeNotFound("No debug probes found".to_string()));
        }

        // Prefer probes in this order: J-Link, ST-Link, DAPLink, others
        let preferred_order = ["j-link", "st-link", "daplink"];
        
        for preferred_type in &preferred_order {
            for probe in &all_probes {
                if probe.probe_type.to_lowercase().contains(preferred_type) {
                    info!("Auto-selected probe: {} ({})", probe.identifier, probe.probe_type);
                    return Ok(probe.clone());
                }
            }
        }

        // If no preferred probe found, use the first one
        let selected = &all_probes[0];
        info!("Auto-selected probe: {} ({})", selected.identifier, selected.probe_type);
        Ok(selected.clone())
    }

    /// Get detailed information about a specific probe
    pub fn get_probe_details(identifier: &str) -> Result<ProbeInfo> {
        debug!("Getting details for probe: {}", identifier);
        
        let all_probes = Self::list_probes()?;
        
        all_probes
            .into_iter()
            .find(|probe| probe.identifier == identifier)
            .ok_or_else(|| DebugError::ProbeNotFound(format!("Probe not found: {}", identifier)))
    }

    /// Check if a probe supports a specific target
    pub fn check_target_support(probe_type: &ProbeType, target_chip: &str) -> bool {
        match probe_type {
            ProbeType::JLink => {
                // J-Link supports most ARM and RISC-V targets
                target_chip.to_lowercase().contains("stm32") ||
                target_chip.to_lowercase().contains("nrf") ||
                target_chip.to_lowercase().contains("cortex") ||
                target_chip.to_lowercase().contains("risc")
            }
            ProbeType::StLink => {
                // ST-Link primarily supports STM32
                target_chip.to_lowercase().contains("stm32")
            }
            ProbeType::DapLink => {
                // DAPLink supports ARM Cortex targets
                target_chip.to_lowercase().contains("cortex") ||
                target_chip.to_lowercase().contains("stm32") ||
                target_chip.to_lowercase().contains("nrf")
            }
            ProbeType::Blackmagic => {
                // Black Magic Probe supports ARM Cortex
                target_chip.to_lowercase().contains("cortex") ||
                target_chip.to_lowercase().contains("stm32")
            }
            ProbeType::Ftdi => {
                // FTDI can support various targets
                true
            }
            ProbeType::Unknown => {
                // Unknown probes might work
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_probe_type_support() {
        assert!(ProbeDiscovery::check_target_support(&ProbeType::JLink, "STM32F407VG"));
        assert!(ProbeDiscovery::check_target_support(&ProbeType::StLink, "STM32F407VG"));
        assert!(ProbeDiscovery::check_target_support(&ProbeType::DapLink, "nRF52832"));
        assert!(!ProbeDiscovery::check_target_support(&ProbeType::StLink, "ESP32"));
    }

    #[tokio::test]
    async fn test_list_probes() {
        // This test will only pass if debug probes are connected
        // In CI/testing environments, this might be empty
        let result = ProbeDiscovery::list_probes();
        assert!(result.is_ok());
        
        let probes = result.unwrap();
        // Just verify the structure is correct
        for probe in probes {
            assert!(!probe.identifier.is_empty());
            assert!(!probe.probe_type.is_empty());
        }
    }
}