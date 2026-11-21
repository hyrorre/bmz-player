//! AUTO SCRATCH / BATTLE ASの自動演奏レーン。
//!
//! beatoraja / LR2oraja Endless Dreamの`AutoplayModifier`は、自動演奏するレーンのノーツを
//! 背景音へ移す。判定・コンボ・EXスコア・ゲージ・総ノーツ数の対象外になり、キー音だけが鳴る。
//! BMZは同じ採点にしつつ、表示設定のためにノーツを判定しないダミーノーツとして残す。

use std::collections::BTreeSet;

use bmz_chart::model::{NoteKind, PlayableChart, SoundEvent};
use bmz_core::lane::Lane;

/// 指定レーンのノーツを採点対象から外し、キー音をBGMへ移す。
///
/// 全譜面オプションの適用後に呼ぶ。配置でスクラッチへ移ったノーツも対象にするため。
/// 地雷はダメージを与えず、Kaleidのフリーゾーンは区間ごと外す。見えないノーツは
/// beatorajaと同じくレーンに残す。譜面を変えた場合だけtrueを返す。
pub fn move_auto_scratch_notes_to_background(chart: &mut PlayableChart, lanes: &[Lane]) -> bool {
    let zone_ends: BTreeSet<_> = chart
        .metadata
        .free_zones
        .iter()
        .filter(|zone| lanes.contains(&zone.lane))
        .map(|zone| zone.note_id)
        .collect();
    chart.metadata.free_zones.retain(|zone| !lanes.contains(&zone.lane));
    let mut changed = !zone_ends.is_empty();
    let mut moved_sound = false;
    for &lane in lanes {
        let notes = &mut chart.lane_notes[lane.index()];
        notes.retain(|note| !zone_ends.contains(&note.id));
        for note in notes.iter_mut() {
            let Some(dummy) = display_only_kind(note.kind) else {
                continue;
            };
            if note.kind.is_scheduled_keysound() {
                for sound in note.sounds() {
                    chart.bgm_events.push(SoundEvent { tick: note.tick, time: note.time, sound });
                    moved_sound = true;
                }
            }
            note.kind = dummy;
            note.sound = None;
            note.layered_sounds.clear();
            changed = true;
        }
    }
    if moved_sound {
        chart.bgm_events.sort_by_key(|event| (event.time, event.tick));
    }
    if changed {
        super::refresh_total_notes(chart);
    }
    changed
}

/// 指定レーンに、まだ採点対象から外していないノーツやフリーゾーンがあるか。
pub fn has_auto_scratch_notes(chart: &PlayableChart, lanes: &[Lane]) -> bool {
    chart.metadata.free_zones.iter().any(|zone| lanes.contains(&zone.lane))
        || lanes.iter().any(|lane| {
            chart.lane_notes[lane.index()].iter().any(|note| display_only_kind(note.kind).is_some())
        })
}

const fn display_only_kind(kind: NoteKind) -> Option<NoteKind> {
    match kind {
        NoteKind::Tap => Some(NoteKind::DummyTap),
        NoteKind::LongStart => Some(NoteKind::DummyLongStart),
        NoteKind::LongEnd => Some(NoteKind::DummyLongEnd),
        NoteKind::Mine => Some(NoteKind::DummyMine),
        NoteKind::Potion => Some(NoteKind::DummyPotion),
        NoteKind::Invisible
        | NoteKind::DummyTap
        | NoteKind::DummyLongStart
        | NoteKind::DummyLongEnd
        | NoteKind::DummyMine
        | NoteKind::DummyPotion
        | NoteKind::FreeZoneEnd => None,
    }
}

#[cfg(test)]
mod tests;
