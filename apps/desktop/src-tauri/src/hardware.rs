use std::process::Command;
use sysinfo::{System, CpuRefreshKind, MemoryRefreshKind};
use serde::{Serialize, Deserialize};

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
                                        .and_then(parse_vram),
                                });
                            }
                        }
                    }
                }
            }
        }

        if let Ok(output) = hidden_command("nvidia-smi")
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

        #[cfg(target_os = "windows")]
        if gpus.is_empty() {
            gpus.extend(Self::detect_gpus_windows());
        }

        if gpus.is_empty() && cfg!(target_os = "linux") {
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

    #[cfg(target_os = "windows")]
    fn detect_gpus_windows() -> Vec<GpuInfo> {
        let mut gpus = Vec::new();
        let output = hidden_command("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Get-CimInstance Win32_VideoController | Select-Object Name,AdapterCompatibility,AdapterRAM | ConvertTo-Json -Compress",
            ])
            .output();
        let Ok(output) = output else { return gpus };
        let Ok(json) = serde_json::from_slice::<serde_json::Value>(&output.stdout) else { return gpus };
        let entries = match json {
            serde_json::Value::Array(a) => a,
            v @ serde_json::Value::Object(_) => vec![v],
            _ => return gpus,
        };
        for entry in entries {
            let name = entry.get("Name").and_then(|v| v.as_str()).unwrap_or("Unknown GPU").to_string();
            let vendor_str = format!(
                "{} {}",
                entry.get("AdapterCompatibility").and_then(|v| v.as_str()).unwrap_or(""),
                name
            )
            .to_lowercase();
            let vendor = if vendor_str.contains("nvidia") {
                GpuVendor::Nvidia
            } else if vendor_str.contains("amd") || vendor_str.contains("advanced micro") || vendor_str.contains("radeon") {
                GpuVendor::Amd
            } else if vendor_str.contains("intel") {
                GpuVendor::Intel
            } else {
                GpuVendor::Unknown
            };
            // AdapterRAM is a 32-bit field and saturates at 4 GiB, so treat it as a lower bound.
            let vram_bytes = entry.get("AdapterRAM").and_then(|v| v.as_u64()).filter(|v| *v > 0);
            gpus.push(GpuInfo { name, vendor, vram_bytes });
        }
        gpus
    }

    #[allow(unused_variables)]
    fn recommend_backend(os: &str, gpus: &[GpuInfo]) -> Backend {
        if os == "macos" {
            return Backend::Metal;
        }

        #[cfg(feature = "cuda")]
        if gpus.iter().any(|g| g.vendor == GpuVendor::Nvidia) {
            return Backend::Cuda;
        }

        #[cfg(feature = "vulkan")]
        if gpus.iter().any(|g| g.vendor != GpuVendor::Unknown) {
            return Backend::Vulkan;
        }

        Backend::Cpu
    }
}

/// Builds a `Command` that does not flash a console window on Windows.
fn hidden_command(program: &str) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

fn parse_vram(s: &str) -> Option<u64> {
    let s = s.trim();
    if let Some(gb) = s.strip_suffix(" GB") {
        gb.parse::<f64>().ok().map(|v| (v * 1024.0 * 1024.0 * 1024.0) as u64)
    } else if let Some(mb) = s.strip_suffix(" MB") {
        mb.parse::<u64>().ok().map(|v| v * 1024 * 1024)
    } else {
        None
    }
}