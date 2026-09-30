use std::io::Read;
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// How long one GPU query tool (nvidia-smi, PowerShell, lspci, system_profiler) may run. A
/// wedged driver can make nvidia-smi hang indefinitely.
const TOOL_TIMEOUT: Duration = Duration::from_secs(8);

const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;

/// Hardware detection, run at most once and cached.
///
/// Detection launches external tools and can take seconds, so it must not run before the window
/// opens: startup calls [`HardwareProbe::warm_up`] to start it in the background, and
/// [`HardwareProbe::get`] (called off the async runtime) waits for that result or runs it.
#[derive(Clone, Default)]
pub struct HardwareProbe {
    cell: Arc<OnceLock<HardwareInfo>>,
}

impl HardwareProbe {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start detection on a background thread.
    pub fn warm_up(&self) {
        let probe = self.clone();
        let spawned = std::thread::Builder::new().name("talkr-hardware".into()).spawn(move || {
            probe.get();
        });
        if let Err(e) = spawned {
            log::warn!("Could not start hardware detection in the background: {}", e);
        }
    }

    /// The detected hardware. Blocks while detection runs (only one detection ever runs; other
    /// callers wait for it), so call it from a blocking context.
    pub fn get(&self) -> HardwareInfo {
        self.cell
            .get_or_init(|| {
                let started = Instant::now();
                let info = HardwareInfo::detect();
                log::info!("Hardware detected in {:?}: {} GPU(s)", started.elapsed(), info.gpus.len());
                info
            })
            .clone()
    }

    /// The result if detection has finished, without waiting.
    pub fn try_get(&self) -> Option<HardwareInfo> {
        self.cell.get().cloned()
    }
}

impl HardwareInfo {
    pub fn detect() -> Self {
        // The CPU list (with brand names) and memory totals; no usage sampling, no process scan.
        let sys = System::new_with_specifics(
            sysinfo::RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::nothing())
                .with_memory(MemoryRefreshKind::everything()),
        );

        let os = std::env::consts::OS.to_string();
        let arch = std::env::consts::ARCH.to_string();

        let cpu_name = sys.cpus().first()
            .map(|c| c.brand().trim().to_string())
            .filter(|b| !b.is_empty())
            .unwrap_or_else(|| "Unknown CPU".to_string());
        let cpu_cores = System::physical_core_count().unwrap_or(sys.cpus().len()).max(1);
        let ram_bytes = sys.total_memory();

        let gpus = detect_gpus();
        let recommended_backend = recommend_backend(&os, &gpus);

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
}

fn detect_gpus() -> Vec<GpuInfo> {
    let mut gpus = Vec::new();

    if cfg!(target_os = "macos") {
        if let Some(out) = run_tool(Command::new("system_profiler").args(["SPDisplaysDataType", "-json"])) {
            gpus.extend(parse_system_profiler(&out));
        }
    }

    if let Some(out) = run_tool(
        hidden_command("nvidia-smi").args(["--query-gpu=name,memory.total", "--format=csv,noheader,nounits"]),
    ) {
        gpus.extend(parse_nvidia_smi(&String::from_utf8_lossy(&out)));
    }

    #[cfg(target_os = "windows")]
    if gpus.is_empty() {
        if let Some(out) = run_tool(hidden_command("powershell").args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-CimInstance Win32_VideoController | Select-Object Name,AdapterCompatibility,AdapterRAM | ConvertTo-Json -Compress",
        ])) {
            gpus.extend(parse_windows_video_controllers(&out));
        }
    }

    if gpus.is_empty() && cfg!(target_os = "linux") {
        if let Some(out) = run_tool(Command::new("lspci").arg("-nn")) {
            gpus.extend(parse_lspci(&String::from_utf8_lossy(&out)));
        }
    }

    gpus
}

/// Run a tool and return its stdout if it exits successfully within [`TOOL_TIMEOUT`]. A tool
/// that is missing, fails or hangs (it is killed) gives `None`.
fn run_tool(cmd: &mut Command) -> Option<Vec<u8>> {
    let output = run_with_timeout(cmd, TOOL_TIMEOUT)?;
    output.status.success().then_some(output.stdout)
}

fn run_with_timeout(cmd: &mut Command, timeout: Duration) -> Option<Output> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    // Read stdout on a thread so a chatty tool cannot block on a full pipe while we wait.
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(25)),
            Ok(None) => {
                log::warn!("{:?} did not finish within {:?}; stopping it", cmd.get_program(), timeout);
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Err(_) => {
                let _ = child.kill();
                return None;
            }
        }
    };
    let stdout = reader.join().unwrap_or_default();
    Some(Output { status, stdout, stderr: Vec::new() })
}

/// `nvidia-smi --query-gpu=name,memory.total --format=csv,noheader,nounits`: one
/// `name, MiB` line per GPU. Names can contain commas, so split at the last one.
fn parse_nvidia_smi(stdout: &str) -> Vec<GpuInfo> {
    stdout
        .lines()
        .filter_map(|line| {
            let (name, mem) = line.rsplit_once(',')?;
            let name = name.trim();
            if name.is_empty() {
                return None;
            }
            Some(GpuInfo {
                name: name.to_string(),
                vendor: GpuVendor::Nvidia,
                vram_bytes: mem.trim().parse::<u64>().ok().map(|mb| mb * MIB),
            })
        })
        .collect()
}

/// `system_profiler SPDisplaysDataType -json`.
fn parse_system_profiler(stdout: &[u8]) -> Vec<GpuInfo> {
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(stdout) else { return Vec::new() };
    let Some(displays) = json.get("SPDisplaysDataType").and_then(|v| v.as_array()) else { return Vec::new() };
    displays
        .iter()
        .filter_map(|display| {
            let name = display.get("sppci_model").and_then(|v| v.as_str())?;
            let vram = ["spdisplays_vram", "spdisplays_vram_shared"]
                .iter()
                .find_map(|k| display.get(*k).and_then(|v| v.as_str()).and_then(parse_vram));
            Some(GpuInfo { name: name.to_string(), vendor: GpuVendor::Apple, vram_bytes: vram })
        })
        .collect()
}

/// PowerShell `Win32_VideoController | ConvertTo-Json`: an object for one adapter, an array for
/// several.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn parse_windows_video_controllers(stdout: &[u8]) -> Vec<GpuInfo> {
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(stdout) else { return Vec::new() };
    let entries = match json {
        serde_json::Value::Array(a) => a,
        v @ serde_json::Value::Object(_) => vec![v],
        _ => return Vec::new(),
    };
    entries
        .into_iter()
        .map(|entry| {
            let name = entry.get("Name").and_then(|v| v.as_str()).unwrap_or("Unknown GPU").trim().to_string();
            let maker = entry.get("AdapterCompatibility").and_then(|v| v.as_str()).unwrap_or("");
            let vendor = vendor_from_name(&format!("{} {}", maker, name));
            // AdapterRAM is a 32-bit field and saturates at 4 GiB, so treat it as a lower bound.
            let vram_bytes = entry.get("AdapterRAM").and_then(|v| v.as_u64()).filter(|v| *v > 0);
            GpuInfo { name, vendor, vram_bytes }
        })
        .collect()
}

/// `lspci -nn`: display controllers (VGA, 3D, Display), vendor from the PCI vendor id.
fn parse_lspci(stdout: &str) -> Vec<GpuInfo> {
    stdout
        .lines()
        .filter_map(|line| {
            // "01:00.0 VGA compatible controller [0300]: NVIDIA Corporation GA106 [GeForce RTX 3060] [10de:2503] (rev a1)"
            let (_slot_and_class, rest) = line.split_once(": ")?;
            let class = line.split_once(" [03")?.0.to_lowercase();
            if !(class.contains("vga") || class.contains("3d") || class.contains("display")) {
                return None;
            }
            let lower = line.to_lowercase();
            let vendor = if lower.contains("[10de:") {
                GpuVendor::Nvidia
            } else if lower.contains("[1002:") || lower.contains("[1022:") {
                GpuVendor::Amd
            } else if lower.contains("[8086:") {
                GpuVendor::Intel
            } else {
                vendor_from_name(rest)
            };
            Some(GpuInfo { name: lspci_device_name(rest), vendor, vram_bytes: None })
        })
        .collect()
}

/// The marketing name in brackets if there is one ("GeForce RTX 3060"), else the description
/// without the trailing `[vendor:device]` id and revision.
fn lspci_device_name(rest: &str) -> String {
    let is_pci_id = |s: &str| {
        s.len() == 9 && s.as_bytes()[4] == b':' && s.chars().filter(|c| *c != ':').all(|c| c.is_ascii_hexdigit())
    };
    let bracketed: Vec<&str> = rest
        .split('[')
        .skip(1)
        .filter_map(|s| s.split(']').next())
        .filter(|s| !is_pci_id(s))
        .collect();
    if let Some(name) = bracketed.last().filter(|s| !s.trim().is_empty()) {
        return name.trim().to_string();
    }
    let plain = rest.split(" [").next().unwrap_or(rest);
    let plain = plain.split(" (rev").next().unwrap_or(plain).trim();
    if plain.is_empty() { "Unknown GPU".into() } else { plain.to_string() }
}

fn vendor_from_name(s: &str) -> GpuVendor {
    let s = s.to_lowercase();
    if s.contains("nvidia") || s.contains("geforce") || s.contains("quadro") {
        GpuVendor::Nvidia
    } else if s.contains("amd") || s.contains("advanced micro") || s.contains("radeon") || s.contains("ati ") {
        GpuVendor::Amd
    } else if s.contains("intel") {
        GpuVendor::Intel
    } else if s.contains("apple") {
        GpuVendor::Apple
    } else {
        GpuVendor::Unknown
    }
}

/// The accelerator the engine would reach for on this hardware: Metal on a Mac, Vulkan with
/// a discrete NVIDIA or AMD card. Whether it actually works is known only once the engine
/// has probed it (see `get_engine_status`).
fn recommend_backend(os: &str, gpus: &[GpuInfo]) -> Backend {
    if os == "macos" {
        return Backend::Metal;
    }
    if gpus.iter().any(|g| matches!(g.vendor, GpuVendor::Nvidia | GpuVendor::Amd)) {
        return Backend::Vulkan;
    }
    Backend::Cpu
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

/// "8 GB", "1536 MB", "1.5 GB" (as `system_profiler` writes VRAM) to bytes.
fn parse_vram(s: &str) -> Option<u64> {
    let s = s.trim();
    let (number, unit) = s.rsplit_once(' ')?;
    let value = number.trim().parse::<f64>().ok().filter(|v| v.is_finite() && *v >= 0.0)?;
    let scale = match unit.trim().to_ascii_uppercase().as_str() {
        "GB" | "GIB" => GIB,
        "MB" | "MIB" => MIB,
        "KB" | "KIB" => 1024,
        _ => return None,
    };
    Some((value * scale as f64).round() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vram_strings() {
        assert_eq!(parse_vram("8 GB"), Some(8 * GIB));
        assert_eq!(parse_vram(" 1536 MB "), Some(1536 * MIB));
        assert_eq!(parse_vram("1.5 GB"), Some(3 * GIB / 2));
        assert_eq!(parse_vram("512 mb"), Some(512 * MIB));
        assert_eq!(parse_vram("16 GiB"), Some(16 * GIB));
        assert_eq!(parse_vram("8GB"), None);
        assert_eq!(parse_vram("lots"), None);
        assert_eq!(parse_vram("-1 GB"), None);
        assert_eq!(parse_vram("NaN GB"), None);
        assert_eq!(parse_vram("8 TB"), None);
        assert_eq!(parse_vram(""), None);
    }

    #[test]
    fn nvidia_smi_output() {
        let out = "NVIDIA GeForce RTX 3060, 12288\nNVIDIA RTX A6000, 49140\r\n\n, 10\nweird, n/a\n";
        let gpus = parse_nvidia_smi(out);
        assert_eq!(gpus.len(), 3);
        assert_eq!(gpus[0], GpuInfo { name: "NVIDIA GeForce RTX 3060".into(), vendor: GpuVendor::Nvidia, vram_bytes: Some(12288 * MIB) });
        assert_eq!(gpus[1].vram_bytes, Some(49140 * MIB));
        assert_eq!(gpus[2], GpuInfo { name: "weird".into(), vendor: GpuVendor::Nvidia, vram_bytes: None });
        assert!(parse_nvidia_smi("").is_empty());
        // A name with a comma in it.
        assert_eq!(parse_nvidia_smi("Tesla T4, rev 2, 15360")[0].name, "Tesla T4, rev 2");
    }

    #[test]
    fn system_profiler_output() {
        let json = br#"{"SPDisplaysDataType":[
            {"sppci_model":"Apple M2 Pro","spdisplays_vram_shared":"16 GB"},
            {"sppci_model":"AMD Radeon Pro 5500M","spdisplays_vram":"8 GB"},
            {"_name":"no model"}
        ]}"#;
        let gpus = parse_system_profiler(json);
        assert_eq!(gpus.len(), 2);
        assert_eq!(gpus[0].name, "Apple M2 Pro");
        assert_eq!(gpus[0].vram_bytes, Some(16 * GIB));
        assert_eq!(gpus[1].vram_bytes, Some(8 * GIB));
        assert!(gpus.iter().all(|g| g.vendor == GpuVendor::Apple));
        assert!(parse_system_profiler(b"not json").is_empty());
        assert!(parse_system_profiler(b"{}").is_empty());
    }

    #[test]
    fn windows_video_controllers_output() {
        let one = br#"{"Name":"NVIDIA GeForce GTX 1650","AdapterCompatibility":"NVIDIA","AdapterRAM":4293918720}"#;
        let gpus = parse_windows_video_controllers(one);
        assert_eq!(gpus, vec![GpuInfo { name: "NVIDIA GeForce GTX 1650".into(), vendor: GpuVendor::Nvidia, vram_bytes: Some(4293918720) }]);

        let many = br#"[
            {"Name":"Intel(R) UHD Graphics 630","AdapterCompatibility":"Intel Corporation","AdapterRAM":1073741824},
            {"Name":"AMD Radeon RX 6600","AdapterCompatibility":"Advanced Micro Devices, Inc.","AdapterRAM":0},
            {"Name":"Microsoft Basic Display Adapter","AdapterCompatibility":"(Standard display types)","AdapterRAM":null},
            {"AdapterCompatibility":null}
        ]"#;
        let gpus = parse_windows_video_controllers(many);
        assert_eq!(gpus.len(), 4);
        assert_eq!(gpus[0].vendor, GpuVendor::Intel);
        assert_eq!(gpus[1].vendor, GpuVendor::Amd);
        assert_eq!(gpus[1].vram_bytes, None, "0 means unknown");
        assert_eq!(gpus[2].vendor, GpuVendor::Unknown);
        assert_eq!(gpus[3].name, "Unknown GPU");
        assert!(parse_windows_video_controllers(b"").is_empty());
        assert!(parse_windows_video_controllers(b"42").is_empty());
    }

    #[test]
    fn lspci_output() {
        let out = "\
00:02.0 VGA compatible controller [0300]: Intel Corporation CometLake-H GT2 [UHD Graphics] [8086:9bc4] (rev 05)
00:1f.3 Audio device [0403]: Intel Corporation Comet Lake PCH cAVS [8086:06c8] (rev 10)
01:00.0 3D controller [0302]: NVIDIA Corporation TU117M [GeForce GTX 1650 Mobile / Max-Q] [10de:1f99] (rev a1)
03:00.0 VGA compatible controller [0300]: Advanced Micro Devices, Inc. [AMD/ATI] Navi 23 [Radeon RX 6600] [1002:73ff] (rev c7)
04:00.0 Display controller [0380]: Some Vendor Thing [abcd:1234]
05:00.0 Ethernet controller [0200]: Realtek RTL8111 [10ec:8168]
";
        let gpus = parse_lspci(out);
        assert_eq!(gpus.len(), 4, "{gpus:?}");
        assert_eq!(gpus[0], GpuInfo { name: "UHD Graphics".into(), vendor: GpuVendor::Intel, vram_bytes: None });
        assert_eq!(gpus[1].name, "GeForce GTX 1650 Mobile / Max-Q");
        assert_eq!(gpus[1].vendor, GpuVendor::Nvidia);
        assert_eq!(gpus[2].name, "Radeon RX 6600");
        assert_eq!(gpus[2].vendor, GpuVendor::Amd);
        assert_eq!(gpus[3].name, "Some Vendor Thing");
        assert_eq!(gpus[3].vendor, GpuVendor::Unknown);
        assert!(parse_lspci("").is_empty());
    }

    #[test]
    fn vendor_names() {
        assert_eq!(vendor_from_name("NVIDIA Quadro"), GpuVendor::Nvidia);
        assert_eq!(vendor_from_name("Radeon Vega 8"), GpuVendor::Amd);
        assert_eq!(vendor_from_name("Intel Arc A770"), GpuVendor::Intel);
        assert_eq!(vendor_from_name("Apple M1"), GpuVendor::Apple);
        assert_eq!(vendor_from_name("VMware SVGA"), GpuVendor::Unknown);
    }

    #[test]
    fn backend_recommendation() {
        let gpu = |vendor| GpuInfo { name: "x".into(), vendor, vram_bytes: None };
        assert_eq!(recommend_backend("macos", &[]), Backend::Metal);
        assert_eq!(recommend_backend("windows", &[gpu(GpuVendor::Intel), gpu(GpuVendor::Nvidia)]), Backend::Vulkan);
        assert_eq!(recommend_backend("linux", &[gpu(GpuVendor::Amd)]), Backend::Vulkan);
        assert_eq!(recommend_backend("windows", &[gpu(GpuVendor::Intel)]), Backend::Cpu);
        assert_eq!(recommend_backend("linux", &[]), Backend::Cpu);
    }

    #[test]
    fn missing_tool_gives_none() {
        assert!(run_tool(&mut Command::new("talkr-no-such-tool-xyz")).is_none());
    }

    #[test]
    fn hanging_tool_is_killed() {
        let mut cmd = if cfg!(windows) {
            let mut c = hidden_command("powershell");
            c.args(["-NoProfile", "-NonInteractive", "-Command", "Start-Sleep -Seconds 30"]);
            c
        } else {
            let mut c = Command::new("sleep");
            c.arg("30");
            c
        };
        let started = Instant::now();
        assert!(run_with_timeout(&mut cmd, Duration::from_millis(300)).is_none());
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn tool_output_is_captured() {
        let mut cmd = if cfg!(windows) {
            let mut c = hidden_command("cmd");
            c.args(["/C", "echo hello"]);
            c
        } else {
            let mut c = Command::new("echo");
            c.arg("hello");
            c
        };
        let out = run_tool(&mut cmd).expect("echo should run");
        assert!(String::from_utf8_lossy(&out).contains("hello"));
    }

    #[test]
    fn probe_caches_one_result() {
        let probe = HardwareProbe::new();
        assert!(probe.try_get().is_none());
        let first = probe.get();
        assert!(first.cpu_cores >= 1);
        assert_eq!(first.os, std::env::consts::OS);
        let clone = probe.clone();
        let again = clone.try_get().expect("cached");
        assert_eq!(serde_json::to_value(&first).unwrap(), serde_json::to_value(&again).unwrap());
    }

    #[test]
    fn serialized_shape_is_unchanged() {
        let info = HardwareInfo {
            os: "windows".into(),
            arch: "x86_64".into(),
            cpu_name: "cpu".into(),
            cpu_cores: 8,
            ram_bytes: 16 * GIB,
            gpus: vec![GpuInfo { name: "g".into(), vendor: GpuVendor::Nvidia, vram_bytes: Some(GIB) }],
            recommended_backend: Backend::Vulkan,
        };
        let v = serde_json::to_value(&info).unwrap();
        assert_eq!(
            v,
            serde_json::json!({
                "os": "windows", "arch": "x86_64", "cpuName": "cpu", "cpuCores": 8, "ramBytes": 16 * GIB,
                "gpus": [{"name": "g", "vendor": "nvidia", "vramBytes": GIB}],
                "recommendedBackend": "vulkan"
            })
        );
    }
}
