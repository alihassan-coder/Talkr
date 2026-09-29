use std::process::Command;
use sysinfo::{System, CpuRefreshKind, MemoryRefreshKind};
use serde::{Serialize, Deserialize};
use crate::error::{AppError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareInfo {
    pub os: String,
    pub arch: String,
    pub cpu_name: String,
    pub cpu_cores: usize,
    pub ram_bytes: u64,
    pub gpus: Vec<GpuInfo>,
    pub recommended_backend: Backend,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuInfo {
    pub name: String,
    pub vendor: GpuVendor,
    pub vram_bytes: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Apple,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    Metal,
    Cuda,
    Vulkan,
    Cpu,
}

impl HardwareInfo {
    pub fn detect() -> Self {
        let mut sys = System::new_with_specifics(
            sysinfo::RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything()),
        );
        sys.refresh_all();

        let os = std::env::consts::OS.to_string();
        let arch = std::env::consts::ARCH.to_string();

        let cpu_name = sys.cpus().first()
            .map(|c| c.brand().to_string())
            .unwrap_or_else(|| "Unknown CPU".to_string());
        let cpu_cores = System::physical_core_count().unwrap_or(sys.cpus().len());
        let ram_bytes = sys.total_memory();

        let gpus = Self::detect_gpus();
        let recommended_backend = Self::recommend_backend(&os, &gpus);

        Self {
            os,
            arch,
            cpu_name,
            cpu_cores,
            ram_bytes,
            gpus,
            recommended_backend,
        }
    }

    fn detect_gpus() -> Vec<GpuInfo> {
        let mut gpus = Vec::new();

        if cfg!(target_os = "macos") {
            if let Ok(output) = Command::new("system_profiler")
                .args(["SPDisplaysDataType", "-json"])
                .output()
            {
                if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&output.stdout) {
                    if let Some(displays) = json.get("SPDisplaysDataType").and_then(|v| v.as_array()) {
                        for display in displays {
                            if let Some(name) = display.get("sppci_model").and_then(|v| v.as_str()) {
                                gpus.push(GpuInfo {
                                    name: name.to_string(),
                                    vendor: GpuVendor::Apple,
                                    vram_bytes: display.get("spdisplays_vram").and_then(|v| v.as_str())
                                        .and_then(|s| parse_vram(s)),
                                });
                            }
                        }
                    }
                }
            }
        }

        if let Ok(output) = Command::new("nvidia-smi")
            .args(["--query-gpu=name,memory.total", "--format=csv,noheader,nounits"])
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
                if parts.len() == 2 {
                    let vram_mb = parts[1].parse::<u64>().ok();
                    gpus.push(GpuInfo {
                        name: parts[0].to_string(),
                        vendor: GpuVendor::Nvidia,
                        vram_bytes: vram_mb.map(|mb| mb * 1024 * 1024),
                    });
                }
            }
        }

        if gpus.is_empty() && cfg!(not(target_os = "macos")) {
            if let Ok(output) = Command::new("lspci")
                .args(["-nn"])
                .output()
            {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    if line.to_lowercase().contains("vga") || line.to_lowercase().contains("3d") {
                        let vendor = if line.contains("10de:") {
                            GpuVendor::Nvidia
                        } else if line.contains("1002:") || line.contains("1022:") {
                            GpuVendor::Amd
                        } else if line.contains("8086:") {
                            GpuVendor::Intel
                        } else {
                            GpuVendor::Unknown
                        };

                        let name = line.split('[').nth(1).and_then(|s| s.split(']').next())
                            .unwrap_or("Unknown GPU").to_string();

                        gpus.push(GpuInfo {
                            name,
                            vendor,
                            vram_bytes: None,
                        });
                    }
                }
            }
        }

        gpus
    }

    fn recommend_backend(os: &str, gpus: &[GpuInfo]) -> Backend {
        if os == "macos" {
            return Backend::Metal;
        }

        let has_nvidia = gpus.iter().any(|g| g.vendor == GpuVendor::Nvidia);
        let has_vulkan = gpus.iter().any(|g| g.vendor != GpuVendor::Unknown);

        #[cfg(feature = "cuda")]
        if has_nvidia {
            return Backend::Cuda;
        }

        #[cfg(feature = "vulkan")]
        if has_vulkan {
            return Backend::Vulkan;
        }

        Backend::Cpu
    }
}

fn parse_vram(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.ends_with(" GB") {
        s[..s.len() - 3].parse::<f64>().ok().map(|v| (v * 1024.0 * 1024.0 * 1024.0) as u64)
    } else if s.ends_with(" MB") {
        s[..s.len() - 3].parse::<u64>().ok().map(|v| v * 1024 * 1024)
    } else {
        None
    }
}