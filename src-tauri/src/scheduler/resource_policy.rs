use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FitLabel {
    Excellent,
    Good,
    Tight,
    NotRecommended,
}

#[derive(Debug, Clone, Copy)]
pub struct ResourceEstimate {
    pub required_ram_bytes: u64,
    pub required_vram_bytes: u64,
}

pub struct ResourcePolicy {
    pub ram_reserve_bytes: u64,
    pub vram_reserve_bytes: u64,
}

impl ResourcePolicy {
    pub fn classify(
        &self,
        estimate: ResourceEstimate,
        available_ram: u64,
        available_vram: u64,
    ) -> FitLabel {
        let usable_ram = available_ram.saturating_sub(self.ram_reserve_bytes);
        let usable_vram = available_vram.saturating_sub(self.vram_reserve_bytes);
        if estimate.required_ram_bytes > usable_ram || estimate.required_vram_bytes > usable_vram {
            return FitLabel::NotRecommended;
        }
        let ram_ratio = ratio(usable_ram, estimate.required_ram_bytes);
        let vram_ratio = ratio(usable_vram, estimate.required_vram_bytes);
        match ram_ratio.min(vram_ratio) {
            value if value >= 1.30 => FitLabel::Excellent,
            value if value >= 1.15 => FitLabel::Good,
            _ => FitLabel::Tight,
        }
    }
}

fn ratio(available: u64, required: u64) -> f64 {
    if required == 0 {
        f64::INFINITY
    } else {
        available as f64 / required as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn respects_memory_reserve() {
        let policy = ResourcePolicy {
            ram_reserve_bytes: 2,
            vram_reserve_bytes: 2,
        };
        let estimate = ResourceEstimate {
            required_ram_bytes: 9,
            required_vram_bytes: 9,
        };
        assert_eq!(policy.classify(estimate, 10, 10), FitLabel::NotRecommended);
    }
}
