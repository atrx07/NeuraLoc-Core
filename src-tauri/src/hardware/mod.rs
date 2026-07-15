mod detector;
mod types;

pub use detector::HardwareService;
pub use types::HardwareSnapshot;
#[cfg(test)]
pub(crate) use types::{CpuInfo, MemoryInfo};
