use super::*;

pub(crate) fn synthetic_table(count: usize) -> crate::difficulty_table::FetchedDifficultyTable {
    use crate::difficulty_table::{FetchedDifficultyTable, FetchedTableEntry};
    FetchedDifficultyTable {
        source_url: "https://example.invalid/performance".into(),
        head_url: "https://example.invalid/performance/header.json".into(),
        name: "Synthetic".into(),
        symbol: "T".into(),
        level_order: vec!["1".into()],
        entries: (0..count)
            .map(|id| FetchedTableEntry {
                level: "1".into(),
                md5: format!("{id:032x}"),
                title: format!("Chart {id:06}"),
                ..Default::default()
            })
            .collect(),
        courses: Vec::new(),
        fetched_at: 1,
    }
}

#[test]
#[ignore = "release CPU/database probe; run alone with --ignored --nocapture"]
fn profile_table_database_work() -> Result<()> {
    use std::time::Instant;

    anyhow::ensure!(!cfg!(debug_assertions), "run with --release");
    for count in [1_000, 10_000, 50_000] {
        let data = crate::bootstrap::profile_tests::ProfileTestDir::new();
        let mut boot = data.boot();
        let table = synthetic_table(count);
        let mut writes = Vec::new();
        let mut reads = Vec::new();
        for iteration in 0..6 {
            let start = Instant::now();
            store_fetched_table(&mut boot.library_db, &table)?;
            let write = start.elapsed().as_secs_f64() * 1_000.0;
            let start = Instant::now();
            let items =
                crate::screens::select_model::load_select_items_in_table_level_for_rule_mode(
                    &boot.library_db,
                    &boot.score_db,
                    &table.source_url,
                    "1",
                    boot.profile_config.play.ln_mode_policy,
                    boot.profile_config.play.rule_mode,
                )?;
            let read = start.elapsed().as_secs_f64() * 1_000.0;
            assert_eq!(items.len(), count);
            if iteration > 0 {
                writes.push(write);
                reads.push(read);
            }
        }
        writes.sort_by(f64::total_cmp);
        reads.sort_by(f64::total_cmp);
        eprintln!(
            "table_db entries={count} store_median_ms={:.3} store_max_ms={:.3} list_median_ms={:.3} list_max_ms={:.3}",
            writes[2], writes[4], reads[2], reads[4]
        );
    }
    Ok(())
}
