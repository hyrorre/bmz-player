//! Allocation-free latency distribution. Quantiles are logarithmic bucket upper
//! bounds (eight subdivisions per power of two); average and maximum are exact.
#[derive(Debug, Clone)]
pub struct LatencyHistogram {
    buckets: [u64; 512],
    count: u64,
    sum: u128,
    max: u64,
}

impl Default for LatencyHistogram {
    fn default() -> Self {
        Self { buckets: [0; 512], count: 0, sum: 0, max: 0 }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct LatencySummary {
    pub count: u64,
    pub avg: u64,
    pub p50: u64,
    pub p95: u64,
    pub p99: u64,
    pub max: u64,
}

impl LatencyHistogram {
    pub fn record(&mut self, value: u64) {
        let exponent = 63 - value.max(1).leading_zeros() as usize;
        let shift = exponent.saturating_sub(3);
        let index = if exponent < 3 {
            value as usize
        } else {
            (exponent - 2) * 8 + ((value >> shift) as usize - 8)
        };
        self.buckets[index] += 1;
        self.count += 1;
        self.sum += u128::from(value);
        self.max = self.max.max(value);
    }

    pub fn summary(&self) -> LatencySummary {
        LatencySummary {
            count: self.count,
            avg: (self.sum / u128::from(self.count.max(1))) as u64,
            p50: self.quantile(50),
            p95: self.quantile(95),
            p99: self.quantile(99),
            max: self.max,
        }
    }

    fn quantile(&self, percent: u64) -> u64 {
        if self.count == 0 {
            return 0;
        }
        let target = (u128::from(self.count) * u128::from(percent)).div_ceil(100) as u64;
        let mut count = 0;
        for (index, bucket) in self.buckets.iter().enumerate() {
            count += bucket;
            if count >= target {
                let upper = if index < 8 {
                    index as u128
                } else {
                    let shift = index / 8 - 1;
                    (((index % 8 + 9) as u128) << shift) - 1
                };
                return upper.min(u128::from(self.max)) as u64;
            }
        }
        self.max
    }
}

/// Fixed storage, single-writer telemetry. Readers take approximate live snapshots;
/// no locks, allocation, formatting or I/O occurs on the producer thread.
#[derive(Debug)]
pub struct AtomicLatencyHistogram {
    buckets: [std::sync::atomic::AtomicU64; 512],
    max: std::sync::atomic::AtomicU64,
}

impl Default for AtomicLatencyHistogram {
    fn default() -> Self {
        Self { buckets: std::array::from_fn(|_| Default::default()), max: Default::default() }
    }
}

impl AtomicLatencyHistogram {
    pub fn record(&self, value: u64) {
        use std::sync::atomic::Ordering::Relaxed;
        let exponent = 63 - value.max(1).leading_zeros() as usize;
        let shift = exponent.saturating_sub(3);
        let index = if exponent < 3 {
            value as usize
        } else {
            (exponent - 2) * 8 + ((value >> shift) as usize - 8)
        };
        self.max.fetch_max(value, Relaxed);
        self.buckets[index].fetch_add(1, Relaxed);
    }

    /// Quantiles are bucket upper bounds; max is exact. Average is not exported
    /// because reconstructing it from buckets would imply false precision.
    pub fn summary(&self) -> DistributionSummary {
        use std::sync::atomic::Ordering::Relaxed;
        let buckets = std::array::from_fn(|i| self.buckets[i].load(Relaxed));
        let histogram = LatencyHistogram {
            count: buckets.iter().sum(),
            buckets,
            sum: 0,
            max: self.max.load(Relaxed),
        };
        DistributionSummary {
            count: histogram.count,
            p50: histogram.quantile(50),
            p95: histogram.quantile(95),
            p99: histogram.quantile(99),
            max: histogram.max,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct DistributionSummary {
    pub count: u64,
    pub p50: u64,
    pub p95: u64,
    pub p99: u64,
    pub max: u64,
}

/// Opt-in once per process, read outside real-time callbacks during setup.
pub fn diagnostics_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("BMZ_LATENCY_DIAGNOSTICS").is_some_and(|v| v == "1"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn distribution_includes_stalls_and_extreme_values() {
        let mut histogram = LatencyHistogram::default();
        for _ in 0..99 {
            histogram.record(1_000);
        }
        histogram.record(250_000);
        assert_eq!(histogram.summary().avg, 3_490);
        assert_eq!(histogram.summary().max, 250_000);
        assert!((1_000..1_125).contains(&histogram.summary().p99));
        histogram.record(u64::MAX);
        assert_eq!(histogram.summary().max, u64::MAX);
    }
}
