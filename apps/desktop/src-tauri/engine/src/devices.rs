//! The compute devices ggml can use in this build (CPU always; Metal on macOS; Vulkan GPUs in the
//! `vulkan` build when a driver is present).

use std::ffi::CStr;
use std::os::raw::c_char;
use talkr_protocol::{Device, DeviceKind};
use whisper_rs::whisper_rs_sys as sys;

pub fn list() -> Vec<Device> {
    // SAFETY: plain queries against ggml's device registry, which initializes itself on first
    // use and lives for the whole process; the returned strings are owned by ggml.
    unsafe {
        (0..sys::ggml_backend_dev_count())
            .map(|i| {
                let dev = sys::ggml_backend_dev_get(i);
                let (mut free, mut total) = (0usize, 0usize);
                sys::ggml_backend_dev_memory(dev, &mut free, &mut total);
                Device {
                    kind: kind(sys::ggml_backend_dev_type(dev)),
                    name: text(sys::ggml_backend_dev_name(dev)),
                    description: text(sys::ggml_backend_dev_description(dev)),
                    memory_free: free as u64,
                    memory_total: total as u64,
                }
            })
            .collect()
    }
}

fn kind(t: sys::ggml_backend_dev_type) -> DeviceKind {
    match t {
        sys::ggml_backend_dev_type_GGML_BACKEND_DEVICE_TYPE_GPU => DeviceKind::Gpu,
        sys::ggml_backend_dev_type_GGML_BACKEND_DEVICE_TYPE_IGPU => DeviceKind::Igpu,
        sys::ggml_backend_dev_type_GGML_BACKEND_DEVICE_TYPE_ACCEL => DeviceKind::Accelerator,
        _ => DeviceKind::Cpu,
    }
}

unsafe fn text(ptr: *const c_char) -> String {
    if ptr.is_null() {
        String::new()
    } else {
        CStr::from_ptr(ptr).to_string_lossy().trim().to_string()
    }
}

/// Pick the GPU for a model of `model_bytes`, as whisper.cpp's `gpu_device` index (it counts
/// discrete and integrated GPUs, in registry order). Prefers a discrete GPU, then the one with
/// the most free memory, and skips any that cannot hold the model. `None` means use the CPU.
pub fn choose_gpu(devices: &[Device], model_bytes: u64) -> Option<(i32, &Device)> {
    // Weights plus compute buffers; the same rule of thumb the app uses for system memory.
    let needed = model_bytes + model_bytes / 2;
    devices
        .iter()
        .filter(|d| matches!(d.kind, DeviceKind::Gpu | DeviceKind::Igpu))
        .enumerate()
        // A driver that reports no memory figures gets the benefit of the doubt.
        .filter(|(_, d)| d.memory_free == 0 || d.memory_free >= needed)
        .max_by_key(|(_, d)| (d.kind == DeviceKind::Gpu, d.memory_free))
        .map(|(i, d)| (i as i32, d))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(kind: DeviceKind, name: &str, free_gb: u64) -> Device {
        Device {
            kind,
            name: name.into(),
            description: name.into(),
            memory_free: free_gb << 30,
            memory_total: free_gb << 30,
        }
    }

    #[test]
    fn prefers_a_discrete_gpu_that_fits() {
        let devices = [
            dev(DeviceKind::Cpu, "CPU", 16),
            dev(DeviceKind::Igpu, "Vulkan0", 8),
            dev(DeviceKind::Gpu, "Vulkan1", 6),
        ];
        // gpu_device counts only GPUs: Vulkan0 is 0, Vulkan1 is 1.
        assert_eq!(choose_gpu(&devices, 1 << 30).map(|(i, d)| (i, d.name.as_str())), Some((1, "Vulkan1")));
    }

    #[test]
    fn skips_gpus_too_small_for_the_model() {
        let devices = [dev(DeviceKind::Gpu, "Vulkan0", 1), dev(DeviceKind::Igpu, "Vulkan1", 4)];
        assert_eq!(choose_gpu(&devices, 1536 << 20).map(|(i, _)| i), Some(1));
        assert_eq!(choose_gpu(&devices[..1], 1536 << 20), None);
    }

    #[test]
    fn cpu_only_means_none() {
        assert_eq!(choose_gpu(&[dev(DeviceKind::Cpu, "CPU", 16)], 1), None);
        assert_eq!(choose_gpu(&[], 1), None);
    }

    #[test]
    fn probing_this_machine_always_finds_the_cpu() {
        let devices = list();
        assert!(devices.iter().any(|d| d.kind == DeviceKind::Cpu), "{devices:?}");
    }
}
