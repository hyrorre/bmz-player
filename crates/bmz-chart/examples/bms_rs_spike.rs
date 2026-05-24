//! bms-rs 採用検討用 spike。
//!
//! 合成 BMS（Mine / Invisible / Long / LNOBJ / RANDOM 入り）を
//! bms-rs と内製パーサーの両方に通して、出力を見比べる。
//!
//! 実行: `cargo run -p bmz-chart --example bms_rs_spike`

use std::collections::BTreeMap;
use std::io::Write;

use bms_rs::bms::command::channel::NoteKind as BmsNoteKind;
use bms_rs::bms::command::channel::mapper::{KeyLayoutBeat, KeyLayoutMapper, KeyMapping};
use bms_rs::bms::{BmsOutput, default_config, parse_bms};
use bmz_chart::import::import_bms_chart;
use bmz_chart::model::NoteKind;

/// 合成 BMS。検証したい要素を一通り詰める。
///
/// チャネル:
/// - `11` = P1 Key1 visible
/// - `12` = P1 Key2 visible
/// - `16` = P1 Scratch visible
/// - `31` = P1 Key1 invisible
/// - `51` = P1 Key1 long-channel (LNTYPE 1 想定)
/// - `D1` = P1 Key1 landmine (object 値 = damage)
const SYNTHETIC_BMS: &str = r#"#PLAYER 1
#GENRE Test
#TITLE bms-rs spike
#ARTIST nobody
#BPM 150
#PLAYLEVEL 5
#RANK 2
#TOTAL 200
#LNTYPE 1
#LNOBJ ZZ

#WAV01 kick.wav
#WAV02 snare.wav
#WAV03 hat.wav
#WAVZZ ln_end.wav

#00111:01010101
#00112:00010001
#00116:01000000
#00131:02020000
#00151:03000003
#001D1:0008000C

#RANDOM 2
#IF 1
#00211:01010101
#00212:01010101
#ENDIF
#ENDRANDOM
"#;

fn main() {
    println!("=== bms-rs spike ===");

    bms_rs_summary();
    println!();
    internal_parser_summary();
}

fn bms_rs_summary() {
    println!("--- bms-rs (KeyLayoutBeat) ---");
    let BmsOutput { bms, warnings } =
        parse_bms::<KeyLayoutBeat, _, _, _>(SYNTHETIC_BMS, default_config());

    println!("warnings: {}", warnings.len());
    for (i, w) in warnings.iter().take(8).enumerate() {
        println!("  [{i}] {w:?}");
    }
    if warnings.len() > 8 {
        println!("  ... +{} more", warnings.len() - 8);
    }

    let bms = match bms {
        Ok(b) => b,
        Err(e) => {
            println!("parse error: {e:?}");
            return;
        }
    };

    let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    for note in bms.notes().all_notes() {
        let mapped = KeyLayoutBeat::from_channel_id(note.channel_id);
        let label = match mapped.map(|m| m.kind()) {
            Some(BmsNoteKind::Visible) => "Visible",
            Some(BmsNoteKind::Invisible) => "Invisible",
            Some(BmsNoteKind::Long) => "Long",
            Some(BmsNoteKind::Landmine) => "Landmine",
            None => "Unmapped",
        };
        *counts.entry(label).or_default() += 1;
    }
    println!("notes by kind:");
    for (k, c) in &counts {
        println!("  {k:?}: {c}");
    }

    println!("landmine details:");
    for note in bms.notes().all_notes() {
        let Some(m) = KeyLayoutBeat::from_channel_id(note.channel_id) else { continue };
        if m.kind() != BmsNoteKind::Landmine {
            continue;
        }
        println!(
            "  track={} num/den={}/{} side={:?} key={:?} wav_id=ObjId({}) -> damage_u16={}",
            note.offset.track().0,
            note.offset.numerator(),
            note.offset.denominator_u64(),
            m.side(),
            m.key(),
            note.wav_id,
            note.wav_id.as_u16(),
        );
    }
}

fn internal_parser_summary() {
    println!("--- 内製 parser (bmz_chart::import) ---");
    let mut tmp = tempfile::Builder::new()
        .prefix("bms_rs_spike_")
        .suffix(".bms")
        .tempfile()
        .expect("tempfile");
    tmp.write_all(SYNTHETIC_BMS.as_bytes()).unwrap();

    let result = match import_bms_chart(tmp.path(), Some(1), false) {
        Ok(r) => r,
        Err(e) => {
            println!("import error: {e:?}");
            return;
        }
    };

    println!("warnings: {}", result.warnings.len());
    for (i, w) in result.warnings.iter().take(12).enumerate() {
        println!("  [{i}] {w:?}");
    }
    if result.warnings.len() > 12 {
        println!("  ... +{} more", result.warnings.len() - 12);
    }

    let chart = &result.chart;
    let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    for lane_notes in &chart.lane_notes {
        for n in lane_notes {
            let k = match n.kind {
                NoteKind::Tap => "Tap",
                NoteKind::Invisible => "Invisible",
                NoteKind::LongStart => "LongStart",
                NoteKind::LongEnd => "LongEnd",
                NoteKind::Mine => "Mine",
            };
            *counts.entry(k).or_default() += 1;
        }
    }
    println!("notes by kind:");
    for (k, c) in &counts {
        println!("  {k}: {c}");
    }
    println!("total_notes (scored): {}", chart.total_notes);
    println!("long_notes pairs: {}", chart.long_notes.len());
}
