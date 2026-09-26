use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use super::select_support::CachedSelectChartDistribution;
use crate::storage::library_db::{ChartListItem, LibraryDatabase};

const CACHE_LIMIT: usize = 256;
const BATCH_LIMIT: usize = 25;
type DistributionCache = HashMap<i64, CachedSelectChartDistribution>;

struct LoadResult {
    generation: u64,
    result: Result<DistributionCache, String>,
}

/// One bounded batch in flight. Scrolling cannot queue obsolete requests behind it.
#[derive(Default)]
pub(super) struct SelectDistributionRuntime {
    pending: Option<Receiver<LoadResult>>,
    generation: u64,
    recent: VecDeque<i64>,
    retry_after: Option<Instant>,
}

impl SelectDistributionRuntime {
    pub(super) fn invalidate(&mut self, cache: &mut DistributionCache) {
        self.generation = self.generation.wrapping_add(1);
        self.recent.clear();
        self.retry_after = None;
        cache.clear();
        // Keep the receiver until completion so invalidation cannot spawn extra workers.
    }

    pub(super) fn refresh(
        &mut self,
        path: &Path,
        charts: &[&ChartListItem],
        cache: &mut DistributionCache,
    ) {
        self.poll(cache);
        for chart in charts {
            if cache.contains_key(&chart.chart_id) {
                self.touch(chart.chart_id);
            }
        }
        if self.pending.is_some() || self.retry_after.is_some_and(|retry| Instant::now() < retry) {
            return;
        }
        let mut missing = Vec::new();
        for chart in charts {
            if !cache.contains_key(&chart.chart_id)
                && !missing.iter().any(|c: &ChartListItem| c.chart_id == chart.chart_id)
            {
                missing.push((*chart).clone());
                if missing.len() == BATCH_LIMIT {
                    break;
                }
            }
        }
        if missing.is_empty() {
            return;
        }
        let path = path.to_owned();
        let generation = self.generation;
        let (tx, rx) = mpsc::channel();
        match std::thread::Builder::new().name("select-distribution".into()).spawn(move || {
            let started = Instant::now();
            let result = (|| -> anyhow::Result<DistributionCache> {
                let db = LibraryDatabase::open_read_only(&path)?;
                let ids: Vec<_> = missing.iter().map(|chart| chart.chart_id).collect();
                let mut distributions = db.chart_distributions_by_chart_ids(&ids)?;
                Ok(missing
                    .iter()
                    .map(|chart| {
                        let notes = distributions.remove(&chart.chart_id).unwrap_or_default();
                        (chart.chart_id, CachedSelectChartDistribution::new(notes, chart))
                    })
                    .collect())
            })()
            .map_err(|error| error.to_string());
            tracing::debug!(target: "bmz_player::select_profile",
                elapsed_us = started.elapsed().as_micros(), charts = missing.len(),
                "select distributions loaded in background");
            let _ = tx.send(LoadResult { generation, result });
        }) {
            Ok(_) => self.pending = Some(rx),
            Err(error) => self.failed(&error.to_string()),
        }
    }

    fn poll(&mut self, cache: &mut DistributionCache) {
        let Some(rx) = &self.pending else { return };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                self.pending = None;
                self.failed("distribution worker disconnected");
                return;
            }
        };
        self.pending = None;
        if result.generation != self.generation {
            return;
        }
        match result.result {
            Ok(entries) => {
                self.retry_after = None;
                for (id, entry) in entries {
                    cache.insert(id, entry);
                    self.touch(id);
                }
                while self.recent.len() > CACHE_LIMIT {
                    if let Some(id) = self.recent.pop_front() {
                        cache.remove(&id);
                    }
                }
            }
            Err(error) => self.failed(&error),
        }
    }

    fn touch(&mut self, id: i64) {
        self.recent.retain(|cached| *cached != id);
        self.recent.push_back(id);
    }

    fn failed(&mut self, error: &str) {
        tracing::warn!(%error, "failed to load select distributions");
        self.retry_after = Some(Instant::now() + Duration::from_secs(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deliver(runtime: &mut SelectDistributionRuntime, generation: u64, ids: &[i64]) {
        let (tx, rx) = mpsc::channel();
        runtime.pending = Some(rx);
        tx.send(LoadResult {
            generation,
            result: Ok(ids
                .iter()
                .map(|id| (*id, CachedSelectChartDistribution::default()))
                .collect()),
        })
        .unwrap();
    }

    #[test]
    fn stale_data_is_discarded_but_folder_navigation_reuses_results() {
        let mut runtime = SelectDistributionRuntime::default();
        let mut cache = HashMap::new();
        deliver(&mut runtime, 0, &[1]);
        runtime.invalidate(&mut cache);
        assert!(runtime.pending.is_some());
        runtime.poll(&mut cache);
        assert!(cache.is_empty());
        deliver(&mut runtime, 1, &[2]);
        runtime.poll(&mut cache);
        // A different visible list doesn't invalidate chart-keyed results.
        runtime.refresh(Path::new("unused"), &[], &mut cache);
        assert!(cache.contains_key(&2));
    }

    #[test]
    fn cache_evicts_least_recently_visible_entries() {
        let mut runtime = SelectDistributionRuntime::default();
        let mut cache = HashMap::new();
        deliver(&mut runtime, 0, &(0..CACHE_LIMIT as i64).collect::<Vec<_>>());
        runtime.poll(&mut cache);
        let oldest = runtime.recent[0];
        let next = runtime.recent[1];
        runtime.touch(oldest);
        deliver(&mut runtime, 0, &[999]);
        runtime.poll(&mut cache);
        assert_eq!(cache.len(), CACHE_LIMIT);
        assert!(cache.contains_key(&oldest));
        assert!(!cache.contains_key(&next));
    }

    #[test]
    fn failed_worker_does_not_cache_empty_data_and_can_retry() {
        let mut runtime = SelectDistributionRuntime::default();
        let mut cache = HashMap::new();
        let (tx, rx) = mpsc::channel();
        runtime.pending = Some(rx);
        tx.send(LoadResult { generation: 0, result: Err("busy".into()) }).unwrap();
        runtime.poll(&mut cache);
        assert!(cache.is_empty());
        assert!(runtime.pending.is_none());
        assert!(runtime.retry_after.is_some());
    }
}
