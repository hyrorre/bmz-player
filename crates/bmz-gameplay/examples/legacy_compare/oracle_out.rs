//! Writes BMZ LEGACY results in the openlr2_oracle output format.
//!
//! Score and gauges are updated as GameSession does on the LEGACY polling
//! clock: each poll applies its judgements in order, then its mine hits, and
//! a closing gauge stops play before the next poll. After the `E` line, the
//! play part of the CustomIR submission comes from `Lr2ScoreRecorder`.
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};

use anyhow::{Result, bail};
use bmz_chart::model::PlayableChart;
use bmz_core::{clear::GaugeType, lane::Lane};
use bmz_gameplay::{
    gauge::{GaugeProperty, GaugeState, gauge_total_for_play},
    judge::model::JudgementEvent,
    lr2_score::{Lr2Judgements, Lr2ScoreRecorder},
    rule::RuleMode,
    score::ScoreState,
};

/// OpenLR2 gauge index -> BMZ gauge. P-ATTACK / G-ATTACK have no BMZ gauge.
pub fn bmz_gauge(openlr2: i32) -> Result<GaugeType> {
    Ok(match openlr2 {
        0 => GaugeType::Normal,
        1 => GaugeType::Hard,
        2 => GaugeType::Hazard,
        3 => GaugeType::Easy,
        other => bail!("unsupported OpenLR2 gauge {other}"),
    })
}

/// BMZ lane -> OpenLR2 bmsobj_note lane (P1 0..7, P2 10..17).
pub fn openlr2_lane(lane: Lane) -> usize {
    match lane {
        Lane::Scratch2 => 10,
        lane if lane.index() < 8 => lane.index(),
        // Key8..Key14 are P2 keys 1..7.
        lane => 11 + lane.index() - Lane::Key8.index(),
    }
}

pub struct OracleOut {
    out: BufWriter<File>,
    gauge: GaugeState,
    score: ScoreState,
    mines: u32,
    dirty: bool,
    submission: Lr2ScoreRecorder,
    columns: [[u32; 2]; 20],
}

impl OracleOut {
    pub fn new(path: &Path, chart: &PlayableChart, openlr2_gauge: i32) -> Result<Self> {
        let total = gauge_total_for_play(&chart.metadata, chart.total_notes, RuleMode::Legacy);
        let gauge = GaugeState::new_with_property_and_rule_mode_and_keymode(
            bmz_gauge(openlr2_gauge)?,
            total,
            chart.total_notes,
            GaugeProperty::Lr2,
            RuleMode::Legacy,
            chart.metadata.key_mode,
        );
        let mut this = Self {
            out: BufWriter::new(File::create(path)?),
            gauge,
            score: ScoreState::for_rule_mode(chart.metadata.key_mode, RuleMode::Legacy),
            mines: 0,
            dirty: false,
            submission: Lr2ScoreRecorder::new(chart, openlr2_gauge as usize, total),
            columns: [[0; 2]; 20],
        };
        writeln!(this.out, "H,totalnotes,{}", chart.total_notes)?;
        writeln!(this.out, "H,gauge_total,{total:.17}")?;
        this.state(-1)?;
        Ok(this)
    }

    fn hp(&self, gauge_type: GaugeType) -> String {
        self.gauge.gauges.iter().find(|g| g.definition.gauge_type == gauge_type).map_or_else(
            || "nan".to_string(),
            |g| format!("{:?}", g.legacy_hp.unwrap_or(f64::from(g.value))),
        )
    }

    fn state(&mut self, ms: i64) -> Result<()> {
        let j = &self.score.judges;
        let jc = [
            self.mines,
            j.fast_poor + j.slow_poor + j.fast_empty_poor + j.slow_empty_poor,
            j.fast_bad + j.slow_bad,
            j.fast_good + j.slow_good,
            j.fast_great + j.slow_great,
            j.fast_pgreat + j.slow_pgreat,
        ];
        let hp = [GaugeType::Normal, GaugeType::Hard, GaugeType::Hazard, GaugeType::Easy]
            .map(|g| self.hp(g))
            .join(",");
        writeln!(
            self.out,
            "S,{ms},{hp},nan,nan,{},{},{},{},{},{},{},{},{},{}",
            self.score.ex_score(),
            self.score.combo,
            self.score.max_combo,
            jc[0],
            jc[1],
            jc[2],
            jc[3],
            jc[4],
            jc[5],
            self.score.past_notes
        )?;
        Ok(())
    }

    /// Applies one lane's outcome of the current poll, in lane order.
    /// `events` holds all of the outcome's events, including long-note heads.
    pub fn lane(
        &mut self,
        ms: i64,
        events: &[JudgementEvent],
        mines: &[(Lane, f64)],
        judge_value: impl Fn(&JudgementEvent) -> usize,
    ) -> Result<()> {
        for event in events {
            self.submission.apply_event(event);
        }
        let events: Vec<_> = events.iter().filter(|e| e.affects_score).collect();
        if !events.is_empty() || !mines.is_empty() {
            self.dirty = true;
        }
        for event in events {
            writeln!(self.out, "J,{ms},{},{}", openlr2_lane(event.lane), judge_value(event))?;
            self.score.apply(event);
            self.gauge.apply_judge(event.judge, 1.0);
        }
        for &(lane, damage) in mines {
            writeln!(self.out, "M,{ms},{},{}", openlr2_lane(lane), damage)?;
            self.gauge.apply_mine(damage);
            self.submission.apply_mine(damage);
            self.score.record_legacy_mine_hit();
            self.mines += 1;
        }
        Ok(())
    }

    /// Ends a poll. Returns true when the selected gauge closes play.
    pub fn end_poll(&mut self, ms: i64) -> Result<bool> {
        if std::env::var_os("BMZ_LR2_COLUMNS").is_some() {
            // Debug: the oracle's `columns 1` lines.
            for lane in 0..20 {
                let v = self.submission.column(lane).values();
                let key = [v[..10].iter().sum::<u32>(), v[11] + v[12]];
                if key != self.columns[lane] {
                    self.columns[lane] = key;
                    let text: Vec<_> = v[..14].iter().map(u32::to_string).collect();
                    writeln!(self.out, "X,{ms},{lane},{}", text.join(","))?;
                }
            }
        }
        self.submission.log_graph(ms);
        if !std::mem::take(&mut self.dirty) {
            return Ok(false);
        }
        self.state(ms)?;
        Ok(self.gauge.current_closes_play_on_zero())
    }

    pub fn finish(mut self, ms: i64, reason: &str) -> Result<()> {
        writeln!(self.out, "E,{ms},{reason}")?;
        let score = self.submission.finish();
        let stats = |out: &mut BufWriter<File>, name: &str, s: &Lr2Judgements| {
            let values: Vec<_> = s.values().iter().map(u32::to_string).collect();
            writeln!(out, "R,{name},{}", values.join(","))
        };
        stats(&mut self.out, "total", &score.judgements_total)?;
        for (lane, column) in score.judgements_column.iter().enumerate() {
            stats(&mut self.out, &format!("col{lane}"), column)?;
        }
        writeln!(
            self.out,
            "R,score,{},{},{},{},{:?},{},{}",
            score.max_combo,
            score.gauge_type,
            score.moneyscore,
            score.exscore,
            score.rate,
            score.clear_type,
            score.notes_total
        )?;
        let hp: Vec<_> = score.hp.iter().map(|v| format!("{v:?}")).collect();
        writeln!(self.out, "R,hp,{}", hp.join(","))?;
        writeln!(self.out, "R,runtime,{:?}", score.song_runtime)?;
        let samples = |out: &mut BufWriter<File>, name: &str, values: &[i32]| {
            let values: Vec<_> = values.iter().map(i32::to_string).collect();
            writeln!(out, "G,{name},{}", values.join(","))
        };
        for (gauge, values) in score.graphs.hp.iter().enumerate() {
            samples(&mut self.out, &format!("hp{gauge}"), values)?;
        }
        samples(&mut self.out, "combo", &score.graphs.combo)?;
        samples(&mut self.out, "exscore", &score.graphs.exscore)?;
        writeln!(self.out, "R,ghost,{}", score.ghost)?;
        self.out.flush()?;
        Ok(())
    }
}
