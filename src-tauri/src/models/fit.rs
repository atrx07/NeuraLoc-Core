use crate::{
    hardware::HardwareSnapshot,
    scheduler::resource_policy::{FitLabel, ResourceEstimate, ResourcePolicy},
};

use super::types::{FitConfidence, ModelFitEstimate, ModelRecord};

const DEFAULT_CONTEXT_SIZE: u32 = 4_096;
const MIN_CONTEXT_SIZE: u64 = 256;
const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;
const MIN_RAM_RESERVE_BYTES: u64 = 2 * GIB;
const KV_BYTES_PER_EMBEDDING_PER_LAYER: u64 = 4;

pub(crate) fn estimate_cpu_fit(
    model: &ModelRecord,
    hardware: &HardwareSnapshot,
) -> ModelFitEstimate {
    let metadata = model.gguf_metadata.as_ref();
    let context_size = metadata
        .and_then(|value| value.context_length)
        .unwrap_or(u64::from(DEFAULT_CONTEXT_SIZE))
        .clamp(MIN_CONTEXT_SIZE, u64::from(DEFAULT_CONTEXT_SIZE)) as u32;
    let (kv_cache_bytes, confidence) =
        match metadata.and_then(|value| value.layer_count.zip(value.embedding_length)) {
            Some((layers, embedding)) => (
                layers
                    .saturating_mul(u64::from(context_size))
                    .saturating_mul(embedding)
                    .saturating_mul(KV_BYTES_PER_EMBEDDING_PER_LAYER),
                FitConfidence::Medium,
            ),
            None => ((model.size_bytes / 4).max(256 * MIB), FitConfidence::Low),
        };
    let runtime_overhead_bytes = (model.size_bytes / 10).max(512 * MIB);
    let estimated_ram_bytes = model
        .size_bytes
        .saturating_add(kv_cache_bytes)
        .saturating_add(runtime_overhead_bytes);
    let reserved_ram_bytes = (hardware.memory.total_bytes / 10).max(MIN_RAM_RESERVE_BYTES);
    let policy = ResourcePolicy {
        ram_reserve_bytes: reserved_ram_bytes,
        vram_reserve_bytes: 0,
    };
    let fit = policy.classify(
        ResourceEstimate {
            required_ram_bytes: estimated_ram_bytes,
            required_vram_bytes: 0,
        },
        hardware.memory.available_bytes,
        0,
    );
    let usable_ram = hardware
        .memory
        .available_bytes
        .saturating_sub(reserved_ram_bytes);
    let headroom_bytes = usable_ram.saturating_sub(estimated_ram_bytes);
    let reason = match fit {
        FitLabel::Excellent => "At least 30% estimated RAM headroom on the verified CPU route.",
        FitLabel::Good => "At least 15% estimated RAM headroom on the verified CPU route.",
        FitLabel::Tight => "Estimated to fit the CPU route with less than 15% RAM headroom.",
        FitLabel::NotRecommended => {
            "Estimated RAM exceeds current memory after the system safety reserve."
        }
    }
    .into();

    ModelFitEstimate {
        model_id: model.id.clone(),
        route: "cpu".into(),
        fit,
        confidence,
        context_size,
        estimated_ram_bytes,
        available_ram_bytes: hardware.memory.available_bytes,
        reserved_ram_bytes,
        weight_bytes: model.size_bytes,
        kv_cache_bytes,
        runtime_overhead_bytes,
        headroom_bytes,
        reason,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{hardware::HardwareSnapshot, models::VerificationState};

    use super::super::types::GgufMetadata;

    use super::*;

    fn model(metadata: Option<GgufMetadata>) -> ModelRecord {
        ModelRecord {
            id: "qwen-4b".into(),
            kind: "llm".into(),
            display_name: "Qwen3 4B".into(),
            family: Some("qwen3".into()),
            format: "gguf".into(),
            path: r"C:\models\qwen.gguf".into(),
            size_bytes: 2_300 * MIB,
            sha256: None,
            verification_state: VerificationState::Ready,
            verification_error: None,
            gguf_metadata: metadata,
            modified_at_unix_ms: 1,
            imported_at: "2026-07-15T00:00:00Z".into(),
            last_verified_at: Some("2026-07-15T00:00:00Z".into()),
            file_identity: None,
        }
    }

    fn hardware(total_gib: u64, available_gib: u64) -> HardwareSnapshot {
        HardwareSnapshot {
            captured_at: "2026-07-15T00:00:00Z".into(),
            source: "test",
            cpu: crate::hardware::CpuInfo {
                name: "Test CPU".into(),
                physical_cores: Some(8),
                logical_cores: 16,
                utilization_percent: None,
            },
            memory: crate::hardware::MemoryInfo {
                total_bytes: total_gib * GIB,
                available_bytes: available_gib * GIB,
            },
            devices: Vec::new(),
            capabilities: Vec::new(),
            warnings: Vec::new(),
        }
    }

    fn qwen_metadata() -> GgufMetadata {
        GgufMetadata {
            version: 3,
            tensor_count: 1,
            metadata_count: 1,
            architecture: Some("qwen3".into()),
            name: Some("Qwen3 4B".into()),
            file_type: Some(15),
            quantization: Some("Q4_K_M".into()),
            parameter_count: Some(4_000_000_000),
            context_length: Some(40_960),
            embedding_length: Some(2_560),
            layer_count: Some(36),
            has_chat_template: true,
            metadata_bytes: 100,
            metadata_preview: BTreeMap::new(),
        }
    }

    #[test]
    fn estimates_a_conservative_cpu_fit_from_gguf_metadata() {
        let estimate = estimate_cpu_fit(&model(Some(qwen_metadata())), &hardware(32, 16));

        assert_eq!(estimate.fit, FitLabel::Excellent);
        assert_eq!(estimate.confidence, FitConfidence::Medium);
        assert_eq!(estimate.context_size, DEFAULT_CONTEXT_SIZE);
        assert!(estimate.kv_cache_bytes > GIB);
        assert!(estimate.estimated_ram_bytes > estimate.weight_bytes);
        assert!(estimate.headroom_bytes > 0);
    }

    #[test]
    fn rejects_a_model_that_exceeds_available_ram_after_reserve() {
        let estimate = estimate_cpu_fit(&model(Some(qwen_metadata())), &hardware(16, 5));

        assert_eq!(estimate.fit, FitLabel::NotRecommended);
        assert_eq!(estimate.headroom_bytes, 0);
    }

    #[test]
    fn labels_missing_shape_metadata_as_low_confidence() {
        let estimate = estimate_cpu_fit(&model(None), &hardware(32, 16));

        assert_eq!(estimate.confidence, FitConfidence::Low);
        assert!(estimate.kv_cache_bytes >= 256 * MIB);
    }
}
