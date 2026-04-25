/// Hardware detection: RAM, chip name, available memory.
/// macOS only (v1). Uses sysctl for total RAM, sysinfo for available.

use anyhow::{Context, Result};
use std::process::Command;

pub struct HardwareInfo {
    pub chip_name: String,
    pub total_ram_gb: f64,
    pub available_ram_gb: f64,
}

impl std::fmt::Display for HardwareInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} \u{00b7} {:.0} GB unified \u{00b7} {:.1} GB available",
            self.chip_name, self.total_ram_gb, self.available_ram_gb
        )
    }
}

/// Detect hardware info via sysctl and sysinfo.
pub fn detect() -> Result<HardwareInfo> {
    let total_ram_bytes = sysctl_total_ram()?;
    let total_ram_gb = total_ram_bytes as f64 / (1024.0 * 1024.0 * 1024.0);

    let available_ram_gb = available_ram()?;
    let chip_name = detect_chip()?;

    Ok(HardwareInfo {
        chip_name,
        total_ram_gb,
        available_ram_gb,
    })
}

/// Read total physical memory via sysctl.
fn sysctl_total_ram() -> Result<u64> {
    let output = Command::new("sysctl")
        .args(["-n", "hw.memsize"])
        .output()
        .context("Failed to run sysctl")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .trim()
        .parse::<u64>()
        .context("Failed to parse hw.memsize")
}

/// Get available (free + inactive) RAM using vm_stat.
fn available_ram() -> Result<f64> {
    let output = Command::new("vm_stat")
        .output()
        .context("Failed to run vm_stat")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let page_size = parse_vm_stat_page_size(&stdout).unwrap_or(16384); // Apple Silicon default

    let free = parse_vm_stat_field(&stdout, "Pages free");
    let inactive = parse_vm_stat_field(&stdout, "Pages inactive");
    let purgeable = parse_vm_stat_field(&stdout, "Pages purgeable");

    let available_pages = free + inactive + purgeable;
    let available_bytes = available_pages * page_size;
    Ok(available_bytes as f64 / (1024.0 * 1024.0 * 1024.0))
}

fn parse_vm_stat_page_size(output: &str) -> Option<u64> {
    // "Mach Virtual Memory Statistics: (page size of 16384 bytes)"
    let start = output.find("page size of ")? + "page size of ".len();
    let end = output[start..].find(' ')? + start;
    output[start..end].parse().ok()
}

fn parse_vm_stat_field(output: &str, field: &str) -> u64 {
    for line in output.lines() {
        if line.contains(field) {
            if let Some(val) = line.split(':').nth(1) {
                return val.trim().trim_end_matches('.').parse().unwrap_or(0);
            }
        }
    }
    0
}

/// Detect Apple Silicon chip name.
fn detect_chip() -> Result<String> {
    let output = Command::new("sysctl")
        .args(["-n", "machdep.cpu.brand_string"])
        .output()
        .context("Failed to detect CPU")?;

    let brand = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // Try to get the marketing name (e.g., "Apple M3 Pro")
    if brand.contains("Apple") {
        Ok(brand)
    } else {
        // Fallback
        Ok(format!("Apple Silicon ({})", brand))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_vm_stat_page_size() {
        let output = "Mach Virtual Memory Statistics: (page size of 16384 bytes)\nPages free: 123.";
        assert_eq!(parse_vm_stat_page_size(output), Some(16384));
    }

    #[test]
    fn test_parse_vm_stat_field() {
        let output = "Pages free:                    12345.\nPages inactive:                6789.";
        assert_eq!(parse_vm_stat_field(output, "Pages free"), 12345);
        assert_eq!(parse_vm_stat_field(output, "Pages inactive"), 6789);
        assert_eq!(parse_vm_stat_field(output, "Pages nonexistent"), 0);
    }

    #[test]
    fn test_detect_runs() {
        // Just verify it doesn't panic on macOS
        let info = detect();
        assert!(info.is_ok());
        let info = info.unwrap();
        assert!(info.total_ram_gb > 0.0);
        assert!(info.available_ram_gb > 0.0);
    }
}
