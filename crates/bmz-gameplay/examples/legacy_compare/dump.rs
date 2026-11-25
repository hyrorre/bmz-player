//! Dumps what BMZ LEGACY loads from a chart, in the openlr2_oracle `H` / `N`
//! format, so the loader can be compared with OpenLR2 on every chart.
//!
//! NORMAL arrangement only. Times are the integer microseconds BMZ stores
//! (OpenLR2's double milliseconds floored to µs).
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};

use anyhow::{Context, Result, ensure};
use bmz_chart::{
    import::{AssetLookup, BmsRandomSource, import_chart_with_assets},
    model::{BgaEventKind, NoteKind, TimingEventKind},
};
use bmz_core::{clear::GaugeType, lane::Lane};
use bmz_gameplay::{
    gauge::{gauge_total_for_play, legacy_gauge_rule},
    judge::window::{
        judge_rank_spec_to_percent_optional_for_keymode_and_rule_mode,
        judge_windows_for_keymode_and_rule_mode, judge_windows_for_rule_mode_and_keymode,
    },
    rule::RuleMode,
};

use super::oracle_out::openlr2_lane;

pub fn dump_chart(chart_path: &str, md5: &str, seed: u32, out_path: &str) -> Result<()> {
    let path = Path::new(chart_path);
    let mut imported =
        import_chart_with_assets(path, BmsRandomSource::Lr2Seed(seed), AssetLookup::Declared)?;
    let actual: String =
        imported.chart.identity.file_md5.iter().map(|b| format!("{b:02x}")).collect();
    ensure!(actual == md5.to_ascii_lowercase(), "chart MD5 does not match");
    bmz_chart::import::openlr2::apply_from_bytes(
        &mut imported.chart,
        path,
        &std::fs::read(path)?,
        &BmsRandomSource::Lr2Seed(seed),
        None,
    )?;
    let chart = &imported.chart;
    let mut out = BufWriter::new(File::create(out_path).context("dump output")?);
    writeln!(out, "H,keymode,{:?}", chart.metadata.key_mode)?;
    writeln!(out, "H,totalnotes,{}", chart.total_notes)?;
    let total = gauge_total_for_play(&chart.metadata, chart.total_notes, RuleMode::Legacy);
    writeln!(out, "H,total,{total}")?;
    let percent = judge_rank_spec_to_percent_optional_for_keymode_and_rule_mode(
        chart.metadata.judge_rank_spec,
        chart.metadata.key_mode,
        RuleMode::Legacy,
    );
    let base = judge_windows_for_keymode_and_rule_mode(chart.metadata.key_mode, RuleMode::Legacy);
    let w = judge_windows_for_rule_mode_and_keymode(
        base,
        percent,
        RuleMode::Legacy,
        chart.metadata.key_mode,
    )
    .note;
    // OpenLR2 judgetime[5..1]: PGREAT, GREAT, GOOD, BAD, EMPTY POOR (ms).
    for (index, us) in [
        (5, w.pgreat_us),
        (4, w.great_us),
        (3, w.good_us),
        (2, w.bad_slow_us),
        (1, w.empty_poor_fast_us),
    ] {
        writeln!(out, "H,judgetime{index},{}", us / 1000)?;
    }
    for (gauge_index, gauge) in
        [GaugeType::Normal, GaugeType::Hard, GaugeType::Hazard, GaugeType::Easy]
            .into_iter()
            .enumerate()
    {
        if let Some(rule) = legacy_gauge_rule(gauge, total, chart.total_notes) {
            for (judge, value) in rule.damage.iter().enumerate() {
                writeln!(out, "H,damage{gauge_index}_{judge},{value:?}")?;
            }
        }
    }
    for lane in Lane::ALL {
        for note in chart.notes_for_lane(lane) {
            let (end, mine) = match note.kind {
                NoteKind::Tap => (-1, -1),
                NoteKind::LongStart => (
                    chart
                        .long_notes
                        .iter()
                        .find(|p| p.start_note_id == note.id)
                        .map_or(-1, |p| p.end_time.0),
                    -1,
                ),
                NoteKind::Mine => (-1, note.damage.unwrap_or(0.0) as i64),
                _ => continue,
            };
            writeln!(out, "N,{},{},{end},{mine}", openlr2_lane(lane), note.time.0)?;
        }
    }
    // Playback events by the LR2 op of their objects: BGM 1, BGA 4 / 6 / 7,
    // BPM 3 (or 8), STOP 9.
    for event in &chart.bgm_events {
        writeln!(out, "E,1,{}", event.time.0)?;
    }
    for event in &chart.bga_events {
        let op = match event.kind {
            BgaEventKind::Base => 4,
            BgaEventKind::Poor => 6,
            BgaEventKind::Layer => 7,
            BgaEventKind::Layer2 => continue,
        };
        writeln!(out, "E,{op},{}", event.time.0)?;
    }
    for event in &chart.timing_events {
        match event.kind {
            TimingEventKind::BpmChange { .. } => writeln!(out, "E,3,{}", event.time.0)?,
            TimingEventKind::Stop { .. } => writeln!(out, "E,9,{}", event.time.0)?,
            TimingEventKind::BpmReflection => {}
        }
    }
    for (index, bar) in chart.bar_lines.iter().enumerate() {
        writeln!(out, "L,{index},{}", bar.time.0)?;
    }
    out.flush()?;
    Ok(())
}
