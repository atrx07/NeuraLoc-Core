use std::sync::Arc;

use chrono::Utc;
use parking_lot::RwLock;
use sysinfo::System;

use crate::{errors::AppResult, processes::ProcessManager};

use super::types::{
    Capability, CapabilityStatus, CpuInfo, DeviceInfo, HardwareSnapshot, MemoryInfo,
};

pub struct HardwareService {
    processes: Arc<ProcessManager>,
    cached: RwLock<Option<HardwareSnapshot>>,
}

impl HardwareService {
    pub fn new(processes: Arc<ProcessManager>) -> Self {
        Self {
            processes,
            cached: RwLock::new(None),
        }
    }

    pub async fn get(&self) -> AppResult<HardwareSnapshot> {
        if let Some(snapshot) = self.cached.read().clone() {
            return Ok(snapshot);
        }
        self.refresh().await
    }

    pub async fn refresh(&self) -> AppResult<HardwareSnapshot> {
        let mut system = System::new_all();
        system.refresh_all();
        let cpu_name = system
            .cpus()
            .first()
            .map(|cpu| cpu.brand().trim().to_owned())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "Unknown processor".into());
        let mut devices = Vec::new();
        let mut capabilities = vec![Capability {
            id: "cpu",
            label: "CPU fallback",
            status: CapabilityStatus::Available,
            evidence: "Native processor APIs responded".into(),
        }];
        let mut warnings = Vec::new();

        let nvidia = self
            .processes
            .run_probe(
                "nvidia-smi.exe",
                &[
                    "--query-gpu=name,memory.total,memory.free,utilization.gpu,temperature.gpu",
                    "--format=csv,noheader,nounits",
                ],
            )
            .await;
        match nvidia {
            Ok(rows) => {
                for (index, row) in rows.lines().enumerate() {
                    if let Some(device) = parse_nvidia_row(index, row) {
                        devices.push(device);
                    }
                }
                capabilities.push(Capability {
                    id: "llm-cuda",
                    label: "LLM / CUDA",
                    status: CapabilityStatus::Available,
                    evidence:
                        "NVIDIA driver probe succeeded; engine package still requires validation"
                            .into(),
                });
                capabilities.push(Capability {
                    id: "image-cuda",
                    label: "Images / CUDA",
                    status: CapabilityStatus::Available,
                    evidence:
                        "NVIDIA driver probe succeeded; engine package still requires validation"
                            .into(),
                });
            }
            Err(_) => {
                capabilities.push(Capability {
                    id: "llm-cuda",
                    label: "LLM / CUDA",
                    status: CapabilityStatus::Unknown,
                    evidence: "nvidia-smi was unavailable; physical GPU state is not assumed"
                        .into(),
                });
                capabilities.push(Capability {
                    id: "image-cuda",
                    label: "Images / CUDA",
                    status: CapabilityStatus::Unknown,
                    evidence: "nvidia-smi was unavailable; physical GPU state is not assumed"
                        .into(),
                });
            }
        }

        let compute_devices = self
            .processes
            .run_probe(
                "pnputil.exe",
                &[
                    "/enum-devices",
                    "/connected",
                    "/class",
                    "ComputeAccelerator",
                ],
            )
            .await
            .unwrap_or_default();
        if compute_devices.to_ascii_lowercase().contains("npu")
            || compute_devices.to_ascii_lowercase().contains("ai boost")
        {
            let name = compute_devices
                .lines()
                .find(|line| {
                    let value = line.to_ascii_lowercase();
                    value.contains("npu") || value.contains("ai boost")
                })
                .and_then(|line| {
                    line.split_once(':')
                        .map(|(_, value)| value.trim().to_owned())
                })
                .unwrap_or_else(|| "Intel AI accelerator".into());
            devices.push(DeviceInfo {
                id: "intel-npu-0".into(),
                kind: "npu",
                name,
                vendor: "Intel".into(),
                memory_total_bytes: None,
                memory_available_bytes: None,
                utilization_percent: None,
                temperature_celsius: None,
            });
            capabilities.push(Capability {
                id: "openvino-npu",
                label: "OpenVINO / NPU",
                status: CapabilityStatus::Experimental,
                evidence: "NPU device detected; each OpenVINO model must pass a compile probe"
                    .into(),
            });
        } else {
            capabilities.push(Capability {
                id: "openvino-npu",
                label: "OpenVINO / NPU",
                status: CapabilityStatus::Unknown,
                evidence: "No NPU was confirmed by the Windows compute-accelerator probe".into(),
            });
        }
        capabilities.push(Capability {
            id: "llm-vulkan",
            label: "LLM / Vulkan",
            status: CapabilityStatus::Unknown,
            evidence: "Vulkan runtime enumeration is not installed yet".into(),
        });

        if devices.is_empty() {
            warnings.push("No accelerator was confirmed. CPU inference remains available.".into());
        }
        let snapshot = HardwareSnapshot {
            captured_at: Utc::now().to_rfc3339(),
            source: "native",
            cpu: CpuInfo {
                name: cpu_name,
                physical_cores: system.physical_core_count(),
                logical_cores: system.cpus().len(),
                utilization_percent: Some(system.global_cpu_usage()),
            },
            memory: MemoryInfo {
                total_bytes: system.total_memory(),
                available_bytes: system.available_memory(),
            },
            devices,
            capabilities,
            warnings,
        };
        *self.cached.write() = Some(snapshot.clone());
        Ok(snapshot)
    }
}

fn parse_nvidia_row(index: usize, row: &str) -> Option<DeviceInfo> {
    let fields: Vec<&str> = row.split(',').map(str::trim).collect();
    if fields.len() != 5 {
        return None;
    }
    let mib = 1024_u64 * 1024;
    Some(DeviceInfo {
        id: format!("nvidia-{index}"),
        kind: "gpu",
        name: fields[0].into(),
        vendor: "NVIDIA".into(),
        memory_total_bytes: fields[1].parse::<u64>().ok().map(|value| value * mib),
        memory_available_bytes: fields[2].parse::<u64>().ok().map(|value| value * mib),
        utilization_percent: fields[3].parse().ok(),
        temperature_celsius: fields[4].parse().ok(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nvidia_csv() {
        let device = parse_nvidia_row(0, "NVIDIA GeForce RTX 5070, 8192, 7000, 12, 48").unwrap();
        assert_eq!(device.name, "NVIDIA GeForce RTX 5070");
        assert_eq!(device.memory_total_bytes, Some(8192 * 1024 * 1024));
    }
}
